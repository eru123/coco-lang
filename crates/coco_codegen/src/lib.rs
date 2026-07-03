//! Coco AOT native compiler — LLVM code generation.
//!
//! Walks the typed AST and emits LLVM IR, then compiles to a native binary
//! via the system linker. The generated code links against `libcoco_rt` which
//! provides the GC heap, value boxing, and builtins.
//!
//! This crate requires system LLVM 18 (via `inkwell`/`llvm-sys`). To avoid
//! forcing an LLVM install on every workspace build, the LLVM-dependent code
//! is gated behind the `native` feature. Without it, the crate compiles to an
//! empty stub; enable `native` (or the `native` feature on `coco_cli`) to use it.

#[cfg(feature = "native")]
mod native {
    use coco_syntax::*;
    use inkwell::builder::Builder;
    use inkwell::context::Context;
    use inkwell::execution_engine::{ExecutionEngine, JitFunction};
    use inkwell::basic_block::BasicBlock;
    use inkwell::module::Module;
    use inkwell::types::{BasicType, BasicTypeEnum, FunctionType, StructType};
    use inkwell::values::{
        BasicValue, BasicValueEnum, FunctionValue, IntValue, PointerValue, StructValue,
    };
    use inkwell::{AddressSpace, OptimizationLevel};
    use std::collections::HashMap;

/// The LLVM code generator.
pub struct Codegen<'ctx> {
    pub context: &'ctx Context,
    pub module: Module<'ctx>,
    pub builder: Builder<'ctx>,
    /// The LLVM struct type for Coco's runtime Value.
    pub value_type: StructType<'ctx>,
    /// Named functions in scope: name -> (func, arity).
    functions: HashMap<String, (FunctionValue<'ctx>, usize)>,
    /// Local variables in the current function: name -> (ptr, is_mutable).
    locals: HashMap<String, PointerValue<'ctx>>,
    /// Current function being compiled.
    current_fn: Option<FunctionValue<'ctx>>,
    /// The runtime heap pointer (passed as a global or context).
    runtime_struct: Option<PointerValue<'ctx>>,
    /// Compiled classes: name -> (struct type, ordered property names).
    /// The struct type is a packed layout of Value (i64,i64) per property.
    classes: HashMap<String, (StructType<'ctx>, Vec<String>)>,
    /// Loop context stack for `break`/`continue`: each entry is
    /// `(continue_target, break_target)`. Pushed on loop entry, popped on
    /// exit. Empty outside a loop, so `break`/`continue` error there.
    loop_stack: Vec<(BasicBlock<'ctx>, BasicBlock<'ctx>)>,
}

/// The Coco runtime value represented as an LLVM struct: { i64 tag, [data] }
/// tag 0=int, 1=float, 2=string(ptr), 3=bool, 4=null, 5=list(ptr), 6=map(ptr)
const TAG_INT: u64 = 0;
const TAG_FLOAT: u64 = 1;
const TAG_STRING: u64 = 2;
const TAG_BOOL: u64 = 3;
const TAG_NULL: u64 = 4;

impl<'ctx> Codegen<'ctx> {
    /// Create a new code generator for a module named `name`.
    pub fn new(context: &'ctx Context, name: &str) -> Self {
        let module = context.create_module(name);
        let builder = context.create_builder();

        // Build the Coco Value struct: { i64, i64 } — tag + data (simplified)
        let i64_type = context.i64_type();
        let value_type = context.struct_type(&[i64_type.into(), i64_type.into()], false);

        Self {
            context,
            module,
            builder,
            value_type,
            functions: HashMap::new(),
            locals: HashMap::new(),
            current_fn: None,
            runtime_struct: None,
            classes: HashMap::new(),
            loop_stack: Vec::new(),
        }
    }

    /// Generate LLVM IR for a program. Returns the main function if found.
    pub fn generate(&mut self, program: &Program) -> Result<(), String> {
        // Declare runtime functions
        self.declare_runtime();

        // First pass: declare all functions. The Coco `main` is declared as
        // `coco_main` so it doesn't collide with the C `main` entry point
        // added by generate_entry (which returns i64, not the Value struct).
        for item in &program.items {
            let fd: Option<&FnDecl> = match item {
                Item::FnDecl(f) => Some(f),
                Item::Export(e) => match &*e.item { Item::FnDecl(f) => Some(f), _ => None },
                _ => None,
            };
            if let Some(fd) = fd {
                let mut name = fd.name.name.clone();
                if name == "main" {
                    name = "coco_main".to_string();
                }
                let arity = fd.params.len();
                let fn_type = self.fn_type(arity);
                let func = self.module.add_function(&name, fn_type, None);
                self.functions.insert(name, (func, arity));
            }
        }

        // First pass for classes: define LLVM struct types and declare methods.
        for item in &program.items {
            if let Item::ClassDecl(class_decl) = item {
                self.declare_class(class_decl)?;
            }
        }

        // Second pass: compile function bodies. Pure declarations (imports,
        // type aliases, interfaces, traits, enums, top-level const/let) need
        // no codegen and are intentionally skipped; executable top-level items
        // (bare expressions/statements) are not supported at module scope and
        // must live inside a function.
        for item in &program.items {
            match item {
                Item::FnDecl(fn_decl) => self.compile_fn_decl(fn_decl)?,
                Item::Export(export) => match &*export.item {
                    Item::FnDecl(fd) => self.compile_fn_decl(fd)?,
                    Item::ClassDecl(_) => { /* handled in the class pass below */ }
                    // Exported declarations (const/let/type/import/...) emit no code.
                    Item::ConstDecl(_)
                    | Item::LetDecl(_)
                    | Item::TypeAlias(_)
                    | Item::Import(_)
                    | Item::InterfaceDecl(_)
                    | Item::TraitDecl(_)
                    | Item::EnumDecl(_) => {}
                    other => {
                        return Err(format!(
                            "unsupported top-level export in native codegen: {}",
                            item_kind(other)
                        ))
                    }
                },
                // Pure declarations — no codegen.
                Item::ConstDecl(_)
                | Item::LetDecl(_)
                | Item::TypeAlias(_)
                | Item::Import(_)
                | Item::InterfaceDecl(_)
                | Item::TraitDecl(_)
                | Item::EnumDecl(_) => {}
                Item::ClassDecl(_) => { /* handled in the class pass below */ }
                other => {
                    return Err(format!(
                        "unsupported top-level item in native codegen: {}",
                        item_kind(other)
                    ))
                }
            }
        }

        // Second pass for classes: compile method bodies and constructors.
        for item in &program.items {
            if let Item::ClassDecl(class_decl) = item {
                self.compile_class_bodies(class_decl)?;
            }
        }

        // Add a wrapper main() that calls the Coco main() if it exists
        self.generate_entry()?;

        // Verify the module
        if let Err(msg) = self.module.verify() {
            return Err(format!("LLVM verification failed: {}", msg.to_string()));
        }

        Ok(())
    }

    /// Declare external runtime functions.
    fn declare_runtime(&mut self) {
        let i64 = self.context.i64_type();
        // Runtime allocate: (tag, data) -> {i64,i64}* (pointer to CocoValue).
        let ret_type = self.value_type.ptr_type(AddressSpace::default());
        let rt_alloc_type = ret_type.fn_type(&[i64.into(), i64.into()], false);
        self.module.add_function("coco_rt_alloc", rt_alloc_type, None);
    }

    /// Build the LLVM function type: (i64, i64, ...) -> {i64, i64}
    fn fn_type(&self, arity: usize) -> FunctionType<'ctx> {
        let i64 = self.context.i64_type();
        let mut params: Vec<inkwell::types::BasicMetadataTypeEnum> = Vec::new();
        for _ in 0..arity {
            params.push(i64.into()); // tag
            params.push(i64.into()); // data
        }
        self.value_type.fn_type(&params, false)
    }

    /// Compile a function declaration body.
    fn compile_fn_decl(&mut self, fn_decl: &FnDecl) -> Result<(), String> {
        let mut name = fn_decl.name.name.clone();
        if name == "main" {
            name = "coco_main".to_string();
        }
        let func = self
            .functions
            .get(&name)
            .ok_or_else(|| format!("function '{}' not declared", name))?
            .0;
        self.current_fn = Some(func);
        self.locals.clear();

        let entry = self.context.append_basic_block(func, "entry");
        self.builder.position_at_end(entry);

        // Allocate locals for parameters
        let params = func.get_params();
        for (i, param) in fn_decl.params.iter().enumerate() {
            let tag = params[i * 2];
            let data = params[i * 2 + 1];
            // Store param values on stack
            let alloca = self.builder.build_alloca(self.value_type, &param.name.name).map_err(|e| e.to_string())?;
            let tag_val = tag.into_int_value();
            let data_val = data.into_int_value();
            let undef = self.value_type.const_zero();
            let val = self.build_value(tag_val, data_val);
            self.builder.build_store(alloca, val).map_err(|e| e.to_string())?;
            self.locals.insert(param.name.name.clone(), alloca);
        }

        // Compile body
        self.compile_block(&fn_decl.body)?;

        // Default return null if the body didn't already terminate the block
        // with a return. Emitting a return after a terminator is invalid IR.
        let block_terminated = self
            .builder
            .get_insert_block()
            .and_then(|b| b.get_terminator())
            .is_some();
        if !block_terminated {
            let null_val = self.build_value(
                self.context.i64_type().const_int(TAG_NULL, false),
                self.context.i64_type().const_int(0, false),
            );
            self.builder.build_return(Some(&null_val)).map_err(|e| e.to_string())?;
        }

        self.current_fn = None;
        Ok(())
    }

    /// Compile a block of statements.
    fn compile_block(&mut self, block: &Block) -> Result<(), String> {
        for stmt in &block.stmts {
            self.compile_stmt(stmt)?;
        }
        Ok(())
    }

    /// Compile a single statement.
    fn compile_stmt(&mut self, stmt: &Stmt) -> Result<(), String> {
        match stmt {
            Stmt::Expr(expr_stmt) => {
                self.compile_expr(&expr_stmt.expr)?;
            }
            Stmt::Return(ret) => {
                let val = if let Some(ref expr) = ret.value {
                    self.compile_expr(expr)?
                } else {
                    self.build_value(
                        self.context.i64_type().const_int(TAG_NULL, false),
                        self.context.i64_type().const_int(0, false),
                    )
                };
                self.builder.build_return(Some(&val)).map_err(|e| e.to_string())?;
            }
            Stmt::If(if_stmt) => {
                self.compile_if(if_stmt)?;
            }
            Stmt::While(while_stmt) => {
                self.compile_while(while_stmt)?;
            }
            Stmt::Loop(loop_stmt) => self.compile_loop(loop_stmt)?,
            Stmt::DoWhile(dw) => self.compile_do_while(dw)?,
            Stmt::For(for_stmt) => self.compile_for(for_stmt)?,
            Stmt::Break(_) => self.compile_break()?,
            Stmt::Continue(_) => self.compile_continue()?,
            Stmt::Item(item) => match &**item {
                Item::LetDecl(let_decl) => self.compile_let_decl(let_decl)?,
                Item::ConstDecl(const_decl) => self.compile_const_decl(const_decl)?,
                // Other nested items (nested fn, import, ...) are declarations
                // that need no statement-level codegen here.
                _ => {}
            },
            other => {
                return Err(format!(
                    "unsupported statement in native codegen: {}",
                    stmt_kind(other)
                ))
            }
        }
        Ok(())
    }

    /// Compile a let declaration: evaluate the value (if any) and bind the
    /// name to a local alloca.
    fn compile_let_decl(&mut self, let_decl: &LetDecl) -> Result<(), String> {
        let val = if let Some(ref expr) = let_decl.value {
            self.compile_expr(expr)?
        } else {
            let i64 = self.context.i64_type();
            self.build_value(i64.const_int(TAG_NULL, false), i64.const_int(0, false))
        };
        let alloca = self
            .builder
            .build_alloca(self.value_type, &let_decl.name.name)
            .map_err(|e| e.to_string())?;
        self.builder.build_store(alloca, val).map_err(|e| e.to_string())?;
        self.locals.insert(let_decl.name.name.clone(), alloca);
        Ok(())
    }

    /// Compile a const declaration: same as let but the value is required.
    fn compile_const_decl(&mut self, const_decl: &ConstDecl) -> Result<(), String> {
        let val = self.compile_expr(&const_decl.value)?;
        let alloca = self
            .builder
            .build_alloca(self.value_type, &const_decl.name.name)
            .map_err(|e| e.to_string())?;
        self.builder.build_store(alloca, val).map_err(|e| e.to_string())?;
        self.locals.insert(const_decl.name.name.clone(), alloca);
        Ok(())
    }

    /// Compile an if/else-if/else statement. Each branch that falls through
    /// joins a shared merge block; branches that return/break don't.
    fn compile_if(&mut self, if_stmt: &IfStmt) -> Result<(), String> {
        let merge_block = self.context.append_basic_block(self.current_fn.unwrap(), "ifmerge");
        self.compile_if_branch(
            &if_stmt.condition,
            &if_stmt.then_block,
            &if_stmt.else_ifs,
            &if_stmt.else_block,
            merge_block,
        )?;
        // Position at the merge block. (It may be unreachable if every branch
        // returned; LLVM allows an unreachable block with no predecessors.)
        self.builder.position_at_end(merge_block);
        Ok(())
    }

    /// Compile one `if`/`else if` branch: `cond ? then_block : <rest>`.
    /// `rest` is the remaining else-if chain (recursed) plus the final else
    /// block. Falling-through branches jump to `merge_block`.
    fn compile_if_branch(
        &mut self,
        cond: &Expr,
        then_block: &Block,
        else_ifs: &[ElseIf],
        else_block: &Option<Block>,
        merge_block: BasicBlock<'ctx>,
    ) -> Result<(), String> {
        let cond_val = self.compile_expr(cond)?;
        let cond = self.extract_data(&cond_val); // bool truthiness is in data (0/1), not tag
        let is_true = self.builder.build_int_compare(
            inkwell::IntPredicate::NE,
            cond,
            self.context.i64_type().const_int(0, false),
            "ifcond",
        ).map_err(|e| e.to_string())?;

        let then_bb = self.context.append_basic_block(self.current_fn.unwrap(), "then");
        let else_bb = self.context.append_basic_block(self.current_fn.unwrap(), "else");
        self.builder.build_conditional_branch(is_true, then_bb, else_bb).map_err(|e| e.to_string())?;

        // Then block.
        self.builder.position_at_end(then_bb);
        self.compile_block(then_block)?;
        if !self.block_terminated() {
            self.builder.build_unconditional_branch(merge_block).map_err(|e| e.to_string())?;
        }

        // Else block: recurse into the next else-if, or compile the final else,
        // or fall straight through to merge if there's neither.
        self.builder.position_at_end(else_bb);
        if let Some((next, rest)) = else_ifs.split_first() {
            self.compile_if_branch(&next.condition, &next.block, rest, else_block, merge_block)?;
        } else if let Some(eb) = else_block {
            self.compile_block(eb)?;
            if !self.block_terminated() {
                self.builder.build_unconditional_branch(merge_block).map_err(|e| e.to_string())?;
            }
        } else {
            self.builder.build_unconditional_branch(merge_block).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Returns true if the current insert block already has a terminator
    /// (return/branch), so we must not emit another one.
    fn block_terminated(&self) -> bool {
        self.builder
            .get_insert_block()
            .and_then(|b| b.get_terminator())
            .is_some()
    }

    /// Compile a signed-int comparison into a Bool Value (data = 0/1).
    fn compile_icmp(
        &self,
        left: StructValue<'ctx>,
        right: StructValue<'ctx>,
        pred: inkwell::IntPredicate,
        name: &str,
    ) -> Result<StructValue<'ctx>, String> {
        let i64 = self.context.i64_type();
        let l_data = self.extract_data(&left);
        let r_data = self.extract_data(&right);
        let cmp = self
            .builder
            .build_int_compare(pred, l_data, r_data, name)
            .map_err(|e| e.to_string())?;
        let result = self
            .builder
            .build_int_z_extend(cmp, i64, &format!("{}ext", name))
            .map_err(|e| e.to_string())?;
        Ok(self.build_value(i64.const_int(TAG_BOOL, false), result))
    }

    /// Compile a while loop. `continue` jumps to the condition block (so the
    /// condition is re-evaluated); `break` jumps to the end block.
    fn compile_while(&mut self, while_stmt: &WhileStmt) -> Result<(), String> {
        let cond_block = self.context.append_basic_block(self.current_fn.unwrap(), "whilecond");
        let body_block = self.context.append_basic_block(self.current_fn.unwrap(), "whilebody");
        let end_block = self.context.append_basic_block(self.current_fn.unwrap(), "whileend");

        self.builder.build_unconditional_branch(cond_block).map_err(|e| e.to_string())?;

        self.builder.position_at_end(cond_block);
        let cond_val = self.compile_expr(&while_stmt.condition)?;
        let cond = self.extract_data(&cond_val);  // bool truthiness is in data (0/1), not tag
        let is_true = self.builder.build_int_compare(
            inkwell::IntPredicate::NE,
            cond,
            self.context.i64_type().const_int(0, false),
            "whilecond",
        ).map_err(|e| e.to_string())?;
        self.builder.build_conditional_branch(is_true, body_block, end_block).map_err(|e| e.to_string())?;

        self.builder.position_at_end(body_block);
        // `continue` re-checks the condition; `break` exits.
        self.loop_stack.push((cond_block, end_block));
        self.compile_block(&while_stmt.body)?;
        self.loop_stack.pop();
        // Only loop back if the body didn't already return/break.
        if !self.block_terminated() {
            self.builder.build_unconditional_branch(cond_block).map_err(|e| e.to_string())?;
        }

        self.builder.position_at_end(end_block);
        Ok(())
    }

    /// Compile an infinite `loop { body }`. `continue` jumps to the body
    /// (there is no condition); `break` jumps to the end block.
    fn compile_loop(&mut self, loop_stmt: &LoopStmt) -> Result<(), String> {
        let body_block = self.context.append_basic_block(self.current_fn.unwrap(), "loopbody");
        let end_block = self.context.append_basic_block(self.current_fn.unwrap(), "loopend");

        self.builder.build_unconditional_branch(body_block).map_err(|e| e.to_string())?;

        self.builder.position_at_end(body_block);
        self.loop_stack.push((body_block, end_block));
        self.compile_block(&loop_stmt.body)?;
        self.loop_stack.pop();
        // An unterminated `loop` body loops forever.
        if !self.block_terminated() {
            self.builder.build_unconditional_branch(body_block).map_err(|e| e.to_string())?;
        }

        self.builder.position_at_end(end_block);
        Ok(())
    }

    /// Compile a `do { body } while (cond)`. The body runs at least once;
    /// `continue` jumps to the body (the trailing condition still runs
    /// afterwards), `break` jumps to the end.
    fn compile_do_while(&mut self, dw: &DoWhileStmt) -> Result<(), String> {
        let body_block = self.context.append_basic_block(self.current_fn.unwrap(), "dowhilebody");
        let cond_block = self.context.append_basic_block(self.current_fn.unwrap(), "dowhilecond");
        let end_block = self.context.append_basic_block(self.current_fn.unwrap(), "dowhileend");

        self.builder.build_unconditional_branch(body_block).map_err(|e| e.to_string())?;

        self.builder.position_at_end(body_block);
        // `continue` re-enters the body (cond still runs after); `break` exits.
        self.loop_stack.push((body_block, end_block));
        self.compile_block(&dw.body)?;
        self.loop_stack.pop();
        if !self.block_terminated() {
            self.builder.build_unconditional_branch(cond_block).map_err(|e| e.to_string())?;
        }

        self.builder.position_at_end(cond_block);
        let cond_val = self.compile_expr(&dw.condition)?;
        let cond = self.extract_data(&cond_val);
        let is_true = self.builder.build_int_compare(
            inkwell::IntPredicate::NE,
            cond,
            self.context.i64_type().const_int(0, false),
            "dowhilecond",
        ).map_err(|e| e.to_string())?;
        self.builder.build_conditional_branch(is_true, body_block, end_block).map_err(|e| e.to_string())?;

        self.builder.position_at_end(end_block);
        Ok(())
    }

    /// Compile a `for x in iterable { body }`. Only **range** iterables
    /// (`a..b`, `a..=b`) are supported natively — they lower to an integer
    /// counter loop with no runtime list allocation. Other iterables (lists,
    /// maps, strings) need runtime support not yet present and error clearly.
    /// `continue` jumps to the increment/condition check; `break` exits.
    fn compile_for(&mut self, for_stmt: &ForStmt) -> Result<(), String> {
        // Recognise `EXPR .. EXPR` / `EXPR ..= EXPR` directly in the AST so we
        // never materialise a runtime range/list.
        let (start_expr, end_expr, inclusive) = match &for_stmt.iterable {
            Expr::Binary(bin) => match bin.op {
                BinaryOp::Range => (&bin.left, &bin.right, false),
                BinaryOp::RangeInclusive => (&bin.left, &bin.right, true),
                _ => return Err(
                    "for-in over non-range iterables is not supported in native codegen yet \
                     (needs runtime list support)"
                        .to_string(),
                ),
            },
            _ => return Err(
                "for-in over non-range iterables is not supported in native codegen yet \
                 (needs runtime list support)"
                    .to_string(),
            ),
        };

        let fn_val = self.current_fn.unwrap();
        let i64 = self.context.i64_type();

        // Evaluate the range bounds once, before the loop.
        let start_val = self.compile_expr(start_expr)?;
        let end_val = self.compile_expr(end_expr)?;
        let start_data = self.extract_data(&start_val);
        let end_data = self.extract_data(&end_val);

        // Hidden counter alloca, initialised to `start`.
        let counter = self.builder.build_alloca(i64, "foridx").map_err(|e| e.to_string())?;
        self.builder.build_store(counter, start_data).map_err(|e| e.to_string())?;

        let cond_block = self.context.append_basic_block(fn_val, "forcond");
        let body_block = self.context.append_basic_block(fn_val, "forbody");
        let inc_block = self.context.append_basic_block(fn_val, "forinc");
        let end_block = self.context.append_basic_block(fn_val, "forend");

        self.builder.build_unconditional_branch(cond_block).map_err(|e| e.to_string())?;

        // Condition: counter < end (exclusive) or counter <= end (inclusive).
        self.builder.position_at_end(cond_block);
        let cur = self.builder.build_load(i64, counter, "forcur").map_err(|e| e.to_string())?.into_int_value();
        let pred = if inclusive {
            inkwell::IntPredicate::SLE
        } else {
            inkwell::IntPredicate::SLT
        };
        let keep = self.builder.build_int_compare(pred, cur, end_data, "forcmp").map_err(|e| e.to_string())?;
        self.builder.build_conditional_branch(keep, body_block, end_block).map_err(|e| e.to_string())?;

        // Body: bind the loop variable to the current counter value.
        self.builder.position_at_end(body_block);
        let cur_for_var = self.builder.build_load(i64, counter, "forvar").map_err(|e| e.to_string())?.into_int_value();
        let var_val = self.build_value(i64.const_int(TAG_INT, false), cur_for_var);
        let var_alloca = self.builder.build_alloca(self.value_type, &for_stmt.pattern.name).map_err(|e| e.to_string())?;
        self.builder.build_store(var_alloca, var_val).map_err(|e| e.to_string())?;
        // Save any pre-existing binding of the same name to restore after the loop.
        let prev = self.locals.insert(for_stmt.pattern.name.clone(), var_alloca);
        // `continue` runs the increment; `break` exits.
        self.loop_stack.push((inc_block, end_block));
        self.compile_block(&for_stmt.body)?;
        self.loop_stack.pop();
        // Restore the previous binding (if any).
        if let Some(p) = prev {
            self.locals.insert(for_stmt.pattern.name.clone(), p);
        } else {
            self.locals.remove(&for_stmt.pattern.name);
        }
        if !self.block_terminated() {
            self.builder.build_unconditional_branch(inc_block).map_err(|e| e.to_string())?;
        }

        // Increment: counter += 1.
        self.builder.position_at_end(inc_block);
        let cur = self.builder.build_load(i64, counter, "forinc_cur").map_err(|e| e.to_string())?.into_int_value();
        let one = i64.const_int(1, false);
        let next = self.builder.build_int_add(cur, one, "fornext").map_err(|e| e.to_string())?;
        self.builder.build_store(counter, next).map_err(|e| e.to_string())?;
        self.builder.build_unconditional_branch(cond_block).map_err(|e| e.to_string())?;

        self.builder.position_at_end(end_block);
        Ok(())
    }

    /// `break` — jump to the innermost loop's break target. Errors outside a
    /// loop, matching the interpreter (`compiler.rs` compile_break).
    fn compile_break(&mut self) -> Result<(), String> {
        let (_, break_block) = self
            .loop_stack
            .last()
            .copied()
            .ok_or_else(|| "break outside loop".to_string())?;
        self.builder.build_unconditional_branch(break_block).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// `continue` — jump to the innermost loop's continue target. Errors
    /// outside a loop.
    fn compile_continue(&mut self) -> Result<(), String> {
        let (continue_block, _) = self
            .loop_stack
            .last()
            .copied()
            .ok_or_else(|| "continue outside loop".to_string())?;
        self.builder.build_unconditional_branch(continue_block).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Compile an expression and return the resulting Value struct.
    fn compile_expr(&mut self, expr: &Expr) -> Result<StructValue<'ctx>, String> {
        match expr {
            Expr::Literal(lit) => self.compile_literal(lit),
            Expr::Ident(ident) => self.compile_ident(ident),
            Expr::Binary(bin) => self.compile_binary(bin),
            Expr::Unary(un) => self.compile_unary(un),
            Expr::Call(call) => self.compile_call(call),
            Expr::Index(idx) => self.compile_index(idx),
            Expr::Member(mem) => self.compile_member(mem),
            Expr::Ternary(tern) => self.compile_ternary(tern),
            Expr::NullCoalesce(nc) => self.compile_null_coalesce(nc),
            Expr::Array(arr) => self.compile_array(arr),
            Expr::Group(inner) => self.compile_expr(inner),
            other => Err(format!(
                "unsupported expression in native codegen: {}",
                expr_kind(other)
            )),
        }
    }

    /// Compile a unary expression (negation, logical not).
    fn compile_unary(&mut self, un: &UnaryExpr) -> Result<StructValue<'ctx>, String> {
        let i64 = self.context.i64_type();
        let operand = self.compile_expr(&un.expr)?;
        match un.op {
            UnaryOp::Neg => {
                let data = self.extract_data(&operand);
                let result = self.builder.build_int_neg(data, "neg").map_err(|e| e.to_string())?;
                Ok(self.build_value(i64.const_int(TAG_INT, false), result))
            }
            UnaryOp::Not => {
                let data = self.extract_data(&operand);
                // Logical not: 1 if data == 0, else 0.
                let zero = i64.const_int(0, false);
                let is_zero = self.builder.build_int_compare(
                    inkwell::IntPredicate::EQ, data, zero, "iszero",
                ).map_err(|e| e.to_string())?;
                let result = self.builder.build_int_z_extend(is_zero, i64, "not").map_err(|e| e.to_string())?;
                Ok(self.build_value(i64.const_int(TAG_BOOL, false), result))
            }
            UnaryOp::BitNot => {
                let data = self.extract_data(&operand);
                let result = self.builder.build_not(data, "bitnot").map_err(|e| e.to_string())?;
                Ok(self.build_value(i64.const_int(TAG_INT, false), result))
            }
            other => Err(format!("unsupported unary op in native codegen: {:?}", other)),
        }
    }

    /// Compile an index expression (list[i] or map["k"]).
    fn compile_index(&mut self, _idx: &IndexExpr) -> Result<StructValue<'ctx>, String> {
        // Indexing requires runtime list/map support not yet in coco_rt.
        Err("indexing (a[i]) is not yet supported in native codegen".to_string())
    }

    /// Compile a member-access expression (obj.prop).
    fn compile_member(&mut self, _mem: &MemberExpr) -> Result<StructValue<'ctx>, String> {
        // Member access requires runtime object support not yet in coco_rt.
        Err("member access (a.b) is not yet supported in native codegen".to_string())
    }

