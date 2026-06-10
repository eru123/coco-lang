use coco_syntax::*;

use crate::error::{ControlFlow, IResult, RuntimeError, Signal};
use crate::value::Value;
use crate::Interpreter;

impl Interpreter {
    /// Execute a statement and return the last expression value (or Null).
    pub(crate) fn exec_stmt(&mut self, stmt: &Stmt) -> IResult {
        match stmt {
            Stmt::Item(item) => self.exec_item(item),
            Stmt::Expr(expr_stmt) => self.eval_expr(&expr_stmt.expr),
            Stmt::If(if_stmt) => self.exec_if(if_stmt),
            Stmt::For(for_stmt) => self.exec_for(for_stmt),
            Stmt::While(while_stmt) => self.exec_while(while_stmt),
            Stmt::Loop(loop_stmt) => self.exec_loop(loop_stmt),
            Stmt::Return(ret) => self.exec_return(ret),
            Stmt::Break(_) => Err(Signal::Flow(ControlFlow::Break)),
            Stmt::Continue(_) => Err(Signal::Flow(ControlFlow::Continue)),
            Stmt::Throw(throw) => self.exec_throw(throw),
            Stmt::Try(try_stmt) => self.exec_try(try_stmt),
            Stmt::Parallel(parallel) => {
                // Execute all runs sequentially in the tree-walking interpreter
                let mut last = Value::Null;
                for run in &parallel.runs {
                    last = self.eval_expr(&run.expr)?;
                }
                Ok(last)
            }
            Stmt::Coro(_coro) => {
                // Fire-and-forget coroutine in the tree-walking interpreter.
                // Execute body in a new scope, return Null regardless.
                self.env.push_scope();
                let _result = self.exec_block(&_coro.body);
                self.env.pop_scope();
                Ok(Value::Null)
            }
            Stmt::Select(select) => self.exec_select(select),
            Stmt::Synchronized(sync) => self.exec_synchronized(sync),
            _ => {
                // Log unhandled statement variants
                eprintln!("[exec_stmt] unhandled statement variant");
                Ok(Value::Null)
            },
        }
    }

    /// Execute a block of statements. Returns the value of the last statement.
    pub(crate) fn exec_block(&mut self, block: &Block) -> IResult {
        let mut last = Value::Null;
        for stmt in &block.stmts {
            last = self.exec_stmt(stmt)?;
        }
        Ok(last)
    }

    fn exec_if(&mut self, if_stmt: &IfStmt) -> IResult {
        let cond = self.eval_expr(&if_stmt.condition)?;
        if cond.is_truthy() {
            self.env.push_scope();
            let result = self.exec_block(&if_stmt.then_block);
            self.env.pop_scope();
            return result;
        }

        // Check else-if branches
        for else_if in &if_stmt.else_ifs {
            let cond = self.eval_expr(&else_if.condition)?;
            if cond.is_truthy() {
                self.env.push_scope();
                let result = self.exec_block(&else_if.block);
                self.env.pop_scope();
                return result;
            }
        }

        // Else branch
        if let Some(else_block) = &if_stmt.else_block {
            self.env.push_scope();
            let result = self.exec_block(else_block);
            self.env.pop_scope();
            return result;
        }

        Ok(Value::Null)
    }

    fn exec_for(&mut self, for_stmt: &ForStmt) -> IResult {
        let iterable = self.eval_expr(&for_stmt.iterable)?;

        self.env.push_scope();
        // Define the loop variable
        self.env
            .define(for_stmt.pattern.name.clone(), Value::Null, true);

        let mut last = Value::Null;

        match &iterable {
            Value::List(items) => {
                let list_len = items.data.len();
                for i in 0..list_len {
                    let item = items.data[i].clone();
                    self.env
                        .set(&for_stmt.pattern.name, item.clone())
                        .map_err(|e| Signal::Error(RuntimeError::new(e)))?;

                    match self.exec_block(&for_stmt.body) {
                        Ok(val) => last = val,
                        Err(Signal::Flow(ControlFlow::Break)) => break,
                        Err(Signal::Flow(ControlFlow::Continue)) => continue,
                        Err(e) => {
                            self.env.pop_scope();
                            return Err(e);
                        }
                    }
                }
            }
            Value::Map(map) => {
                // Iterate over keys and bind each key as a string value.
                // Collect keys first to avoid borrow issues during iteration.
                let keys: Vec<String> = map.data.keys().cloned().collect();
                for key in keys {
                    let item = Value::String(key);
                    self.env
                        .set(&for_stmt.pattern.name, item.clone())
                        .map_err(|e| Signal::Error(RuntimeError::new(e)))?;

                    match self.exec_block(&for_stmt.body) {
                        Ok(val) => last = val,
                        Err(Signal::Flow(ControlFlow::Break)) => break,
                        Err(Signal::Flow(ControlFlow::Continue)) => continue,
                        Err(e) => {
                            self.env.pop_scope();
                            return Err(e);
                        }
                    }
                }
            }
            _ => {
                self.env.pop_scope();
                return Err(Signal::Error(RuntimeError::new(
                    "for-in requires a list or map to iterate",
                )));
            }
        }

        self.env.pop_scope();
        Ok(last)
    }

