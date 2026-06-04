# Phase 3: Tree-Walking Interpreter Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a tree-walking interpreter that can execute Coco programs from examples/01-hello.co through examples/05-collections.co — covering literals, variables, functions, classes, control flow, and collections.

**Architecture:** New `coco_interpreter` crate walks the AST produced by `coco_parser`. An `Environment` struct holds variable bindings with lexical scoping (parent chain). A `Value` enum represents runtime values. The interpreter evaluates expressions recursively and executes statements imperatively. A `run` subcommand is added to `coco_cli`.

**Tech Stack:** Rust stable, `coco_syntax` AST types, `coco_parser` for parsing, `coco_cli` for CLI integration

---

## File Structure

```
crates/coco_interpreter/
├── Cargo.toml
├── src/
│   ├── lib.rs          — pub exports, Interpreter struct
│   ├── value.rs        — Value enum (Int, Float, String, Bool, Null, List, Map, Object, Function, BuiltinFn)
│   ├── env.rs          — Environment (scoped variable bindings)
│   ├── eval_expr.rs    — Expression evaluation (Expr → Value)
│   ├── exec_stmt.rs    — Statement execution (Stmt → control flow)
│   ├── exec_item.rs    — Top-level item execution (FnDecl, ClassDecl, etc.)
│   ├── builtins.rs     — Built-in functions (print, len, push, etc.)
│   └── error.rs        — RuntimeError type
```

Modified files:
- `Cargo.toml` (workspace) — add coco_interpreter member
- `crates/coco_cli/Cargo.toml` — add coco_interpreter dep
- `crates/coco_cli/src/main.rs` — add `run` subcommand

---

### Task 1: Create coco_interpreter crate scaffold

**Files:**
- Create: `crates/coco_interpreter/Cargo.toml`
- Create: `crates/coco_interpreter/src/lib.rs`
- Create: `crates/coco_interpreter/src/value.rs`
- Create: `crates/coco_interpreter/src/env.rs`
- Create: `crates/coco_interpreter/src/error.rs`
- Modify: `Cargo.toml` (workspace root)

- [ ] **Step 1: Create Cargo.toml for coco_interpreter**

```toml
[package]
name = "coco_interpreter"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[dependencies]
coco_syntax = { path = "../coco_syntax" }
coco_span = { path = "../coco_span" }
```

- [ ] **Step 2: Create error.rs**

```rust
use std::fmt;

#[derive(Debug, Clone)]
pub struct RuntimeError {
    pub message: String,
}

impl RuntimeError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self { message: msg.into() }
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RuntimeError: {}", self.message)
    }
}

impl std::error::Error for RuntimeError {}
```

- [ ] **Step 3: Create value.rs**

```rust
use std::collections::HashMap;
use std::fmt;
use coco_syntax::{FnDecl, Param, Block};

#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Null,
    List(Vec<Value>),
    Map(HashMap<String, Value>),
    Function(FunctionValue),
    BuiltinFn(BuiltinFnValue),
    Object(ObjectValue),
}

#[derive(Debug, Clone)]
pub struct FunctionValue {
    pub name: String,
    pub params: Vec<Param>,
    pub body: Block,
    pub closure_env_id: usize,
}

#[derive(Debug, Clone)]
pub struct BuiltinFnValue {
    pub name: String,
    pub arity: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct ObjectValue {
    pub class_name: String,
    pub fields: HashMap<String, Value>,
}

impl Value {
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::String(_) => "string",
            Value::Bool(_) => "bool",
            Value::Null => "null",
            Value::List(_) => "list",
            Value::Map(_) => "map",
            Value::Function(_) => "function",
            Value::BuiltinFn(_) => "builtin",
            Value::Object(o) => "object",
        }
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Null => false,
            Value::Int(0) => false,
            Value::Float(f) if *f == 0.0 => false,
            Value::String(s) if s.is_empty() => false,
            Value::List(l) if l.is_empty() => false,
            _ => true,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(v) => write!(f, "{}", v),
            Value::Float(v) => write!(f, "{}", v),
            Value::String(v) => write!(f, "{}", v),
            Value::Bool(v) => write!(f, "{}", v),
            Value::Null => write!(f, "null"),
            Value::List(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", item)?;
                }
                write!(f, "]")
            }
            Value::Map(map) => {
                write!(f, "{{")?;
                for (i, (k, v)) in map.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "\"{}\": {}", k, v)?;
                }
                write!(f, "}}")
            }
            Value::Function(fv) => write!(f, "<fn {}>", fv.name),
            Value::BuiltinFn(bv) => write!(f, "<builtin {}>", bv.name),
            Value::Object(o) => write!(f, "<{} instance>", o.class_name),
        }
    }
}
```

- [ ] **Step 4: Create env.rs**

```rust
use std::collections::HashMap;
use crate::value::Value;
use crate::error::RuntimeError;

#[derive(Debug, Clone)]
pub struct Environment {
    scopes: Vec<HashMap<String, Value>>,
}

impl Environment {
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
        }
    }

    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    pub fn define(&mut self, name: &str, value: Value) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), value);
        }
    }

    pub fn get(&self, name: &str) -> Result<&Value, RuntimeError> {
        for scope in self.scopes.iter().rev() {
            if let Some(val) = scope.get(name) {
                return Ok(val);
            }
        }
        Err(RuntimeError::new(format!("undefined variable: {}", name)))
    }

    pub fn set(&mut self, name: &str, value: Value) -> Result<(), RuntimeError> {
        for scope in self.scopes.iter_mut().rev() {
            if scope.contains_key(name) {
                scope.insert(name.to_string(), value);
                return Ok(());
            }
        }
        Err(RuntimeError::new(format!("undefined variable: {}", name)))
    }

    pub fn scope_depth(&self) -> usize {
        self.scopes.len()
    }
}
```

- [ ] **Step 5: Create lib.rs**

```rust
pub mod value;
pub mod env;
pub mod error;

pub use value::Value;
pub use env::Environment;
pub use error::RuntimeError;
```

- [ ] **Step 6: Add crate to workspace**

In root `Cargo.toml`, add `"crates/coco_interpreter"` to workspace members and add `coco_interpreter = { path = "crates/coco_interpreter" }` to workspace.dependencies.

- [ ] **Step 7: Verify it compiles**

Run: `cargo build -p coco_interpreter 2>&1`
Expected: success

- [ ] **Step 8: Commit**

```bash
git add crates/coco_interpreter/ Cargo.toml
git commit -m "feat(interpreter): scaffold crate with Value, Environment, RuntimeError"
```

