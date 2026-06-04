use coco_syntax::*;

use crate::builtins::call_builtin;
use crate::error::{ControlFlow, IResult, RuntimeError, Signal};
use crate::value::{Function, Value};
use crate::Interpreter;

impl Interpreter {
    /// Evaluate an expression and return its value.
    pub(crate) fn eval_expr(&mut self, expr: &Expr) -> IResult {
        match expr {
            Expr::Literal(lit) => self.eval_literal(lit),
            Expr::Ident(ident) => self.eval_ident(ident),
            Expr::Binary(bin) => self.eval_binary(bin),
            Expr::Unary(un) => self.eval_unary(un),
            Expr::Call(call) => self.eval_call(call),
            Expr::Index(idx) => self.eval_index(idx),
            Expr::Member(mem) => self.eval_member(mem),
            Expr::Array(arr) => self.eval_array(arr),
            Expr::Object(obj) => self.eval_object(obj),
            Expr::Ternary(tern) => self.eval_ternary(tern),
            Expr::NullCoalesce(nc) => self.eval_null_coalesce(nc),
            Expr::Group(inner) => self.eval_expr(inner),
            Expr::Assignment(assign) => self.eval_assignment(assign),
            Expr::Lambda(lambda) => self.eval_lambda(lambda),
            _ => Err(Signal::Error(RuntimeError::new(format!(
                "unsupported expression: {:?}",
                std::mem::discriminant(expr)
            )))),
        }
    }

    fn eval_literal(&self, lit: &Literal) -> IResult {
        Ok(match lit {
            Literal::Int(n, _) => Value::Int(*n),
            Literal::Float(f, _) => Value::Float(*f),
            Literal::String(s, _) => Value::String(s.clone()),
            Literal::Bool(b, _) => Value::Bool(*b),
            Literal::Null(_) => Value::Null,
            Literal::Char(c, _) => Value::String(c.to_string()),
        })
    }

    fn eval_ident(&self, ident: &Ident) -> IResult {
        match self.env.get(&ident.name) {
            Some(val) => Ok(val.clone()),
            None => Err(Signal::Error(RuntimeError::new(format!(
                "undefined variable '{}'",
                ident.name
            )))),
        }
    }

    fn eval_binary(&mut self, bin: &BinaryExpr) -> IResult {
        // Handle assignment operators
        match bin.op {
            BinaryOp::Assign => return self.eval_assign(&bin.left, &bin.right),
            BinaryOp::AddAssign => return self.eval_compound_assign(&bin.left, &bin.right, BinaryOp::Add),
            BinaryOp::SubAssign => return self.eval_compound_assign(&bin.left, &bin.right, BinaryOp::Sub),
            BinaryOp::MulAssign => return self.eval_compound_assign(&bin.left, &bin.right, BinaryOp::Mul),
            BinaryOp::DivAssign => return self.eval_compound_assign(&bin.left, &bin.right, BinaryOp::Div),
            BinaryOp::ModAssign => return self.eval_compound_assign(&bin.left, &bin.right, BinaryOp::Mod),
            BinaryOp::PowAssign => return self.eval_compound_assign(&bin.left, &bin.right, BinaryOp::Pow),
            BinaryOp::NullCoalesce => {
                let left = self.eval_expr(&bin.left)?;
                if matches!(left, Value::Null) {
                    return self.eval_expr(&bin.right);
                }
                return Ok(left);
            }
            BinaryOp::And => {
                let left = self.eval_expr(&bin.left)?;
                if !left.is_truthy() {
                    return Ok(Value::Bool(false));
                }
                let right = self.eval_expr(&bin.right)?;
                return Ok(Value::Bool(right.is_truthy()));
            }
            BinaryOp::Or => {
                let left = self.eval_expr(&bin.left)?;
                if left.is_truthy() {
                    return Ok(Value::Bool(true));
                }
                let right = self.eval_expr(&bin.right)?;
                return Ok(Value::Bool(right.is_truthy()));
            }
            _ => {}
        }

        let left = self.eval_expr(&bin.left)?;
        let right = self.eval_expr(&bin.right)?;

        match bin.op {
            BinaryOp::Add => self.add_values(left, right),
            BinaryOp::Sub => self.sub_values(left, right),
            BinaryOp::Mul => self.mul_values(left, right),
            BinaryOp::Div => self.div_values(left, right),
            BinaryOp::Mod => self.mod_values(left, right),
            BinaryOp::Pow => self.pow_values(left, right),
            BinaryOp::Eq => Ok(Value::Bool(self.values_eq(&left, &right))),
            BinaryOp::Ne => Ok(Value::Bool(!self.values_eq(&left, &right))),
            BinaryOp::Lt => self.compare_values(left, right, |o| o == std::cmp::Ordering::Less),
            BinaryOp::Gt => self.compare_values(left, right, |o| o == std::cmp::Ordering::Greater),
            BinaryOp::Le => self.compare_values(left, right, |o| o != std::cmp::Ordering::Greater),
            BinaryOp::Ge => self.compare_values(left, right, |o| o != std::cmp::Ordering::Less),
            BinaryOp::BitAnd => self.bitwise_op(left, right, |a, b| a & b),
            BinaryOp::BitOr => self.bitwise_op(left, right, |a, b| a | b),
            BinaryOp::BitXor => self.bitwise_op(left, right, |a, b| a ^ b),
            BinaryOp::Shl => self.bitwise_op(left, right, |a, b| a << b),
            BinaryOp::Shr => self.bitwise_op(left, right, |a, b| a >> b),
            _ => Err(Signal::Error(RuntimeError::new(format!(
                "unsupported binary op {:?}",
                bin.op
            )))),
        }
    }

