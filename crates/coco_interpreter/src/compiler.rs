//! Bytecode compiler: walks the AST and emits instructions into a Chunk.
//!
//! ## Architecture
//!
//! The compiler processes Coco source in two modes:
//!
//! - **Script mode** (top-level): all variables use global access opcodes.
//!   Function declarations compile their bodies internally and store `FnObj`
//!   values in the constant pool.
//!
//! - **Function mode**: parameters and `let`/`const` declarations allocate
//!   local slots. Local variable resolution is lexical (innermost scope wins).
//!
//! The output is a `Chunk` ready to be executed by the VM.

use crate::ir::{
    Chunk, ChunkBuilder, FnObj, Label, OP_ADD, OP_BIT_AND, OP_BIT_NOT, OP_BIT_OR, OP_BIT_XOR,
    OP_BUILD_LIST, OP_BUILD_MAP, OP_CALL, OP_CATCH, OP_CONST, OP_DEFINE_GLOBAL, OP_DIV, OP_DUP,
    OP_EQ, OP_FALSE, OP_GE, OP_GT, OP_INDEX, OP_JUMP, OP_JUMP_IF_FALSE, OP_JUMP_IF_TRUE, OP_LE,
    OP_LOAD_GLOBAL, OP_LOAD_LOCAL, OP_LT, OP_MAKE_CLOSURE, OP_MEMBER, OP_METHOD_CALL,
    OP_MOD, OP_MUL, OP_NE, OP_NEG, OP_NEW, OP_NOT, OP_NULL, OP_POP, OP_POP_JUMP_IF_FALSE,
    OP_POW, OP_RETURN, OP_SHL, OP_SHR, OP_STORE_GLOBAL, OP_STORE_INDEX, OP_STORE_LOCAL,
    OP_STORE_MEMBER, OP_STORE_MEMBER_LOCAL, OP_STORE_INDEX_LOCAL,
    OP_SUB, OP_SUPER_METHOD, OP_SWAP, OP_THIS, OP_THROW, OP_TRUE,
    OP_TRY_BEGIN, OP_TRY_END, OP_AWAIT, OP_LAZY_CALL, OP_ASYNC_CALL, OP_TRY,
    OP_SELECT_TRY_RECV, OP_TYPE_IS, OP_TYPEOF, OP_PARALLEL_RUN,
    // OP_PIPE_VAL, OP_ITER_MAP, OP_CLOSE_UPVALUE are dispatched by the VM but
    // not yet emitted by the compiler; import them here when compile_for /
    // compile_pipe / scope-close are wired to emit them.
};
use crate::value::Value;
use coco_parser::Parser;
use coco_syntax::*;
use num_bigint::BigInt;

// ============================================================================
// Compile error
// ============================================================================

/// Error produced by the bytecode compiler.
#[derive(Debug, Clone)]
pub struct CompileError {
    pub message: String,
}

impl CompileError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
        }
    }
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CompileError: {}", self.message)
    }
}

/// Result type for compilation.
pub type CResult<T> = Result<T, CompileError>;

// ============================================================================
// Local variable tracking
// ============================================================================

/// A local variable slot.
#[derive(Debug, Clone)]
struct Local {
    name: String,
    /// Slot index in the frame.
    slot: usize,
    /// Scope depth at which this local was declared.
    depth: usize,
}

/// Loop bookkeeping for break/continue.
#[derive(Debug, Clone)]
struct LoopLabels {
    /// Where to jump on `break`.
    end_label: Label,
    /// Where to jump on `continue`.
    start_label: Label,
    /// The hidden local slot holding the loop variable (for-in only).
    _loop_var_slot: Option<usize>,
}

// ============================================================================
// Compiler
// ============================================================================

/// The bytecode compiler.
///
/// Create with `Compiler::new()`, feed it a `Program` via `compile_script`,
/// and retrieve the resulting `Chunk`.
pub struct Compiler {
    builder: ChunkBuilder,
    /// Locals in the current function (empty in script mode).
    locals: Vec<Local>,
    /// Current scope depth. Incremented on block entry, decremented on exit.
    scope_depth: usize,
    /// Stack of enclosing loops for break/continue resolution.
    loop_stack: Vec<LoopLabels>,
    /// Whether we are inside a function body.
    in_function: bool,
    /// Whether we are compiling inside a `lazy` expression wrapper.
    in_lazy: bool,
    /// Enable tree-shaking (dead function elimination) after compilation.
    pub enable_tree_shake: bool,
}

impl Compiler {
    /// Create a new compiler.
    pub fn new() -> Self {
        Self {
            builder: ChunkBuilder::new(),
            locals: Vec::new(),
            scope_depth: 0,
            loop_stack: Vec::new(),
            in_function: false,
            in_lazy: false,
            enable_tree_shake: false,
        }
    }

    // ========================================================================
    // Public API
    // ========================================================================

    /// Compile a top-level program into a Chunk.
    pub fn compile_script(&mut self, program: &Program) -> CResult<Chunk> {
        self.in_function = false;
        // First pass: register all function declarations so forward references
        // work (functions can call other functions defined later).
        for item in &program.items {
            match item {
                Item::FnDecl(fn_decl) => { self.declare_function(fn_decl)?; }
                Item::Export(export) => {
                    if let Item::FnDecl(fn_decl) = &*export.item {
                        self.declare_function(fn_decl)?;
                    }
                }
                _ => {}
            }
        }
        // Second pass: compile each item.
        for item in &program.items {
            self.compile_item(item)?;
        }
        // Ensure the script returns something.
        self.emit_op(OP_NULL);
        self.emit_op(OP_RETURN);

        let mut chunk = self.finish_chunk();
        if self.enable_tree_shake {
            self.tree_shake(&mut chunk);
        }
        Ok(chunk)
    }

    // ========================================================================
    // Tree-shaking — remove unreachable function declarations
    // ========================================================================

    /// Remove unreachable functions from the compiled chunk.
    ///
    /// Starting from `main()`, walks the call graph through bytecode
    /// to find which functions are actually called. Any `OP_DEFINE_GLOBAL`
    /// instruction that defines an unreachable function is removed along
    /// with its associated FnObj constant.
    fn tree_shake(&self, chunk: &mut Chunk) {
        // Find the FnObj constants and their names
        let mut fn_names: std::collections::HashMap<usize, String> =
            std::collections::HashMap::new();
        let mut reachable: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        let mut worklist: Vec<String> = Vec::new();

        // Scan constants for FnObj values
        for (i, constant) in chunk.constants.iter().enumerate() {
            if let Value::FnObj(fn_obj) = constant {
                let name = fn_obj.name.clone();
                if name != "<script>" {
                    fn_names.insert(i, name.clone());
                }
            }
        }

        // Start with main()
        if fn_names.values().any(|n| n == "main") {
            worklist.push("main".to_string());
            reachable.insert("main".to_string());
        }

        // Walk the bytecode of reachable functions to find calls
        while let Some(name) = worklist.pop() {
            // Find the FnObj constant index for this function
            let const_idx = fn_names
                .iter()
                .find(|(_, n)| *n == &name)
                .map(|(i, _)| *i);

            if let Some(idx) = const_idx {
                let fn_obj = match &chunk.constants[idx] {
                    Value::FnObj(f) => f.clone(),
                    _ => continue,
                };

                // Scan the function's bytecode for CALL instructions
                let code = &fn_obj.chunk.code;
                let mut ip: i32 = 0;
                while (ip as usize) < code.len() {
                    let op = code[ip as usize];
                    let step = 1
                        + crate::ir::operand_bytes(op).unwrap_or(0) as i32;

                    if op == OP_CALL || op == OP_ASYNC_CALL || op == OP_LAZY_CALL {
                        // Walk backward from CALL to find the preceding
                        // LOAD_GLOBAL that pushes the callee.
                        let mut back: i32 = ip - 1;
                        while back >= 0 {
                            let bop = code[back as usize];
                            if bop == OP_LOAD_GLOBAL || bop == OP_DEFINE_GLOBAL {
                                let idx = crate::ir::read_u16(
                                    &code[(back + 1) as usize..],
                                ) as usize;
                                if let Some(Value::String(callee_name)) =
                                    fn_obj.chunk.constants.get(idx)
                                {
                                    if fn_names.values().any(|n| n == callee_name)
                                        && !reachable.contains(callee_name)
                                    {
                                        reachable.insert(callee_name.clone());
                                        worklist.push(callee_name.clone());
                                    }
                                }
                                break;
                            }
                            back -= 1
                                + crate::ir::operand_bytes(bop).unwrap_or(0)
                                    as i32;
                        }
                    }

                    ip += step;
                }
            }
        }

        // Remove unreachable DEFINE_GLOBAL instructions from the script chunk
        let code = &chunk.code;
        let constants = &chunk.constants;
        let mut new_code: Vec<u8> = Vec::new();
        let mut ip: i32 = 0;

        while (ip as usize) < code.len() {
            let op = code[ip as usize];
            if op == OP_DEFINE_GLOBAL {
                // Check if this defines an unreachable function
                let name_idx =
                    crate::ir::read_u16(&code[(ip + 1) as usize..]) as usize;
                if let Some(Value::String(fn_name)) = constants.get(name_idx) {
                    if fn_names.values().any(|n| n == fn_name)
                        && !reachable.contains(fn_name)
                    {
                        // Skip this instruction (2 bytes op+idx)
                        ip += 3;
                        // Also skip the preceding LOAD_GLOBAL + MAKE_CLOSURE pattern
                        // by checking if previous instructions are part of fn decl
                        continue;
                    }
                }
            }

            // Copy the instruction
            new_code.push(op);
            let step = 1 + crate::ir::operand_bytes(op).unwrap_or(0) as i32;
            for j in 1..step {
                let pos = (ip + j) as usize;
                if pos < code.len() {
                    new_code.push(code[pos]);
                }
            }
            ip += step;
        }

        chunk.code = new_code;

        // Log tree-shaking results
        let removed: Vec<String> = fn_names
            .values()
            .filter(|n| !reachable.contains(*n) && *n != "main")
            .cloned()
            .collect();
        if !removed.is_empty() {
            eprintln!(
                "[tree-shake] removed {} unreachable function(s): {:?}",
                removed.len(),
                removed
            );
        }
    }