---

### Task 2: Expression evaluator — literals and arithmetic

**Files:**
- Create: `crates/coco_interpreter/src/eval_expr.rs`
- Modify: `crates/coco_interpreter/src/lib.rs`
- Create: `crates/coco_interpreter/tests/eval_expr_test.rs`

- [ ] **Step 1: Write failing tests**

Create `crates/coco_interpreter/tests/eval_expr_test.rs`:

```rust
use coco_interpreter::{Interpreter, Value};

fn eval(src: &str) -> Value {
    let mut interp = Interpreter::new();
    interp.eval_source(src).unwrap()
}

#[test]
fn eval_int_literal() {
    assert!(matches!(eval("42;"), Value::Int(42)));
}

#[test]
fn eval_float_literal() {
    match eval("3.14;") {
        Value::Float(f) => assert!((f - 3.14).abs() < f64::EPSILON),
        other => panic!("expected float, got {:?}", other),
    }
}

#[test]
fn eval_string_literal() {
    assert!(matches!(eval("\"hello\";"), Value::String(ref s) if s == "hello"));
}

#[test]
fn eval_bool_literal() {
    assert!(matches!(eval("true;"), Value::Bool(true)));
    assert!(matches!(eval("false;"), Value::Bool(false)));
}

#[test]
fn eval_null_literal() {
    assert!(matches!(eval("null;"), Value::Null));
}

#[test]
fn eval_addition() {
    assert!(matches!(eval("1 + 2;"), Value::Int(3)));
}

#[test]
fn eval_subtraction() {
    assert!(matches!(eval("10 - 3;"), Value::Int(7)));
}

#[test]
fn eval_multiplication() {
    assert!(matches!(eval("4 * 5;"), Value::Int(20)));
}

#[test]
fn eval_division() {
    assert!(matches!(eval("10 / 3;"), Value::Int(3)));
}

#[test]
fn eval_modulo() {
    assert!(matches!(eval("10 % 3;"), Value::Int(1)));
}

#[test]
fn eval_power() {
    assert!(matches!(eval("2 ** 10;"), Value::Int(1024)));
}

#[test]
fn eval_float_arithmetic() {
    match eval("1.5 + 2.5;") {
        Value::Float(f) => assert!((f - 4.0).abs() < f64::EPSILON),
        other => panic!("expected float, got {:?}", other),
    }
}

#[test]
fn eval_comparison_lt() {
    assert!(matches!(eval("1 < 2;"), Value::Bool(true)));
    assert!(matches!(eval("2 < 1;"), Value::Bool(false)));
}

#[test]
fn eval_comparison_eq() {
    assert!(matches!(eval("1 == 1;"), Value::Bool(true)));
    assert!(matches!(eval("1 == 2;"), Value::Bool(false)));
}

#[test]
fn eval_logical_and() {
    assert!(matches!(eval("true && false;"), Value::Bool(false)));
    assert!(matches!(eval("true && true;"), Value::Bool(true)));
}

#[test]
fn eval_logical_or() {
    assert!(matches!(eval("false || true;"), Value::Bool(true)));
    assert!(matches!(eval("false || false;"), Value::Bool(false)));
}

#[test]
fn eval_unary_neg() {
    assert!(matches!(eval("-5;"), Value::Int(-5)));
}

#[test]
fn eval_unary_not() {
    assert!(matches!(eval("!true;"), Value::Bool(false)));
    assert!(matches!(eval("!false;"), Value::Bool(true)));
}

#[test]
fn eval_string_concat() {
    match eval("\"hello\" + \" world\";") {
        Value::String(s) => assert_eq!(s, "hello world"),
        other => panic!("expected string, got {:?}", other),
    }
}

#[test]
fn eval_precedence() {
    assert!(matches!(eval("2 + 3 * 4;"), Value::Int(14)));
}

#[test]
fn eval_grouped() {
    assert!(matches!(eval("(2 + 3) * 4;"), Value::Int(20)));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p coco_interpreter 2>&1`