    fn exec_while(&mut self, while_stmt: &WhileStmt) -> IResult {
        let mut last = Value::Null;
        loop {
            let cond = self.eval_expr(&while_stmt.condition)?;
            if !cond.is_truthy() {
                break;
            }
            self.env.push_scope();
            match self.exec_block(&while_stmt.body) {
                Ok(val) => last = val,
                Err(Signal::Flow(ControlFlow::Break)) => {
                    self.env.pop_scope();
                    break;
                }
                Err(Signal::Flow(ControlFlow::Continue)) => {
                    self.env.pop_scope();
                    continue;
                }
                Err(e) => {
                    self.env.pop_scope();
                    return Err(e);
                }
            }
            self.env.pop_scope();
        }
        Ok(last)
    }

    fn exec_loop(&mut self, loop_stmt: &LoopStmt) -> IResult {
        let mut last = Value::Null;
        loop {
            self.env.push_scope();
            match self.exec_block(&loop_stmt.body) {
                Ok(val) => last = val,
                Err(Signal::Flow(ControlFlow::Break)) => {
                    self.env.pop_scope();
                    break;
                }
                Err(Signal::Flow(ControlFlow::Continue)) => {
                    self.env.pop_scope();
                    continue;
                }
                Err(e) => {
                    self.env.pop_scope();
                    return Err(e);
                }
            }
            self.env.pop_scope();
        }
        Ok(last)
    }

    fn exec_return(&mut self, ret: &ReturnStmt) -> IResult {
        let val = if let Some(expr) = &ret.value {
            self.eval_expr(expr)?
        } else {
            Value::Null
        };
        Err(Signal::Flow(ControlFlow::Return(val)))
    }

    fn exec_throw(&mut self, throw: &ThrowStmt) -> IResult {
        let val = self.eval_expr(&throw.value)?;
        Err(Signal::Error(RuntimeError::new(format!("{}", val))))
    }

    fn exec_try(&mut self, try_stmt: &TryStmt) -> IResult {
        match self.exec_block(&try_stmt.body) {
            Ok(val) => Ok(val),
            Err(Signal::Error(err)) => {
                // Try to match a catch clause
                if let Some(catch) = try_stmt.catches.first() {
                    self.env.push_scope();
                    self.env
                        .define(catch.param.name.clone(), Value::String(err.message), false);
                    let result = self.exec_block(&catch.body);
                    self.env.pop_scope();
                    result
                } else {
                    // No catch clause, re-raise
                    Err(Signal::Error(RuntimeError::new("unhandled error")))
                }
            }
            Err(flow) => Err(flow), // Don't catch control flow
        }
    }

    /// Execute a select statement: multiplex over channel operations.
    /// Each case binds a pattern to a channel receive expression.
    /// The first ready channel wins.
    fn exec_select(&mut self, select: &SelectStmt) -> IResult {
        for case in &select.cases {
            let ch_val = self.eval_expr(&case.expr)?;
            match &ch_val {
                Value::Channel(arc) => {
                    let mut inner = arc.lock().map_err(|_| {
                        Signal::Error(RuntimeError::new("channel lock poisoned"))
                    })?;
                    if !inner.queue.is_empty() {
                        let val = inner.queue.pop_front().unwrap_or(Value::Null);
                        self.env.push_scope();
                        self.env
                            .define(case.pattern.name.clone(), val, true);
                        let result = self.exec_block(&Block {
                            span: select.span,
                            stmts: case.body.clone(),
                        });
                        self.env.pop_scope();
                        return result;
                    }
                    if inner.closed {
                        continue;
                    }
                }
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "select case expression must evaluate to a channel",
                    )));
                }
            }
        }
        Ok(Value::Null)
    }

    /// Execute a synchronized block: mutual exclusion via scope isolation.
    /// In the tree-walking interpreter, this is a scoped no-op.
    fn exec_synchronized(&mut self, sync: &SynchronizedStmt) -> IResult {
        self.env.push_scope();
        let result = self.exec_block(&sync.body);
        self.env.pop_scope();
        result
    }
}