    fn eval_assign(&mut self, target: &Expr, value_expr: &Expr) -> IResult {
        let value = self.eval_expr(value_expr)?;
        match target {
            Expr::Ident(ident) => {
                self.env.set(&ident.name, value.clone()).map_err(|e| Signal::Error(RuntimeError::new(e)))?;
                Ok(value)
            }
            _ => Err(Signal::Error(RuntimeError::new("invalid assignment target"))),
        }
    }

    fn eval_compound_assign(&mut self, target: &Expr, value_expr: &Expr, op: BinaryOp) -> IResult {
        let current = self.eval_expr(target)?;
        let rhs = self.eval_expr(value_expr)?;
        let result = match op {
            BinaryOp::Add => self.add_values(current, rhs)?,
            BinaryOp::Sub => self.sub_values(current, rhs)?,
            BinaryOp::Mul => self.mul_values(current, rhs)?,
            BinaryOp::Div => self.div_values(current, rhs)?,
            BinaryOp::Mod => self.mod_values(current, rhs)?,
            BinaryOp::Pow => self.pow_values(current, rhs)?,
            _ => return Err(Signal::Error(RuntimeError::new("unsupported compound assign op"))),
        };
        match target {
            Expr::Ident(ident) => {
                self.env.set(&ident.name, result.clone()).map_err(|e| Signal::Error(RuntimeError::new(e)))?;
                Ok(result)
            }
            _ => Err(Signal::Error(RuntimeError::new("invalid assignment target"))),
        }
    }

    fn eval_assignment(&mut self, assign: &AssignmentExpr) -> IResult {
        match assign.op {
            AssignmentOp::Assign => self.eval_assign(&assign.target, &assign.value),
            AssignmentOp::AddAssign => self.eval_compound_assign(&assign.target, &assign.value, BinaryOp::Add),
            AssignmentOp::SubAssign => self.eval_compound_assign(&assign.target, &assign.value, BinaryOp::Sub),
            AssignmentOp::MulAssign => self.eval_compound_assign(&assign.target, &assign.value, BinaryOp::Mul),
            AssignmentOp::DivAssign => self.eval_compound_assign(&assign.target, &assign.value, BinaryOp::Div),
            AssignmentOp::ModAssign => self.eval_compound_assign(&assign.target, &assign.value, BinaryOp::Mod),
            AssignmentOp::PowAssign => self.eval_compound_assign(&assign.target, &assign.value, BinaryOp::Pow),
            _ => Err(Signal::Error(RuntimeError::new("unsupported assignment op"))),
        }
    }

    fn eval_unary(&mut self, un: &UnaryExpr) -> IResult {
        let val = self.eval_expr(&un.expr)?;
        match un.op {
            UnaryOp::Neg => match val {
                Value::Int(n) => Ok(Value::Int(-n)),
                Value::Float(f) => Ok(Value::Float(-f)),
                _ => Err(Signal::Error(RuntimeError::new("cannot negate non-number"))),
            },
            UnaryOp::Not => Ok(Value::Bool(!val.is_truthy())),
            UnaryOp::BitNot => match val {
                Value::Int(n) => Ok(Value::Int(!n)),
                _ => Err(Signal::Error(RuntimeError::new("bitwise NOT requires integer"))),
            },
            _ => Err(Signal::Error(RuntimeError::new(format!(
                "unsupported unary op {:?}",
                un.op
            )))),
        }
    }

