use num_bigint::BigInt;
use num_traits::ToPrimitive;
use coco_syntax::*;

use crate::builtins::call_builtin;
use crate::error::{ControlFlow, IResult, RuntimeError, Signal};
use crate::stack::StackFrame;
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
            Expr::Postfix(postfix) => self.eval_postfix(postfix),
            Expr::Parallel(parallel) => self.eval_parallel(parallel),
            Expr::Dollar(_) => self.eval_dollar(),
            Expr::DollarDollar(_) => self.eval_dollar_dollar(),
            Expr::This(_) => self.eval_dollar(), // this == $
            Expr::Super(_) => self.eval_super(),
            Expr::New(new_expr) => self.eval_new(new_expr),
            Expr::Match(match_expr) => self.eval_match(match_expr),
            _ => Err(Signal::Error(RuntimeError::new(format!(
                "unsupported expression: {:?}",
                std::mem::discriminant(expr)
            )))),
        }
    }

    fn eval_literal(&self, lit: &Literal) -> IResult {
        Ok(match lit {
            Literal::Int(n, _) => Value::Int(BigInt::from(*n)),
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
            BinaryOp::AddAssign => {
                return self.eval_compound_assign(&bin.left, &bin.right, BinaryOp::Add)
            }
            BinaryOp::SubAssign => {
                return self.eval_compound_assign(&bin.left, &bin.right, BinaryOp::Sub)
            }
            BinaryOp::MulAssign => {
                return self.eval_compound_assign(&bin.left, &bin.right, BinaryOp::Mul)
            }
            BinaryOp::DivAssign => {
                return self.eval_compound_assign(&bin.left, &bin.right, BinaryOp::Div)
            }
            BinaryOp::ModAssign => {
                return self.eval_compound_assign(&bin.left, &bin.right, BinaryOp::Mod)
            }
            BinaryOp::PowAssign => {
                return self.eval_compound_assign(&bin.left, &bin.right, BinaryOp::Pow)
            }
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
            BinaryOp::Shl => self.bitwise_op(left, right, |a, b| {
                let shift = b.to_usize().unwrap_or(0);
                a << shift
            }),
            BinaryOp::Shr => self.bitwise_op(left, right, |a, b| {
                let shift = b.to_usize().unwrap_or(0);
                a >> shift
            }),
            _ => Err(Signal::Error(RuntimeError::new(format!(
                "unsupported binary op {:?}",
                bin.op
            )))),
        }
    }

    fn eval_assign(&mut self, target: &Expr, value_expr: &Expr) -> IResult {
        let value = self.eval_expr(value_expr)?;
        self.set_target(target, value)
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
            _ => {
                return Err(Signal::Error(RuntimeError::new(
                    "unsupported compound assign op",
                )))
            }
        };
        self.set_target(target, result)
    }

    /// Set a value to an assignment target: variable, member, or index.
    fn set_target(&mut self, target: &Expr, value: Value) -> IResult {
        match target {
            Expr::Ident(ident) => {
                self.env
                    .set(&ident.name, value.clone())
                    .map_err(|e| Signal::Error(RuntimeError::new(e)))?;
                Ok(value)
            }
            Expr::Member(member) => {
                let obj = self.eval_expr(&member.object)?;
                self.set_member_property(obj, &member.property.name, value)
            }
            Expr::Index(index_expr) => {
                let obj = self.eval_expr(&index_expr.object)?;
                let idx = self.eval_expr(&index_expr.index)?;
                self.set_index_value(obj, idx, value)
            }
            _ => Err(Signal::Error(RuntimeError::new(
                "invalid assignment target",
            ))),
        }
    }

    /// Set a property on an object (map/instance).
    /// Walks up the scope chain to find and replace the `$` binding
    /// so the mutation survives function scope pop.
    fn set_member_property(&mut self, _obj: Value, prop: &str, value: Value) -> IResult {
        // Get current `$` instance
        let current = self.env.get("$").cloned().unwrap_or(Value::Null);
        let mut new_map = std::collections::HashMap::new();
        if let Value::Map(ref existing) = current {
            for (k, v) in &existing.data {
                new_map.insert(k.clone(), v.clone());
            }
        }
        new_map.insert(prop.to_string(), value.clone());
        let new_val = self.alloc_map(new_map);
        let _ = self.env.set("$", new_val);
        Ok(value)
    }

    /// Set an index on a collection.
    fn set_index_value(&mut self, _obj: Value, idx: Value, value: Value) -> IResult {
        let current = self.env.get("$").cloned().unwrap_or(Value::Null);
        let key = match &idx {
            Value::String(s) => s.clone(),
            other => format!("{}", other),
        };
        let mut new_map = std::collections::HashMap::new();
        if let Value::Map(ref existing) = current {
            for (k, v) in &existing.data {
                new_map.insert(k.clone(), v.clone());
            }
        }
        new_map.insert(key, value.clone());
        let new_val = self.alloc_map(new_map);
        let _ = self.env.set("$", new_val);
        Ok(value)
    }

    fn eval_assignment(&mut self, assign: &AssignmentExpr) -> IResult {
        match assign.op {
            AssignmentOp::Assign => self.eval_assign(&assign.target, &assign.value),
            AssignmentOp::AddAssign => {
                self.eval_compound_assign(&assign.target, &assign.value, BinaryOp::Add)
            }
            AssignmentOp::SubAssign => {
                self.eval_compound_assign(&assign.target, &assign.value, BinaryOp::Sub)
            }
            AssignmentOp::MulAssign => {
                self.eval_compound_assign(&assign.target, &assign.value, BinaryOp::Mul)
            }
            AssignmentOp::DivAssign => {
                self.eval_compound_assign(&assign.target, &assign.value, BinaryOp::Div)
            }
            AssignmentOp::ModAssign => {
                self.eval_compound_assign(&assign.target, &assign.value, BinaryOp::Mod)
            }
            AssignmentOp::PowAssign => {
                self.eval_compound_assign(&assign.target, &assign.value, BinaryOp::Pow)
            }
            _ => Err(Signal::Error(RuntimeError::new(
                "unsupported assignment op",
            ))),
        }
    }

    fn eval_unary(&mut self, un: &UnaryExpr) -> IResult {
        let val = self.eval_expr(&un.expr)?;
        match un.op {
            UnaryOp::Neg => match val {
                Value::Int(n) => Ok(Value::Int(-n.clone())),
                Value::Float(f) => Ok(Value::Float(-f)),
                _ => Err(Signal::Error(RuntimeError::new("cannot negate non-number"))),
            },
            UnaryOp::Not => Ok(Value::Bool(!val.is_truthy())),
            UnaryOp::BitNot => match val {
                Value::Int(n) => Ok(Value::Int(!n)),
                _ => Err(Signal::Error(RuntimeError::new(
                    "bitwise NOT requires integer",
                ))),
            },
            UnaryOp::Await => {
                // In the tree-walking interpreter, await is a no-op.
                // The value is already computed synchronously.
                Ok(val)
            }
            UnaryOp::Lazy => {
                // Lazy is a no-op in the tree-walking interpreter.
                Ok(val)
            }
            _ => Err(Signal::Error(RuntimeError::new(format!(
                "unsupported unary op {:?}",
                un.op
            )))),
        }
    }

    fn eval_parallel(&mut self, parallel: &ParallelExpr) -> IResult {
        // In the tree-walking interpreter, evaluate all runs sequentially.
        let mut results = Vec::new();
        for run in &parallel.runs {
            results.push(self.eval_expr(&run.expr)?);
        }
        // Return the last result (or null if empty)
        Ok(results.pop().unwrap_or(Value::Null))
    }

    /// Evaluate `$` — the current instance reference.
    fn eval_dollar(&self) -> IResult {
        self.env.get("$").cloned().ok_or_else(|| {
            Signal::Error(RuntimeError::new(
                "`$` (this) used outside of a class method or constructor",
            ))
        })
    }

    /// Evaluate `$$` — the outer class context.
    fn eval_dollar_dollar(&self) -> IResult {
        self.env.get("$$").cloned().ok_or_else(|| {
            Signal::Error(RuntimeError::new(
                "`$$` used outside of class context",
            ))
        })
    }

    /// Evaluate a method via super dispatch — look up the method
    /// starting from the parent class, skipping the current class.
    fn eval_super_method(&self, method_name: &str) -> IResult {
        let current = self.env.get("$").cloned().unwrap_or(Value::Null);
        let class_name = match &current {
            Value::Map(ref m) => m.data.get("__class__")
                .and_then(|v| match v { Value::String(s) => Some(s.clone()), _ => None }),
            _ => None,
        };
        match class_name {
            Some(name) => {
                let class_val = self.env.get(&name).cloned().unwrap_or(Value::Null);
                match &class_val {
                    Value::Map(ref class_map) => {
                        // Start from parent, not current class
                        if let Some(parent_val) = class_map.data.get("__parent__") {
                            let mut current = Some(parent_val.clone());
                            while let Some(Value::Map(ref cmap)) = current {
                                if let Some(method) = cmap.data.get(method_name) {
                                    return Ok(method.clone());
                                }
                                current = cmap.data.get("__parent__").cloned();
                            }
                        }
                        Err(Signal::Error(RuntimeError::new(format!(
                            "no method '{}' found in super chain",
                            method_name
                        ))))
                    }
                    _ => Err(Signal::Error(RuntimeError::new(format!(
                        "class '{}' not found", name
                    )))),
                }
            }
            None => Err(Signal::Error(RuntimeError::new(
                "super used outside of class context",
            ))),
        }
    }

    /// Evaluate `super` — dispatch to the parent class.
    /// Returns the parent class prototype map.
    fn eval_super(&self) -> IResult {
        // Look up the parent class via the current instance's `__class__`
        let current = self.env.get("$").cloned().unwrap_or(Value::Null);
        let class_name = match &current {
            Value::Map(ref m) => m
                .data
                .get("__class__")
                .and_then(|v| match v {
                    Value::String(s) => Some(s.clone()),
                    _ => None,
                }),
            _ => None,
        };

        match class_name {
            Some(name) => {
                // Look up the class definition
                let class_val = self.env.get(&name).cloned().unwrap_or(Value::Null);
                // Look for `__parent__` on the class
                match &class_val {
                    Value::Map(ref m) => m
                        .data
                        .get("__parent__")
                        .cloned()
                        .ok_or_else(|| {
                            Signal::Error(RuntimeError::new(format!(
                                "class '{}' has no parent (super called on root class)",
                                name
                            )))
                        }),
                    _ => Err(Signal::Error(RuntimeError::new(format!(
                        "class '{}' not found",
                        name
                    )))),
                }
            }
            None => Err(Signal::Error(RuntimeError::new(
                "super used outside of class context",
            ))),
        }
    }

    fn eval_postfix(&mut self, postfix: &PostfixExpr) -> IResult {
        let val = self.eval_expr(&postfix.object)?;
        match &postfix.op {
            PostfixOp::Question => match val {
                Value::Ok(v) => Ok(*v),
                Value::Err(e) => Err(Signal::Error(RuntimeError::new(format!(
                    "error propagated: {}",
                    e
                )))),
                _ => Err(Signal::Error(RuntimeError::new(
                    "? operator requires a Result value",
                ))),
            },
            _ => Err(Signal::Error(RuntimeError::new(format!(
                "unsupported postfix op {:?}",
                postfix.op
            )))),
        }
    }

    fn eval_new(&mut self, new_expr: &NewExpr) -> IResult {
        let class_name = &new_expr.type_name.name;

        // Look up the class definition
        let class_val = match self.env.get(class_name) {
            Some(v) => v.clone(),
            None => {
                return Err(Signal::Error(RuntimeError::new(format!(
                    "class '{}' not found",
                    class_name
                ))));
            }
        };

        // Extract class metadata
        let class_map = match &class_val {
            Value::Map(gc) => &gc.data,
            _ => {
                return Err(Signal::Error(RuntimeError::new(format!(
                    "'{}' is not a class",
                    class_name
                ))));
            }
        };

        // Create a new instance (empty map)
        let instance = std::collections::HashMap::new();

        // Evaluate constructor arguments
        let mut args = Vec::new();
        for arg in &new_expr.args {
            args.push(self.eval_expr(&arg.value)?);
        }

        // Tag instance with its class name for method dispatch
        let mut instance_map = instance;
        instance_map.insert(
            "__class__".to_string(),
            Value::String(class_name.to_string()),
        );

        // Initialize property defaults from the class definition.
        // Copy all __prop_* entries from the class map to the instance.
        for (key, val) in class_map.iter() {
            if key.starts_with("__prop_") {
                let prop_name = key.strip_prefix("__prop_").unwrap_or(key);
                instance_map.insert(prop_name.to_string(), val.clone());
            }
        }

        // Call the constructor if present
        if let Some(ctor_val) = class_map.get("__constructor__") {
            if let Value::Function(ctor_func) = ctor_val {
                // Allocate the instance value once
                let instance_val = self.alloc_map(instance_map);

                // Push `$` scope for constructor body
                self.env.push_scope();
                self.env.define("$".to_string(), instance_val, true);

                // Push call stack frame
                self.call_stack.push(StackFrame {
                    function_name: format!("new {}", class_name),
                    def_span: None,
                    call_site: Some(new_expr.span),
                    file: self.source_file.clone(),
                });

                // Call constructor
                let result = self.call_function(ctor_func, args);

                // On success, get back the instance (modified by constructor via $)
                let final_instance = self
                    .env
                    .get("$")
                    .cloned()
                    .unwrap_or(Value::Null);

                self.call_stack.pop();
                self.env.pop_scope();

                match result {
                    Ok(_) => return Ok(final_instance),
                    Err(e) => return Err(e),
                }
            }
        }

        // No constructor — just return the instance
        Ok(self.alloc_map(instance_map))
    }

    fn eval_call(&mut self, call: &CallExpr) -> IResult {
        // If callee is a member expression (obj.method() or super.method()),
        // bind `$` to the object.
        let (method_func, instance) = if let Expr::Member(member) = &call.callee {
            // Check for super.method() — dispatch from parent class
            if matches!(&member.object, Expr::Super(_)) {
                let obj = self.eval_dollar()?; // `$` is the current instance
                let method_val = self.eval_super_method(&member.property.name)?;
                match method_val {
                    Value::Function(func) => (func, Some(obj)),
                    _ => return Err(Signal::Error(RuntimeError::new("not a callable value"))),
                }
            } else {
                let obj = self.eval_expr(&member.object)?;
                let method_val = self.eval_expr(&call.callee)?;
                match method_val {
                    Value::Function(func) => (func, Some(obj)),
                    Value::BuiltinFn(name) => {
                        // Channel/Atomic method dispatch: prepend receiver to args.
                        let mut args = vec![obj];
                        for arg in &call.args {
                            args.push(self.eval_expr(&arg.value)?);
                        }
                        return call_builtin(&name, &args, &mut self.heap);
                    }
                    _ => return Err(Signal::Error(RuntimeError::new("not a callable value"))),
                }
            }
        } else {
            let callee = self.eval_expr(&call.callee)?;
            match callee {
                Value::Function(func) => (func, None),
                Value::BuiltinFn(name) => {
                    let mut args = Vec::new();
                    for arg in &call.args {
                        args.push(self.eval_expr(&arg.value)?);
                    }
                    return call_builtin(&name, &args, &mut self.heap);
                }
                _ => return Err(Signal::Error(RuntimeError::new("not a callable value"))),
            }
        };

        let mut args = Vec::new();
        for arg in &call.args {
            args.push(self.eval_expr(&arg.value)?);
        }

        // If this is an instance method, bind `$` to the instance
        if let Some(inst) = instance {
            self.env.push_scope();
            self.env.define("$".to_string(), inst, true);
            let result = self.call_function(&method_func, args);
            self.env.pop_scope();
            result
        } else {
            self.call_function(&method_func, args)
        }
    }

    /// Call a user-defined function with the given arguments.
    pub(crate) fn call_function(&mut self, func: &Function, args: Vec<Value>) -> IResult {
        // Push call stack frame
        self.call_stack.push(StackFrame {
            function_name: func.name.clone(),
            def_span: None,
            call_site: None,
            file: self.source_file.clone(),
        });

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
            Ok(val) => {
                self.call_stack.pop();
                Ok(val)
            }
            Err(Signal::Flow(ControlFlow::Return(val))) => Ok(val),
            Err(e) => Err(e),
        }
    }

    fn eval_index(&mut self, idx: &IndexExpr) -> IResult {
        let object = self.eval_expr(&idx.object)?;
        let index = self.eval_expr(&idx.index)?;
        match (&object, &index) {
            (Value::List(list), Value::Int(i)) => {
                let idx = if *i < BigInt::from(0) {
                    (list.data.len() as i64 + i.to_i64().unwrap_or(0)) as usize
                } else {
                    i.to_usize().unwrap_or(0)
                };
                list.data
                    .get(idx)
                    .cloned()
                    .ok_or_else(|| Signal::Error(RuntimeError::new("index out of bounds")))
            }
            (Value::Map(map), Value::String(key)) => {
                Ok(map.data.get(key).cloned().unwrap_or(Value::Null))
            }
            _ => Err(Signal::Error(RuntimeError::new("invalid index operation"))),
        }
    }

    fn eval_member(&mut self, mem: &MemberExpr) -> IResult {
        let object = self.eval_expr(&mem.object)?;
        let prop = &mem.property.name;

        match &object {
            Value::List(list) if prop == "length" => Ok(Value::Int(BigInt::from(list.data.len()))),
            Value::String(s) if prop == "length" => Ok(Value::Int(BigInt::from(s.len()))),
            Value::Map(map) => {
                // Direct property access
                if let Some(val) = map.data.get(prop) {
                    return Ok(val.clone());
                }
                // Method lookup via class prototype chain (includes inheritance)
                if let Some(Value::String(class_name)) = map.data.get("__class__") {
                    let mut current_class = self.env.get(class_name).cloned();
                    while let Some(Value::Map(ref class_map)) = current_class {
                        if let Some(method) = class_map.data.get(prop) {
                            return Ok(method.clone());
                        }
                        // Walk up to parent class
                        current_class = class_map
                            .data
                            .get("__parent__")
                            .cloned();
                    }
                }
                Ok(Value::Null)
            }
            Value::Channel(_) => {
                match prop.as_str() {
                    "send" => Ok(Value::BuiltinFn("chan_send".to_string())),
                    "recv" => Ok(Value::BuiltinFn("chan_recv".to_string())),
                    "close" => Ok(Value::BuiltinFn("chan_close".to_string())),
                    _ => Err(Signal::Error(RuntimeError::new(format!(
                        "channel has no property '{}'",
                        prop
                    )))),
                }
            }
            Value::Atomic(_) => {
                match prop.as_str() {
                    "load" => Ok(Value::BuiltinFn("atomic_load".to_string())),
                    "store" => Ok(Value::BuiltinFn("atomic_store".to_string())),
                    "add" => Ok(Value::BuiltinFn("atomic_add".to_string())),
                    "sub" => Ok(Value::BuiltinFn("atomic_sub".to_string())),
                    "compareAndSwap" => Ok(Value::BuiltinFn("atomic_cas".to_string())),
                    _ => Err(Signal::Error(RuntimeError::new(format!(
                        "atomic has no property '{}'",
                        prop
                    )))),
                }
            }
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
        Ok(self.alloc_list(elements))
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
        Ok(self.alloc_map(map))
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
            (Value::Int(a), Value::Float(b)) => Ok(Value::Float(a.to_f64().unwrap_or(0.0) + b)),
            (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a + b.to_f64().unwrap_or(0.0))),
            (Value::String(a), Value::String(b)) => Ok(Value::String(format!("{}{}", a, b))),
            _ => Err(Signal::Error(RuntimeError::new("invalid operands for +"))),
        }
    }

    fn sub_values(&self, left: Value, right: Value) -> IResult {
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a - b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a - b)),
            (Value::Int(a), Value::Float(b)) => Ok(Value::Float(a.to_f64().unwrap_or(0.0) - b)),
            (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a - b.to_f64().unwrap_or(0.0))),
            _ => Err(Signal::Error(RuntimeError::new("invalid operands for -"))),
        }
    }

    fn mul_values(&self, left: Value, right: Value) -> IResult {
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a * b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a * b)),
            (Value::Int(a), Value::Float(b)) => Ok(Value::Float(a.to_f64().unwrap_or(0.0) * b)),
            (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a * b.to_f64().unwrap_or(0.0))),
            _ => Err(Signal::Error(RuntimeError::new("invalid operands for *"))),
        }
    }

    fn div_values(&self, left: Value, right: Value) -> IResult {
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => {
                if b == BigInt::from(0) {
                    return Err(Signal::Error(RuntimeError::new("division by zero")));
                }
                Ok(Value::Int(a / b))
            }
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a / b)),
            (Value::Int(a), Value::Float(b)) => Ok(Value::Float(a.to_f64().unwrap_or(0.0) / b)),
            (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a / b.to_f64().unwrap_or(0.0))),
            _ => Err(Signal::Error(RuntimeError::new("invalid operands for /"))),
        }
    }

    fn mod_values(&self, left: Value, right: Value) -> IResult {
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => {
                if b == BigInt::from(0) {
                    return Err(Signal::Error(RuntimeError::new("modulo by zero")));
                }
                Ok(Value::Int(a % b))
            }
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a % b)),
            (Value::Int(a), Value::Float(b)) => Ok(Value::Float(a.to_f64().unwrap_or(0.0) % b)),
            (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a % b.to_f64().unwrap_or(0.0))),
            _ => Err(Signal::Error(RuntimeError::new("invalid operands for %"))),
        }
    }

    fn pow_values(&self, left: Value, right: Value) -> IResult {
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => {
                if b >= BigInt::from(0) {
                    Ok(Value::Int(a.pow(b.to_u32().unwrap_or(0))))
                } else {
                    Ok(Value::Float((a.to_f64().unwrap_or(0.0)).powi(b.to_i32().unwrap_or(0))))
                }
            }
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a.powf(b))),
            (Value::Int(a), Value::Float(b)) => Ok(Value::Float((a.to_f64().unwrap_or(0.0)).powf(b))),
            (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a.powi(b.to_i32().unwrap_or(0)))),
            _ => Err(Signal::Error(RuntimeError::new("invalid operands for **"))),
        }
    }

    fn bitwise_op<F>(&self, left: Value, right: Value, op: F) -> IResult
    where
        F: FnOnce(BigInt, BigInt) -> BigInt,
    {
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(op(a, b))),
            (Value::Bool(a), Value::Bool(b)) => {
                let ai = BigInt::from(a as i64);
                let bi = BigInt::from(b as i64);
                Ok(Value::Bool(op(ai, bi) != BigInt::from(0)))
            }
            _ => Err(Signal::Error(RuntimeError::new(
                "bitwise ops require integers or booleans",
            ))),
        }
    }

    fn eval_match(&mut self, match_expr: &MatchExpr) -> IResult {
        let scrutinee = self.eval_expr(&match_expr.scrutinee)?;
        for arm in &match_expr.arms {
            if self.pattern_matches(&scrutinee, &arm.pattern) {
                self.env.push_scope();
                self.bind_pattern(&arm.pattern, &scrutinee)?;
                let result = self.eval_expr(&arm.body);
                self.env.pop_scope();
                return result;
            }
        }
        Err(Signal::Error(RuntimeError::new(
            "match: no arm matched the scrutinee value",
        )))
    }

    /// Check whether a value matches a pattern.
    fn pattern_matches(&self, value: &Value, pattern: &Pattern) -> bool {
        match pattern {
            Pattern::Literal(lit) => {
                let lit_val = self.eval_literal(lit).unwrap_or(Value::Null);
                self.values_eq(value, &lit_val)
            }
            Pattern::Ident(_) => {
                // Identifier patterns always match — they bind the value.
                true
            }
            Pattern::IsType(_type) => {
                // Type guard: check if the value is of the expected type.
                // This is a simple runtime type check based on the type name.
                match _type {
                    Type::Primitive(prim, _) => self.type_matches_primitive(value, *prim),
                    Type::Named(named) => self.type_matches_named(value, &named.name.name),
                    _ => false,
                }
            }
            Pattern::Wildcard(_) => true,
        }
    }

    /// Bind a pattern's variables in the current scope.
    fn bind_pattern(&mut self, pattern: &Pattern, value: &Value) -> Result<(), Signal> {
        match pattern {
            Pattern::Ident(ident) => {
                self.env
                    .define(ident.name.clone(), value.clone(), true);
            }
            Pattern::Literal(_) | Pattern::IsType(_) | Pattern::Wildcard(_) => {
                // No binding needed.
            }
        }
        Ok(())
    }

    /// Check if a value matches a primitive type name.
    fn type_matches_primitive(&self, value: &Value, prim: PrimitiveType) -> bool {
        match (prim, value) {
            (PrimitiveType::Int, Value::Int(_)) => true,
            (PrimitiveType::Float, Value::Float(_)) => true,
            (PrimitiveType::String, Value::String(_)) => true,
            (PrimitiveType::Bool, Value::Bool(_)) => true,
            (PrimitiveType::Null, Value::Null) => true,
            _ => false,
        }
    }

    /// Check if a value matches a named type (e.g., class instance).
    /// This is a best-effort check — in the tree-walking interpreter,
    /// we check if the value is a map with a `__class` marker.
    fn type_matches_named(&self, value: &Value, name: &str) -> bool {
        match value {
            Value::Map(map) => {
                if let Some(Value::String(class_name)) = map.data.get("__class") {
                    class_name == name
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    fn values_eq(&self, left: &Value, right: &Value) -> bool {
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Int(a), Value::Float(b)) => (a.to_f64().unwrap_or(0.0)) == *b,
            (Value::Float(a), Value::Int(b)) => *a == (b.to_f64().unwrap_or(0.0)),
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
            (Value::Float(a), Value::Float(b)) => {
                a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
            }
            (Value::Int(a), Value::Float(b)) => (a.to_f64().unwrap_or(0.0))
                .partial_cmp(b)
                .unwrap_or(std::cmp::Ordering::Equal),
            (Value::Float(a), Value::Int(b)) => a
                .partial_cmp(&(b.to_f64().unwrap_or(0.0)))
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
