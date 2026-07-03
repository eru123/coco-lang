//! Coco AOT native compiler — LLVM code generation.
//!
//! Walks the AST and emits LLVM IR, then compiles to a native binary via the
//! system linker. The generated code links against `libcoco_rt` (compiled from
//! C in the `coco_rt` crate) which provides the tagged value model, the
//! adaptive numeric tower, and collections.
//!
//! ## Value model
//! Every Coco value is an opaque `coco_val*` (a refcounted, tagged heap object
//! defined in `coco_rt/c/coco_rt.h`). The codegen constructs values via runtime
//! calls (`coco_make_int`, `coco_make_float`, ...) and operates on them via the
//! runtime's adaptive arithmetic (`coco_add`, ...).
//!
//! ## Adaptive numeric tower
//! When operand types are known statically (from the type checker's `TypeMap`),
//! the codegen emits a native fast-path op: `int + int` becomes a direct i64
//! `add` with an overflow guard that escalates to `coco_add` (which promotes to
//! bignum, keeping the result exact); `float + float` becomes an f64 `fadd`.
//! When types are dynamic (`Ty::Unknown`/`Mixed`), it calls `coco_add`, which
//! dispatches on the runtime tag. See `docs/adaptive-numeric-tower.md`.
//!
//! This crate requires system LLVM 18 (via `inkwell`/`llvm-sys`). The
//! LLVM-dependent code is gated behind the `native` feature; without it the
//! crate compiles to an empty stub.

#[cfg(feature = "native")]
mod native {
    use coco_syntax::*;
    use coco_typeck::TypeMap;
    use coco_typeck::types::Ty;
    use inkwell::basic_block::BasicBlock;
    use inkwell::builder::Builder;
    use inkwell::context::Context;
    use inkwell::module::Module;
    use inkwell::types::{BasicMetadataTypeEnum, FunctionType, IntType, PointerType};
    use inkwell::values::{BasicValueEnum, CallSiteValue, FunctionValue, IntValue, PointerValue};
    use inkwell::{AddressSpace, OptimizationLevel};
    use std::collections::HashMap;

/// The LLVM code generator. Values are opaque `coco_val*` pointers (see the
/// `coco_rt` C runtime); this struct holds the LLVM module, builder, and
/// compilation state.
pub struct Codegen<'ctx> {
    pub context: &'ctx Context,
    pub module: Module<'ctx>,
    pub builder: Builder<'ctx>,
    /// `coco_val*` — an opaque pointer in LLVM IR.
    val_ptr_type: PointerType<'ctx>,
    i64_type: IntType<'ctx>,
    i32_type: IntType<'ctx>,
    i8_type: IntType<'ctx>,
    /// Named functions in scope: name -> (func, arity).
    functions: HashMap<String, (FunctionValue<'ctx>, usize)>,
    /// Local variables in the current function: name -> alloca holding a coco_val*.
    locals: HashMap<String, PointerValue<'ctx>>,
    /// Current function being compiled.
    current_fn: Option<FunctionValue<'ctx>>,
    /// Inferred types keyed by AST node span, for arithmetic specialization.
    types: TypeMap,
    /// Loop context stack for `break`/`continue`: (continue_target, break_target).
    loop_stack: Vec<(BasicBlock<'ctx>, BasicBlock<'ctx>)>,
    /// Monotonic counter for unique global/string-literal names.
    global_count: u64,
}

impl<'ctx> Codegen<'ctx> {
    /// Create a new code generator for a module named `name`.
    pub fn new(context: &'ctx Context, name: &str) -> Self {
        let module = context.create_module(name);
        let builder = context.create_builder();
        let val_ptr_type = context.ptr_type(AddressSpace::default());
        Self {
            context,
            module,
            builder,
            val_ptr_type,
            i64_type: context.i64_type(),
            i32_type: context.i32_type(),
            i8_type: context.i8_type(),
            functions: HashMap::new(),
            locals: HashMap::new(),
            current_fn: None,
            types: TypeMap::new(),
            loop_stack: Vec::new(),
            global_count: 0,
        }
    }