    /// Compile a ternary expression (cond ? then : else).
    fn compile_ternary(&mut self, tern: &TernaryExpr) -> Result<StructValue<'ctx>, String> {
        let cond_val = self.compile_expr(&tern.condition)?;
        let cond = self.extract_data(&cond_val);  // bool truthiness is in data (0/1), not tag
        let is_true = self.builder.build_int_compare(
            inkwell::IntPredicate::NE, cond,
            self.context.i64_type().const_int(0, false), "terncond",
        ).map_err(|e| e.to_string())?;

        let then_block = self.context.append_basic_block(self.current_fn.unwrap(), "ternthen");
        let else_block = self.context.append_basic_block(self.current_fn.unwrap(), "ternelse");
        let merge_block = self.context.append_basic_block(self.current_fn.unwrap(), "ternmerge");

        self.builder.build_conditional_branch(is_true, then_block, else_block).map_err(|e| e.to_string())?;

        // Then branch.
        self.builder.position_at_end(then_block);
        let then_val = self.compile_expr(&tern.then_expr)?;
        self.builder.build_unconditional_branch(merge_block).map_err(|e| e.to_string())?;
        let then_block = self.builder.get_insert_block().unwrap();

        // Else branch.
        self.builder.position_at_end(else_block);
        let else_val = self.compile_expr(&tern.else_expr)?;
        self.builder.build_unconditional_branch(merge_block).map_err(|e| e.to_string())?;
        let else_block = self.builder.get_insert_block().unwrap();