    fn eval_call(&mut self, call: &CallExpr) -> IResult {
        let callee = self.eval_expr(&call.callee)?;
        let mut args = Vec::new();
        for arg in &call.args {
            args.push(self.eval_expr(&arg.value)?);
        }

        match callee {
            Value::BuiltinFn(name) => call_builtin(&name, &args),
            Value::Function(func) => self.call_function(&func, args),
            _ => Err(Signal::Error(RuntimeError::new("not a callable value"))),
        }
    }

    /// Call a user-defined function with the given arguments.
    pub(crate) fn call_function(&mut self, func: &Function, args: Vec<Value>) -> IResult {
        // Push a new scope for the function call
        self.env.push_scope();

        // Bind parameters to arguments
        for (i, param_name) in func.params.iter().enumerate() {
            let val = args.get(i).cloned().unwrap_or(Value::Null);
            self.env.define(param_name.clone(), val, true);
        }

        // Execute body
        let result = self.exec_block(&func.body);

        // Pop the function scope
        self.env.pop_scope();

        match result {
            Ok(val) => Ok(val),
            Err(Signal::Flow(ControlFlow::Return(val))) => Ok(val),
            Err(e) => Err(e),
        }
    }

    fn eval_index(&mut self, idx: &IndexExpr) -> IResult {
        let object = self.eval_expr(&idx.object)?;
        let index = self.eval_expr(&idx.index)?;
        match (&object, &index) {
            (Value::List(list), Value::Int(i)) => {
                let idx = if *i < 0 {
                    (list.len() as i64 + *i) as usize
                } else {
                    *i as usize
                };
                list.get(idx)
                    .cloned()
                    .ok_or_else(|| Signal::Error(RuntimeError::new("index out of bounds")))
            }
            (Value::Map(map), Value::String(key)) => {
                Ok(map.get(key).cloned().unwrap_or(Value::Null))
            }
            _ => Err(Signal::Error(RuntimeError::new("invalid index operation"))),
        }
    }

    fn eval_member(&mut self, mem: &MemberExpr) -> IResult {
        let object = self.eval_expr(&mem.object)?;
        let prop = &mem.property.name;

        match &object {
            Value::List(list) if prop == "length" => Ok(Value::Int(list.len() as i64)),
            Value::String(s) if prop == "length" => Ok(Value::Int(s.len() as i64)),
            Value::Map(map) => Ok(map.get(prop).cloned().unwrap_or(Value::Null)),
            _ => Err(Signal::Error(RuntimeError::new(format!(
                "cannot access property '{}' on {:?}",
                prop, object
            )))),
        }
    }

    fn eval_array(&mut self, arr: &ArrayLiteral) -> IResult {
        let mut elements = Vec::new();
        for elem in &arr.elements {
            elements.push(self.eval_expr(elem)?);
        }
        Ok(Value::List(elements))
    }

    fn eval_object(&mut self, obj: &ObjectLiteral) -> IResult {
        let mut map = std::collections::HashMap::new();
        for field in &obj.fields {
            let key = match &field.key {
                ObjectKey::Ident(ident) => ident.name.clone(),
                ObjectKey::String(s, _) => s.clone(),
            };
            let value = self.eval_expr(&field.value)?;
            map.insert(key, value);
        }
        Ok(Value::Map(map))
    }

    fn eval_ternary(&mut self, tern: &TernaryExpr) -> IResult {
        let cond = self.eval_expr(&tern.condition)?;
        if cond.is_truthy() {
            self.eval_expr(&tern.then_expr)
        } else {
            self.eval_expr(&tern.else_expr)
        }
    }

    fn eval_null_coalesce(&mut self, nc: &NullCoalesceExpr) -> IResult {
        let left = self.eval_expr(&nc.left)?;
        if matches!(left, Value::Null) {
            self.eval_expr(&nc.right)
        } else {
            Ok(left)
        }
    }

    fn eval_lambda(&mut self, lambda: &Lambda) -> IResult {
        let params: Vec<String> = lambda.params.iter().map(|p| p.name.name.clone()).collect();
        let body = match &lambda.body {
            LambdaBody::Block(block) => block.clone(),
            LambdaBody::Expr(expr) => {
                // Wrap expression in a block with a return statement
                Block {
                    span: lambda.span,
                    stmts: vec![Stmt::Return(ReturnStmt {
                        span: lambda.span,
                        value: Some(expr.clone()),
                    })],
                }
            }
        };
        Ok(Value::Function(Function {
            name: "<lambda>".to_string(),
            params,
            body,
        }))
    }