Expected: compile error (Interpreter doesn't exist yet)

- [ ] **Step 3: Implement eval_expr.rs**

```rust
use coco_syntax::*;
use crate::env::Environment;
use crate::value::Value;
use crate::error::RuntimeError;

pub fn eval_expr(expr: &Expr, env: &mut Environment) -> Result<Value, RuntimeError> {
    match expr {
        Expr::Literal(lit) => eval_literal(lit),
        Expr::Ident(ident) => env.get(&ident.name).cloned(),
        Expr::Binary(b) => eval_binary(b, env),
        Expr::Unary(u) => eval_unary(u, env),
        Expr::Group(inner) => eval_expr(inner, env),
        Expr::Array(arr) => {
            let elements: Result<Vec<Value>, _> = arr.elements.iter()
                .map(|e| eval_expr(e, env))
                .collect();
            Ok(Value::List(elements?))
        }
        Expr::Object(obj) => {
            let mut map = std::collections::HashMap::new();
            for field in &obj.fields {
                let key = match &field.key {
                    ObjectKey::Ident(id) => id.name.clone(),
                    ObjectKey::String(s, _) => s.clone(),
                };
                let val = eval_expr(&field.value, env)?;
                map.insert(key, val);
            }
            Ok(Value::Map(map))
        }
        Expr::Call(call) => eval_call(call, env),
        Expr::Member(m) => eval_member(m, env),
        Expr::Index(idx) => eval_index(idx, env),
        Expr::NullCoalesce(nc) => {
            let left = eval_expr(&nc.left, env)?;
            if matches!(left, Value::Null) {
                eval_expr(&nc.right, env)
            } else {
                Ok(left)
            }
        }
        Expr::Ternary(t) => {
            let cond = eval_expr(&t.condition, env)?;
            if cond.is_truthy() {
                eval_expr(&t.then_expr, env)
            } else {
                eval_expr(&t.else_expr, env)
            }
        }
        Expr::This(_) | Expr::Dollar(_) => {
            env.get("this").cloned()
        }
        Expr::Pipe(p) => eval_pipe(p, env),
        _ => Err(RuntimeError::new(format!("unsupported expression: {:?}", std::mem::discriminant(expr)))),
    }
}

fn eval_literal(lit: &Literal) -> Result<Value, RuntimeError> {
    Ok(match lit {
        Literal::Int(v, _) => Value::Int(*v),
        Literal::Float(v, _) => Value::Float(*v),
        Literal::String(v, _) => Value::String(v.clone()),
        Literal::Char(v, _) => Value::String(v.to_string()),
        Literal::Bool(v, _) => Value::Bool(*v),
        Literal::Null(_) => Value::Null,
    })
}

fn eval_binary(b: &BinaryExpr, env: &mut Environment) -> Result<Value, RuntimeError> {
    // Short-circuit for logical operators
    if b.op == BinaryOp::And {
        let left = eval_expr(&b.left, env)?;
        if !left.is_truthy() { return Ok(Value::Bool(false)); }
        let right = eval_expr(&b.right, env)?;
        return Ok(Value::Bool(right.is_truthy()));
    }
    if b.op == BinaryOp::Or {
        let left = eval_expr(&b.left, env)?;
        if left.is_truthy() { return Ok(Value::Bool(true)); }
        let right = eval_expr(&b.right, env)?;
        return Ok(Value::Bool(right.is_truthy()));
    }

    // Assignment operators
    if b.op == BinaryOp::Assign {
        let val = eval_expr(&b.right, env)?;
        if let Expr::Ident(id) = &b.left {
            env.set(&id.name, val.clone())?;
            return Ok(val);
        }
        return Err(RuntimeError::new("invalid assignment target"));
    }

    let left = eval_expr(&b.left, env)?;
    let right = eval_expr(&b.right, env)?;

    match (&left, &right) {
        (Value::Int(l), Value::Int(r)) => eval_int_binary(*l, *r, b.op),
        (Value::Float(l), Value::Float(r)) => eval_float_binary(*l, *r, b.op),
        (Value::Int(l), Value::Float(r)) => eval_float_binary(*l as f64, *r, b.op),
        (Value::Float(l), Value::Int(r)) => eval_float_binary(*l, *r as f64, b.op),
        (Value::String(l), Value::String(r)) => eval_string_binary(l, r, b.op),
        _ => eval_generic_binary(&left, &right, b.op),
    }
}

fn eval_int_binary(l: i64, r: i64, op: BinaryOp) -> Result<Value, RuntimeError> {
    Ok(match op {
        BinaryOp::Add => Value::Int(l + r),
        BinaryOp::Sub => Value::Int(l - r),
        BinaryOp::Mul => Value::Int(l * r),
        BinaryOp::Div => {
            if r == 0 { return Err(RuntimeError::new("division by zero")); }
            Value::Int(l / r)
        }
        BinaryOp::Mod => {
            if r == 0 { return Err(RuntimeError::new("division by zero")); }
            Value::Int(l % r)
        }
        BinaryOp::Pow => Value::Int(l.pow(r as u32)),
        BinaryOp::Eq => Value::Bool(l == r),
        BinaryOp::Ne => Value::Bool(l != r),
        BinaryOp::Lt => Value::Bool(l < r),
        BinaryOp::Gt => Value::Bool(l > r),
        BinaryOp::Le => Value::Bool(l <= r),
        BinaryOp::Ge => Value::Bool(l >= r),
        BinaryOp::BitAnd => Value::Int(l & r),
        BinaryOp::BitOr => Value::Int(l | r),
        BinaryOp::BitXor => Value::Int(l ^ r),
        BinaryOp::Shl => Value::Int(l << r),
        BinaryOp::Shr => Value::Int(l >> r),
        _ => return Err(RuntimeError::new(format!("unsupported int op: {:?}", op))),
    })
}

fn eval_float_binary(l: f64, r: f64, op: BinaryOp) -> Result<Value, RuntimeError> {
    Ok(match op {
        BinaryOp::Add => Value::Float(l + r),
        BinaryOp::Sub => Value::Float(l - r),
        BinaryOp::Mul => Value::Float(l * r),
        BinaryOp::Div => Value::Float(l / r),
        BinaryOp::Mod => Value::Float(l % r),
        BinaryOp::Pow => Value::Float(l.powf(r)),
        BinaryOp::Eq => Value::Bool((l - r).abs() < f64::EPSILON),
        BinaryOp::Ne => Value::Bool((l - r).abs() >= f64::EPSILON),
        BinaryOp::Lt => Value::Bool(l < r),
        BinaryOp::Gt => Value::Bool(l > r),
        BinaryOp::Le => Value::Bool(l <= r),
        BinaryOp::Ge => Value::Bool(l >= r),
        _ => return Err(RuntimeError::new(format!("unsupported float op: {:?}", op))),
    })
}

fn eval_string_binary(l: &str, r: &str, op: BinaryOp) -> Result<Value, RuntimeError> {
    Ok(match op {
        BinaryOp::Add => Value::String(format!("{}{}", l, r)),
        BinaryOp::Eq => Value::Bool(l == r),
        BinaryOp::Ne => Value::Bool(l != r),
        _ => return Err(RuntimeError::new(format!("unsupported string op: {:?}", op))),
    })
}

fn eval_generic_binary(l: &Value, r: &Value, op: BinaryOp) -> Result<Value, RuntimeError> {
    match op {
        BinaryOp::Eq => Ok(Value::Bool(matches!((l, r), (Value::Null, Value::Null)))),
        BinaryOp::Ne => Ok(Value::Bool(!matches!((l, r), (Value::Null, Value::Null)))),
        _ => Err(RuntimeError::new(format!(
            "cannot apply {:?} to {} and {}", op, l.type_name(), r.type_name()
        ))),
    }
}

fn eval_unary(u: &UnaryExpr, env: &mut Environment) -> Result<Value, RuntimeError> {
    let val = eval_expr(&u.expr, env)?;
    match u.op {
        UnaryOp::Neg => match val {
            Value::Int(v) => Ok(Value::Int(-v)),
            Value::Float(v) => Ok(Value::Float(-v)),
            _ => Err(RuntimeError::new("cannot negate non-number")),
        },
        UnaryOp::Not => Ok(Value::Bool(!val.is_truthy())),
        UnaryOp::BitNot => match val {
            Value::Int(v) => Ok(Value::Int(!v)),
            _ => Err(RuntimeError::new("cannot bitwise-not non-int")),
        },
        _ => Err(RuntimeError::new(format!("unsupported unary op: {:?}", u.op))),
    }
}

pub fn eval_call(call: &CallExpr, env: &mut Environment) -> Result<Value, RuntimeError> {
    let callee = eval_expr(&call.callee, env)?;
    let mut args = Vec::new();
    for arg in &call.args {
        args.push(eval_expr(&arg.value, env)?);
    }

    match callee {
        Value::BuiltinFn(bf) => crate::builtins::call_builtin(&bf.name, args),
        Value::Function(fv) => {
            env.push_scope();
            for (i, param) in fv.params.iter().enumerate() {
                let val = args.get(i).cloned().unwrap_or(Value::Null);
                env.define(&param.name.name, val);
            }
            let result = crate::exec_stmt::exec_block(&fv.body, env);
            env.pop_scope();
            match result {
                Ok(()) => Ok(Value::Null),
                Err(e) if e.message.starts_with("__return__:") => {
                    // Extract return value — stored in env under __return__
                    // Actually use a ControlFlow approach instead
                    Ok(Value::Null)
                }
                Err(e) => Err(e),
            }
        }
        _ => Err(RuntimeError::new(format!("cannot call {}", callee.type_name()))),
    }
}

fn eval_member(m: &MemberExpr, env: &mut Environment) -> Result<Value, RuntimeError> {
    let obj = eval_expr(&m.object, env)?;
    let prop = &m.property.name;

    match &obj {
        Value::Object(o) => {
            o.fields.get(prop).cloned().ok_or_else(|| {
                RuntimeError::new(format!("undefined property: {}", prop))
            })
        }
        Value::Map(map) => {
            Ok(map.get(prop).cloned().unwrap_or(Value::Null))
        }
        Value::String(s) => match prop.as_str() {
            "length" => Ok(Value::Int(s.len() as i64)),
            "isEmpty" => Ok(Value::Bool(s.is_empty())),
            _ => Err(RuntimeError::new(format!("string has no property: {}", prop))),
        },
        Value::List(l) => match prop.as_str() {
            "length" => Ok(Value::Int(l.len() as i64)),
            _ => Err(RuntimeError::new(format!("list has no property: {}", prop))),
        },
        _ => Err(RuntimeError::new(format!("cannot access property on {}", obj.type_name()))),
    }
}

fn eval_index(idx: &IndexExpr, env: &mut Environment) -> Result<Value, RuntimeError> {
    let obj = eval_expr(&idx.object, env)?;
    let index = eval_expr(&idx.index, env)?;

    match (&obj, &index) {
        (Value::List(list), Value::Int(i)) => {
            let i = *i as usize;
            list.get(i).cloned().ok_or_else(|| {
                RuntimeError::new(format!("index {} out of bounds (len {})", i, list.len()))
            })
        }
        (Value::Map(map), Value::String(key)) => {
            Ok(map.get(key).cloned().unwrap_or(Value::Null))
        }
        _ => Err(RuntimeError::new("invalid index operation")),
    }
}

fn eval_pipe(p: &PipeExpr, env: &mut Environment) -> Result<Value, RuntimeError> {
    let left = eval_expr(&p.left, env)?;
    // For pipe, the right side should be a function — call it with left as argument
    let right = eval_expr(&p.right, env)?;
    match right {
        Value::Function(_) | Value::BuiltinFn(_) => {
            // Construct a synthetic call
            env.define("$$", left.clone());
            let result = eval_expr(&p.right, env);
            result
        }
        _ => Err(RuntimeError::new("pipe right-hand side must be a function")),
    }
}
```

- [ ] **Step 4: Add Interpreter struct to lib.rs**

Update `crates/coco_interpreter/src/lib.rs`:

```rust
pub mod value;
pub mod env;
pub mod error;
pub mod eval_expr;
pub mod exec_stmt;
pub mod exec_item;
pub mod builtins;

pub use value::Value;
pub use env::Environment;
pub use error::RuntimeError;

use coco_parser::Parser;
use coco_syntax::*;

pub struct Interpreter {
    pub env: Environment,
}

impl Interpreter {
    pub fn new() -> Self {
        let mut env = Environment::new();
        builtins::register_builtins(&mut env);
        Self { env }
    }

    pub fn eval_source(&mut self, source: &str) -> Result<Value, RuntimeError> {
        let mut parser = Parser::new(source);
        let program = parser.parse_program();
        self.exec_program(&program)
    }

    pub fn exec_program(&mut self, program: &Program) -> Result<Value, RuntimeError> {
        let mut last = Value::Null;
        for item in &program.items {
            last = exec_item::exec_item(item, &mut self.env)?;
        }
        Ok(last)
    }

    pub fn run_main(&mut self, source: &str) -> Result<Value, RuntimeError> {
        let mut parser = Parser::new(source);
        let program = parser.parse_program();
        // Register all top-level declarations
        for item in &program.items {
            exec_item::register_item(item, &mut self.env)?;
        }
        // Call main()
        let main_fn = self.env.get("main").cloned()?;
        match main_fn {
            Value::Function(fv) => {
                self.env.push_scope();
                let result = exec_stmt::exec_block(&fv.body, &mut self.env);
                self.env.pop_scope();
                match result {
                    Ok(()) => Ok(Value::Int(0)),
                    Err(ref e) if e.message.starts_with("return:") => {
                        let val_str = &e.message[7..];
                        Ok(Value::Int(val_str.parse().unwrap_or(0)))
                    }
                    Err(e) => Err(e),
                }
            }
            _ => Err(RuntimeError::new("main is not a function")),
        }
    }
}
```

- [ ] **Step 5: Add coco_parser dep to coco_interpreter Cargo.toml**

Add under `[dependencies]`:
```toml
coco_parser = { path = "../coco_parser" }
```

- [ ] **Step 6: Run tests — verify they fail meaningfully (missing modules)**

Run: `cargo test -p coco_interpreter 2>&1`
Expected: compile errors for missing exec_stmt, exec_item, builtins modules

- [ ] **Step 7: Commit (WIP — tests will pass after Task 3)**

```bash
git add crates/coco_interpreter/
git commit -m "feat(interpreter): add expression evaluator with arithmetic and comparisons"
```

---

### Task 3: Statement executor and builtins

**Files:**
- Create: `crates/coco_interpreter/src/exec_stmt.rs`
- Create: `crates/coco_interpreter/src/exec_item.rs`
- Create: `crates/coco_interpreter/src/builtins.rs`

- [ ] **Step 1: Create builtins.rs**

```rust
use crate::env::Environment;
use crate::value::{Value, BuiltinFnValue};
use crate::error::RuntimeError;

pub fn register_builtins(env: &mut Environment) {
    let builtins = vec!["print", "println", "len", "push", "pop", "toString", "parseInt", "parseFloat"];
    for name in builtins {
        env.define(name, Value::BuiltinFn(BuiltinFnValue {
            name: name.to_string(),
            arity: None,
        }));
    }
}

pub fn call_builtin(name: &str, args: Vec<Value>) -> Result<Value, RuntimeError> {
    match name {
        "print" => {
            let output: Vec<String> = args.iter().map(|a| format!("{}", a)).collect();
            println!("{}", output.join(" "));
            Ok(Value::Null)
        }
        "println" => {
            let output: Vec<String> = args.iter().map(|a| format!("{}", a)).collect();
            println!("{}", output.join(" "));
            Ok(Value::Null)
        }
        "len" => {
            match args.first() {
                Some(Value::String(s)) => Ok(Value::Int(s.len() as i64)),
                Some(Value::List(l)) => Ok(Value::Int(l.len() as i64)),
                Some(Value::Map(m)) => Ok(Value::Int(m.len() as i64)),
                _ => Err(RuntimeError::new("len() requires a string, list, or map")),
            }
        }
        "toString" => {
            match args.first() {
                Some(v) => Ok(Value::String(format!("{}", v))),
                None => Ok(Value::String(String::new())),
            }
        }
        "parseInt" => {
            match args.first() {
                Some(Value::String(s)) => {
                    Ok(Value::Int(s.parse().unwrap_or(0)))
                }
                Some(Value::Float(f)) => Ok(Value::Int(*f as i64)),
                Some(Value::Int(i)) => Ok(Value::Int(*i)),
                _ => Ok(Value::Int(0)),
            }
        }
        "parseFloat" => {
            match args.first() {
                Some(Value::String(s)) => {
                    Ok(Value::Float(s.parse().unwrap_or(0.0)))
                }
                Some(Value::Int(i)) => Ok(Value::Float(*i as f64)),
                Some(Value::Float(f)) => Ok(Value::Float(*f)),
                _ => Ok(Value::Float(0.0)),
            }
        }
        _ => Err(RuntimeError::new(format!("unknown builtin: {}", name))),
    }
}
```

- [ ] **Step 2: Create exec_stmt.rs**

```rust
use coco_syntax::*;
use crate::env::Environment;
use crate::value::Value;
use crate::error::RuntimeError;
use crate::eval_expr::eval_expr;

/// Sentinel error message prefix for return values
const RETURN_PREFIX: &str = "return:";

pub fn exec_block(block: &Block, env: &mut Environment) -> Result<(), RuntimeError> {
    for stmt in &block.stmts {
        exec_stmt(stmt, env)?;
    }
    Ok(())
}

pub fn exec_stmt(stmt: &Stmt, env: &mut Environment) -> Result<(), RuntimeError> {
    match stmt {
        Stmt::Expr(es) => {
            eval_expr(&es.expr, env)?;
            Ok(())
        }
        Stmt::If(if_stmt) => exec_if(if_stmt, env),
        Stmt::For(for_stmt) => exec_for(for_stmt, env),
        Stmt::While(while_stmt) => exec_while(while_stmt, env),
        Stmt::Loop(loop_stmt) => exec_loop(loop_stmt, env),
        Stmt::Return(ret) => {
            let val = match &ret.value {
                Some(expr) => eval_expr(expr, env)?,
                None => Value::Null,
            };
            Err(RuntimeError::new(format!("return:{}", serialize_value(&val))))
        }
        Stmt::Break(_) => Err(RuntimeError::new("break")),
        Stmt::Continue(_) => Err(RuntimeError::new("continue")),
        Stmt::Throw(t) => {
            let val = eval_expr(&t.value, env)?;
            Err(RuntimeError::new(format!("throw: {}", val)))
        }
        Stmt::Try(try_stmt) => exec_try(try_stmt, env),
        _ => Ok(()),
    }
}

fn exec_if(if_stmt: &IfStmt, env: &mut Environment) -> Result<(), RuntimeError> {
    let cond = eval_expr(&if_stmt.condition, env)?;
    if cond.is_truthy() {
        env.push_scope();
        exec_block(&if_stmt.then_block, env)?;
        env.pop_scope();
    } else {
        let mut handled = false;
        for else_if in &if_stmt.else_ifs {
            let ei_cond = eval_expr(&else_if.condition, env)?;
            if ei_cond.is_truthy() {
                env.push_scope();
                exec_block(&else_if.block, env)?;
                env.pop_scope();
                handled = true;
                break;
            }
        }
        if !handled {
            if let Some(else_block) = &if_stmt.else_block {
                env.push_scope();
                exec_block(else_block, env)?;
                env.pop_scope();
            }
        }
    }
    Ok(())
}

fn exec_for(for_stmt: &ForStmt, env: &mut Environment) -> Result<(), RuntimeError> {
    let iterable = eval_expr(&for_stmt.iterable, env)?;
    let items = match iterable {
        Value::List(items) => items,
        Value::String(s) => s.chars().map(|c| Value::String(c.to_string())).collect(),
        _ => return Err(RuntimeError::new("for-in requires iterable")),
    };

    env.push_scope();
    for item in items {
        env.define(&for_stmt.pattern.name, item);
        match exec_block(&for_stmt.body, env) {
            Ok(()) => {}
            Err(e) if e.message == "break" => break,
            Err(e) if e.message == "continue" => continue,
            Err(e) => { env.pop_scope(); return Err(e); }
        }
    }
    env.pop_scope();
    Ok(())
}

fn exec_while(while_stmt: &WhileStmt, env: &mut Environment) -> Result<(), RuntimeError> {
    loop {
        let cond = eval_expr(&while_stmt.condition, env)?;
        if !cond.is_truthy() { break; }
        env.push_scope();
        match exec_block(&while_stmt.body, env) {
            Ok(()) => {}
            Err(e) if e.message == "break" => { env.pop_scope(); break; }
            Err(e) if e.message == "continue" => { env.pop_scope(); continue; }
            Err(e) => { env.pop_scope(); return Err(e); }
        }
        env.pop_scope();
    }
    Ok(())
}

fn exec_loop(loop_stmt: &LoopStmt, env: &mut Environment) -> Result<(), RuntimeError> {
    loop {
        env.push_scope();
        match exec_block(&loop_stmt.body, env) {
            Ok(()) => {}
            Err(e) if e.message == "break" => { env.pop_scope(); break; }
            Err(e) if e.message == "continue" => { env.pop_scope(); continue; }
            Err(e) => { env.pop_scope(); return Err(e); }
        }
        env.pop_scope();
    }
    Ok(())
}

fn exec_try(try_stmt: &TryStmt, env: &mut Environment) -> Result<(), RuntimeError> {
    env.push_scope();
    let result = exec_block(&try_stmt.body, env);
    env.pop_scope();

    match result {
        Ok(()) => {}
        Err(e) if e.message.starts_with("throw:") => {
            if let Some(catch) = try_stmt.catches.first() {
                env.push_scope();
                let msg = e.message[6..].trim().to_string();
                env.define(&catch.param.name, Value::String(msg));
                exec_block(&catch.body, env)?;
                env.pop_scope();
            }
        }
        Err(e) => return Err(e),
    }

    if let Some(finally) = &try_stmt.finally {
        env.push_scope();
        exec_block(finally, env)?;
        env.pop_scope();
    }

    Ok(())
}

fn serialize_value(val: &Value) -> String {
    match val {
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::String(s) => format!("\"{}\"", s),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
        _ => format!("{}", val),
    }
}

pub fn deserialize_return_value(msg: &str) -> Value {
    let s = &msg[RETURN_PREFIX.len()..];
    if s == "null" { return Value::Null; }
    if s == "true" { return Value::Bool(true); }
    if s == "false" { return Value::Bool(false); }
    if let Ok(i) = s.parse::<i64>() { return Value::Int(i); }
    if let Ok(f) = s.parse::<f64>() { return Value::Float(f); }
    if s.starts_with('"') && s.ends_with('"') {
        return Value::String(s[1..s.len()-1].to_string());
    }
    Value::String(s.to_string())
}
```

- [ ] **Step 3: Create exec_item.rs**

```rust
use coco_syntax::*;
use crate::env::Environment;
use crate::value::{Value, FunctionValue};
use crate::error::RuntimeError;
use crate::eval_expr::eval_expr;
use crate::exec_stmt;

pub fn exec_item(item: &Item, env: &mut Environment) -> Result<Value, RuntimeError> {
    match item {
        Item::FnDecl(f) => {
            register_fn(f, env);
            Ok(Value::Null)
        }
        Item::LetDecl(l) => {
            let val = match &l.value {
                Some(expr) => eval_expr(expr, env)?,
                None => Value::Null,
            };
            env.define(&l.name.name, val.clone());
            Ok(val)
        }
        Item::ConstDecl(c) => {
            let val = eval_expr(&c.value, env)?;
            env.define(&c.name.name, val.clone());
            Ok(val)
        }
        Item::Stmt(stmt) => {
            exec_stmt::exec_stmt(stmt, env)?;
            Ok(Value::Null)
        }
        Item::ExprStmt(es) => {
            eval_expr(&es.expr, env)
        }
        _ => Ok(Value::Null),
    }
}

pub fn register_item(item: &Item, env: &mut Environment) -> Result<(), RuntimeError> {
    match item {
        Item::FnDecl(f) => {
            register_fn(f, env);
            Ok(())
        }
        Item::ConstDecl(c) => {
            let val = eval_expr(&c.value, env)?;
            env.define(&c.name.name, val);
            Ok(())
        }
        Item::LetDecl(l) => {
            let val = match &l.value {
                Some(expr) => eval_expr(expr, env)?,
                None => Value::Null,
            };
            env.define(&l.name.name, val);
            Ok(())
        }
        _ => Ok(()),
    }
}

fn register_fn(f: &FnDecl, env: &mut Environment) {
    let fv = FunctionValue {
        name: f.name.name.clone(),
        params: f.params.clone(),
        body: f.body.clone(),
        closure_env_id: env.scope_depth(),
    };
    env.define(&f.name.name, Value::Function(fv));
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p coco_interpreter 2>&1`
Expected: All expression tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/coco_interpreter/
git commit -m "feat(interpreter): add statement executor, builtins, and function calls"
```

---

### Task 4: Function calls with return values

**Files:**
- Modify: `crates/coco_interpreter/src/eval_expr.rs` (fix call to handle returns)
- Modify: `crates/coco_interpreter/src/lib.rs` (fix run_main)
- Create: `crates/coco_interpreter/tests/functions_test.rs`

- [ ] **Step 1: Write function tests**

Create `crates/coco_interpreter/tests/functions_test.rs`:

```rust
use coco_interpreter::{Interpreter, Value};

fn eval(src: &str) -> Value {
    let mut interp = Interpreter::new();
    interp.eval_source(src).unwrap()
}

#[test]
fn call_simple_function() {
    let result = eval("fn add(a, b) { return a + b; } add(2, 3);");
    assert!(matches!(result, Value::Int(5)));
}

#[test]
fn call_function_no_return() {
    let result = eval("fn noop() { } noop();");
    assert!(matches!(result, Value::Null));
}

#[test]
fn call_nested_function() {
    let result = eval("fn double(x) { return x * 2; } fn quad(x) { return double(double(x)); } quad(3);");
    assert!(matches!(result, Value::Int(12)));
}

#[test]
fn variable_scoping() {
    let result = eval("let x = 10; fn getX() { return x; } getX();");
    assert!(matches!(result, Value::Int(10)));
}

#[test]
fn function_with_default_like_behavior() {
    let result = eval("fn greet(name) { return name; } greet(\"Coco\");");
    assert!(matches!(result, Value::String(ref s) if s == "Coco"));
}

#[test]
fn recursive_function() {
    let result = eval("fn fib(n) { if n <= 1 { return n; } return fib(n - 1) + fib(n - 2); } fib(10);");
    assert!(matches!(result, Value::Int(55)));
}
```

- [ ] **Step 2: Fix eval_call to properly handle return values**

In `eval_expr.rs`, the `eval_call` for `Value::Function` needs to catch the "return:" sentinel:

```rust
Value::Function(fv) => {
    env.push_scope();
    for (i, param) in fv.params.iter().enumerate() {
        let val = args.get(i).cloned().unwrap_or(Value::Null);
        env.define(&param.name.name, val);
    }
    let result = crate::exec_stmt::exec_block(&fv.body, env);
    env.pop_scope();
    match result {
        Ok(()) => Ok(Value::Null),
        Err(e) if e.message.starts_with("return:") => {
            Ok(crate::exec_stmt::deserialize_return_value(&e.message))
        }
        Err(e) => Err(e),
    }
}
```

- [ ] **Step 3: Fix run_main similarly**

In lib.rs `run_main`, use the same return-value deserialization.

- [ ] **Step 4: Run tests**

Run: `cargo test -p coco_interpreter 2>&1`
Expected: All tests pass

- [ ] **Step 5: Commit**

```bash
git add crates/coco_interpreter/
git commit -m "feat(interpreter): function calls with return value propagation"
```

---

### Task 5: Variables — let/const, assignment, compound assignment

**Files:**
- Create: `crates/coco_interpreter/tests/variables_test.rs`
- Modify: `crates/coco_interpreter/src/eval_expr.rs` (compound assignment)

- [ ] **Step 1: Write variable tests**

Create `crates/coco_interpreter/tests/variables_test.rs`:

```rust
use coco_interpreter::{Interpreter, Value};

fn eval(src: &str) -> Value {
    let mut interp = Interpreter::new();
    interp.eval_source(src).unwrap()
}

#[test]
fn let_binding() {
    assert!(matches!(eval("let x = 42; x;"), Value::Int(42)));
}

#[test]
fn const_binding() {
    assert!(matches!(eval("const y = 99; y;"), Value::Int(99)));
}

#[test]
fn reassignment() {
    assert!(matches!(eval("let x = 1; x = 2; x;"), Value::Int(2)));
}

#[test]
fn compound_add_assign() {
    assert!(matches!(eval("let x = 10; x += 5; x;"), Value::Int(15)));
}

#[test]
fn compound_sub_assign() {
    assert!(matches!(eval("let x = 10; x -= 3; x;"), Value::Int(7)));
}

#[test]
fn compound_mul_assign() {
    assert!(matches!(eval("let x = 4; x *= 3; x;"), Value::Int(12)));
}

#[test]
fn multiple_reassignments() {
    assert!(matches!(eval("let x = 0; x += 1; x += 1; x += 1; x;"), Value::Int(3)));
}
```

- [ ] **Step 2: Add compound assignment handling in eval_binary**

In `eval_binary`, handle `AddAssign`, `SubAssign`, `MulAssign`, `DivAssign`, `ModAssign`:

```rust
// Before the general left/right evaluation, add:
if matches!(b.op, BinaryOp::AddAssign | BinaryOp::SubAssign | BinaryOp::MulAssign | BinaryOp::DivAssign | BinaryOp::ModAssign) {
    if let Expr::Ident(id) = &b.left {
        let current = env.get(&id.name)?.clone();
        let right = eval_expr(&b.right, env)?;
        let result = match b.op {
            BinaryOp::AddAssign => eval_int_binary_or_promote(&current, &right, BinaryOp::Add)?,
            BinaryOp::SubAssign => eval_int_binary_or_promote(&current, &right, BinaryOp::Sub)?,
            BinaryOp::MulAssign => eval_int_binary_or_promote(&current, &right, BinaryOp::Mul)?,
            BinaryOp::DivAssign => eval_int_binary_or_promote(&current, &right, BinaryOp::Div)?,
            BinaryOp::ModAssign => eval_int_binary_or_promote(&current, &right, BinaryOp::Mod)?,
            _ => unreachable!(),
        };
        env.set(&id.name, result.clone())?;
        return Ok(result);
    }
    return Err(RuntimeError::new("invalid compound assignment target"));
}
```

Add helper:
```rust
fn eval_int_binary_or_promote(left: &Value, right: &Value, op: BinaryOp) -> Result<Value, RuntimeError> {
    match (left, right) {
        (Value::Int(l), Value::Int(r)) => eval_int_binary(*l, *r, op),
        (Value::Float(l), Value::Float(r)) => eval_float_binary(*l, *r, op),
        (Value::Int(l), Value::Float(r)) => eval_float_binary(*l as f64, *r, op),
        (Value::Float(l), Value::Int(r)) => eval_float_binary(*l, *r as f64, op),
        (Value::String(l), Value::String(r)) => eval_string_binary(l, r, op),
        _ => Err(RuntimeError::new("type mismatch in compound assignment")),
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p coco_interpreter 2>&1`
Expected: All tests pass

- [ ] **Step 4: Commit**

```bash
git add crates/coco_interpreter/
git commit -m "feat(interpreter): let/const bindings and compound assignment"
```

---

### Task 6: Control flow — if/else, for, while, loop, break/continue

**Files:**
- Create: `crates/coco_interpreter/tests/control_flow_test.rs`

- [ ] **Step 1: Write control flow tests**

Create `crates/coco_interpreter/tests/control_flow_test.rs`:

```rust
use coco_interpreter::{Interpreter, Value};

fn eval(src: &str) -> Value {
    let mut interp = Interpreter::new();
    interp.eval_source(src).unwrap()
}

#[test]
fn if_true_branch() {
    assert!(matches!(eval("let x = 0; if true { x = 1; } x;"), Value::Int(1)));
}

#[test]
fn if_false_branch() {
    assert!(matches!(eval("let x = 0; if false { x = 1; } x;"), Value::Int(0)));
}

#[test]
fn if_else() {
    assert!(matches!(eval("let x = 0; if false { x = 1; } else { x = 2; } x;"), Value::Int(2)));
}

#[test]
fn while_loop() {
    assert!(matches!(eval("let x = 0; while x < 5 { x += 1; } x;"), Value::Int(5)));
}

#[test]
fn for_loop_list() {
    assert!(matches!(eval("let sum = 0; for n in [1, 2, 3, 4, 5] { sum += n; } sum;"), Value::Int(15)));
}

#[test]
fn loop_with_break() {
    assert!(matches!(eval("let x = 0; loop { x += 1; if x == 3 { break; } } x;"), Value::Int(3)));
}

#[test]
fn while_with_continue() {
    // Sum only odd numbers 1-10
    let result = eval("let sum = 0; let i = 0; while i < 10 { i += 1; if i % 2 == 0 { continue; } sum += i; } sum;");
    assert!(matches!(result, Value::Int(25)));
}

#[test]
fn nested_if() {
    let result = eval("let x = 10; let r = 0; if x > 5 { if x > 8 { r = 1; } } r;");
    assert!(matches!(result, Value::Int(1)));
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test -p coco_interpreter 2>&1`
Expected: All pass (exec_stmt already implements these)

- [ ] **Step 3: Commit**

```bash
git add crates/coco_interpreter/tests/
git commit -m "test(interpreter): add control flow tests"
```

---

### Task 7: Collections — list and map operations

**Files:**
- Modify: `crates/coco_interpreter/src/builtins.rs` (add push/pop)
- Modify: `crates/coco_interpreter/src/eval_expr.rs` (method calls on collections)
- Create: `crates/coco_interpreter/tests/collections_test.rs`

- [ ] **Step 1: Write collection tests**

Create `crates/coco_interpreter/tests/collections_test.rs`:

```rust
use coco_interpreter::{Interpreter, Value};

fn eval(src: &str) -> Value {
    let mut interp = Interpreter::new();
    interp.eval_source(src).unwrap()
}

#[test]
fn list_literal() {
    match eval("[1, 2, 3];") {
        Value::List(l) => assert_eq!(l.len(), 3),
        other => panic!("expected list, got {:?}", other),
    }
}

#[test]
fn list_index() {
    assert!(matches!(eval("const arr = [10, 20, 30]; arr[1];"), Value::Int(20)));
}

#[test]
fn list_length() {
    assert!(matches!(eval("const arr = [1, 2, 3, 4]; arr.length;"), Value::Int(4)));
}

#[test]
fn map_literal() {
    match eval("{\"x\": 1, \"y\": 2};") {
        Value::Map(m) => assert_eq!(m.len(), 2),
        other => panic!("expected map, got {:?}", other),
    }
}

#[test]
fn map_index() {
    assert!(matches!(eval("const m = {\"a\": 42}; m[\"a\"];"), Value::Int(42)));
}

#[test]
fn string_length() {
    assert!(matches!(eval("const s = \"hello\"; s.length;"), Value::Int(5)));
}

#[test]
fn for_over_list() {
    assert!(matches!(
        eval("let sum = 0; const nums = [1, 2, 3]; for n in nums { sum += n; } sum;"),
        Value::Int(6)
    ));
}
```

- [ ] **Step 2: Run tests and fix any failures**

Run: `cargo test -p coco_interpreter 2>&1`
Expected: All pass

- [ ] **Step 3: Commit**

```bash
git add crates/coco_interpreter/
git commit -m "test(interpreter): add collection tests (list, map, indexing)"
```

---

### Task 8: CLI integration — add `run` subcommand

**Files:**
- Modify: `crates/coco_cli/Cargo.toml`
- Modify: `crates/coco_cli/src/main.rs`

- [ ] **Step 1: Add coco_interpreter dependency to CLI**

In `crates/coco_cli/Cargo.toml`, add:
```toml
coco_interpreter = { path = "../coco_interpreter" }
```

- [ ] **Step 2: Add `run` subcommand to main.rs**

Read current main.rs and add a `Run` variant to the CLI enum and its handler:

```rust
// In the Command enum, add:
/// Run a .co file
Run {
    /// Path to .co file
    file: PathBuf,
},
```

Handler:
```rust
Command::Run { file } => {
    let source = std::fs::read_to_string(&file)
        .unwrap_or_else(|e| {
            eprintln!("Error reading {}: {}", file.display(), e);
            std::process::exit(1);
        });
    let mut interp = coco_interpreter::Interpreter::new();
    match interp.run_main(&source) {
        Ok(val) => {
            if let coco_interpreter::Value::Int(code) = val {
                std::process::exit(code as i32);
            }
        }
        Err(e) => {
            eprintln!("Runtime error: {}", e);
            std::process::exit(1);
        }
    }
}
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo build -p coco_cli 2>&1`
Expected: success

- [ ] **Step 4: Test with hello.co**

Run: `cargo run -- run examples/01-hello.co 2>&1`
Expected: prints "Hello, World!" and exits with code 0

- [ ] **Step 5: Commit**

```bash
git add crates/coco_cli/
git commit -m "feat(cli): add 'run' subcommand for executing .co files"
```

---

### Task 9: Integration test — run examples/01-hello.co and 02-variables.co

**Files:**
- Create: `crates/coco_interpreter/tests/integration_test.rs`

- [ ] **Step 1: Write integration tests that parse and execute real example files**

```rust
use coco_interpreter::Interpreter;

fn run_source(src: &str) -> Result<coco_interpreter::Value, coco_interpreter::RuntimeError> {
    let mut interp = Interpreter::new();
    interp.run_main(src)
}

#[test]
fn run_hello_world() {
    let src = r#"
fn main(): int {
    print("Hello, World!");
    return 0;
}
"#;
    let result = run_source(src).unwrap();
    assert!(matches!(result, coco_interpreter::Value::Int(0)));
}

#[test]
fn run_variables() {
    let src = r#"
fn main(): int {
    const pi = 3.14159;
    let counter = 0;
    counter += 1;
    counter += 1;
    let age = 28;
    let name = "Jericho";
    let active = true;
    print(counter);
    print(name);
    return 0;
}
"#;
    let result = run_source(src).unwrap();
    assert!(matches!(result, coco_interpreter::Value::Int(0)));
}

#[test]
fn run_functions() {
    let src = r#"
fn add(a, b) {
    return a + b;
}

fn main(): int {
    print(add(2, 3));
    return 0;
}
"#;
    let result = run_source(src).unwrap();
    assert!(matches!(result, coco_interpreter::Value::Int(0)));
}

#[test]
fn run_control_flow() {
    let src = r#"
fn fib(n) {
    if n <= 1 { return n; }
    return fib(n - 1) + fib(n - 2);
}

fn main(): int {
    const result = fib(10);
    print(result);
    return 0;
}
"#;
    let result = run_source(src).unwrap();
    assert!(matches!(result, coco_interpreter::Value::Int(0)));
}

#[test]
fn run_collections() {
    let src = r#"
fn main(): int {
    const numbers = [1, 2, 3, 4, 5];
    let sum = 0;
    for n in numbers {
        sum += n;
    }
    print(sum);
    return 0;
}
"#;
    let result = run_source(src).unwrap();
    assert!(matches!(result, coco_interpreter::Value::Int(0)));
}
```

- [ ] **Step 2: Run integration tests**

Run: `cargo test -p coco_interpreter -- integration 2>&1`
Expected: All pass

- [ ] **Step 3: Commit**

```bash
git add crates/coco_interpreter/tests/
git commit -m "test(interpreter): add integration tests for hello, variables, functions, collections"
```

---

### Task 10: Final verification and cleanup

- [ ] **Step 1: Run full test suite**

Run: `cargo test 2>&1`
Expected: All tests pass (lexer 10 + span 3 + diagnostics 1 + parser 30 + formatter 7 + interpreter ~40+)

- [ ] **Step 2: Run clippy**

Run: `cargo clippy 2>&1`
Expected: No errors

- [ ] **Step 3: Test CLI run command with examples**

Run: `cargo run -- run examples/01-hello.co 2>&1`
Expected: "Hello, World!"

- [ ] **Step 4: Update CLAUDE.md with interpreter info**

Add to the Build & Development Commands section:
```bash
cargo run -- run FILE.co     # Execute a .co file
```

Add `coco_interpreter` to the architecture table:
```
| `coco_interpreter` | Tree-walking interpreter. `Interpreter::new()` + `run_main(source)` |
```

Update the pipeline diagram to include the interpreter.

- [ ] **Step 5: Commit**

```bash
git add .
git commit -m "docs: update CLAUDE.md for Phase 3 interpreter"
```