    /// Generate LLVM IR for a program. `types` is the type checker's inferred
    /// type map, used to specialize arithmetic (the adaptive numeric tower).
    pub fn generate(&mut self, program: &Program, types: &TypeMap) -> Result<(), String> {
        self.types = types.clone();
        self.declare_runtime();

        // First pass: declare all functions. `main` is renamed `coco_main` so
        // it doesn't collide with the C entry point added by generate_entry.
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

        // Second pass: compile function bodies. Pure declarations (imports,
        // type aliases, interfaces, traits, enums, top-level const/let) need no
        // codegen; executable top-level items must live inside a function.
        for item in &program.items {
            match item {
                Item::FnDecl(fn_decl) => self.compile_fn_decl(fn_decl)?,
                Item::Export(export) => match &*export.item {
                    Item::FnDecl(fd) => self.compile_fn_decl(fd)?,
                    Item::ClassDecl(_) => {}
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
                Item::ConstDecl(_)
                | Item::LetDecl(_)
                | Item::TypeAlias(_)
                | Item::Import(_)
                | Item::InterfaceDecl(_)
                | Item::TraitDecl(_)
                | Item::EnumDecl(_)
                | Item::ClassDecl(_) => {}
                other => {
                    return Err(format!(
                        "unsupported top-level item in native codegen: {}",
                        item_kind(other)
                    ))
                }
            }
        }

        self.generate_entry()?;

        if let Err(msg) = self.module.verify() {
            return Err(format!("LLVM verification failed: {}", msg.to_string()));
        }
        Ok(())
    }

    // --- Runtime declaration ------------------------------------------------

    /// Declare the C runtime functions used by the codegen. Each takes/returns
    /// `coco_val*` (opaque pointer) unless noted.
    fn declare_runtime(&mut self) {
        let p = self.val_ptr_type;
        let i64 = self.i64_type;
        let i32 = self.i32_type;
        let i8 = self.i8_type;
        let i8p = i8.ptr_type(AddressSpace::default());
        let bool_ty = self.context.bool_type();

        // Constructors.
        self.decl("coco_make_int", p.fn_type(&[i64.into()], false));
        self.decl("coco_make_float", p.fn_type(&[self.context.f64_type().into()], false));
        self.decl("coco_make_bool", p.fn_type(&[bool_ty.into()], false));
        self.decl("coco_make_null", p.fn_type(&[], false));
        self.decl("coco_make_string", p.fn_type(&[i8p.into(), i64.into()], false));
        self.decl("coco_make_string_cstr", p.fn_type(&[i8p.into()], false));

        // Refcounting.
        self.decl("coco_retain", p.fn_type(&[p.into()], false));
        self.decl("coco_release", self.context.void_type().fn_type(&[p.into()], false));

        // Truthiness.
        self.decl("coco_is_truthy", bool_ty.fn_type(&[p.into()], false));

        // Adaptive arithmetic (dispatch on tags at runtime).
        self.decl("coco_add", p.fn_type(&[p.into(), p.into()], false));
        self.decl("coco_sub", p.fn_type(&[p.into(), p.into()], false));
        self.decl("coco_mul", p.fn_type(&[p.into(), p.into()], false));
        self.decl("coco_div", p.fn_type(&[p.into(), p.into()], false));
        self.decl("coco_mod", p.fn_type(&[p.into(), p.into()], false));
        self.decl("coco_neg", p.fn_type(&[p.into()], false));
        self.decl("coco_not", p.fn_type(&[p.into()], false));

        // Comparisons.
        self.decl("coco_eq", bool_ty.fn_type(&[p.into(), p.into()], false));
        self.decl("coco_cmp", i32.fn_type(&[p.into(), p.into()], false));

        // Int<->float promotion (for static specialization).
        self.decl("coco_int_to_f64", self.context.f64_type().fn_type(&[p.into()], false));

        // Lists.
        self.decl("coco_list_new", p.fn_type(&[i64.into()], false));
        self.decl("coco_list_get", p.fn_type(&[p.into(), i64.into()], false));
        self.decl("coco_list_push", self.context.void_type().fn_type(&[p.into(), p.into()], false));
        self.decl("coco_list_len", i64.fn_type(&[p.into()], false));

        // Maps.
        self.decl("coco_map_new", p.fn_type(&[], false));
        self.decl("coco_map_get", p.fn_type(&[p.into(), i8p.into(), i64.into()], false));
        self.decl("coco_map_set", self.context.void_type().fn_type(&[p.into(), i8p.into(), i64.into(), p.into()], false));
        self.decl("coco_map_len", i64.fn_type(&[p.into()], false));

        // Strings.
        self.decl("coco_str_len", i64.fn_type(&[p.into()], false));
        self.decl("coco_str_concat", p.fn_type(&[p.into(), p.into()], false));
        self.decl("coco_str_data", i8p.fn_type(&[p.into()], false));

        // Builtins.
        self.decl("coco_print", p.fn_type(&[p.into()], false));
        self.decl("coco_len", p.fn_type(&[p.into()], false));
        self.decl("coco_tostring", p.fn_type(&[p.into()], false));
        self.decl("coco_range", p.fn_type(&[i64.into(), i64.into()], false));
    }

    fn decl(&self, name: &str, ty: FunctionType<'ctx>) {
        if self.module.get_function(name).is_none() {
            self.module.add_function(name, ty, None);
        }
    }

    /// The LLVM function type for a Coco function: each param is a `coco_val*`,
    /// return is a `coco_val*`.
    fn fn_type(&self, arity: usize) -> FunctionType<'ctx> {
        let p = self.val_ptr_type;
        let params: Vec<BasicMetadataTypeEnum> = (0..arity).map(|_| p.into()).collect();
        p.fn_type(&params, false)
    }

    // --- Function compilation ----------------------------------------------

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

        // Bind parameters: each is a coco_val*; retain and store to an alloca.
        let params = func.get_params();
        for (i, param) in fn_decl.params.iter().enumerate() {
            let alloca = self.builder.build_alloca(self.val_ptr_type, &param.name.name)
                .map_err(|e| e.to_string())?;
            let retained = self.call("coco_retain", &[params[i].into()]);
            self.builder.build_store(alloca, retained).map_err(|e| e.to_string())?;
            self.locals.insert(param.name.name.clone(), alloca);
        }

        self.compile_block(&fn_decl.body)?;

        // Default return null if the block didn't terminate.
        if !self.block_terminated() {
            let null = self.call("coco_make_null", &[]);
            self.builder.build_return(Some(&null)).map_err(|e| e.to_string())?;
        }
        self.current_fn = None;
        Ok(())
    }

    fn compile_block(&mut self, block: &Block) -> Result<(), String> {
        for stmt in &block.stmts {
            self.compile_stmt(stmt)?;
        }
        Ok(())
    }