    // ========================================================================
    // Items
    // ========================================================================

    fn compile_item(&mut self, item: &Item) -> CResult<()> {
        match item {
            Item::FnDecl(fn_decl) => {
                // Top-level functions are declared in the first pass of
                // `compile_script`, so skip them here to avoid double-compiling.
                // Nested function declarations (encountered while compiling a
                // function body, where `in_function` is true) were NOT covered
                // by the first pass and must be declared now so they bind to a
                // local closure and become callable.
                if self.in_function {
                    self.declare_function(fn_decl)?;
                }
                Ok(())
            }
            Item::LetDecl(let_decl) => self.compile_let(let_decl),
            Item::ConstDecl(const_decl) => self.compile_const(const_decl),
            Item::ExprStmt(expr_stmt) => {
                self.compile_expr(&expr_stmt.expr)?;
                self.emit_op(OP_POP); // discard expression value
                Ok(())
            }
            Item::Stmt(stmt) => self.compile_stmt(stmt),
            Item::ClassDecl(class_decl) => self.compile_class_decl(class_decl),
            Item::InterfaceDecl(iface_decl) => self.compile_interface_decl(iface_decl),
            Item::TraitDecl(trait_decl) => self.compile_trait_decl(trait_decl),
            Item::Export(export) => self.compile_item(&export.item),
            Item::Import(import) => self.compile_import(import),
            _ => Ok(()),
        }
    }

    /// Compile an import statement by resolving the module, parsing it,
    /// and inlining its exported items into the current chunk as globals.
    fn compile_import(&mut self, import: &Import) -> CResult<()> {
        let src = if import.source.starts_with("std/") {
            match crate::get_stdlib_source(&import.source) {
                Some(s) => s.to_string(),
                None => {
                    return Err(CompileError::new(format!(
                        "stdlib module '{}' not found",
                        import.source
                    )));
                }
            }
        } else {
            // Resolve file-based import relative to the current source file
            let mut path = std::path::PathBuf::from(&import.source);
            if !path.exists() && path.extension().is_none() {
                path.set_extension("co");
            }
            match std::fs::read_to_string(&path) {
                Ok(s) => s,
                Err(e) => {
                    return Err(CompileError::new(format!(
                        "cannot read module '{}': {}",
                        path.display(),
                        e
                    )));
                }
            }
        };

        // Parse the module
        let mut parser = Parser::new(&src);
        let module = parser.parse_program();

        // Compile ALL top-level items in the module so that private helper
        // functions/consts are defined and callable from exported functions.
        // (The import statement selects which names the importer binds, but
        // the module's own code needs all its definitions to run.) Exported
        // and non-exported fns are both declared; consts/classes compiled via
        // compile_item. This is a first+second pass combined: declare fns
        // first so forward references work, then compile non-fn items.
        for item in &module.items {
            let fd = match item {
                Item::FnDecl(f) => Some(f),
                Item::Export(e) => match &*e.item {
                    Item::FnDecl(f) => Some(f),
                    _ => None,
                },
                _ => None,
            };
            if let Some(fn_decl) = fd {
                self.declare_function(fn_decl)?;
            }
        }
        for item in &module.items {
            match item {
                Item::FnDecl(_) | Item::Export(_) => { /* already declared */ }
                _ => self.compile_item(item)?,
            }
        }
        Ok(())
    }

    fn compile_let(&mut self, let_decl: &LetDecl) -> CResult<()> {
        let name = let_decl.name.name.clone();
        if let Some(expr) = &let_decl.value {
            self.compile_expr(expr)?;
        } else {
            // let x; — default to null
            self.emit_op(OP_NULL);
        }

        if self.in_function {
            let slot = self.add_local(&name);
            self.emit_op_u16(OP_STORE_LOCAL, slot as u16);
        } else {
            let name_idx = self.name_constant(&name);
            self.emit_op_u16(OP_DEFINE_GLOBAL, name_idx);
        }
        Ok(())
    }

    fn compile_const(&mut self, const_decl: &ConstDecl) -> CResult<()> {
        let name = const_decl.name.name.clone();
        self.compile_expr(&const_decl.value)?;

        if self.in_function {
            let slot = self.add_local(&name);
            self.emit_op_u16(OP_STORE_LOCAL, slot as u16);
        } else {
            let name_idx = self.name_constant(&name);
            self.emit_op_u16(OP_DEFINE_GLOBAL, name_idx);
        }
        Ok(())
    }

    // ========================================================================
    // Statements
    // ========================================================================

    fn compile_stmt(&mut self, stmt: &Stmt) -> CResult<()> {
        match stmt {
            Stmt::Item(item) => self.compile_item(item),
            Stmt::Expr(expr_stmt) => {
                self.compile_expr(&expr_stmt.expr)?;
                self.emit_op(OP_POP);
                Ok(())
            }
            Stmt::If(if_stmt) => self.compile_if(if_stmt),
            Stmt::For(for_stmt) => self.compile_for(for_stmt),
            Stmt::While(while_stmt) => self.compile_while(while_stmt),
            Stmt::DoWhile(dw) => self.compile_do_while(dw),
            Stmt::Loop(loop_stmt) => self.compile_loop(loop_stmt),
            Stmt::Return(ret) => self.compile_return(ret),
            Stmt::Break(_) => self.compile_break(),
            Stmt::Continue(_) => self.compile_continue(),
            Stmt::Throw(throw) => self.compile_throw(throw),
            Stmt::Try(try_stmt) => self.compile_try(try_stmt),
            Stmt::Parallel(parallel) => self.compile_parallel(parallel),
            Stmt::Coro(coro) => self.compile_coro(coro),
            Stmt::Select(select) => self.compile_select(select),
            Stmt::Synchronized(sync) => self.compile_synchronized(sync),
            _ => Ok(()),
        }
    }

    // -----------------------------------------------------------------------
    // if / else-if / else
    // -----------------------------------------------------------------------

    fn compile_if(&mut self, if_stmt: &IfStmt) -> CResult<()> {
        // Compile condition
        self.compile_expr(&if_stmt.condition)?;

        // Number of else-if branches + optional else
        let has_else = if_stmt.else_block.is_some();
        let else_if_count = if_stmt.else_ifs.len();

        if else_if_count == 0 && !has_else {
            // Simple if without else
            let end_label = self.new_label();
            self.emit_jump(OP_JUMP_IF_FALSE, end_label);
            self.begin_scope();
            self.compile_block(&if_stmt.then_block)?;
            self.end_scope();
            self.place_label(end_label);
        } else {
            // if ... else-if ... else chain
            let end_label = self.new_label();
            let mut jump_labels: Vec<Label> = vec![self.new_label()]; // jump to first else-if/else
            self.emit_jump(OP_JUMP_IF_FALSE, jump_labels[0]);

            // Then block
            self.begin_scope();
            self.compile_block(&if_stmt.then_block)?;
            self.end_scope();
            self.emit_jump(OP_JUMP, end_label);

            // Else-if branches
            for else_if in &if_stmt.else_ifs {
                self.place_label(jump_labels.last().copied().unwrap());

                let next_label = self.new_label();
                jump_labels.push(next_label);

                self.compile_expr(&else_if.condition)?;
                self.emit_jump(OP_JUMP_IF_FALSE, next_label);

                self.begin_scope();
                self.compile_block(&else_if.block)?;
                self.end_scope();
                self.emit_jump(OP_JUMP, end_label);
            }

            // Else block (if present)
            if has_else {
                self.place_label(jump_labels.last().copied().unwrap());
                self.begin_scope();
                self.compile_block(if_stmt.else_block.as_ref().unwrap())?;
                self.end_scope();
            } else {
                self.place_label(jump_labels.last().copied().unwrap());
            }

            self.place_label(end_label);
        }

        Ok(())
    }

    // -----------------------------------------------------------------------
    // while
    // -----------------------------------------------------------------------

    fn compile_while(&mut self, while_stmt: &WhileStmt) -> CResult<()> {
        let start_label = self.new_label();
        let end_label = self.new_label();

        self.loop_stack.push(LoopLabels {
            end_label,
            start_label,
            _loop_var_slot: None,
        });

        self.place_label(start_label);

        // Condition
        self.compile_expr(&while_stmt.condition)?;
        self.emit_jump(OP_JUMP_IF_FALSE, end_label);

        // Body
        self.begin_scope();
        self.compile_block(&while_stmt.body)?;
        self.end_scope();

        self.emit_loop(start_label);
        self.place_label(end_label);

        self.loop_stack.pop();
        Ok(())
    }

