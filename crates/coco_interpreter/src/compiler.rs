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
    OP_LOAD_GLOBAL, OP_LOAD_LOCAL, OP_LT, OP_MAKE_CLOSURE, OP_MEMBER, OP_MOD, OP_MUL, OP_NE,
    OP_NEG, OP_NOT, OP_NULL, OP_POP, OP_POP_JUMP_IF_FALSE, OP_POW, OP_RETURN, OP_SHL, OP_SHR,
    OP_STORE_GLOBAL, OP_STORE_INDEX, OP_STORE_LOCAL, OP_STORE_MEMBER, OP_SUB, OP_THROW, OP_TRUE,
    OP_TRY_BEGIN, OP_TRY_END, OP_AWAIT, OP_LAZY_CALL, OP_TRY,
};
use crate::value::Value;
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
            if let Item::FnDecl(fn_decl) = item {
                self.declare_function(fn_decl)?;
            }
        }
        // Second pass: compile each item.
        for item in &program.items {
            self.compile_item(item)?;
        }
        // Ensure the script returns something.
        self.emit_op(OP_NULL);
        self.emit_op(OP_RETURN);
        Ok(self.finish_chunk())
    }

    // ========================================================================
    // Items
    // ========================================================================

    fn compile_item(&mut self, item: &Item) -> CResult<()> {
        match item {
            Item::FnDecl(_fn_decl) => {
                // Already declared in first pass; skip re-compilation here
                // unless we want to support redefinition. For now, first pass
                // handles it.
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
            _ => Ok(()), // classes, traits, etc. — deferred
        }
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
            Stmt::Loop(loop_stmt) => self.compile_loop(loop_stmt),
            Stmt::Return(ret) => self.compile_return(ret),
            Stmt::Break(_) => self.compile_break(),
            Stmt::Continue(_) => self.compile_continue(),
            Stmt::Throw(throw) => self.compile_throw(throw),
            Stmt::Try(try_stmt) => self.compile_try(try_stmt),
            Stmt::Parallel(parallel) => self.compile_parallel(parallel),
            Stmt::Coro(coro) => self.compile_coro(coro),
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

        // Compile iterable and store
        self.compile_expr(&for_stmt.iterable)?;
        self.emit_op_u16(OP_STORE_LOCAL, iter_slot as u16);
        self.emit_op(OP_POP);

        // Initialize index to 0
        self.emit_op(OP_NULL); // fallthrough: OP_CONST 0 would need a constant
        self.emit_op(OP_POP);
        let zero_idx = self.add_constant(Value::Int(BigInt::from(0)));
        self.emit_op_u16(OP_CONST, zero_idx);
        self.emit_op_u16(OP_STORE_LOCAL, idx_slot as u16);
        self.emit_op(OP_POP);

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
        self.emit_op(OP_POP);

        // Body
        self.begin_scope();
        self.compile_block(&for_stmt.body)?;
        self.end_scope();

        // Increment index
        self.emit_op_u16(OP_LOAD_LOCAL, idx_slot as u16);
        let one_idx = self.add_constant(Value::Int(BigInt::from(1)));
        self.emit_op_u16(OP_CONST, one_idx);
        self.emit_op(OP_ADD);
        self.emit_op_u16(OP_STORE_LOCAL, idx_slot as u16);
        self.emit_op(OP_POP);

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
    // parallel { run expr; ... }
    // -----------------------------------------------------------------------

    fn compile_parallel(&mut self, parallel: &ParallelStmt) -> CResult<()> {
        let count = parallel.runs.len();
        if count == 0 {
            return Ok(());
        }

        // Compile each run — the call expression returns a TaskHandle.
        for run in &parallel.runs {
            self.compile_expr(&run.expr)?;
        }

        // Await all results in reverse order (stack discipline).
        // Each OP_AWAIT pops a TaskHandle, suspends if needed, pushes result.
        for _ in 0..count {
            self.emit_op(OP_AWAIT);
        }

        // The results are now on the stack in order of the runs.
        Ok(())
    }

    // -----------------------------------------------------------------------
    // coro { body }
    // -----------------------------------------------------------------------

    fn compile_coro(&mut self, _coro: &CoroStmt) -> CResult<()> {
        // For fire-and-forget coroutines, we compile the body as an
        // immediately-invoked async lambda and discard the handle.
        // The body scope is already handled by the parser.

        // We emit a simple approach: push a null placeholder since coro
        // is fire-and-forget and doesn't return a value to the caller.
        // Future: compile body as a separate task and spawn it.
        self.emit_op(OP_NULL);
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
            _ => Err(CompileError::new("unsupported expression")),
        }
    }

    // -----------------------------------------------------------------------
    // Literal
    // -----------------------------------------------------------------------

    fn compile_literal(&mut self, lit: &Literal) -> CResult<()> {
        match lit {
            Literal::Int(n, _) => {
                let idx = self.add_constant(Value::Int(BigInt::from(*n as i64)));
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
                // a[i] = value  or  a[i] += value
                self.compile_expr(&idx_expr.object)?;
                self.compile_expr(&idx_expr.index)?;

                if let Some(op) = compound_op {
                    // Get current: stack is [obj, key]
                    self.emit_op(OP_DUP);
                    self.emit_op(OP_INDEX); // [obj, key, current]
                    // compile RHS: [obj, key, current, rhs]
                    self.compile_expr(value_expr)?;
                    // apply op: [obj, key, result]
                    self.emit_op(op);
                } else {
                    // Simple: [obj, key, value]
                    self.compile_expr(value_expr)?;
                }

                // Dup result: [obj, key, result, result]
                self.emit_op(OP_DUP);
                // Store: pops (result_copy, key, obj), leaves [result]
                self.emit_op(OP_STORE_INDEX);
                Ok(())
            }
            Expr::Member(mem_expr) => {
                let prop_name = &mem_expr.property.name;
                let name_idx = self.name_constant(prop_name);

                self.compile_expr(&mem_expr.object)?;

                if let Some(op) = compound_op {
                    // Get current
                    self.emit_op(OP_DUP);
                    self.emit_op_u16(OP_MEMBER, name_idx);
                    self.compile_expr(value_expr)?;
                    self.emit_op(op);
                } else {
                    self.compile_expr(value_expr)?;
                }

                self.emit_op(OP_DUP);
                self.emit_op_u16(OP_STORE_MEMBER, name_idx);
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
            UnaryOp::Await => {
                self.compile_expr(&un.expr)?;
                self.emit_op(OP_AWAIT);
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
        self.compile_expr(&call.callee)?;
        let arg_count = call.args.len();
        for arg in &call.args {
            self.compile_expr(&arg.value)?;
        }
        let op = if self.in_lazy { OP_LAZY_CALL } else { OP_CALL };
        self.emit_op_u8(op, arg_count as u8);
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
        let name_idx = self.name_constant(&mem.property.name);
        self.emit_op_u16(OP_MEMBER, name_idx);
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
            let slot = self.add_local(&name);
            self.emit_op_u16(OP_STORE_LOCAL, slot as u16);
            self.emit_op(OP_POP);
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
    fn emit_op_u16(&mut self, op: u8, val: u16) {
        self.builder.emit_op_u16(op, val);
    }
    fn emit_op_u8(&mut self, op: u8, val: u8) {
        self.builder.emit_op_u8(op, val);
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