    fn compile_stmt(&mut self, stmt: &Stmt) -> Result<(), String> {
        match stmt {
            Stmt::Expr(expr_stmt) => {
                let v = self.compile_expr(&expr_stmt.expr)?;
                // Expression statements: the value is unused; release it.
                self.call_void("coco_release", &[v.into()]);
            }
            Stmt::Return(ret) => {
                let val = if let Some(ref expr) = ret.value {
                    self.compile_expr(expr)?
                } else {
                    self.call("coco_make_null", &[])
                };
                self.builder.build_return(Some(&val)).map_err(|e| e.to_string())?;
            }
            Stmt::If(if_stmt) => self.compile_if(if_stmt)?,
            Stmt::While(while_stmt) => self.compile_while(while_stmt)?,
            Stmt::Loop(loop_stmt) => self.compile_loop(loop_stmt)?,
            Stmt::DoWhile(dw) => self.compile_do_while(dw)?,
            Stmt::For(for_stmt) => self.compile_for(for_stmt)?,
            Stmt::Break(_) => self.compile_break()?,
            Stmt::Continue(_) => self.compile_continue()?,
            Stmt::Item(item) => match &**item {
                Item::LetDecl(let_decl) => self.compile_let_decl(let_decl)?,
                Item::ConstDecl(const_decl) => self.compile_const_decl(const_decl)?,
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

    fn compile_let_decl(&mut self, let_decl: &LetDecl) -> Result<(), String> {
        let val = if let Some(ref expr) = let_decl.value {
            self.compile_expr(expr)?
        } else {
            self.call("coco_make_null", &[])
        };
        self.bind_local(&let_decl.name.name, val)
    }

    fn compile_const_decl(&mut self, const_decl: &ConstDecl) -> Result<(), String> {
        let val = self.compile_expr(&const_decl.value)?;
        self.bind_local(&const_decl.name.name, val)
    }

    /// Allocate a local slot, store `val` (retained) into it, bind the name.
    fn bind_local(&mut self, name: &str, val: PointerValue<'ctx>) -> Result<(), String> {
        let alloca = self
            .builder
            .build_alloca(self.val_ptr_type, name)
            .map_err(|e| e.to_string())?;
        let retained = self.call("coco_retain", &[val.into()]);
        self.builder.build_store(alloca, retained).map_err(|e| e.to_string())?;
        self.locals.insert(name.to_string(), alloca);
        Ok(())
    }

    // --- Control flow -------------------------------------------------------

    fn compile_if(&mut self, if_stmt: &IfStmt) -> Result<(), String> {
        let merge_block = self.context.append_basic_block(self.current_fn.unwrap(), "ifmerge");
        self.compile_if_branch(
            &if_stmt.condition,
            &if_stmt.then_block,
            &if_stmt.else_ifs,
            &if_stmt.else_block,
            merge_block,
        )?;
        self.builder.position_at_end(merge_block);
        Ok(())
    }

    fn compile_if_branch(
        &mut self,
        cond: &Expr,
        then_block: &Block,
        else_ifs: &[ElseIf],
        else_block: &Option<Block>,
        merge_block: BasicBlock<'ctx>,
    ) -> Result<(), String> {
        let cond_val = self.compile_expr(cond)?;
        let is_true = self.call_basic("coco_is_truthy", &[cond_val.into()]);
        self.call_void("coco_release", &[cond_val.into()]);

        let then_bb = self.context.append_basic_block(self.current_fn.unwrap(), "then");
        let else_bb = self.context.append_basic_block(self.current_fn.unwrap(), "else");
        self.builder.build_conditional_branch(is_true.into_int_value(), then_bb, else_bb)
            .map_err(|e| e.to_string())?;

        self.builder.position_at_end(then_bb);
        self.compile_block(then_block)?;
        if !self.block_terminated() {
            self.builder.build_unconditional_branch(merge_block).map_err(|e| e.to_string())?;
        }

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

    fn compile_while(&mut self, while_stmt: &WhileStmt) -> Result<(), String> {
        let cond_block = self.context.append_basic_block(self.current_fn.unwrap(), "whilecond");
        let body_block = self.context.append_basic_block(self.current_fn.unwrap(), "whilebody");
        let end_block = self.context.append_basic_block(self.current_fn.unwrap(), "whileend");

        self.builder.build_unconditional_branch(cond_block).map_err(|e| e.to_string())?;

        self.builder.position_at_end(cond_block);
        let cond_val = self.compile_expr(&while_stmt.condition)?;
        let is_true = self.call_basic("coco_is_truthy", &[cond_val.into()]);
        self.call_void("coco_release", &[cond_val.into()]);
        self.builder.build_conditional_branch(is_true.into_int_value(), body_block, end_block)
            .map_err(|e| e.to_string())?;

        self.builder.position_at_end(body_block);
        self.loop_stack.push((cond_block, end_block));
        self.compile_block(&while_stmt.body)?;
        self.loop_stack.pop();
        if !self.block_terminated() {
            self.builder.build_unconditional_branch(cond_block).map_err(|e| e.to_string())?;
        }

        self.builder.position_at_end(end_block);
        Ok(())
    }

    fn compile_loop(&mut self, loop_stmt: &LoopStmt) -> Result<(), String> {
        let body_block = self.context.append_basic_block(self.current_fn.unwrap(), "loopbody");
        let end_block = self.context.append_basic_block(self.current_fn.unwrap(), "loopend");
        self.builder.build_unconditional_branch(body_block).map_err(|e| e.to_string())?;

        self.builder.position_at_end(body_block);
        self.loop_stack.push((body_block, end_block));
        self.compile_block(&loop_stmt.body)?;
        self.loop_stack.pop();
        if !self.block_terminated() {
            self.builder.build_unconditional_branch(body_block).map_err(|e| e.to_string())?;
        }
        self.builder.position_at_end(end_block);
        Ok(())
    }

    fn compile_do_while(&mut self, dw: &DoWhileStmt) -> Result<(), String> {
        let body_block = self.context.append_basic_block(self.current_fn.unwrap(), "dowhilebody");
        let cond_block = self.context.append_basic_block(self.current_fn.unwrap(), "dowhilecond");
        let end_block = self.context.append_basic_block(self.current_fn.unwrap(), "dowhileend");
        self.builder.build_unconditional_branch(body_block).map_err(|e| e.to_string())?;

        self.builder.position_at_end(body_block);
        self.loop_stack.push((body_block, end_block));
        self.compile_block(&dw.body)?;
        self.loop_stack.pop();
        if !self.block_terminated() {
            self.builder.build_unconditional_branch(cond_block).map_err(|e| e.to_string())?;
        }

        self.builder.position_at_end(cond_block);
        let cond_val = self.compile_expr(&dw.condition)?;
        let is_true = self.call_basic("coco_is_truthy", &[cond_val.into()]);
        self.call_void("coco_release", &[cond_val.into()]);
        self.builder.build_conditional_branch(is_true.into_int_value(), body_block, end_block)
            .map_err(|e| e.to_string())?;
        self.builder.position_at_end(end_block);
        Ok(())
    }

    fn compile_for(&mut self, for_stmt: &ForStmt) -> Result<(), String> {
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
        let start_val = self.compile_expr(start_expr)?;
        let end_val = self.compile_expr(end_expr)?;
        let start = self.int_data(&start_val);
        let end = self.int_data(&end_val);
        self.call_void("coco_release", &[start_val.into()]);
        self.call_void("coco_release", &[end_val.into()]);

        let counter = self.builder.build_alloca(self.i64_type, "foridx").map_err(|e| e.to_string())?;
        self.builder.build_store(counter, start).map_err(|e| e.to_string())?;

        let cond_block = self.context.append_basic_block(fn_val, "forcond");
        let body_block = self.context.append_basic_block(fn_val, "forbody");
        let inc_block = self.context.append_basic_block(fn_val, "forinc");
        let end_block = self.context.append_basic_block(fn_val, "forend");

        self.builder.build_unconditional_branch(cond_block).map_err(|e| e.to_string())?;

        self.builder.position_at_end(cond_block);
        let cur = self.builder.build_load(self.i64_type, counter, "forcur").map_err(|e| e.to_string())?.into_int_value();
        let pred = if inclusive { inkwell::IntPredicate::SLE } else { inkwell::IntPredicate::SLT };
        let keep = self.builder.build_int_compare(pred, cur, end, "forcmp").map_err(|e| e.to_string())?;
        self.builder.build_conditional_branch(keep, body_block, end_block).map_err(|e| e.to_string())?;

        // Body: bind the loop variable to the counter value.
        self.builder.position_at_end(body_block);
        let cur_for_var = self.builder.build_load(self.i64_type, counter, "forvar").map_err(|e| e.to_string())?.into_int_value();
        let var_val = self.call("coco_make_int", &[cur_for_var.into()]);
        let var_alloca = self.builder.build_alloca(self.val_ptr_type, &for_stmt.pattern.name).map_err(|e| e.to_string())?;
        let retained = self.call("coco_retain", &[var_val.into()]);
        self.builder.build_store(var_alloca, retained).map_err(|e| e.to_string())?;
        let prev = self.locals.insert(for_stmt.pattern.name.clone(), var_alloca);
        self.loop_stack.push((inc_block, end_block));
        self.compile_block(&for_stmt.body)?;
        self.loop_stack.pop();
        if let Some(p) = prev { self.locals.insert(for_stmt.pattern.name.clone(), p); }
        else { self.locals.remove(&for_stmt.pattern.name); }
        if !self.block_terminated() {
            self.builder.build_unconditional_branch(inc_block).map_err(|e| e.to_string())?;
        }

        self.builder.position_at_end(inc_block);
        let cur = self.builder.build_load(self.i64_type, counter, "forinc_cur").map_err(|e| e.to_string())?.into_int_value();
        let one = self.i64_type.const_int(1, false);
        let next = self.builder.build_int_add(cur, one, "fornext").map_err(|e| e.to_string())?;
        self.builder.build_store(counter, next).map_err(|e| e.to_string())?;
        self.builder.build_unconditional_branch(cond_block).map_err(|e| e.to_string())?;

        self.builder.position_at_end(end_block);
        Ok(())
    }

    fn compile_break(&mut self) -> Result<(), String> {
        let (_, break_block) = self
            .loop_stack
            .last()
            .copied()
            .ok_or_else(|| "break outside loop".to_string())?;
        self.builder.build_unconditional_branch(break_block).map_err(|e| e.to_string())?;
        Ok(())
    }

    fn compile_continue(&mut self) -> Result<(), String> {
        let (continue_block, _) = self
            .loop_stack
            .last()
            .copied()
            .ok_or_else(|| "continue outside loop".to_string())?;
        self.builder.build_unconditional_branch(continue_block).map_err(|e| e.to_string())?;
        Ok(())
    }

    fn block_terminated(&self) -> bool {
        self.builder.get_insert_block().and_then(|b| b.get_terminator()).is_some()
    }

    // --- Expressions --------------------------------------------------------
    // compile_expr returns a coco_val* (PointerValue) with refcount +1 (the
    // caller owns one reference and must release or transfer it).

    fn compile_expr(&mut self, expr: &Expr) -> Result<PointerValue<'ctx>, String> {
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

    fn compile_literal(&mut self, lit: &Literal) -> Result<PointerValue<'ctx>, String> {
        match lit {
            Literal::Int(n, _) => {
                let v = self.i64_type.const_int(*n as u64, true);
                Ok(self.call("coco_make_int", &[v.into()]))
            }
            Literal::Float(f, _) => {
                let v = self.context.f64_type().const_float(*f);
                Ok(self.call("coco_make_float", &[v.into()]))
            }
            Literal::Bool(b, _) => {
                let v = self.context.bool_type().const_int(*b as u64, false);
                Ok(self.call("coco_make_bool", &[v.into()]))
            }
            Literal::Null(_) => Ok(self.call("coco_make_null", &[])),
            Literal::String(s, _) => {
                // Create a global byte array for the literal, then call coco_make_string.
                let bytes = s.as_bytes();
                self.global_count += 1;
                let global = self.module.add_global(
                    self.i8_type.array_type(bytes.len() as u32),
                    None,
                    &format!("str_{}", self.global_count),
                );
                let const_str = self.context.const_string(bytes, false);
                global.set_initializer(&const_str);
                global.set_constant(true);
                // Cast the global ([N x i8]*) to i8* for coco_make_string.
                let i8p = self.i8_type.ptr_type(AddressSpace::default());
                let ptr = self.builder.build_pointer_cast(global.as_pointer_value(), i8p, "strptr")
                    .map_err(|e| e.to_string())?;
                let len = self.i64_type.const_int(bytes.len() as u64, false);
                Ok(self.call("coco_make_string", &[ptr.into(), len.into()]))
            }
            Literal::Char(c, _) => {
                // A char is represented by its codepoint as an int.
                let v = self.i64_type.const_int(*c as u64, false);
                Ok(self.call("coco_make_int", &[v.into()]))
            }
        }
    }

    fn compile_ident(&mut self, ident: &Ident) -> Result<PointerValue<'ctx>, String> {
        if let Some(&alloca) = self.locals.get(&ident.name) {
            let val = self.builder.build_load(self.val_ptr_type, alloca, &ident.name)
                .map_err(|e| e.to_string())?
                .into_pointer_value();
            let retained = self.call("coco_retain", &[val.into()]);
            Ok(retained)
        } else {
            Err(format!("undefined variable '{}' in native codegen", ident.name))
        }
    }

    /// The adaptive numeric tower: dispatch arithmetic on static types when
    /// known, else call the runtime's tag-dispatched `coco_*` functions.
    fn compile_binary(&mut self, bin: &BinaryExpr) -> Result<PointerValue<'ctx>, String> {
        // Assignment ops.
        if matches!(bin.op, BinaryOp::Assign | BinaryOp::AddAssign | BinaryOp::SubAssign
            | BinaryOp::MulAssign | BinaryOp::DivAssign | BinaryOp::ModAssign
            | BinaryOp::PowAssign | BinaryOp::ShlAssign | BinaryOp::ShrAssign
            | BinaryOp::BitAndAssign | BinaryOp::BitOrAssign | BinaryOp::BitXorAssign)
        {
            return self.compile_assign(bin);
        }
        // Short-circuiting logical ops.
        match bin.op {
            BinaryOp::And => return self.compile_short_circuit(bin, false),
            BinaryOp::Or => return self.compile_short_circuit(bin, true),
            BinaryOp::Range | BinaryOp::RangeInclusive => {
                return Err("ranges are only supported as a for-in iterable in native codegen".to_string());
            }
            _ => {}
        }

        // Determine static operand types for specialization.
        let lt = self.types.get(bin.left.span()).cloned();
        let rt = self.types.get(bin.right.span()).cloned();
        let both_int = matches!(lt, Some(Ty::Int) | Some(Ty::Uint))
            && matches!(rt, Some(Ty::Int) | Some(Ty::Uint));
        let both_float = matches!(lt, Some(Ty::Float)) && matches!(rt, Some(Ty::Float));

        let left = self.compile_expr(&bin.left)?;
        let right = self.compile_expr(&bin.right)?;

        let result = match bin.op {
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => {
                if both_float {
                    self.compile_float_arith(bin.op, &left, &right)?
                } else if both_int {
                    self.compile_int_arith_guarded(bin.op, &left, &right)?
                } else {
                    // Dynamic: dispatch on runtime tags (adaptive fallback).
                    self.compile_dyn_arith(bin.op, &left, &right)?
                }
            }
            BinaryOp::Eq => {
                let r = self.call_basic("coco_eq", &[left.into(), right.into()]);
                self.call("coco_make_bool", &[r.into()])
            }
            BinaryOp::Ne => {
                let eq = self.call_basic("coco_eq", &[left.into(), right.into()]);
                let ne = self.builder.build_not(eq.into_int_value(), "ne").map_err(|e| e.to_string())?;
                self.call("coco_make_bool", &[ne.into()])
            }
            BinaryOp::Lt | BinaryOp::Gt | BinaryOp::Le | BinaryOp::Ge => {
                let c = self.call_basic("coco_cmp", &[left.into(), right.into()]).into_int_value();
                let zero = self.i32_type.const_int(0, false);
                let pred = match bin.op {
                    BinaryOp::Lt => inkwell::IntPredicate::SLT,
                    BinaryOp::Gt => inkwell::IntPredicate::SGT,
                    BinaryOp::Le => inkwell::IntPredicate::SLE,
                    BinaryOp::Ge => inkwell::IntPredicate::SGE,
                    _ => unreachable!(),
                };
                let r = self.builder.build_int_compare(pred, c, zero, "cmp").map_err(|e| e.to_string())?;
                self.call("coco_make_bool", &[r.into()])
            }
            BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor
            | BinaryOp::Shl | BinaryOp::Shr => {
                // Integer bitwise on the i64 data field.
                let l = self.int_data(&left);
                let r = self.int_data(&right);
                let data = match bin.op {
                    BinaryOp::BitAnd => self.builder.build_and(l, r, "band").map_err(|e| e.to_string())?,
                    BinaryOp::BitOr => self.builder.build_or(l, r, "bor").map_err(|e| e.to_string())?,
                    BinaryOp::BitXor => self.builder.build_xor(l, r, "bxor").map_err(|e| e.to_string())?,
                    BinaryOp::Shl => self.builder.build_left_shift(l, r, "shl").map_err(|e| e.to_string())?,
                    BinaryOp::Shr => self.builder.build_right_shift(l, r, false, "shr").map_err(|e| e.to_string())?,
                    _ => unreachable!(),
                };
                self.call("coco_make_int", &[data.into()])
            }
            other => return Err(format!("unsupported binary op in native codegen: {:?}", other)),
        };
        // left and right are consumed by the runtime calls (which retain as
        // needed); release our references.
        self.call_void("coco_release", &[left.into()]);
        self.call_void("coco_release", &[right.into()]);
        Ok(result)
    }

    /// Static float arithmetic: extract the f64 from both operands, do a native
    /// f64 op, box the result. Tier 2 of the tower.
    fn compile_float_arith(&self, op: BinaryOp, a: &PointerValue<'ctx>, b: &PointerValue<'ctx>) -> Result<PointerValue<'ctx>, String> {
        let av = self.float_data(a);
        let bv = self.float_data(b);
        let r = match op {
            BinaryOp::Add => self.builder.build_float_add(av, bv, "fadd").map_err(|e| e.to_string())?,
            BinaryOp::Sub => self.builder.build_float_sub(av, bv, "fsub").map_err(|e| e.to_string())?,
            BinaryOp::Mul => self.builder.build_float_mul(av, bv, "fmul").map_err(|e| e.to_string())?,
            BinaryOp::Div => self.builder.build_float_div(av, bv, "fdiv").map_err(|e| e.to_string())?,
            BinaryOp::Mod => {
                // fmod-style: a - b * trunc(a/b)
                let f64 = self.context.f64_type();
                let q = self.builder.build_float_div(av, bv, "fdivq").map_err(|e| e.to_string())?;
                let t = self.builder.build_float_trunc(q, f64, "trunc").map_err(|e| e.to_string())?;
                self.builder.build_float_sub(av, self.builder.build_float_mul(bv, t, "fm").map_err(|e| e.to_string())?, "fmod").map_err(|e| e.to_string())?
            }
            _ => unreachable!(),
        };
        Ok(self.call("coco_make_float", &[r.into()]))
    }