    // -----------------------------------------------------------------------
    // do-while — body always executes once; condition checked after
    // -----------------------------------------------------------------------

    fn compile_do_while(&mut self, dw: &DoWhileStmt) -> CResult<()> {
        let body_label = self.new_label();
        let end_label = self.new_label();

        self.loop_stack.push(LoopLabels {
            end_label,
            start_label: body_label,
            _loop_var_slot: None,
        });

        // Body executes at least once
        self.place_label(body_label);
        self.begin_scope();
        self.compile_block(&dw.body)?;
        self.end_scope();

        // Condition — loop back if true
        self.compile_expr(&dw.condition)?;
        self.emit_jump(OP_JUMP_IF_TRUE, body_label);

        self.place_label(end_label);

        self.loop_stack.pop();
        Ok(())
    }

    // -----------------------------------------------------------------------
    // for-in
    // -----------------------------------------------------------------------

    fn compile_for(&mut self, for_stmt: &ForStmt) -> CResult<()> {
        // We need three hidden locals: iterable, index, and the loop variable.
        // The loop variable name is for_stmt.pattern.name.
        let iter_name = format!("__for_iter_{}", self.locals.len());
        let idx_name = format!("__for_idx_{}", self.locals.len());
        let elem_name = for_stmt.pattern.name.clone();

        // Only valid in function bodies. For top-level scripts, we use globals.
        if !self.in_function {
            return Err(CompileError::new(
                "for-in loops currently require function scope (compile inside fn main)",
            ));
        }

        // Reserve local slots
        let iter_slot = self.add_local(&iter_name);
        let idx_slot = self.add_local(&idx_name);
        let elem_slot = self.add_local(&elem_name);

        // Compile iterable and store — no POP since STORE_LOCAL consumes the top
        self.compile_expr(&for_stmt.iterable)?;
        self.emit_op_u16(OP_STORE_LOCAL, iter_slot as u16);

        // Initialize index to 0
        let zero_idx = self.add_constant(Value::int_from_i64(0));
        self.emit_op_u16(OP_CONST, zero_idx);
        self.emit_op_u16(OP_STORE_LOCAL, idx_slot as u16);

        let start_label = self.new_label();
        let end_label = self.new_label();

        self.loop_stack.push(LoopLabels {
            end_label,
            start_label,
            _loop_var_slot: Some(elem_slot),
        });

        self.place_label(start_label);

        // Check: index < iterable.length
        self.emit_op_u16(OP_LOAD_LOCAL, idx_slot as u16);
        self.emit_op_u16(OP_LOAD_LOCAL, iter_slot as u16);
        let len_idx = self.name_constant("length");
        self.emit_op_u16(OP_MEMBER, len_idx);
        self.emit_op(OP_LT);
        self.emit_jump(OP_JUMP_IF_FALSE, end_label);

        // Get element: iterable[index]
        self.emit_op_u16(OP_LOAD_LOCAL, iter_slot as u16);
        self.emit_op_u16(OP_LOAD_LOCAL, idx_slot as u16);
        self.emit_op(OP_INDEX);
        self.emit_op_u16(OP_STORE_LOCAL, elem_slot as u16);

        // Body
        self.begin_scope();
        self.compile_block(&for_stmt.body)?;
        self.end_scope();

        // Increment index
        self.emit_op_u16(OP_LOAD_LOCAL, idx_slot as u16);
        let one_idx = self.add_constant(Value::int_from_i64(1));
        self.emit_op_u16(OP_CONST, one_idx);
        self.emit_op(OP_ADD);
        self.emit_op_u16(OP_STORE_LOCAL, idx_slot as u16);

        self.emit_loop(start_label);
        self.place_label(end_label);

        self.loop_stack.pop();
        Ok(())
    }

    // -----------------------------------------------------------------------
    // loop
    // -----------------------------------------------------------------------

    fn compile_loop(&mut self, loop_stmt: &LoopStmt) -> CResult<()> {
        let start_label = self.new_label();
        let end_label = self.new_label();

        self.loop_stack.push(LoopLabels {
            end_label,
            start_label,
            _loop_var_slot: None,
        });

        self.place_label(start_label);
        self.begin_scope();
        self.compile_block(&loop_stmt.body)?;
        self.end_scope();
        self.emit_loop(start_label);
        self.place_label(end_label);

        self.loop_stack.pop();
        Ok(())
    }

    // -----------------------------------------------------------------------
    // return
    // -----------------------------------------------------------------------