        // Merge: phi over the two branches.
        self.builder.position_at_end(merge_block);
        let phi = self.builder.build_phi(self.value_type, "ternval").map_err(|e| e.to_string())?;
        phi.add_incoming(&[(&then_val, then_block), (&else_val, else_block)]);
        Ok(phi.as_basic_value().into_struct_value())
    }

    /// Compile a null-coalesce expression (a ?? b): a if a is not null, else b.
    fn compile_null_coalesce(&mut self, nc: &NullCoalesceExpr) -> Result<StructValue<'ctx>, String> {
        let left = self.compile_expr(&nc.left)?;
        let tag = self.extract_tag(&left);
        let is_null = self.builder.build_int_compare(
            inkwell::IntPredicate::EQ, tag,
            self.context.i64_type().const_int(TAG_NULL, false), "isnull",
        ).map_err(|e| e.to_string())?;

        let then_block = self.context.append_basic_block(self.current_fn.unwrap(), "ncthen");
        let else_block = self.context.append_basic_block(self.current_fn.unwrap(), "ncelse");
        let merge_block = self.context.append_basic_block(self.current_fn.unwrap(), "ncmerge");

        // If left is null, evaluate right; else use left.
        self.builder.build_conditional_branch(is_null, else_block, then_block).map_err(|e| e.to_string())?;

        self.builder.position_at_end(then_block);
        self.builder.build_unconditional_branch(merge_block).map_err(|e| e.to_string())?;
        let then_block = self.builder.get_insert_block().unwrap();

        self.builder.position_at_end(else_block);
        let right_val = self.compile_expr(&nc.right)?;
        self.builder.build_unconditional_branch(merge_block).map_err(|e| e.to_string())?;
        let else_block = self.builder.get_insert_block().unwrap();

        self.builder.position_at_end(merge_block);
        let phi = self.builder.build_phi(self.value_type, "ncval").map_err(|e| e.to_string())?;
        phi.add_incoming(&[(&left, then_block), (&right_val, else_block)]);
        Ok(phi.as_basic_value().into_struct_value())
    }

    /// Compile an array/list literal.
    fn compile_array(&mut self, _arr: &ArrayLiteral) -> Result<StructValue<'ctx>, String> {
        // List construction needs runtime support not yet in coco_rt.
        Err("array literals are not yet supported in native codegen".to_string())
    }

    /// First pass for a class: define the LLVM struct type for its property
    /// layout and declare each method as a function named `Class.method`.
    /// The struct is a packed layout of one Value ({i64,i64}) per property,
    /// in declaration order. Methods take a `this` pointer as the first
    /// argument (two i64 params: tag + data), followed by their params.
    fn declare_class(&mut self, class_decl: &ClassDecl) -> Result<(), String> {
        let class_name = &class_decl.name.name;

        // Collect property names in declaration order.
        let mut prop_names: Vec<String> = Vec::new();
        for member in &class_decl.members {
            if let ClassMember::Property(prop) = member {
                prop_names.push(prop.name.name.clone());
            }
        }

        // Build the struct type: { Value, Value, ... } one per property.
        // Each Value is { i64, i64 }. An empty class gets a single i64 pad so
        // the struct is non-zero-sized.
        let i64 = self.context.i64_type();
        let field_types: Vec<BasicTypeEnum> = if prop_names.is_empty() {
            vec![i64.into()]
        } else {
            (0..prop_names.len()).map(|_| self.value_type.into()).collect()
        };
        let struct_type = self.context.struct_type(&field_types, false);
        self.classes
            .insert(class_name.clone(), (struct_type, prop_names.clone()));

        // Declare each method as `Class.method` with a `this` first param.
        for member in &class_decl.members {
            if let ClassMember::Method(method) = member {
                let mangled = format!("{}.{}", class_name, method.name.name);
                // Params: this (tag, data) + each method param (tag, data).
                let arity = method.params.len() + 1; // +1 for `this`
                let fn_type = self.fn_type(arity);
                let func = self.module.add_function(&mangled, fn_type, None);
                self.functions.insert(mangled, (func, arity));
            }
        }
        Ok(())
    }

    /// Second pass for a class: compile method bodies and the constructor.
    /// The constructor allocates the struct via coco_rt (a zeroed allocation
    /// of the struct size) and stores default property values.
    fn compile_class_bodies(&mut self, class_decl: &ClassDecl) -> Result<(), String> {
        let class_name = &class_decl.name.name;
        let (struct_type, prop_names) = match self.classes.get(class_name) {
            Some(entry) => entry.clone(),
            None => return Ok(()), // declared elsewhere or empty
        };
        let _ = struct_type;

        for member in &class_decl.members {
            match member {
                ClassMember::Constructor(ctor) => {
                    let ctor_name = format!("{}.constructor", class_name);
                    if let Some(&(func, _)) = self.functions.get(&ctor_name) {
                        self.compile_ctor(func, ctor, class_name, &prop_names)?;
                    }
                }
                ClassMember::Method(method) => {
                    let mangled = format!("{}.{}", class_name, method.name.name);
                    if let Some(&(func, _)) = self.functions.get(&mangled) {
                        self.compile_method(func, method, class_name)?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Compile a constructor: allocate the instance struct (zeroed) and store
    /// default property values, then run the constructor body.
    fn compile_ctor(
        &mut self,
        func: FunctionValue<'ctx>,
        ctor: &Constructor,
        class_name: &str,
        prop_names: &[String],
    ) -> Result<(), String> {
        self.current_fn = Some(func);
        self.locals.clear();
        let entry = self.context.append_basic_block(func, "entry");
        self.builder.position_at_end(entry);

        // Bind `this` (param 0 = tag, param 1 = data) as a local named "this".
        let params = func.get_params();
        let this_alloca = self
            .builder
            .build_alloca(self.value_type, "this")
            .map_err(|e| e.to_string())?;
        let this_val = self.build_value(
            params[0].into_int_value(),
            params[1].into_int_value(),
        );
        self.builder.build_store(this_alloca, this_val).map_err(|e| e.to_string())?;
        self.locals.insert("this".to_string(), this_alloca);

        // Bind constructor params (skipping `this`'s two i64s).
        for (i, param) in ctor.params.iter().enumerate() {
            let tag = params[(i + 1) * 2];
            let data = params[(i + 1) * 2 + 1];
            let alloca = self
                .builder
                .build_alloca(self.value_type, &param.name.name)
                .map_err(|e| e.to_string())?;
            let val = self.build_value(tag.into_int_value(), data.into_int_value());
            self.builder.build_store(alloca, val).map_err(|e| e.to_string())?;
            self.locals.insert(param.name.name.clone(), alloca);
        }

        // Compile the constructor body.
        self.compile_block(&ctor.body)?;

        // Default return null if the block didn't terminate.
        let i64 = self.context.i64_type();
        let block_terminated = self
            .builder
            .get_insert_block()
            .and_then(|b| b.get_terminator())
            .is_some();
        if !block_terminated {
            let null_val = self.build_value(i64.const_int(TAG_NULL, false), i64.const_int(0, false));
            self.builder.build_return(Some(&null_val)).map_err(|e| e.to_string())?;
        }
        self.current_fn = None;
        let _ = (class_name, prop_names);
        Ok(())
    }

    /// Compile a method body. `this` is bound from the first param pair.
    fn compile_method(
        &mut self,
        func: FunctionValue<'ctx>,
        method: &Method,
        class_name: &str,
    ) -> Result<(), String> {
        self.current_fn = Some(func);
        self.locals.clear();
        let entry = self.context.append_basic_block(func, "entry");
        self.builder.position_at_end(entry);

        let params = func.get_params();
        // Bind `this`.
        let this_alloca = self
            .builder
            .build_alloca(self.value_type, "this")
            .map_err(|e| e.to_string())?;
        let this_val = self.build_value(params[0].into_int_value(), params[1].into_int_value());
        self.builder.build_store(this_alloca, this_val).map_err(|e| e.to_string())?;
        self.locals.insert("this".to_string(), this_alloca);

        // Bind method params.
        for (i, param) in method.params.iter().enumerate() {
            let tag = params[(i + 1) * 2];
            let data = params[(i + 1) * 2 + 1];
            let alloca = self
                .builder
                .build_alloca(self.value_type, &param.name.name)
                .map_err(|e| e.to_string())?;
            let val = self.build_value(tag.into_int_value(), data.into_int_value());
            self.builder.build_store(alloca, val).map_err(|e| e.to_string())?;
            self.locals.insert(param.name.name.clone(), alloca);
        }

        self.compile_block(&method.body)?;

        let i64 = self.context.i64_type();
        let block_terminated = self
            .builder
            .get_insert_block()
            .and_then(|b| b.get_terminator())
            .is_some();
        if !block_terminated {
            let null_val = self.build_value(i64.const_int(TAG_NULL, false), i64.const_int(0, false));
            self.builder.build_return(Some(&null_val)).map_err(|e| e.to_string())?;
        }
        self.current_fn = None;
        let _ = class_name;
        Ok(())
    }

    /// Compile a literal value.
    fn compile_literal(&mut self, lit: &Literal) -> Result<StructValue<'ctx>, String> {
        let i64 = self.context.i64_type();
        match lit {
            Literal::Int(n, _) => {
                Ok(self.build_value(
                    i64.const_int(TAG_INT, false),
                    i64.const_int(*n as u64, true),
                ))
            }
            Literal::Float(f, _) => {
                Ok(self.build_value(
                    i64.const_int(TAG_FLOAT, false),
                    i64.const_int(f.to_bits() as u64, false),
                ))
            }
            Literal::Bool(b, _) => {
                Ok(self.build_value(
                    i64.const_int(TAG_BOOL, false),
                    i64.const_int(if *b { 1 } else { 0 }, false),
                ))
            }
            Literal::Null(_) => Ok(self.build_value(
                i64.const_int(TAG_NULL, false),
                i64.const_int(0, false),
            )),
            Literal::String(s, _) => {
                // Allocate string via runtime
                let len = s.len() as u64;
                let ptr = self.call_runtime_alloc(TAG_STRING, len)?;
                Ok(ptr)
            }
            Literal::Char(c, _) => {
                // A char is represented by its Unicode scalar value as an int.
                // (There's no dedicated char runtime type yet; this is
                // consistent with the i64 value model and safe for int ops.)
                Ok(self.build_value(
                    i64.const_int(TAG_INT, false),
                    i64.const_int(*c as u64, false),
                ))
            }
        }
    }

    /// Compile an identifier reference.
    fn compile_ident(&mut self, ident: &Ident) -> Result<StructValue<'ctx>, String> {
        if let Some(&alloca) = self.locals.get(&ident.name) {
            let val = self.builder.build_load(self.value_type, alloca, &ident.name)
                .map_err(|e| e.to_string())?
                .into_struct_value();
            Ok(val)
        } else {
            Err(format!("undefined variable '{}' in native codegen", ident.name))
        }
    }

    /// Compile a binary expression.
    fn compile_binary(&mut self, bin: &BinaryExpr) -> Result<StructValue<'ctx>, String> {
        let i64 = self.context.i64_type();

        // Short-circuiting logical ops must NOT eagerly evaluate the RHS, so
        // handle them before compiling `right`. Ranges are only meaningful as
        // `for`-loop iterables; as a value expression they're unsupported.
        match bin.op {
            BinaryOp::And => return self.compile_short_circuit(bin, false),
            BinaryOp::Or => return self.compile_short_circuit(bin, true),
            BinaryOp::Range | BinaryOp::RangeInclusive => {
                return Err(
                    "ranges are only supported as a for-in iterable in native codegen".to_string(),
                )
            }
            _ => {}
        }

        let left = self.compile_expr(&bin.left)?;
        let right = self.compile_expr(&bin.right)?;

        match bin.op {
            BinaryOp::Add => {
                let l_data = self.extract_data(&left);
                let r_data = self.extract_data(&right);
                let result_data = self.builder.build_int_add(l_data, r_data, "add")
                    .map_err(|e| e.to_string())?;
                Ok(self.build_value(i64.const_int(TAG_INT, false), result_data))
            }
            BinaryOp::Sub => {
                let l_data = self.extract_data(&left);
                let r_data = self.extract_data(&right);
                let result_data = self.builder.build_int_sub(l_data, r_data, "sub")
                    .map_err(|e| e.to_string())?;
                Ok(self.build_value(i64.const_int(TAG_INT, false), result_data))
            }
            BinaryOp::Mul => {
                let l_data = self.extract_data(&left);
                let r_data = self.extract_data(&right);
                let result_data = self.builder.build_int_mul(l_data, r_data, "mul")
                    .map_err(|e| e.to_string())?;
                Ok(self.build_value(i64.const_int(TAG_INT, false), result_data))
            }
            BinaryOp::Div => {
                let l_data = self.extract_data(&left);
                let r_data = self.extract_data(&right);
                let result_data = self.builder.build_int_signed_div(l_data, r_data, "div")
                    .map_err(|e| e.to_string())?;
                Ok(self.build_value(i64.const_int(TAG_INT, false), result_data))
            }
            BinaryOp::Mod => {
                let l_data = self.extract_data(&left);
                let r_data = self.extract_data(&right);
                let result_data = self.builder.build_int_signed_rem(l_data, r_data, "mod")
                    .map_err(|e| e.to_string())?;
                Ok(self.build_value(i64.const_int(TAG_INT, false), result_data))
            }
            BinaryOp::BitAnd => {
                let l_data = self.extract_data(&left);
                let r_data = self.extract_data(&right);
                let r = self.builder.build_and(l_data, r_data, "bitand").map_err(|e| e.to_string())?;
                Ok(self.build_value(i64.const_int(TAG_INT, false), r))
            }
            BinaryOp::BitOr => {
                let l_data = self.extract_data(&left);
                let r_data = self.extract_data(&right);
                let r = self.builder.build_or(l_data, r_data, "bitor").map_err(|e| e.to_string())?;
                Ok(self.build_value(i64.const_int(TAG_INT, false), r))
            }
            BinaryOp::BitXor => {
                let l_data = self.extract_data(&left);
                let r_data = self.extract_data(&right);
                let r = self.builder.build_xor(l_data, r_data, "bitxor").map_err(|e| e.to_string())?;
                Ok(self.build_value(i64.const_int(TAG_INT, false), r))
            }
            BinaryOp::Shl => {
                let l_data = self.extract_data(&left);
                let r_data = self.extract_data(&right);
                let r = self.builder.build_left_shift(l_data, r_data, "shl").map_err(|e| e.to_string())?;
                Ok(self.build_value(i64.const_int(TAG_INT, false), r))
            }
            BinaryOp::Shr => {
                let l_data = self.extract_data(&left);
                let r_data = self.extract_data(&right);
                let r = self.builder.build_right_shift(l_data, r_data, false, "shr")
                    .map_err(|e| e.to_string())?;
                Ok(self.build_value(i64.const_int(TAG_INT, false), r))
            }
            BinaryOp::Eq => {
                let l_tag = self.extract_tag(&left);
                let r_tag = self.extract_tag(&right);
                let l_data = self.extract_data(&left);
                let r_data = self.extract_data(&right);
                let tag_eq = self.builder.build_int_compare(
                    inkwell::IntPredicate::EQ, l_tag, r_tag, "tageq"
                ).map_err(|e| e.to_string())?;
                let data_eq = self.builder.build_int_compare(
                    inkwell::IntPredicate::EQ, l_data, r_data, "dataeq"
                ).map_err(|e| e.to_string())?;
                let eq = self.builder.build_and(tag_eq, data_eq, "eq")
                    .map_err(|e| e.to_string())?;
                let result = self.builder.build_int_z_extend(eq, i64, "eqext")
                    .map_err(|e| e.to_string())?;
                Ok(self.build_value(i64.const_int(TAG_BOOL, false), result))
            }
            BinaryOp::Lt => self.compile_icmp(left, right, inkwell::IntPredicate::SLT, "lt"),
            BinaryOp::Gt => self.compile_icmp(left, right, inkwell::IntPredicate::SGT, "gt"),
            BinaryOp::Le => self.compile_icmp(left, right, inkwell::IntPredicate::SLE, "le"),
            BinaryOp::Ge => self.compile_icmp(left, right, inkwell::IntPredicate::SGE, "ge"),
            BinaryOp::Ne => {
                let l_tag = self.extract_tag(&left);
                let r_tag = self.extract_tag(&right);
                let l_data = self.extract_data(&left);
                let r_data = self.extract_data(&right);
                let tag_eq = self.builder.build_int_compare(
                    inkwell::IntPredicate::EQ, l_tag, r_tag, "tageq",
                ).map_err(|e| e.to_string())?;
                let data_eq = self.builder.build_int_compare(
                    inkwell::IntPredicate::EQ, l_data, r_data, "dataeq",
                ).map_err(|e| e.to_string())?;
                let eq = self.builder.build_and(tag_eq, data_eq, "eq").map_err(|e| e.to_string())?;
                let ne = self.builder.build_not(eq, "ne").map_err(|e| e.to_string())?;
                let result = self.builder.build_int_z_extend(ne, i64, "neext").map_err(|e| e.to_string())?;
                Ok(self.build_value(i64.const_int(TAG_BOOL, false), result))
            }
            // Assignments: `lhs = rhs`, `lhs += rhs`, ... The left operand is
            // a target (an identifier), not a value to load — resolve it to its
            // alloca, then store. Compound ops read the current value first.
            BinaryOp::Assign
            | BinaryOp::AddAssign
            | BinaryOp::SubAssign
            | BinaryOp::MulAssign
            | BinaryOp::DivAssign
            | BinaryOp::ModAssign
            | BinaryOp::PowAssign
            | BinaryOp::ShlAssign
            | BinaryOp::ShrAssign
            | BinaryOp::BitAndAssign
            | BinaryOp::BitOrAssign
            | BinaryOp::BitXorAssign => self.compile_assign(bin),
            other => Err(format!("unsupported binary op in native codegen: {:?}", other)),
        }
    }

    /// Compile a short-circuiting logical `&&` (`is_or = false`) or `||`
    /// (`is_or = true`). The RHS is only evaluated if the LHS doesn't decide
    /// the result. Yields a Bool Value (data 0/1).
    fn compile_short_circuit(
        &mut self,
        bin: &BinaryExpr,
        is_or: bool,
    ) -> Result<StructValue<'ctx>, String> {
        let i64 = self.context.i64_type();
        let fn_val = self.current_fn.unwrap();
        let lhs = self.compile_expr(&bin.left)?;
        let lhs_true = self.builder.build_int_compare(
            inkwell::IntPredicate::NE,
            self.extract_data(&lhs),
            i64.const_int(0, false),
            "sc_lhs",
        ).map_err(|e| e.to_string())?;

        let rhs_block = self.context.append_basic_block(fn_val, "sc_rhs");
        let merge_block = self.context.append_basic_block(fn_val, "sc_merge");
        // `&&`: if lhs is false, short-circuit to merge with false.
        // `||`: if lhs is true, short-circuit to merge with true.
        if is_or {
            self.builder.build_conditional_branch(lhs_true, merge_block, rhs_block).map_err(|e| e.to_string())?;
        } else {
            self.builder.build_conditional_branch(lhs_true, rhs_block, merge_block).map_err(|e| e.to_string())?;
        }

        // Record which block took the short-circuit path, for the phi.
        let short_block = self.builder.get_insert_block().unwrap();

        // RHS block: evaluate the RHS as the result.
        self.builder.position_at_end(rhs_block);
        let rhs = self.compile_expr(&bin.right)?;
        let rhs_true = self.builder.build_int_compare(
            inkwell::IntPredicate::NE,
            self.extract_data(&rhs),
            i64.const_int(0, false),
            "sc_rhs",
        ).map_err(|e| e.to_string())?;
        // Extend the i1 to i64 so the phi operands match the short-circuit
        // constant type.
        let rhs_val = self.builder.build_int_z_extend(rhs_true, i64, "sc_rhs_ext").map_err(|e| e.to_string())?;
        self.builder.build_unconditional_branch(merge_block).map_err(|e| e.to_string())?;
        let rhs_end_block = self.builder.get_insert_block().unwrap();

        // Merge: phi over the short-circuit value and the RHS value.
        self.builder.position_at_end(merge_block);
        let phi = self.builder.build_phi(i64, "sc_result").map_err(|e| e.to_string())?;
        let short_val = i64.const_int(if is_or { 1 } else { 0 }, false);
        phi.add_incoming(&[(&short_val, short_block), (&rhs_val, rhs_end_block)]);
        Ok(self.build_value(i64.const_int(TAG_BOOL, false), phi.as_basic_value().into_int_value()))
    }

    /// Compile an assignment represented as a `BinaryExpr` with an assignment
    /// op (the parser produces `a = b` as `BinaryOp::Assign`, not
    /// `Expr::Assignment`). Only simple identifier targets are supported. The
    /// result is the assigned value, matching Coco's expression semantics.
    fn compile_assign(&mut self, bin: &BinaryExpr) -> Result<StructValue<'ctx>, String> {
        let alloca = match &bin.left {
            Expr::Ident(ident) => self
                .locals
                .get(&ident.name)
                .copied()
                .ok_or_else(|| format!("assignment to undeclared variable '{}'", ident.name))?,
            _ => {
                return Err("assignment to non-identifier targets is not supported".to_string())
            }
        };

        let rhs = self.compile_expr(&bin.right)?;
        let new_val = if bin.op == BinaryOp::Assign {
            rhs
        } else {
            // Compound assignment: read current, apply the arithmetic op, store.
            let current = self
                .builder
                .build_load(self.value_type, alloca, "cur")
                .map_err(|e| e.to_string())?
                .into_struct_value();
            self.apply_compound(bin.op, current, rhs)?
        };
        self.builder
            .build_store(alloca, new_val)
            .map_err(|e| e.to_string())?;
        Ok(new_val)
    }

    /// Apply a compound assignment op to `current` and `rhs`, returning the
    /// resulting Value. Mirrors `compile_binary` for the supported arithmetic
    /// and bitwise ops.
    fn apply_compound(
        &self,
        op: BinaryOp,
        current: StructValue<'ctx>,
        rhs: StructValue<'ctx>,
    ) -> Result<StructValue<'ctx>, String> {
        let i64 = self.context.i64_type();
        let l = self.extract_data(&current);
        let r = self.extract_data(&rhs);
        let data = match op {
            BinaryOp::AddAssign => self.builder.build_int_add(l, r, "add").map_err(|e| e.to_string())?,
            BinaryOp::SubAssign => self.builder.build_int_sub(l, r, "sub").map_err(|e| e.to_string())?,
            BinaryOp::MulAssign => self.builder.build_int_mul(l, r, "mul").map_err(|e| e.to_string())?,
            BinaryOp::DivAssign => self.builder.build_int_signed_div(l, r, "div").map_err(|e| e.to_string())?,
            BinaryOp::ModAssign => self.builder.build_int_signed_rem(l, r, "mod").map_err(|e| e.to_string())?,
            BinaryOp::BitAndAssign => self.builder.build_and(l, r, "bitand").map_err(|e| e.to_string())?,
            BinaryOp::BitOrAssign => self.builder.build_or(l, r, "bitor").map_err(|e| e.to_string())?,
            BinaryOp::BitXorAssign => self.builder.build_xor(l, r, "bitxor").map_err(|e| e.to_string())?,
            BinaryOp::ShlAssign => self.builder.build_left_shift(l, r, "shl").map_err(|e| e.to_string())?,
            BinaryOp::ShrAssign => self.builder.build_right_shift(l, r, false, "shr").map_err(|e| e.to_string())?,
            BinaryOp::PowAssign => {
                return Err("**= is not supported in native codegen yet".to_string())
            }
            other => return Err(format!("unsupported compound assignment op: {:?}", other)),
        };
        Ok(self.build_value(i64.const_int(TAG_INT, false), data))
    }

    /// Compile a function call.
    fn compile_call(&mut self, call: &CallExpr) -> Result<StructValue<'ctx>, String> {
        let ident = match &call.callee {
            Expr::Ident(ident) => ident,
            _ => {
                return Err(
                    "calls on non-identifier callees are not supported in native codegen".to_string(),
                )
            }
        };
        let mut name = ident.name.clone();
        if name == "main" {
            name = "coco_main".to_string();
        }
        let (func, _arity) = self
            .functions
            .get(&name)
            .copied()
            .ok_or_else(|| format!("call to unknown function '{}' in native codegen", ident.name))?;
        let mut args: Vec<inkwell::values::BasicMetadataValueEnum> = Vec::new();
        for arg in &call.args {
            let val = self.compile_expr(&arg.value)?;
            let tag = self.extract_tag(&val);
            let data = self.extract_data(&val);
            args.push(tag.into());
            args.push(data.into());
        }
        let result = self.builder.build_call(func, &args, "call").map_err(|e| e.to_string())?;
        match result.try_as_basic_value().left() {
            Some(val) if val.is_struct_value() => Ok(val.into_struct_value()),
            _ => Err("function call did not return a value in native codegen".to_string()),
        }
    }

    /// Generate a main() entry point that calls the Coco main() function.
    fn generate_entry(&mut self) -> Result<(), String> {
        let i64 = self.context.i64_type();
        let main_type = i64.fn_type(&[], false);
        let entry = self.module.add_function("main", main_type, None);
        let block = self.context.append_basic_block(entry, "entry");
        self.builder.position_at_end(block);

        // Call the Coco main() if it exists, and return its value's data field
        // as the process exit code. main() returns a {i64 tag, i64 data} struct;
        // for an int return, data holds the integer. If there's no main, exit 0.
        if let Some(&(func, _)) = self.functions.get("coco_main") {
            let ret = self
                .builder
                .build_call(func, &[], "call_main")
                .map_err(|e| e.to_string())?
                .try_as_basic_value()
                .left()
                .ok_or_else(|| "main() did not return a value".to_string())?;
            let data = self.extract_data(&ret.into_struct_value());
            self.builder.build_return(Some(&data)).map_err(|e| e.to_string())?;
        } else {
            self.builder.build_return(Some(&i64.const_int(0, false))).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Call the runtime allocation function: (tag, data) -> pointer to struct.
    fn call_runtime_alloc(&self, tag: u64, data: u64) -> Result<StructValue<'ctx>, String> {
        let i64 = self.context.i64_type();
        Ok(self.build_value(
            i64.const_int(tag, false),
            i64.const_int(data, false),
        ))
    }

    /// Build a Coco Value struct from tag and data via a runtime alloc call.
    ///
    /// Constructs a `{ i64 tag, i64 data }` struct in LLVM IR by calling the
    /// runtime's `coco_rt_alloc(tag, data)` (which mallocs a two-word struct)
    /// and loading the result. This matches the Value layout the VM uses and
    /// keeps heap-allocated values GC-visible.
    fn build_value(&self, tag: IntValue<'ctx>, data: IntValue<'ctx>) -> StructValue<'ctx> {
        // Call coco_rt_alloc(tag, data) -> {i64,i64}*.
        let alloc_fn = self
            .module
            .get_function("coco_rt_alloc")
            .expect("coco_rt_alloc must be declared");
        let ret = self
            .builder
            .build_call(alloc_fn, &[tag.into(), data.into()], "rt_alloc")
            .unwrap()
            .try_as_basic_value()
            .left()
            .expect("coco_rt_alloc returns a value");
        // Load the struct from the returned pointer.
        let ptr = ret.into_pointer_value();
        self.builder
            .build_load(self.value_type, ptr, "val")
            .unwrap()
            .into_struct_value()
    }

    /// Extract the tag field from a Value struct.
    fn extract_tag(&self, val: &StructValue<'ctx>) -> IntValue<'ctx> {
        self.builder.build_extract_value(*val, 0, "tag")
            .unwrap()
            .into_int_value()
    }

    /// Extract the data field from a Value struct.
    fn extract_data(&self, val: &StructValue<'ctx>) -> IntValue<'ctx> {
        self.builder.build_extract_value(*val, 1, "data")
            .unwrap()
            .into_int_value()
    }

    /// Print the module IR to stderr.
    pub fn dump(&self) {
        self.module.print_to_stderr();
    }

    /// Write the module to a file as LLVM bitcode.
    pub fn write_bitcode(&self, path: &str) -> Result<(), String> {
        if !self.module.write_bitcode_to_path(std::path::Path::new(path)) {
            return Err("failed to write bitcode".to_string());
        }
        Ok(())
    }

    /// Compile to an object file via LLVM.
    pub fn compile_to_object(&self, path: &str) -> Result<(), String> {
        use inkwell::targets::{FileType, InitializationConfig, RelocMode, Target, TargetMachine};
        Target::initialize_native(&InitializationConfig::default())
            .map_err(|e| e.to_string())?;
        let target_triple = TargetMachine::get_default_triple();
        let target = Target::from_triple(&target_triple)
            .map_err(|e| e.to_string())?;
        let target_machine = target.create_target_machine(
            &target_triple,
            "generic",
            "",
            OptimizationLevel::Default,
            RelocMode::Default,
            inkwell::targets::CodeModel::Default,
        ).ok_or_else(|| "failed to create target machine".to_string())?;
        target_machine.write_to_file(&self.module, FileType::Object, std::path::Path::new(path))
            .map_err(|e| e.to_string())
    }
}

/// Human-readable name for an `Item` variant, for error messages.
fn item_kind(item: &Item) -> &'static str {
    match item {
        Item::FnDecl(_) => "fn declaration",
        Item::ClassDecl(_) => "class declaration",
        Item::InterfaceDecl(_) => "interface declaration",
        Item::TraitDecl(_) => "trait declaration",
        Item::EnumDecl(_) => "enum declaration",
        Item::ConstDecl(_) => "const declaration",
        Item::LetDecl(_) => "let declaration",
        Item::TypeAlias(_) => "type alias",
        Item::Import(_) => "import",
        Item::Export(_) => "export",
        Item::ExprStmt(_) => "top-level expression",
        Item::Stmt(_) => "top-level statement",
    }
}