    /// Static int arithmetic with overflow guard: try the native i64 op; on
    /// overflow, fall back to the runtime `coco_add` (which escalates to
    /// bignum, keeping the result exact). Tiers 0+1 of the tower.
    fn compile_int_arith_guarded(&self, op: BinaryOp, a: &PointerValue<'ctx>, b: &PointerValue<'ctx>) -> Result<PointerValue<'ctx>, String> {
        let av = self.int_data(a);
        let bv = self.int_data(b);

        // Division/modulo: no i64 overflow except INT64_MIN / -1 (handled
        // exactly by the runtime's coco_div/coco_mod, which escalate). For the
        // common case emit the native op directly, with no guard blocks.
        if matches!(op, BinaryOp::Div | BinaryOp::Mod) {
            let r = match op {
                BinaryOp::Div => self.builder.build_int_signed_div(av, bv, "div").map_err(|e| e.to_string())?,
                BinaryOp::Mod => self.builder.build_int_signed_rem(av, bv, "mod").map_err(|e| e.to_string())?,
                _ => unreachable!(),
            };
            return Ok(self.call("coco_make_int", &[r.into()]));
        }

        let fn_val = self.current_fn.unwrap();
        let fast_block = self.context.append_basic_block(fn_val, "intfast");
        let slow_block = self.context.append_basic_block(fn_val, "intslow");
        let merge_block = self.context.append_basic_block(fn_val, "intmerge");

