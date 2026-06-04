use coco_syntax::*;

use crate::error::IResult;
use crate::value::{Function, Value};
use crate::Interpreter;

impl Interpreter {
    /// Execute a top-level item (declaration/statement).
    pub(crate) fn exec_item(&mut self, item: &Item) -> IResult {
        match item {
            Item::FnDecl(fn_decl) => self.exec_fn_decl(fn_decl),
            Item::LetDecl(let_decl) => self.exec_let_decl(let_decl),
            Item::ConstDecl(const_decl) => self.exec_const_decl(const_decl),
            Item::ExprStmt(expr_stmt) => self.eval_expr(&expr_stmt.expr),
            Item::Stmt(stmt) => self.exec_stmt(stmt),
            _ => Ok(Value::Null),
        }
    }

    fn exec_fn_decl(&mut self, fn_decl: &FnDecl) -> IResult {
        let name = fn_decl.name.name.clone();
        let params: Vec<String> = fn_decl.params.iter().map(|p| p.name.name.clone()).collect();
        let func = Function {
            name: name.clone(),
            params,
            body: fn_decl.body.clone(),
        };
        self.env.define(name, Value::Function(func), false);
        Ok(Value::Null)
    }

    fn exec_let_decl(&mut self, let_decl: &LetDecl) -> IResult {
        let value = if let Some(expr) = &let_decl.value {
            self.eval_expr(expr)?
        } else {
            Value::Null
        };
        self.env.define(let_decl.name.name.clone(), value, true);
        Ok(Value::Null)
    }

    fn exec_const_decl(&mut self, const_decl: &ConstDecl) -> IResult {
        let value = self.eval_expr(&const_decl.value)?;
        self.env.define(const_decl.name.name.clone(), value, false);
        Ok(Value::Null)
    }
}