    fn compile_return(&mut self, ret: &ReturnStmt) -> CResult<()> {
        if let Some(expr) = &ret.value {
            self.compile_expr(expr)?;
        } else {
            self.emit_op(OP_NULL);
        }
        self.emit_op(OP_RETURN);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // break / continue
    // -----------------------------------------------------------------------

    fn compile_break(&mut self) -> CResult<()> {
        let loop_info = self
            .loop_stack
            .last()
            .ok_or_else(|| CompileError::new("break outside loop"))?;
        self.emit_jump(OP_JUMP, loop_info.end_label);
        Ok(())
    }

    fn compile_continue(&mut self) -> CResult<()> {
        let loop_info = self
            .loop_stack
            .last()
            .ok_or_else(|| CompileError::new("continue outside loop"))?;
        self.emit_loop(loop_info.start_label);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // throw
    // -----------------------------------------------------------------------

    fn compile_throw(&mut self, throw: &ThrowStmt) -> CResult<()> {
        self.compile_expr(&throw.value)?;
        self.emit_op(OP_THROW);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // try / catch
    // -----------------------------------------------------------------------

    fn compile_try(&mut self, try_stmt: &TryStmt) -> CResult<()> {
        let catch_label = self.new_label();
        let end_label = self.new_label();

        // TRY_BEGIN sets up the handler
        self.emit_jump(OP_TRY_BEGIN, catch_label);

        // Try body
        self.compile_block(&try_stmt.body)?;
        self.emit_op(OP_TRY_END);
        self.emit_jump(OP_JUMP, end_label);

        // Catch handler
        self.place_label(catch_label);
        self.emit_op(OP_CATCH); // error value is now on stack

        if let Some(catch) = try_stmt.catches.first() {
            let name = catch.param.name.clone();

            if self.in_function {
                self.begin_scope();
                let slot = self.add_local(&name);
                self.emit_op_u16(OP_STORE_LOCAL, slot as u16);
                self.emit_op(OP_POP);
                self.compile_block(&catch.body)?;
                self.end_scope();
            } else {
                let name_idx = self.name_constant(&name);
                // Define a global for the catch parameter (temporary)
                self.emit_op_u16(OP_DEFINE_GLOBAL, name_idx);
                self.compile_block(&catch.body)?;
            }
        } else {
            // No catch clause; re-throw
            self.emit_op(OP_THROW);
        }

        self.place_label(end_label);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // parallel expression (in expression context)
    // -----------------------------------------------------------------------

    fn compile_parallel_expr(&mut self, parallel: &ParallelExpr) -> CResult<()> {
        // Spawn each run clause (leaving TaskHandles), then join in parallel
        // on OS threads via OP_PARALLEL_RUN. The `await` prefix is a no-op
        // here because the join blocks until all runs complete.
        if parallel.runs.is_empty() {
            self.emit_op(OP_NULL);
            return Ok(());
        }
        for run in &parallel.runs {
            self.compile_expr(&run.expr)?;
        }
        self.emit_op_u8(OP_PARALLEL_RUN, parallel.runs.len() as u8);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // parallel { run expr; ... } (statement)
    // -----------------------------------------------------------------------

    fn compile_parallel(&mut self, parallel: &ParallelStmt) -> CResult<()> {
        if parallel.runs.is_empty() {
            self.emit_op(OP_NULL);
            return Ok(());
        }
        // Spawn each run clause (each leaves a TaskHandle on the stack), then
        // join them in parallel on OS threads and push the last result. This
        // replaces the previous serial await-per-run which ran tasks one at a
        // time on the cooperative scheduler.
        for run in &parallel.runs {
            self.compile_expr(&run.expr)?;
        }
        self.emit_op_u8(OP_PARALLEL_RUN, parallel.runs.len() as u8);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // coro { body }
    // -----------------------------------------------------------------------

    fn compile_coro(&mut self, coro: &CoroStmt) -> CResult<()> {
        // Fire-and-forget coroutine: compile the body as an async lambda,
        // spawn it as a task, and discard the handle.
        let params: Vec<String> = Vec::new();
        let chunk = self.compile_function_body("<coro>", &params, &coro.body)?;
        let fn_obj = FnObj {
            name: "<coro>".to_string(),
            arity: 0,
            chunk,
            is_async: true,
        };
        let const_idx = self.add_constant(Value::FnObj(fn_obj));
        self.emit_op_u16(OP_MAKE_CLOSURE, const_idx);
        // Spawn as async task (0 args)
        self.emit_op_u8(OP_ASYNC_CALL, 0);
        // Discard the TaskHandle — fire and forget
        self.emit_op(OP_POP);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // select { case pattern = expr { body }; ... }
    // -----------------------------------------------------------------------

    /// Compile a select statement: evaluate each channel expression in order,
    /// check if it has pending data, and execute the first ready case body.
    /// Falls through (returns Null) if no channel has data.
    fn compile_select(&mut self, select: &SelectStmt) -> CResult<()> {
        if select.cases.is_empty() {
            self.emit_op(OP_NULL);
            self.emit_op(OP_POP);
            return Ok(());
        }

        let end_label = self.new_label();

        for case in &select.cases {
            let next_case_label = self.new_label();

            // Evaluate the channel expression -> stack has [channel]
            self.compile_expr(&case.expr)?;

            // OP_SELECT_TRY_RECV: pops channel from stack.
            // If channel has data, pushes the received value and falls through.
            // If channel has no data, pushes nothing and jumps to next_case_label.
            self.emit_jump(OP_SELECT_TRY_RECV, next_case_label);

            // Body: bind pattern var to the received value and execute body
            self.begin_scope();
            let name = case.pattern.name.clone();
            if self.in_function {
                let slot = self.add_local(&name);
                self.emit_op_u16(OP_STORE_LOCAL, slot as u16);
            } else {
                let name_idx = self.name_constant(&name);
                self.emit_op_u16(OP_DEFINE_GLOBAL, name_idx);
            }
            for stmt in &case.body {
                self.compile_stmt(stmt)?;
            }
            self.end_scope();
            self.emit_jump(OP_JUMP, end_label);

            // Next case checkpoint
            self.place_label(next_case_label);
        }

        // No case matched: push Null as result
        self.emit_op(OP_NULL);

        self.place_label(end_label);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // synchronized { body }
    // -----------------------------------------------------------------------

    /// Compile a synchronized block: mutual exclusion.
    /// In the single-threaded VM this is equivalent to a scoped block.
    fn compile_synchronized(&mut self, sync: &SynchronizedStmt) -> CResult<()> {
        self.begin_scope();
        self.compile_block(&sync.body)?;
        self.end_scope();
        Ok(())
    }

    // ========================================================================
    // Block / scope helpers
    // ========================================================================

    fn compile_block(&mut self, block: &Block) -> CResult<()> {
        for stmt in &block.stmts {
            self.compile_stmt(stmt)?;
        }
        Ok(())
    }

    fn begin_scope(&mut self) {
        self.scope_depth += 1;
    }

    fn end_scope(&mut self) {
        // Pop locals declared at this scope depth
        let depth = self.scope_depth;
        while self
            .locals
            .last()
            .map(|l| l.depth == depth)
            .unwrap_or(false)
        {
            self.locals.pop();
        }
        self.scope_depth = self.scope_depth.saturating_sub(1);
    }

    // ========================================================================
    // Expressions
    // ========================================================================

    fn compile_expr(&mut self, expr: &Expr) -> CResult<()> {
        match expr {
            Expr::Literal(lit) => self.compile_literal(lit),
            Expr::Ident(ident) => self.compile_ident(ident),
            Expr::Binary(bin) => self.compile_binary(bin),
            Expr::Unary(un) => self.compile_unary(un),
            Expr::Call(call) => self.compile_call(call),
            Expr::Index(idx) => self.compile_index(idx),
            Expr::Member(mem) => self.compile_member(mem),
            Expr::Array(arr) => self.compile_array(arr),
            Expr::Object(obj) => self.compile_object(obj),
            Expr::Ternary(tern) => self.compile_ternary(tern),
            Expr::NullCoalesce(nc) => self.compile_null_coalesce(nc),
            Expr::Group(inner) => self.compile_expr(inner),
            Expr::Assignment(assign) => self.compile_assignment(assign),
            Expr::Lambda(lambda) => self.compile_lambda(lambda),
            Expr::Postfix(postfix) => self.compile_postfix(postfix),
            Expr::Parallel(parallel) => self.compile_parallel_expr(parallel),
            Expr::Dollar(_) | Expr::This(_) => {
                self.emit_op(OP_THIS);
                Ok(())
            }
            Expr::New(new_expr) => self.compile_new(new_expr),
            Expr::Super(_) => {
                // super pushes the parent class map via OP_THIS + prototype lookup
                // For now, we'll handle it in compile_call when used as super.method()
                Err(CompileError::new("super only valid in method call position"))
            }
            Expr::Match(match_expr) => self.compile_match(match_expr),
            Expr::Elvis(elvis) => self.compile_elvis(elvis),
            Expr::Pipe(pipe) => self.compile_pipe(pipe),
            Expr::Template(t) => self.compile_template(t),
            Expr::Lazy(inner) => {
                // Compile inner expression as an async lambda and spawn it
                let params: Vec<String> = Vec::new();
                let body = Block { span: inner.span(), stmts: vec![
                    Stmt::Return(ReturnStmt { span: inner.span(), value: Some((**inner).clone()) })
                ]};
                let chunk = self.compile_function_body("<lazy>", &params, &body)?;
                let fn_obj = FnObj {
                    name: "<lazy>".to_string(),
                    arity: 0,
                    chunk,
                    is_async: true,
                };
                let const_idx = self.add_constant(Value::FnObj(fn_obj));
                self.emit_op_u16(OP_MAKE_CLOSURE, const_idx);
                self.emit_op_u8(OP_ASYNC_CALL, 0);
                Ok(())
            }
            _ => Err(CompileError::new("unsupported expression")),
        }
    }

    // -----------------------------------------------------------------------
    // Literal
    // -----------------------------------------------------------------------

    fn compile_literal(&mut self, lit: &Literal) -> CResult<()> {
        match lit {
            Literal::Int(n, _) => {
                let idx = self.add_constant(Value::int_from_i64(*n as i64));
                self.emit_op_u16(OP_CONST, idx);
            }
            Literal::Float(n, _) => {
                let idx = self.add_constant(Value::Float(*n));
                self.emit_op_u16(OP_CONST, idx);
            }
            Literal::String(s, _) => {
                let idx = self.add_constant(Value::String(s.clone()));
                self.emit_op_u16(OP_CONST, idx);
            }
            Literal::Bool(b, _) => {
                if *b {
                    self.emit_op(OP_TRUE);
                } else {
                    self.emit_op(OP_FALSE);
                }
            }
            Literal::Null(_) => self.emit_op(OP_NULL),
            Literal::Char(c, _) => {
                let idx = self.add_constant(Value::String(c.to_string()));
                self.emit_op_u16(OP_CONST, idx);
            }
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Identifier
    // -----------------------------------------------------------------------

    fn compile_ident(&mut self, ident: &Ident) -> CResult<()> {
        let name = &ident.name;
        if self.in_function {
            if let Some(local) = self.resolve_local(name) {
                self.emit_op_u16(OP_LOAD_LOCAL, local.slot as u16);
            } else {
                let name_idx = self.name_constant(name);
                self.emit_op_u16(OP_LOAD_GLOBAL, name_idx);
            }
        } else {
            let name_idx = self.name_constant(name);
            self.emit_op_u16(OP_LOAD_GLOBAL, name_idx);
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Binary
    // -----------------------------------------------------------------------

    fn compile_binary(&mut self, bin: &BinaryExpr) -> CResult<()> {
        use BinaryOp::*;

        match bin.op {
            // Short-circuit AND
            And => {
                self.compile_expr(&bin.left)?;
                let end_label = self.new_label();
                self.emit_jump(OP_POP_JUMP_IF_FALSE, end_label);
                // Left was truthy: discard it (POP_JUMP_IF_FALSE only pops on the
                // false branch) and evaluate the right operand as the result.
                self.emit_op(OP_POP);
                self.compile_expr(&bin.right)?;
                self.place_label(end_label);
                return Ok(());
            }
            // Short-circuit OR
            Or => {
                self.compile_expr(&bin.left)?;
                let end_label = self.new_label();
                self.emit_op(OP_DUP);
                self.emit_jump(OP_JUMP_IF_TRUE, end_label);
                self.emit_op(OP_POP);
                self.compile_expr(&bin.right)?;
                self.place_label(end_label);
                return Ok(());
            }
            // Null-coalesce in binary position
            NullCoalesce => {
                self.compile_expr(&bin.left)?;
                let end_label = self.new_label();
                self.emit_op(OP_DUP);
                self.emit_op(OP_NULL);
                self.emit_op(OP_EQ);
                self.emit_jump(OP_JUMP_IF_FALSE, end_label);
                self.emit_op(OP_POP);
                self.compile_expr(&bin.right)?;
                self.place_label(end_label);
                return Ok(());
            }
            // Assignment in binary position (a = b)
            Assign => {
                return self.compile_binary_assign(&bin.left, &bin.right, None);
            }
            // Compound assignment
            AddAssign => {
                return self.compile_binary_assign(&bin.left, &bin.right, Some(OP_ADD));
            }
            SubAssign => {
                return self.compile_binary_assign(&bin.left, &bin.right, Some(OP_SUB));
            }
            MulAssign => {
                return self.compile_binary_assign(&bin.left, &bin.right, Some(OP_MUL));
            }
            DivAssign => {
                return self.compile_binary_assign(&bin.left, &bin.right, Some(OP_DIV));
            }
            ModAssign => {
                return self.compile_binary_assign(&bin.left, &bin.right, Some(OP_MOD));
            }
            PowAssign => {
                return self.compile_binary_assign(&bin.left, &bin.right, Some(OP_POW));
            }
            // Is type check — only compile left, use constant for type name
            Is => {
                self.compile_expr(&bin.left)?;
                let type_name = match &bin.right {
                    Expr::Literal(Literal::String(s, _)) => s.clone(),
                    _ => "unknown".to_string(),
                };
                let type_idx = self.add_constant(Value::String(type_name));
                self.emit_op_u16(OP_TYPE_IS, type_idx);
                return Ok(());
            }
            _ => {}
        }

        // Regular binary operations
        self.compile_expr(&bin.left)?;
        self.compile_expr(&bin.right)?;

        match bin.op {
            Add => self.emit_op(OP_ADD),
            Sub => self.emit_op(OP_SUB),
            Mul => self.emit_op(OP_MUL),
            Div => self.emit_op(OP_DIV),
            Mod => self.emit_op(OP_MOD),
            Pow => self.emit_op(OP_POW),
            Eq => self.emit_op(OP_EQ),
            Ne => self.emit_op(OP_NE),
            Lt => self.emit_op(OP_LT),
            Gt => self.emit_op(OP_GT),
            Le => self.emit_op(OP_LE),
            Ge => self.emit_op(OP_GE),
            BitAnd => self.emit_op(OP_BIT_AND),
            BitOr => self.emit_op(OP_BIT_OR),
            BitXor => self.emit_op(OP_BIT_XOR),
            Shl => self.emit_op(OP_SHL),
            Shr => self.emit_op(OP_SHR),
            // Ranges: push both ends and build range list
            Range => {
                // Pop right and left, build range by calling builtin range()
                let range_idx = self.name_constant("range");
                self.emit_op_u16(OP_LOAD_GLOBAL, range_idx);
                self.emit_op_u8(OP_CALL, 2); // range(start, end)
            }
            RangeInclusive => {
                // Inclusive: range(start, end + 1)
                let one_idx = self.add_constant(Value::int_from_i64(1));
                self.emit_op_u16(OP_CONST, one_idx);
                self.emit_op(OP_ADD);
                let range_idx = self.name_constant("range");
                self.emit_op_u16(OP_LOAD_GLOBAL, range_idx);
                self.emit_op_u8(OP_CALL, 2);
            }
            Elvis => {
                // Already handled as Expr::Elvis, shouldn't reach here
                return Err(CompileError::new("elvis should be handled as Expr::Elvis"));
            }
            _ => return Err(CompileError::new(format!("unhandled binary op"))),
        }
        Ok(())
    }

    /// Handle simple and compound assignment to an identifier target.
    fn compile_binary_assign(
        &mut self,
        target: &Expr,
        value_expr: &Expr,
        compound_op: Option<u8>,
    ) -> CResult<()> {
        match target {
            Expr::Ident(ident) => {
                let name = &ident.name;
                let is_local = self.in_function && self.resolve_local(name).is_some();

                if let Some(op) = compound_op {
                    // Compound: load current, compute, store
                    if is_local {
                        let local = self.resolve_local(name).unwrap();
                        self.emit_op_u16(OP_LOAD_LOCAL, local.slot as u16);
                    } else {
                        let name_idx = self.name_constant(name);
                        self.emit_op_u16(OP_LOAD_GLOBAL, name_idx);
                    }
                    self.compile_expr(value_expr)?;
                    self.emit_op(op);

                    // Store with value duplication for expression result
                    self.emit_op(OP_DUP);
                    if is_local {
                        let local = self.resolve_local(name).unwrap();
                        self.emit_op_u16(OP_STORE_LOCAL, local.slot as u16);
                    } else {
                        let name_idx = self.name_constant(name);
                        self.emit_op_u16(OP_STORE_GLOBAL, name_idx);
                    }
                } else {
                    // Simple assignment
                    self.compile_expr(value_expr)?;
                    self.emit_op(OP_DUP);
                    if is_local {
                        let local = self.resolve_local(name).unwrap();
                        self.emit_op_u16(OP_STORE_LOCAL, local.slot as u16);
                    } else {
                        let name_idx = self.name_constant(name);
                        self.emit_op_u16(OP_STORE_GLOBAL, name_idx);
                    }
                }
                Ok(())
            }
            Expr::Index(idx_expr) => {
                // a[i] = value  or  a[i] += value.
                //
                // When `a` is a local, emit OP_STORE_INDEX_LOCAL, which mutates
                // the local's Arc<CoW> in place and writes it back (so the
                // mutation is visible). For non-local targets, fall back to
                // OP_STORE_INDEX, which mutates the stack copy (CoW: visible
                // only when the Arc is uniquely owned).
                let local_slot = match &idx_expr.object {
                    Expr::Ident(id) => self.resolve_local(&id.name).map(|l| l.slot),
                    _ => None,
                };
                if let Some(slot) = local_slot {
                    if let Some(op) = compound_op {
                        // read current: [current]
                        self.emit_op_u16(OP_LOAD_LOCAL, slot as u16);
                        self.compile_expr(&idx_expr.index)?;
                        self.emit_op(OP_INDEX);
                        self.compile_expr(value_expr)?;
                        self.emit_op(op); // [result]
                        // store-back: [key, result] -> STORE_INDEX_LOCAL
                        self.compile_expr(&idx_expr.index)?;
                        self.emit_op(OP_SWAP);
                        self.emit_op_u16(OP_STORE_INDEX_LOCAL, slot as u16);
                    } else {
                        self.compile_expr(&idx_expr.index)?;
                        self.compile_expr(value_expr)?;
                        self.emit_op_u16(OP_STORE_INDEX_LOCAL, slot as u16);
                    }
                } else if let Some(op) = compound_op {
                    self.compile_expr(&idx_expr.object)?;
                    self.compile_expr(&idx_expr.index)?;
                    self.emit_op(OP_INDEX);
                    self.compile_expr(value_expr)?;
                    self.emit_op(op); // [result]
                    self.compile_expr(&idx_expr.object)?;
                    self.emit_op(OP_SWAP);
                    self.compile_expr(&idx_expr.index)?;
                    self.emit_op(OP_SWAP);
                    self.emit_op(OP_STORE_INDEX);
                } else {
                    self.compile_expr(&idx_expr.object)?;
                    self.compile_expr(&idx_expr.index)?;
                    self.compile_expr(value_expr)?;
                    self.emit_op(OP_STORE_INDEX);
                }
                Ok(())
            }
            Expr::Member(mem_expr) => {
                let prop_name = &mem_expr.property.name;
                let name_idx = self.name_constant(prop_name);

                let local_slot = match &mem_expr.object {
                    Expr::Ident(id) => self.resolve_local(&id.name).map(|l| l.slot),
                    _ => None,
                };
                if let Some(slot) = local_slot {
                    if let Some(op) = compound_op {
                        self.emit_op_u16(OP_LOAD_LOCAL, slot as u16);
                        self.emit_op_u16(OP_MEMBER, name_idx); // [current]
                        self.compile_expr(value_expr)?;
                        self.emit_op(op); // [result]
                        self.emit_op_u16_u16(OP_STORE_MEMBER_LOCAL, name_idx, slot as u16);
                    } else {
                        self.compile_expr(value_expr)?;
                        self.emit_op_u16_u16(OP_STORE_MEMBER_LOCAL, name_idx, slot as u16);
                    }
                } else if let Some(op) = compound_op {
                    self.compile_expr(&mem_expr.object)?;
                    self.emit_op_u16(OP_MEMBER, name_idx);
                    self.compile_expr(value_expr)?;
                    self.emit_op(op);
                    self.compile_expr(&mem_expr.object)?;
                    self.emit_op(OP_SWAP);
                    self.emit_op_u16(OP_STORE_MEMBER, name_idx);
                } else {
                    self.compile_expr(&mem_expr.object)?;
                    self.compile_expr(value_expr)?;
                    self.emit_op_u16(OP_STORE_MEMBER, name_idx);
                }
                Ok(())
            }
            _ => Err(CompileError::new("invalid assignment target")),
        }
    }

    // -----------------------------------------------------------------------
    // Unary
    // -----------------------------------------------------------------------

    fn compile_unary(&mut self, un: &UnaryExpr) -> CResult<()> {
        match un.op {
            UnaryOp::Typeof => {
                self.compile_expr(&un.expr)?;
                self.emit_op(OP_TYPEOF);
                return Ok(());
            }
            UnaryOp::Await => {
                // If the inner expression is a parallel block, it already emitted AWAITs
                let is_parallel = matches!(&un.expr, Expr::Parallel(_));
                self.compile_expr(&un.expr)?;
                if !is_parallel {
                    self.emit_op(OP_AWAIT);
                }
                return Ok(());
            }
            UnaryOp::Lazy => {
                self.in_lazy = true;
                self.compile_expr(&un.expr)?;
                self.in_lazy = false;
                return Ok(());
            }
            _ => {}
        }
        self.compile_expr(&un.expr)?;
        match un.op {
            UnaryOp::Neg => self.emit_op(OP_NEG),
            UnaryOp::Not => self.emit_op(OP_NOT),
            UnaryOp::BitNot => self.emit_op(OP_BIT_NOT),
            _ => return Err(CompileError::new("unsupported unary op")),
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Call
    // -----------------------------------------------------------------------

    fn compile_call(&mut self, call: &CallExpr) -> CResult<()> {
        // Check if this is a method call: obj.method(...) or super.method(...)
        if let Expr::Member(member) = &call.callee {
            let is_super = matches!(&member.object, Expr::Super(_));
            if is_super {
                // super.method(args) — use OP_SUPER_METHOD
                let name_idx = self.name_constant(&member.property.name);
                let arg_count = call.args.len();
                for arg in &call.args {
                    self.compile_expr(&arg.value)?;
                }
                self.emit_op_u16_u8(OP_SUPER_METHOD, name_idx, arg_count as u8);
            } else {
                // obj.method(args) — use OP_METHOD_CALL
                self.compile_expr(&member.object)?; // push obj first
                let name_idx = self.name_constant(&member.property.name);
                let arg_count = call.args.len();
                for arg in &call.args {
                    self.compile_expr(&arg.value)?;
                }
                self.emit_op_u16_u8(OP_METHOD_CALL, name_idx, arg_count as u8);
            }
        } else {
            self.compile_expr(&call.callee)?;
            let arg_count = call.args.len();
            for arg in &call.args {
                self.compile_expr(&arg.value)?;
            }
            let op = if self.in_lazy { OP_LAZY_CALL } else { OP_CALL };
            self.emit_op_u8(op, arg_count as u8);
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Postfix (? operator)
    // -----------------------------------------------------------------------

    fn compile_postfix(&mut self, postfix: &PostfixExpr) -> CResult<()> {
        match &postfix.op {
            PostfixOp::Question => {
                self.compile_expr(&postfix.object)?;
                self.emit_op(OP_TRY);
                Ok(())
            }
            _ => Err(CompileError::new("unsupported postfix operation")),
        }
    }

    // -----------------------------------------------------------------------
    // Index expression (read)
    // -----------------------------------------------------------------------

    fn compile_index(&mut self, idx: &IndexExpr) -> CResult<()> {
        self.compile_expr(&idx.object)?;
        self.compile_expr(&idx.index)?;
        self.emit_op(OP_INDEX);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Member expression (read)
    // -----------------------------------------------------------------------

    fn compile_member(&mut self, mem: &MemberExpr) -> CResult<()> {
        self.compile_expr(&mem.object)?;
        if mem.optional {
            // a?.b — if object is null, push null; else access member
            let end_label = self.new_label();
            self.emit_op(OP_DUP);
            self.emit_op(OP_NULL);
            self.emit_op(OP_EQ);
            self.emit_jump(OP_JUMP_IF_TRUE, end_label);
            let name_idx = self.name_constant(&mem.property.name);
            self.emit_op_u16(OP_MEMBER, name_idx);
            self.place_label(end_label);
        } else {
            let name_idx = self.name_constant(&mem.property.name);
            self.emit_op_u16(OP_MEMBER, name_idx);
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Array literal
    // -----------------------------------------------------------------------

    fn compile_array(&mut self, arr: &ArrayLiteral) -> CResult<()> {
        let count = arr.elements.len();
        for elem in &arr.elements {
            self.compile_expr(elem)?;
        }
        self.emit_op_u16(OP_BUILD_LIST, count as u16);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Object literal
    // -----------------------------------------------------------------------

    fn compile_object(&mut self, obj: &ObjectLiteral) -> CResult<()> {
        let count = obj.fields.len();
        for field in &obj.fields {
            // Push key
            let key = match &field.key {
                ObjectKey::Ident(ident) => ident.name.clone(),
                ObjectKey::String(s, _) => s.clone(),
            };
            let key_idx = self.add_constant(Value::String(key));
            self.emit_op_u16(OP_CONST, key_idx);
            // Push value
            self.compile_expr(&field.value)?;
        }
        self.emit_op_u16(OP_BUILD_MAP, count as u16);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Ternary (cond ? then : else)
    // -----------------------------------------------------------------------

    fn compile_ternary(&mut self, tern: &TernaryExpr) -> CResult<()> {
        self.compile_expr(&tern.condition)?;
        let else_label = self.new_label();
        let end_label = self.new_label();

        self.emit_jump(OP_JUMP_IF_FALSE, else_label);
        self.compile_expr(&tern.then_expr)?;
        self.emit_jump(OP_JUMP, end_label);

        self.place_label(else_label);
        self.compile_expr(&tern.else_expr)?;

        self.place_label(end_label);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Match expression
    // -----------------------------------------------------------------------

    fn compile_match(&mut self, match_expr: &MatchExpr) -> CResult<()> {
        // Compile the scrutinee and store it in a temporary local.
        // Only valid in function bodies.
        if !self.in_function {
            return Err(CompileError::new(
                "match expressions currently require function scope",
            ));
        }

        let scrut_name = format!("__match_scrut_{}", self.locals.len());
        let scrut_slot = self.add_local(&scrut_name);
        self.compile_expr(&match_expr.scrutinee)?;
        self.emit_op_u16(OP_STORE_LOCAL, scrut_slot as u16);
        // No POP — STORE_LOCAL consumes the top value

        let end_label = self.new_label();
        let mut arm_labels: Vec<Label> = Vec::new();

        // Create a label for each arm (for jump-if-false to target).
        for _ in 0..match_expr.arms.len() {
            arm_labels.push(self.new_label());
        }

        for (i, arm) in match_expr.arms.iter().enumerate() {
            let is_last = i == match_expr.arms.len() - 1;

            // For non-last arms (and non-wildcard last arms), emit pattern check.
            let needs_check = !is_last || !matches!(arm.pattern, Pattern::Wildcard(_));

            if needs_check {
                // Emit the pattern comparison.
                self.compile_pattern_test(scrut_slot, &arm.pattern)?;
                self.emit_jump(OP_JUMP_IF_FALSE, arm_labels[i]);
            }

            // Compile the arm body in a new scope (for pattern bindings).
            self.begin_scope();
            self.compile_pattern_bind(scrut_slot, &arm.pattern)?;
            self.compile_expr(&arm.body)?;
            self.end_scope();

            // Jump to end (skip remaining arms).
            self.emit_jump(OP_JUMP, end_label);

            // Place the label for the next arm to jump here.
            self.place_label(arm_labels[i]);
        }

        // If no arm matched, push Null (fallback).
        self.emit_op(OP_NULL);

        self.place_label(end_label);
        Ok(())
    }

    /// Emit instructions to test whether the scrutinee value (in `scrut_slot`)
    /// matches a pattern. Leaves the result on the stack as a bool.
    fn compile_pattern_test(&mut self, scrut_slot: usize, pattern: &Pattern) -> CResult<()> {
        match pattern {
            Pattern::Literal(lit) => {
                // Load scrutinee, load literal, compare.
                self.emit_op_u16(OP_LOAD_LOCAL, scrut_slot as u16);
                match lit {
                    Literal::Int(n, _) => {
                        let idx = self.add_constant(Value::int_from_i64(*n as i64));
                        self.emit_op_u16(OP_CONST, idx);
                    }
                    Literal::Float(n, _) => {
                        let idx = self.add_constant(Value::Float(*n));
                        self.emit_op_u16(OP_CONST, idx);
                    }
                    Literal::String(s, _) => {
                        let idx = self.add_constant(Value::String(s.clone()));
                        self.emit_op_u16(OP_CONST, idx);
                    }
                    Literal::Bool(b, _) => {
                        if *b {
                            self.emit_op(OP_TRUE);
                        } else {
                            self.emit_op(OP_FALSE);
                        }
                    }
                    Literal::Null(_) => self.emit_op(OP_NULL),
                    Literal::Char(c, _) => {
                        let idx = self.add_constant(Value::String(c.to_string()));
                        self.emit_op_u16(OP_CONST, idx);
                    }
                }
                self.emit_op(OP_EQ);
            }
            Pattern::Ident(_) | Pattern::Wildcard(_) => {
                // Always matches — push true.
                self.emit_op(OP_TRUE);
            }
            Pattern::IsType(_) => {
                return Err(CompileError::new(
                    "is-type patterns not yet supported in bytecode compiler",
                ));
            }
        }
        Ok(())
    }

    /// Emit instructions to bind pattern variables in the current scope.
    fn compile_pattern_bind(&mut self, scrut_slot: usize, pattern: &Pattern) -> CResult<()> {
        match pattern {
            Pattern::Ident(ident) => {
                // Bind the scrutinee value to a new local.
                let local_slot = self.add_local(&ident.name);
                self.emit_op_u16(OP_LOAD_LOCAL, scrut_slot as u16);
                self.emit_op_u16(OP_STORE_LOCAL, local_slot as u16);
            }
            Pattern::Literal(_) | Pattern::IsType(_) | Pattern::Wildcard(_) => {
                // No binding needed.
            }
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Null coalesce (a ?? b)
    // -----------------------------------------------------------------------

    fn compile_null_coalesce(&mut self, nc: &NullCoalesceExpr) -> CResult<()> {
        self.compile_expr(&nc.left)?;
        let end_label = self.new_label();
        self.emit_op(OP_DUP);
        self.emit_op(OP_NULL);
        self.emit_op(OP_EQ);
        self.emit_jump(OP_JUMP_IF_FALSE, end_label);
        self.emit_op(OP_POP);
        self.compile_expr(&nc.right)?;
        self.place_label(end_label);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Pipe operator (a |> f)
    // -----------------------------------------------------------------------

    fn compile_pipe(&mut self, pipe: &PipeExpr) -> CResult<()> {
        // Compile left, then call right with left as first arg.
        // |>: left |> right → right(left)
        self.compile_expr(&pipe.left)?;
        self.emit_op(OP_DUP); // keep copy for $$
        self.compile_expr(&pipe.right)?;
        self.emit_op_u8(OP_CALL, 1); // call right(left)
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Elvis operator (a ?: b) — short-circuit on truthy
    // -----------------------------------------------------------------------

    fn compile_elvis(&mut self, elvis: &ElvisExpr) -> CResult<()> {
        // Evaluate left, if truthy keep it, else evaluate right
        self.compile_expr(&elvis.left)?;
        let end_label = self.new_label();
        self.emit_op(OP_DUP);
        self.emit_jump(OP_JUMP_IF_TRUE, end_label);
        self.emit_op(OP_POP);
        self.compile_expr(&elvis.right)?;
        self.place_label(end_label);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Template literal: "text ${expr} more text"
    // -----------------------------------------------------------------------

    fn compile_template(&mut self, t: &TemplateExpr) -> CResult<()> {
        // Compile as string concatenation of static parts and toString'd expressions
        let mut first = true;
        for part in &t.parts {
            match part {
                TemplatePart::Static(s) => {
                    if first {
                        first = false;
                    } else {
                        // String concat with previous part
                        self.emit_op(OP_ADD);
                    }
                    let idx = self.add_constant(Value::String(s.clone()));
                    self.emit_op_u16(OP_CONST, idx);
                }
                TemplatePart::Expr(e) => {
                    if first {
                        first = false;
                    } else {
                        self.emit_op(OP_ADD);
                    }
                    self.compile_expr(e)?;
                    // Convert to string via builtin: toString()
                    let to_string_idx = self.name_constant("toString");
                    self.emit_op_u16(OP_LOAD_GLOBAL, to_string_idx);
                    self.emit_op_u8(OP_CALL, 1);
                }
            }
        }
        if first {
            // Empty template: ""
            let idx = self.add_constant(Value::String(String::new()));
            self.emit_op_u16(OP_CONST, idx);
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Assignment expression
    // -----------------------------------------------------------------------

    fn compile_assignment(&mut self, assign: &AssignmentExpr) -> CResult<()> {
        let op = match assign.op {
            AssignmentOp::Assign => None,
            AssignmentOp::AddAssign => Some(OP_ADD),
            AssignmentOp::SubAssign => Some(OP_SUB),
            AssignmentOp::MulAssign => Some(OP_MUL),
            AssignmentOp::DivAssign => Some(OP_DIV),
            AssignmentOp::ModAssign => Some(OP_MOD),
            AssignmentOp::PowAssign => Some(OP_POW),
            _ => return Err(CompileError::new("unsupported assignment op")),
        };
        self.compile_binary_assign(&assign.target, &assign.value, op)
    }

    // -----------------------------------------------------------------------
    // Lambda (arrow function)
    // -----------------------------------------------------------------------

    fn compile_lambda(&mut self, lambda: &Lambda) -> CResult<()> {
        let params: Vec<String> = lambda.params.iter().map(|p| p.name.name.clone()).collect();
        let arity = params.len();

        // Extract the body block
        let body = match &lambda.body {
            LambdaBody::Block(block) => block.clone(),
            LambdaBody::Expr(expr) => Block {
                span: lambda.span,
                stmts: vec![Stmt::Return(ReturnStmt {
                    span: lambda.span,
                    value: Some(expr.clone()),
                })],
            },
        };

        // Compile the lambda body as a separate function
        let chunk = self.compile_function_body("<lambda>", &params, &body)?;
        let fn_obj = FnObj {
            name: "<lambda>".to_string(),
            arity,
            chunk,
            is_async: lambda.is_async,
        };
        let const_idx = self.add_constant(Value::FnObj(fn_obj));
        self.emit_op_u16(OP_MAKE_CLOSURE, const_idx);
        Ok(())
    }

    // ========================================================================
    // OOP compilation
    // ========================================================================

    /// Compile a `new ClassName(args)` expression.
    fn compile_new(&mut self, new_expr: &NewExpr) -> CResult<()> {
        // Push class value
        let name_idx = self.name_constant(&new_expr.type_name.name);
        if self.in_function {
            if let Some(local) = self.resolve_local(&new_expr.type_name.name) {
                self.emit_op_u16(OP_LOAD_LOCAL, local.slot as u16);
            } else {
                self.emit_op_u16(OP_LOAD_GLOBAL, name_idx);
            }
        } else {
            self.emit_op_u16(OP_LOAD_GLOBAL, name_idx);
        }
        // Compile constructor args
        let arg_count = new_expr.args.len();
        for arg in &new_expr.args {
            self.compile_expr(&arg.value)?;
        }
        self.emit_op_u8(OP_NEW, arg_count as u8);
        Ok(())
    }

    /// Compile a class declaration into a runtime map constant.
    fn compile_class_decl(&mut self, class_decl: &ClassDecl) -> CResult<()> {
        let class_name = &class_decl.name.name;
        let mut values: Vec<(String, Value)> = Vec::new();

        // __class__ marker
        values.push(("__class__".to_string(), Value::String(class_name.clone())));

        // extends — store parent class name for runtime resolution
        if let Some(parent) = &class_decl.extends {
            if let Type::Named(named) = parent {
                values.push(("__parent_name__".to_string(), Value::String(named.name.name.clone())));
            }
        }

        // implements — store interface names for runtime validation
        if !class_decl.implements.is_empty() {
            let iface_names: Vec<String> = class_decl.implements.iter()
                .filter_map(|t| match t { Type::Named(n) => Some(n.name.name.clone()), _ => None })
                .collect();
            if !iface_names.is_empty() {
                values.push(("__implements__".to_string(), Value::String(iface_names.join(","))));
            }
        }

        // use traits — store trait names for runtime mixin
        let mut trait_names: Vec<String> = Vec::new();
        for member in &class_decl.members {
            if let ClassMember::UseTrait(use_trait) = member {
                for t in &use_trait.traits {
                    trait_names.push(t.name.clone());
                }
            }
        }
        if !trait_names.is_empty() {
            values.push(("__use_traits__".to_string(), Value::String(trait_names.join(","))));
        }

        // Members: constructor, methods, properties
        for member in &class_decl.members {
            match member {
                ClassMember::Constructor(ctor) => {
                    let params: Vec<String> = ctor.params.iter().map(|p| p.name.name.clone()).collect();
                    let ctor_chunk = self.compile_function_body(
                        &format!("{}.constructor", class_name),
                        &params,
                        &ctor.body,
                    )?;
                    values.push(("__constructor__".to_string(), Value::FnObj(FnObj {
                        name: format!("{}.constructor", class_name),
                        arity: params.len(),
                        chunk: ctor_chunk,
                        is_async: false,
                    })));
                }
                ClassMember::Method(method) => {
                    let params: Vec<String> = method.params.iter().map(|p| p.name.name.clone()).collect();
                    let meth_chunk = self.compile_function_body(
                        &format!("{}.{}", class_name, method.name.name),
                        &params,
                        &method.body,
                    )?;
                    let is_static = method.modifiers.contains(&Modifier::Static);
                    values.push((method.name.name.clone(), Value::FnObj(FnObj {
                        name: format!("{}.{}", class_name, method.name.name),
                        arity: params.len(),
                        chunk: meth_chunk,
                        is_async: method.is_async,
                    })));
                    // Store static methods too
                    if !is_static {
                        // Non-static methods are already stored above
                    }
                    if !method.modifiers.is_empty() {
                        let mod_str = method.modifiers.iter()
                            .map(|m| format!("{:?}", m))
                            .collect::<Vec<_>>().join(",");
                        values.push((format!("__mod_{}", method.name.name), Value::String(mod_str)));
                    }
                }
                ClassMember::Property(prop) => {
                    // Store property default as __prop_<name>
                    let prop_key = format!("__prop_{}", prop.name.name);
                    let default_val = Value::Null; // Default values resolved at runtime
                    values.push((prop_key, default_val));
                    if !prop.modifiers.is_empty() {
                        let mod_str = prop.modifiers.iter()
                            .map(|m| format!("{:?}", m))
                            .collect::<Vec<_>>().join(",");
                        values.push((format!("__modprop_{}", prop.name.name), Value::String(mod_str)));
                    }
                }
                ClassMember::UseTrait(_) => {} // handled above
            }
        }

        // Push pairs of [key, value] for BUILD_MAP
        for (key, val) in &values {
            let key_idx = self.add_constant(Value::String(key.clone()));
            self.emit_op_u16(OP_CONST, key_idx);
            match val {
                Value::FnObj(fn_obj) => {
                    let val_idx = self.add_constant(Value::FnObj(fn_obj.clone()));
                    self.emit_op_u16(OP_MAKE_CLOSURE, val_idx);
                }
                Value::String(s) => {
                    let val_idx = self.add_constant(Value::String(s.clone()));
                    self.emit_op_u16(OP_CONST, val_idx);
                }
                Value::Null => {
                    self.emit_op(OP_NULL);
                }
                _ => {}
            }
        }

        let pair_count = values.len();
        self.emit_op_u16(OP_BUILD_MAP, pair_count as u16);

        let name_idx = self.name_constant(class_name);
        self.emit_op_u16(OP_DEFINE_GLOBAL, name_idx);
        Ok(())
    }

    /// Compile a trait declaration — similar to class but with only methods.
    fn compile_trait_decl(&mut self, trait_decl: &TraitDecl) -> CResult<()> {
        let mut values: Vec<(String, Value)> = Vec::new();

        values.push(("__trait__".to_string(), Value::String(trait_decl.name.name.clone())));

        for member in &trait_decl.members {
            if let TraitMember::Method(method) = member {
                let params: Vec<String> = method.params.iter().map(|p| p.name.name.clone()).collect();
                let meth_chunk = self.compile_function_body(
                    &format!("{}.{}", trait_decl.name.name, method.name.name),
                    &params,
                    &method.body,
                )?;
                let fn_obj = FnObj {
                    name: format!("{}.{}", trait_decl.name.name, method.name.name),
                    arity: params.len(),
                    chunk: meth_chunk,
                    is_async: method.is_async,
                };
                values.push((method.name.name.clone(), Value::FnObj(fn_obj)));
            }
        }

        for (key, val) in &values {
            let key_idx = self.add_constant(Value::String(key.clone()));
            self.emit_op_u16(OP_CONST, key_idx);
            if let Value::FnObj(fn_obj) = val {
                let val_idx = self.add_constant(Value::FnObj(fn_obj.clone()));
                self.emit_op_u16(OP_MAKE_CLOSURE, val_idx);
            }
        }

        let pair_count = values.len();
        self.emit_op_u16(OP_BUILD_MAP, pair_count as u16);

        let name_idx = self.name_constant(&trait_decl.name.name);
        self.emit_op_u16(OP_DEFINE_GLOBAL, name_idx);
        Ok(())
    }

    /// Compile an interface declaration — store method signatures as a map.
    fn compile_interface_decl(&mut self, iface_decl: &InterfaceDecl) -> CResult<()> {
        let mut values: Vec<(String, Value)> = Vec::new();

        values.push(("__interface__".to_string(), Value::String(iface_decl.name.name.clone())));

        if let Some(parent) = &iface_decl.extends {
            if let Type::Named(named) = parent {
                values.push(("__extends__".to_string(), Value::String(named.name.name.clone())));
            }
        }

        for member in &iface_decl.members {
            match member {
                InterfaceMember::MethodSignature(sig) => {
                    values.push((sig.name.name.clone(), Value::String("method".to_string())));
                }
                InterfaceMember::PropertySignature(sig) => {
                    values.push((format!("__prop_{}", sig.name.name), Value::String("property".to_string())));
                }
            }
        }

        for (key, val) in &values {
            let key_idx = self.add_constant(Value::String(key.clone()));
            self.emit_op_u16(OP_CONST, key_idx);
            let val_idx = if let Value::String(s) = val {
                self.add_constant(Value::String(s.clone()))
            } else {
                self.add_constant(Value::Null)
            };
            self.emit_op_u16(OP_CONST, val_idx);
        }

        self.emit_op_u16(OP_BUILD_MAP, values.len() as u16);
        let name_idx = self.name_constant(&iface_decl.name.name);
        self.emit_op_u16(OP_DEFINE_GLOBAL, name_idx);
        Ok(())
    }

    // ========================================================================
    // Function compilation
    // ========================================================================

    /// Pre-declare a function so it can be called before its definition.
    fn declare_function(&mut self, fn_decl: &FnDecl) -> CResult<()> {
        let name = fn_decl.name.name.clone();
        let params: Vec<String> = fn_decl.params.iter().map(|p| p.name.name.clone()).collect();
        let arity = params.len();

        let chunk = self.compile_function_body(&name, &params, &fn_decl.body)?;
        let fn_obj = FnObj {
            name: name.clone(),
            arity,
            chunk,
            is_async: fn_decl.is_async,
        };

        let const_idx = self.add_constant(Value::FnObj(fn_obj));
        self.emit_op_u16(OP_MAKE_CLOSURE, const_idx);

        if self.in_function {
            // Nested function: bind the closure as a local variable. Both
            // OP_MAKE_CLOSURE (push) and OP_STORE_LOCAL (pop) are balanced, so
            // no extra OP_POP is needed — the closure value is consumed by the
            // store. (An earlier spurious POP here corrupted the caller's stack.)
            let slot = self.add_local(&name);
            self.emit_op_u16(OP_STORE_LOCAL, slot as u16);
        } else {
            let name_idx = self.name_constant(&name);
            self.emit_op_u16(OP_DEFINE_GLOBAL, name_idx);
        }
        Ok(())
    }

    /// Compile a function body into its own Chunk.
    fn compile_function_body(
        &self,
        _name: &str,
        params: &[String],
        body: &Block,
    ) -> CResult<Chunk> {
        // Create a fresh compiler for the function body.
        let mut func_compiler = Compiler::new();
        func_compiler.in_function = true;
        func_compiler.begin_scope();

        // Parameters are the first local slots
        for param in params {
            func_compiler.add_local(param);
        }

        func_compiler.compile_block(body)?;

        // Implicit return null at end of function
        func_compiler.emit_op(OP_NULL);
        func_compiler.emit_op(OP_RETURN);

        func_compiler.end_scope();
        Ok(func_compiler.finish_chunk())
    }

    // ========================================================================
    // Helpers
    // ========================================================================

    /// Add a local variable and return its slot index.
    fn add_local(&mut self, name: &str) -> usize {
        let slot = self.locals.len();
        self.locals.push(Local {
            name: name.to_string(),
            slot,
            depth: self.scope_depth,
        });
        slot
    }

    /// Resolve a local variable by name (innermost first).
    fn resolve_local(&self, name: &str) -> Option<&Local> {
        self.locals.iter().rev().find(|l| l.name == name)
    }

    /// Get or create a constant index for a string name.
    fn name_constant(&mut self, name: &str) -> u16 {
        self.add_constant(Value::String(name.to_string()))
    }

    // Delegates to ChunkBuilder
    fn new_label(&mut self) -> Label {
        self.builder.new_label()
    }
    fn place_label(&mut self, label: Label) {
        self.builder.place_label(label);
    }
    fn emit_op(&mut self, op: u8) {
        self.builder.emit_op(op);
    }
    fn emit_op_u16_u16(&mut self, op: u8, a: u16, b: u16) {
        self.builder.emit_op_u16_u16(op, a, b);
    }
    fn emit_op_u16(&mut self, op: u8, val: u16) {
        self.builder.emit_op_u16(op, val);
    }
    fn emit_op_u8(&mut self, op: u8, val: u8) {
        self.builder.emit_op_u8(op, val);
    }
    fn emit_op_u16_u8(&mut self, op: u8, val16: u16, val8: u8) {
        self.builder.emit_op_u16_u8(op, val16, val8);
    }
    fn emit_jump(&mut self, jump_op: u8, label: Label) {
        self.builder.emit_jump(jump_op, label);
    }
    fn emit_loop(&mut self, label: Label) {
        self.builder.emit_loop(label);
    }
    fn add_constant(&mut self, value: Value) -> u16 {
        self.builder.add_constant(value)
    }
    fn finish_chunk(&mut self) -> Chunk {
        // Take the builder and replace with a fresh one
        std::mem::replace(&mut self.builder, ChunkBuilder::new()).finish()
    }
}

impl Default for Compiler {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::disassemble;
    use coco_parser::Parser;

    fn compile_src(src: &str) -> Chunk {
        let mut parser = Parser::new(src);
        let program = parser.parse_program();
        let mut compiler = Compiler::new();
        compiler.compile_script(&program).unwrap()
    }

    #[test]
    fn test_compile_literal_int() {
        let chunk = compile_src("42;");
        let d = disassemble(&chunk, "test");
        assert!(d.contains("CONST"));
    }

    #[test]
    fn test_compile_literal_string() {
        let chunk = compile_src("\"hello\";");
        let d = disassemble(&chunk, "test");
        assert!(d.contains("CONST"));
        assert!(d.contains("hello"));
    }

    #[test]
    fn test_compile_add() {
        let chunk = compile_src("1 + 2;");
        let d = disassemble(&chunk, "test");
        assert!(d.contains("ADD"));
    }

    #[test]
    fn test_compile_if() {
        let chunk = compile_src("let x = 0; if true { x = 1; }");
        let d = disassemble(&chunk, "test");
        assert!(d.contains("JUMP_IF_FALSE"));
    }

    #[test]
    fn test_compile_function() {
        let chunk = compile_src("fn add(a, b) { return a + b; }");
        let d = disassemble(&chunk, "test");
        assert!(d.contains("MAKE_CLOSURE"));
        assert!(d.contains("DEFINE_GLOBAL"));
    }

    #[test]
    fn test_compile_while() {
        let chunk = compile_src("let x = 0; while x < 5 { x += 1; }");
        let d = disassemble(&chunk, "test");
        assert!(d.contains("LOOP"));
        assert!(d.contains("JUMP_IF_FALSE"));
    }

    #[test]
    fn test_compile_list() {
        let chunk = compile_src("[1, 2, 3];");
        let d = disassemble(&chunk, "test");
        assert!(d.contains("BUILD_LIST"));
    }

    #[test]
    fn test_compile_lambda() {
        let chunk = compile_src("const x = () => 1;");
        let d = disassemble(&chunk, "test");
        assert!(d.contains("MAKE_CLOSURE"));
        // The FnObj constant is stored in the pool; check via the chunk
        let has_lambda = chunk
            .constants
            .iter()
            .any(|c| matches!(c, Value::FnObj(f) if f.name == "<lambda>"));
        assert!(has_lambda, "expected <lambda> FnObj in constant pool");
    }

    #[test]
    fn test_compile_ternary_disasm() {
        let chunk = compile_src("fn main() { let x = true ? 1 : 2; return x; }");
        for c in &chunk.constants {
            if let Value::FnObj(fo) = c {
                let d = disassemble(&fo.chunk, &fo.name);
                assert!(d.contains("JUMP_IF_FALSE"));
                assert!(d.contains("JUMP"));
            }
        }
    }
}