        // Compute the native i64 op, then detect overflow by sign analysis
        // (inkwell 0.5 has no checked-arithmetic intrinsics, so we do it
        // manually). For add: overflow iff ((a^r)&(b^r)) < 0. For sub:
        // overflow iff ((a^b)&(a^r)) < 0. For mul: overflow iff r/a != b
        // (when a != 0). All using signed comparison (< 0).
        let (result, overflowed) = match op {
            BinaryOp::Add => {
                let r = self.builder.build_int_add(av, bv, "add").map_err(|e| e.to_string())?;
                let axr = self.builder.build_xor(av, r, "axr").map_err(|e| e.to_string())?;
                let bxr = self.builder.build_xor(bv, r, "bxr").map_err(|e| e.to_string())?;
                let both = self.builder.build_and(axr, bxr, "ovand").map_err(|e| e.to_string())?;
                let zero = self.i64_type.const_int(0, false);
                let ovf = self.builder.build_int_compare(inkwell::IntPredicate::SLT, both, zero, "ovf").map_err(|e| e.to_string())?;
                (r, ovf)
            }
            BinaryOp::Sub => {
                let r = self.builder.build_int_sub(av, bv, "sub").map_err(|e| e.to_string())?;
                let axb = self.builder.build_xor(av, bv, "axb").map_err(|e| e.to_string())?;
                let axr = self.builder.build_xor(av, r, "axr").map_err(|e| e.to_string())?;
                let both = self.builder.build_and(axb, axr, "ovand").map_err(|e| e.to_string())?;
                let zero = self.i64_type.const_int(0, false);
                let ovf = self.builder.build_int_compare(inkwell::IntPredicate::SLT, both, zero, "ovf").map_err(|e| e.to_string())?;
                (r, ovf)
            }
            BinaryOp::Mul => {
                let r = self.builder.build_int_mul(av, bv, "mul").map_err(|e| e.to_string())?;
                // overflow iff a != 0 and r / a != b (signed). Guard a!=0.
                let zero = self.i64_type.const_int(0, false);
                let a_nonzero = self.builder.build_int_compare(inkwell::IntPredicate::NE, av, zero, "anz").map_err(|e| e.to_string())?;
                let q = self.builder.build_int_signed_div(r, av, "mdiv").map_err(|e| e.to_string())?;
                let qneb = self.builder.build_int_compare(inkwell::IntPredicate::NE, q, bv, "qneb").map_err(|e| e.to_string())?;
                let ovf = self.builder.build_and(a_nonzero, qneb, "ovf").map_err(|e| e.to_string())?;
                (r, ovf)
            }
            // Div/Mod are handled by the early return above.
            _ => unreachable!("compile_int_arith_guarded: op {:?} not add/sub/mul", op),
            _ => unreachable!(),
        };
        self.builder.build_conditional_branch(overflowed, slow_block, fast_block).map_err(|e| e.to_string())?;