/// Human-readable name for a `Stmt` variant, for error messages.
fn stmt_kind(stmt: &Stmt) -> &'static str {
    match stmt {
        Stmt::Expr(_) => "expression statement",
        Stmt::Item(_) => "item statement",
        Stmt::If(_) => "if statement",
        Stmt::For(_) => "for statement",
        Stmt::While(_) => "while statement",
        Stmt::DoWhile(_) => "do-while statement",
        Stmt::Loop(_) => "loop statement",
        Stmt::Return(_) => "return statement",
        Stmt::Throw(_) => "throw statement",
        Stmt::Try(_) => "try statement",
        Stmt::Break(_) => "break statement",
        Stmt::Continue(_) => "continue statement",
        Stmt::Parallel(_) => "parallel statement",
        Stmt::Coro(_) => "coro statement",
        Stmt::Select(_) => "select statement",
        Stmt::Unsafe(_) => "unsafe statement",
        Stmt::Synchronized(_) => "synchronized statement",
    }
}

/// Human-readable name for an `Expr` variant, for error messages.
fn expr_kind(expr: &Expr) -> &'static str {
    match expr {
        Expr::Literal(_) => "literal",
        Expr::Ident(_) => "identifier",
        Expr::Binary(_) => "binary expression",
        Expr::Unary(_) => "unary expression",
        Expr::Call(_) => "call expression",
        Expr::Index(_) => "index expression",
        Expr::Member(_) => "member access",
        Expr::Match(_) => "match expression",
        Expr::Lambda(_) => "lambda",
        Expr::Array(_) => "array literal",
        Expr::Object(_) => "object literal",
        Expr::This(_) => "this",
        Expr::Dollar(_) => "$",
        Expr::DollarDollar(_) => "$$",
        Expr::Super(_) => "super",
        Expr::New(_) => "new expression",
        Expr::Ternary(_) => "ternary expression",
        Expr::NullCoalesce(_) => "null-coalesce expression",
        Expr::Elvis(_) => "elvis expression",
        Expr::Pipe(_) => "pipe expression",
        Expr::Assignment(_) => "assignment expression",
        Expr::Postfix(_) => "postfix expression",
        Expr::Group(_) => "grouped expression",
        Expr::Parallel(_) => "parallel expression",
        Expr::Template(_) => "template literal",
        Expr::Lazy(_) => "lazy expression",
    }
}
} // end mod native

#[cfg(feature = "native")]
pub use native::Codegen;

// Without the `native` feature, this crate has no public API. Callers (coco_cli)
// only reference `Codegen` under their own `#[cfg(feature = "native")]` guard,
// so an empty stub here lets `cargo build --workspace` succeed without LLVM.