    // ============================================================
    // Arithmetic helpers
    // ============================================================

    fn add_values(&self, left: Value, right: Value) -> IResult {
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a + b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
            (Value::Int(a), Value::Float(b)) => Ok(Value::Float(a as f64 + b)),
            (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a + b as f64)),
            (Value::String(a), Value::String(b)) => Ok(Value::String(format!("{}{}", a, b))),
            _ => Err(Signal::Error(RuntimeError::new("invalid operands for +"))),
        }
    }

    fn sub_values(&self, left: Value, right: Value) -> IResult {
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a - b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a - b)),
            (Value::Int(a), Value::Float(b)) => Ok(Value::Float(a as f64 - b)),
            (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a - b as f64)),
            _ => Err(Signal::Error(RuntimeError::new("invalid operands for -"))),
        }
    }

    fn mul_values(&self, left: Value, right: Value) -> IResult {
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a * b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a * b)),
            (Value::Int(a), Value::Float(b)) => Ok(Value::Float(a as f64 * b)),
            (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a * b as f64)),
            _ => Err(Signal::Error(RuntimeError::new("invalid operands for *"))),
        }
    }

    fn div_values(&self, left: Value, right: Value) -> IResult {
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => {
                if b == 0 {
                    return Err(Signal::Error(RuntimeError::new("division by zero")));
                }
                Ok(Value::Int(a / b))
            }
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a / b)),
            (Value::Int(a), Value::Float(b)) => Ok(Value::Float(a as f64 / b)),
            (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a / b as f64)),
            _ => Err(Signal::Error(RuntimeError::new("invalid operands for /"))),
        }
    }

    fn mod_values(&self, left: Value, right: Value) -> IResult {
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => {
                if b == 0 {
                    return Err(Signal::Error(RuntimeError::new("modulo by zero")));
                }
                Ok(Value::Int(a % b))
            }
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a % b)),
            (Value::Int(a), Value::Float(b)) => Ok(Value::Float(a as f64 % b)),
            (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a % b as f64)),
            _ => Err(Signal::Error(RuntimeError::new("invalid operands for %"))),
        }
    }

    fn pow_values(&self, left: Value, right: Value) -> IResult {
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => {
                if b >= 0 {
                    Ok(Value::Int(a.pow(b as u32)))
                } else {
                    Ok(Value::Float((a as f64).powi(b as i32)))
                }
            }
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a.powf(b))),
            (Value::Int(a), Value::Float(b)) => Ok(Value::Float((a as f64).powf(b))),
            (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a.powi(b as i32))),
            _ => Err(Signal::Error(RuntimeError::new("invalid operands for **"))),
        }
    }

    fn bitwise_op(&self, left: Value, right: Value, op: fn(i64, i64) -> i64) -> IResult {
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(op(a, b))),
            (Value::Bool(a), Value::Bool(b)) => Ok(Value::Bool(op(a as i64, b as i64) != 0)),
            _ => Err(Signal::Error(RuntimeError::new("bitwise ops require integers or booleans"))),
        }
    }

    fn values_eq(&self, left: &Value, right: &Value) -> bool {
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Int(a), Value::Float(b)) => (*a as f64) == *b,
            (Value::Float(a), Value::Int(b)) => *a == (*b as f64),
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Null, Value::Null) => true,
            _ => false,
        }
    }

    fn compare_values(
        &self,
        left: Value,
        right: Value,
        pred: impl Fn(std::cmp::Ordering) -> bool,
    ) -> IResult {
        let ord = match (&left, &right) {
            (Value::Int(a), Value::Int(b)) => a.cmp(b),
            (Value::Float(a), Value::Float(b)) => a
                .partial_cmp(b)
                .unwrap_or(std::cmp::Ordering::Equal),
            (Value::Int(a), Value::Float(b)) => (*a as f64)
                .partial_cmp(b)
                .unwrap_or(std::cmp::Ordering::Equal),
            (Value::Float(a), Value::Int(b)) => a
                .partial_cmp(&(*b as f64))
                .unwrap_or(std::cmp::Ordering::Equal),
            (Value::String(a), Value::String(b)) => a.cmp(b),
            _ => {
                return Err(Signal::Error(RuntimeError::new(
                    "cannot compare these values",
                )))
            }
        };
        Ok(Value::Bool(pred(ord)))
    }
}