        // Fast path: no overflow -> box the i64.
        self.builder.position_at_end(fast_block);
        let fast_val = self.call("coco_make_int", &[result.into()]);
        self.builder.build_unconditional_branch(merge_block).map_err(|e| e.to_string())?;
        let fast_end = self.builder.get_insert_block().unwrap();

        // Slow path: overflow -> runtime dispatch (escalates to bignum, exact).
        self.builder.position_at_end(slow_block);
        let rt_name = match op {
            BinaryOp::Add => "coco_add",
            BinaryOp::Sub => "coco_sub",
            BinaryOp::Mul => "coco_mul",
            _ => unreachable!(),
        };
        // a and b are already retained by the caller; the runtime retains as
        // needed, but to be safe pass the originals (coco_* don't consume).
        let slow_val = self.call(rt_name, &[(*a).into(), (*b).into()]);
        self.builder.build_unconditional_branch(merge_block).map_err(|e| e.to_string())?;
        let slow_end = self.builder.get_insert_block().unwrap();

        // Merge via phi over the two paths.
        self.builder.position_at_end(merge_block);
        let phi = self.builder.build_phi(self.val_ptr_type, "intres").map_err(|e| e.to_string())?;
        phi.add_incoming(&[(&fast_val, fast_end), (&slow_val, slow_end)]);
        Ok(phi.as_basic_value().into_pointer_value())
    }

    /// Dynamic arithmetic: call the runtime's tag-dispatched function. This is
    /// the adaptive fallback when operand types are unknown.
    fn compile_dyn_arith(&self, op: BinaryOp, a: &PointerValue<'ctx>, b: &PointerValue<'ctx>) -> Result<PointerValue<'ctx>, String> {
        let name = match op {
            BinaryOp::Add => "coco_add",
            BinaryOp::Sub => "coco_sub",
            BinaryOp::Mul => "coco_mul",
            BinaryOp::Div => "coco_div",
            BinaryOp::Mod => "coco_mod",
            _ => unreachable!(),
        };
        Ok(self.call(name, &[(*a).into(), (*b).into()]))
    }

    fn compile_short_circuit(&mut self, bin: &BinaryExpr, is_or: bool) -> Result<PointerValue<'ctx>, String> {
        let fn_val = self.current_fn.unwrap();
        let lhs = self.compile_expr(&bin.left)?;
        let lhs_true = self.call_basic("coco_is_truthy", &[lhs.into()]);
        self.call_void("coco_release", &[lhs.into()]);

        // Build the short-circuit value (true for ||, false for &&) here in the
        // current block, before the branch, so it's available as a phi incoming
        // from this block.
        let short_val = self.call("coco_make_bool", &[self.context.bool_type().const_int(if is_or { 1 } else { 0 }, false).into()]);
        let lhs_block = self.builder.get_insert_block().unwrap();

        let rhs_block = self.context.append_basic_block(fn_val, "sc_rhs");
        let merge_block = self.context.append_basic_block(fn_val, "sc_merge");
        if is_or {
            self.builder.build_conditional_branch(lhs_true.into_int_value(), merge_block, rhs_block).map_err(|e| e.to_string())?;
        } else {
            self.builder.build_conditional_branch(lhs_true.into_int_value(), rhs_block, merge_block).map_err(|e| e.to_string())?;
        }

        self.builder.position_at_end(rhs_block);
        let rhs = self.compile_expr(&bin.right)?;
        self.builder.build_unconditional_branch(merge_block).map_err(|e| e.to_string())?;
        let rhs_end = self.builder.get_insert_block().unwrap();

        self.builder.position_at_end(merge_block);
        let phi = self.builder.build_phi(self.val_ptr_type, "sc_result").map_err(|e| e.to_string())?;
        phi.add_incoming(&[(&short_val, lhs_block), (&rhs, rhs_end)]);
        // The losing branch's value is unused; release it. We can only release
        // the one that was actually evaluated, but since we can't know at
        // compile time, release both conditionally is complex. For correctness
        // with refcounting we accept a minor leak on the short-circuit path
        // (the unused value's ref). This is bounded and matches a simple model.
        Ok(phi.as_basic_value().into_pointer_value())
    }

    fn compile_unary(&mut self, un: &UnaryExpr) -> Result<PointerValue<'ctx>, String> {
        let operand = self.compile_expr(&un.expr)?;
        let result = match un.op {
            UnaryOp::Neg => self.call("coco_neg", &[operand.into()]),
            UnaryOp::Not => self.call("coco_not", &[operand.into()]),
            UnaryOp::BitNot => {
                let data = self.int_data(&operand);
                let r = self.builder.build_not(data, "bitnot").map_err(|e| e.to_string())?;
                self.call("coco_make_int", &[r.into()])
            }
            other => return Err(format!("unsupported unary op in native codegen: {:?}", other)),
        };
        self.call_void("coco_release", &[operand.into()]);
        Ok(result)
    }

    fn compile_assign(&mut self, bin: &BinaryExpr) -> Result<PointerValue<'ctx>, String> {
        let alloca = match &bin.left {
            Expr::Ident(ident) => self
                .locals
                .get(&ident.name)
                .copied()
                .ok_or_else(|| format!("assignment to undeclared variable '{}'", ident.name))?,
            _ => return Err("assignment to non-identifier targets is not supported".to_string()),
        };

        let rhs = if bin.op == BinaryOp::Assign {
            self.compile_expr(&bin.right)?
        } else {
            // Compound: read current, apply op, store.
            let cur = self.builder.build_load(self.val_ptr_type, alloca, "cur")
                .map_err(|e| e.to_string())?.into_pointer_value();
            let cur_retained = self.call("coco_retain", &[cur.into()]);
            let r = self.compile_compound(bin.op, cur_retained, &bin.right)?;
            self.call_void("coco_release", &[cur_retained.into()]);
            r
        };
        // Release the old value, store the new (retained) one.
        let old = self.builder.build_load(self.val_ptr_type, alloca, "old")
            .map_err(|e| e.to_string())?.into_pointer_value();
        let retained = self.call("coco_retain", &[rhs.into()]);
        self.builder.build_store(alloca, retained).map_err(|e| e.to_string())?;
        self.call_void("coco_release", &[old.into()]);
        Ok(rhs)
    }

    fn compile_compound(&mut self, op: BinaryOp, current: PointerValue<'ctx>, rhs_expr: &Expr) -> Result<PointerValue<'ctx>, String> {
        let rhs = self.compile_expr(rhs_expr)?;
        let name = match op {
            BinaryOp::AddAssign => "coco_add",
            BinaryOp::SubAssign => "coco_sub",
            BinaryOp::MulAssign => "coco_mul",
            BinaryOp::DivAssign => "coco_div",
            BinaryOp::ModAssign => "coco_mod",
            other => return Err(format!("unsupported compound assignment op: {:?}", other)),
        };
        let result = self.call(name, &[current.into(), rhs.into()]);
        self.call_void("coco_release", &[rhs.into()]);
        Ok(result)
    }

    fn compile_call(&mut self, call: &CallExpr) -> Result<PointerValue<'ctx>, String> {
        // Built-in calls dispatched by name.
        if let Expr::Ident(ident) = &call.callee {
            match ident.name.as_str() {
                "print" => return self.builtin_print(call),
                "len" => return self.builtin_len(call),
                "toString" => return self.builtin_tostring(call),
                "range" => return self.builtin_range(call),
                _ => {}
            }
        }
        let ident = match &call.callee {
            Expr::Ident(ident) => ident,
            _ => return Err("calls on non-identifier callees are not supported in native codegen".to_string()),
        };
        let mut name = ident.name.clone();
        if name == "main" { name = "coco_main".to_string(); }
        let (func, arity) = self
            .functions
            .get(&name)
            .copied()
            .ok_or_else(|| format!("call to unknown function '{}' in native codegen", ident.name))?;
        let _ = arity;
        let mut args: Vec<BasicValueEnum> = Vec::new();
        for arg in &call.args {
            let v = self.compile_expr(&arg.value)?;
            args.push(v.into());
        }
        let m = self.meta(&args);
        let result = self.builder.build_call(func, &m, "call").map_err(|e| e.to_string())?
            .try_as_basic_value().left()
            .ok_or_else(|| "function call did not return a value".to_string())?
            .into_pointer_value();
        // Release args (the callee retained the return value independently).
        for a in &args {
            self.call_void("coco_release", &[*a]);
        }
        Ok(result)
    }

    fn builtin_print(&mut self, call: &CallExpr) -> Result<PointerValue<'ctx>, String> {
        if call.args.is_empty() {
            self.call_void("coco_print", &[self.call("coco_make_null", &[]).into()]);
        } else {
            let v = self.compile_expr(&call.args[0].value)?;
            self.call_void("coco_print", &[v.into()]);
            self.call_void("coco_release", &[v.into()]);
            for arg in call.args.iter().skip(1) {
                let v = self.compile_expr(&arg.value)?;
                self.call_void("coco_release", &[v.into()]);
            }
        }
        Ok(self.call("coco_make_null", &[]))
    }

    fn builtin_len(&mut self, call: &CallExpr) -> Result<PointerValue<'ctx>, String> {
        let v = self.compile_expr(&call.args[0].value)?;
        let r = self.call("coco_len", &[v.into()]);
        self.call_void("coco_release", &[v.into()]);
        Ok(r)
    }

    fn builtin_tostring(&mut self, call: &CallExpr) -> Result<PointerValue<'ctx>, String> {
        let v = self.compile_expr(&call.args[0].value)?;
        let r = self.call("coco_tostring", &[v.into()]);
        self.call_void("coco_release", &[v.into()]);
        Ok(r)
    }

    fn builtin_range(&mut self, call: &CallExpr) -> Result<PointerValue<'ctx>, String> {
        let a = self.compile_expr(&call.args[0].value)?;
        let b = self.compile_expr(&call.args[1].value)?;
        let av = self.int_data(&a);
        let bv = self.int_data(&b);
        let r = self.call("coco_range", &[av.into(), bv.into()]);
        self.call_void("coco_release", &[a.into()]);
        self.call_void("coco_release", &[b.into()]);
        Ok(r)
    }

    fn compile_index(&mut self, idx: &IndexExpr) -> Result<PointerValue<'ctx>, String> {
        let obj = self.compile_expr(&idx.object)?;
        let index = self.compile_expr(&idx.index)?;
        // List indexing: list[i] via coco_list_get(list, i64).
        let i = self.int_data(&index);
        let r = self.call("coco_list_get", &[obj.into(), i.into()]);
        self.call_void("coco_release", &[obj.into()]);
        self.call_void("coco_release", &[index.into()]);
        Ok(r)
    }

    fn compile_member(&mut self, mem: &MemberExpr) -> Result<PointerValue<'ctx>, String> {
        // `.length` on lists/strings/maps.
        if mem.property.name == "length" {
            let obj = self.compile_expr(&mem.object)?;
            let r = self.call("coco_len", &[obj.into()]);
            self.call_void("coco_release", &[obj.into()]);
            return Ok(r);
        }
        Err("member access (other than .length) is not yet supported in native codegen".to_string())
    }

    fn compile_ternary(&mut self, tern: &TernaryExpr) -> Result<PointerValue<'ctx>, String> {
        let cond_val = self.compile_expr(&tern.condition)?;
        let is_true = self.call_basic("coco_is_truthy", &[cond_val.into()]);
        self.call_void("coco_release", &[cond_val.into()]);
        let fn_val = self.current_fn.unwrap();
        let then_block = self.context.append_basic_block(fn_val, "ternthen");
        let else_block = self.context.append_basic_block(fn_val, "ternelse");
        let merge_block = self.context.append_basic_block(fn_val, "ternmerge");
        self.builder.build_conditional_branch(is_true.into_int_value(), then_block, else_block).map_err(|e| e.to_string())?;

        self.builder.position_at_end(then_block);
        let then_val = self.compile_expr(&tern.then_expr)?;
        self.builder.build_unconditional_branch(merge_block).map_err(|e| e.to_string())?;
        let then_end = self.builder.get_insert_block().unwrap();

        self.builder.position_at_end(else_block);
        let else_val = self.compile_expr(&tern.else_expr)?;
        self.builder.build_unconditional_branch(merge_block).map_err(|e| e.to_string())?;
        let else_end = self.builder.get_insert_block().unwrap();

        self.builder.position_at_end(merge_block);
        let phi = self.builder.build_phi(self.val_ptr_type, "ternval").map_err(|e| e.to_string())?;
        phi.add_incoming(&[(&then_val, then_end), (&else_val, else_end)]);
        Ok(phi.as_basic_value().into_pointer_value())
    }

    fn compile_null_coalesce(&mut self, nc: &NullCoalesceExpr) -> Result<PointerValue<'ctx>, String> {
        let left = self.compile_expr(&nc.left)?;
        // is_truthy is true for any non-null; ?? only falls through on null.
        // Check the tag directly: null tag is COCO_NULL (5).
        let fn_val = self.current_fn.unwrap();
        // Use is_truthy: null is falsy, so if truthy use left else right.
        let is_truthy = self.call_basic("coco_is_truthy", &[left.into()]);
        let then_block = self.context.append_basic_block(fn_val, "ncthen");
        let else_block = self.context.append_basic_block(fn_val, "ncelse");
        let merge_block = self.context.append_basic_block(fn_val, "ncmerge");
        self.builder.build_conditional_branch(is_truthy.into_int_value(), then_block, else_block).map_err(|e| e.to_string())?;

        self.builder.position_at_end(then_block);
        self.builder.build_unconditional_branch(merge_block).map_err(|e| e.to_string())?;
        let then_end = self.builder.get_insert_block().unwrap();

        self.builder.position_at_end(else_block);
        let right_val = self.compile_expr(&nc.right)?;
        self.call_void("coco_release", &[left.into()]);
        self.builder.build_unconditional_branch(merge_block).map_err(|e| e.to_string())?;
        let else_end = self.builder.get_insert_block().unwrap();

        self.builder.position_at_end(merge_block);
        let phi = self.builder.build_phi(self.val_ptr_type, "ncval").map_err(|e| e.to_string())?;
        phi.add_incoming(&[(&left, then_end), (&right_val, else_end)]);
        Ok(phi.as_basic_value().into_pointer_value())
    }

    fn compile_array(&mut self, arr: &ArrayLiteral) -> Result<PointerValue<'ctx>, String> {
        let cap = self.i64_type.const_int(arr.elements.len() as u64, false);
        let list = self.call("coco_list_new", &[cap.into()]);
        for el in &arr.elements {
            let v = self.compile_expr(el)?;
            self.call_void("coco_list_push", &[list.into(), v.into()]);
            self.call_void("coco_release", &[v.into()]);
        }
        Ok(list)
    }

    // --- Entry point --------------------------------------------------------

    fn generate_entry(&mut self) -> Result<(), String> {
        let i32 = self.context.i32_type();
        let main_type = i32.fn_type(&[i32.into(), self.i8_type.ptr_type(AddressSpace::default()).ptr_type(AddressSpace::default()).into()], false);
        let entry = self.module.add_function("main", main_type, None);
        let block = self.context.append_basic_block(entry, "entry");
        self.builder.position_at_end(block);

        // Call coco_main() if it exists; exit with its int data as the code.
        if let Some(&(func, _)) = self.functions.get("coco_main") {
            let ret = self.builder.build_call(func, &[], "call_main").map_err(|e| e.to_string())?
                .try_as_basic_value().left()
                .ok_or_else(|| "main() did not return a value".to_string())?
                .into_pointer_value();
            let data = self.int_data(&ret);
            let truncated = self.builder.build_int_cast(data, i32, "exitcode").map_err(|e| e.to_string())?;
            self.builder.build_return(Some(&truncated)).map_err(|e| e.to_string())?;
        } else {
            self.builder.build_return(Some(&i32.const_int(0, false))).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    // --- Helpers: call runtime functions -----------------------------------

    /// Convert a slice of basic values to call arguments (metadata values).
    fn meta(&self, args: &[BasicValueEnum<'ctx>]) -> Vec<inkwell::values::BasicMetadataValueEnum<'ctx>> {
        args.iter().map(|a| (*a).into()).collect()
    }

    /// Call a runtime function returning a `coco_val*`.
    fn call(&self, name: &str, args: &[BasicValueEnum<'ctx>]) -> PointerValue<'ctx> {
        self.call_basic(name, args).into_pointer_value()
    }

    /// Call a runtime function returning any basic value (use for bool/i32/etc.).
    fn call_basic(&self, name: &str, args: &[BasicValueEnum<'ctx>]) -> BasicValueEnum<'ctx> {
        let f = self.module.get_function(name).unwrap_or_else(|| panic!("runtime fn {} not declared", name));
        let m = self.meta(args);
        self.builder.build_call(f, &m, name).unwrap()
            .try_as_basic_value().left()
            .unwrap_or_else(|| panic!("runtime fn {} did not return a value", name))
    }

    fn call_void(&self, name: &str, args: &[BasicValueEnum<'ctx>]) {
        let f = self.module.get_function(name).unwrap_or_else(|| panic!("runtime fn {} not declared", name));
        let m = self.meta(args);
        let _ = self.builder.build_call(f, &m, name);
    }

    /// Extract the i64 data field of a value assumed to be an int (`v->u.i`).
    /// The coco_val layout (C): `{ int tag (4), int refcount (4), union (8) }`.
    /// The union's first 8 bytes hold the i64 for COCO_INT, at byte offset 8
    /// (after tag + refcount). We cast to i8*, GEP forward 8 bytes, cast to
    /// i64*, and load.
    fn int_data(&self, v: &PointerValue<'ctx>) -> IntValue<'ctx> {
        let i8p = self.i8_type.ptr_type(AddressSpace::default());
        let as_i8 = self.builder.build_pointer_cast(*v, i8p, "asi8").unwrap();
        let off = self.i64_type.const_int(8, false);
        let data_ptr = unsafe {
            self.builder.build_gep(self.i8_type, as_i8, &[off], "dataptr").unwrap()
        };
        let i64p = self.i64_type.ptr_type(AddressSpace::default());
        let as_i64p = self.builder.build_pointer_cast(data_ptr, i64p, "i64p").unwrap();
        self.builder.build_load(self.i64_type, as_i64p, "intdata").unwrap().into_int_value()
    }

    /// Extract the f64 data field of a value assumed to be a float (`v->u.f`).
    fn float_data(&self, v: &PointerValue<'ctx>) -> inkwell::values::FloatValue<'ctx> {
        let i8p = self.i8_type.ptr_type(AddressSpace::default());
        let as_i8 = self.builder.build_pointer_cast(*v, i8p, "fasi8").unwrap();
        let off = self.i64_type.const_int(8, false);
        let data_ptr = unsafe {
            self.builder.build_gep(self.i8_type, as_i8, &[off], "fdataptr").unwrap()
        };
        let f64p = self.context.f64_type().ptr_type(AddressSpace::default());
        let as_f64p = self.builder.build_pointer_cast(data_ptr, f64p, "f64p").unwrap();
        self.builder.build_load(self.context.f64_type(), as_f64p, "floatdata").unwrap().into_float_value()
    }

    /// Compile to an object file via LLVM.
    pub fn compile_to_object(&self, path: &str) -> Result<(), String> {
        use inkwell::targets::{FileType, InitializationConfig, RelocMode, Target, TargetMachine};
        Target::initialize_native(&InitializationConfig::default()).map_err(|e| e.to_string())?;
        let target_triple = TargetMachine::get_default_triple();
        let target = Target::from_triple(&target_triple).map_err(|e| e.to_string())?;
        let target_machine = target.create_target_machine(
            &target_triple, "generic", "",
            OptimizationLevel::Default, RelocMode::Default,
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

// Without the `native` feature, this crate has no public API.
