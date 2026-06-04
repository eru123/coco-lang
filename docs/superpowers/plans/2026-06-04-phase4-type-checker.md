# Phase 4: Type Checker Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a gradual type checker that enforces type correctness on annotated code while leaving unannotated code unchecked, with CLI integration including file resolution and a `typecheck` command.

**Architecture:** New `coco_typeck` crate with a two-pass algorithm (collect declarations, then check bodies). A `Ty` enum represents inferred/declared types independent from the AST `Type` enum. The CLI gains file resolution (optional `.co` extension), a `typecheck` command, and `run` gates on type errors with `--no-check` escape.

**Tech Stack:** Rust stable, coco_syntax AST, coco_span for Span, coco_parser for parsing

---

### Task 1: Create coco_typeck crate scaffold with Type representation

**Files:**
- Create: `crates/coco_typeck/Cargo.toml`
- Create: `crates/coco_typeck/src/lib.rs`
- Create: `crates/coco_typeck/src/types.rs`
- Create: `crates/coco_typeck/src/errors.rs`
- Modify: `Cargo.toml` (workspace root)

- [ ] **Step 1: Create Cargo.toml**

```toml
[package]
name = "coco_typeck"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[dependencies]
coco_syntax = { path = "../coco_syntax" }
coco_span = { path = "../coco_span" }
coco_parser = { path = "../coco_parser" }
```

- [ ] **Step 2: Create types.rs**

```rust
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ty {
    Int,
    Float,
    String,
    Bool,
    Null,
    Void,
    Never,
    Mixed,
    List(Box<Ty>),
    Map(Box<Ty>, Box<Ty>),
    Tuple(Vec<Ty>),
    Union(Vec<Ty>),
    Function { params: Vec<Ty>, ret: Box<Ty> },
    Named(String),
    Result(Box<Ty>, Box<Ty>),
    Unknown,
}

impl Ty {
    pub fn is_numeric(&self) -> bool {
        matches!(self, Ty::Int | Ty::Float)
    }

    pub fn is_mixed(&self) -> bool {
        matches!(self, Ty::Mixed | Ty::Unknown)
    }

    pub fn is_nullable(&self) -> bool {
        match self {
            Ty::Null => true,
            Ty::Union(types) => types.iter().any(|t| t == &Ty::Null),
            _ => false,
        }
    }

    pub fn strip_null(&self) -> Ty {
        match self {
            Ty::Union(types) => {
                let non_null: Vec<Ty> = types.iter().filter(|t| *t != &Ty::Null).cloned().collect();
                if non_null.len() == 1 {
                    non_null.into_iter().next().unwrap()
                } else if non_null.is_empty() {
                    Ty::Never
                } else {
                    Ty::Union(non_null)
                }
            }
            _ => self.clone(),
        }
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Int => write!(f, "int"),
            Ty::Float => write!(f, "float"),
            Ty::String => write!(f, "string"),
            Ty::Bool => write!(f, "bool"),
            Ty::Null => write!(f, "null"),
            Ty::Void => write!(f, "void"),
            Ty::Never => write!(f, "never"),
            Ty::Mixed => write!(f, "mixed"),
            Ty::List(t) => write!(f, "list<{}>", t),
            Ty::Map(k, v) => write!(f, "map<{}, {}>", k, v),
            Ty::Tuple(ts) => {
                write!(f, "tuple<")?;
                for (i, t) in ts.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", t)?;
                }
                write!(f, ">")
            }
            Ty::Union(ts) => {
                for (i, t) in ts.iter().enumerate() {
                    if i > 0 { write!(f, " | ")?; }
                    write!(f, "{}", t)?;
                }
                Ok(())
            }
            Ty::Function { params, ret } => {
                write!(f, "(")?;
                for (i, p) in params.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", p)?;
                }
                write!(f, ") => {}", ret)
            }
            Ty::Named(n) => write!(f, "{}", n),
            Ty::Result(ok, err) => write!(f, "Result<{}, {}>", ok, err),
            Ty::Unknown => write!(f, "unknown"),
        }
    }
}
```

- [ ] **Step 3: Create errors.rs**

```rust
use coco_span::Span;
use std::fmt;

#[derive(Debug, Clone)]
pub struct TypeckError {
    pub code: &'static str,
    pub message: String,
    pub span: Span,
    pub severity: Severity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

impl TypeckError {
    pub fn error(code: &'static str, message: impl Into<String>, span: Span) -> Self {
        Self { code, message: message.into(), span, severity: Severity::Error }
    }

    pub fn warning(code: &'static str, message: impl Into<String>, span: Span) -> Self {
        Self { code, message: message.into(), span, severity: Severity::Warning }
    }
}

impl fmt::Display for TypeckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let level = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        write!(f, "{}[{}]: {}", level, self.code, self.message)
    }
}
```

- [ ] **Step 4: Create lib.rs**

```rust
pub mod types;
pub mod errors;

pub use types::Ty;
pub use errors::{TypeckError, Severity};

use coco_syntax::Program;

pub struct TypeckResult {
    pub errors: Vec<TypeckError>,
    pub warnings: Vec<TypeckError>,
}

impl TypeckResult {
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

pub fn check(_program: &Program) -> TypeckResult {
    TypeckResult {
        errors: Vec::new(),
        warnings: Vec::new(),
    }
}
```

- [ ] **Step 5: Add to workspace**

In root `Cargo.toml`:
- Add `"crates/coco_typeck"` to `members`
- Add `coco_typeck = { path = "crates/coco_typeck" }` to `[workspace.dependencies]`

- [ ] **Step 6: Verify build**

Run: `cargo build -p coco_typeck 2>&1`
Expected: success

- [ ] **Step 7: Commit**

```bash
git add crates/coco_typeck/ Cargo.toml
git commit -m "feat(typeck): scaffold crate with Ty enum and TypeckError"
```

---

### Task 2: Type environment and AST-to-Ty conversion

**Files:**
- Create: `crates/coco_typeck/src/env.rs`
- Create: `crates/coco_typeck/src/convert.rs`

- [ ] **Step 1: Create env.rs — type environment with scoped bindings**

```rust
use std::collections::HashMap;
use crate::types::Ty;

#[derive(Debug, Clone)]
pub struct TypeEnv {
    scopes: Vec<HashMap<String, Ty>>,
    functions: HashMap<String, FnSig>,
}

#[derive(Debug, Clone)]
pub struct FnSig {
    pub params: Vec<(String, Ty)>,
    pub ret: Ty,
    pub is_fully_typed: bool,
}

impl TypeEnv {
    pub fn new() -> Self {
        let mut env = Self {
            scopes: vec![HashMap::new()],
            functions: HashMap::new(),
        };
        env.register_builtins();
        env
    }

    fn register_builtins(&mut self) {
        self.functions.insert("print".into(), FnSig {
            params: vec![("value".into(), Ty::Mixed)],
            ret: Ty::Void,
            is_fully_typed: true,
        });
        self.functions.insert("len".into(), FnSig {
            params: vec![("value".into(), Ty::Mixed)],
            ret: Ty::Int,
            is_fully_typed: true,
        });
        self.functions.insert("toString".into(), FnSig {
            params: vec![("value".into(), Ty::Mixed)],
            ret: Ty::String,
            is_fully_typed: true,
        });
        self.functions.insert("parseInt".into(), FnSig {
            params: vec![("value".into(), Ty::Mixed)],
            ret: Ty::Int,
            is_fully_typed: true,
        });
        self.functions.insert("parseFloat".into(), FnSig {
            params: vec![("value".into(), Ty::Mixed)],
            ret: Ty::Float,
            is_fully_typed: true,
        });
    }

    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    pub fn define(&mut self, name: &str, ty: Ty) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), ty);
        }
    }

    pub fn get(&self, name: &str) -> Option<&Ty> {
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Some(ty);
            }
        }
        None
    }

    pub fn define_fn(&mut self, name: &str, sig: FnSig) {
        self.functions.insert(name.to_string(), sig);
    }

    pub fn get_fn(&self, name: &str) -> Option<&FnSig> {
        self.functions.get(name)
    }
}
```

- [ ] **Step 2: Create convert.rs — convert AST Type nodes to Ty**

```rust
use coco_syntax::{Type as AstType, PrimitiveType};
use crate::types::Ty;

pub fn ast_type_to_ty(ast_type: &AstType) -> Ty {
    match ast_type {
        AstType::Primitive(prim, _) => match prim {
            PrimitiveType::Int => Ty::Int,
            PrimitiveType::Uint => Ty::Int,
            PrimitiveType::Float => Ty::Float,
            PrimitiveType::Bool => Ty::Bool,
            PrimitiveType::String => Ty::String,
            PrimitiveType::Char => Ty::String,
            PrimitiveType::Byte => Ty::Int,
            PrimitiveType::Null => Ty::Null,
            PrimitiveType::Void => Ty::Void,
            PrimitiveType::Never => Ty::Never,
            PrimitiveType::Mixed => Ty::Mixed,
        },
        AstType::Named(named) => Ty::Named(named.name.name.clone()),
        AstType::Union(u) => {
            let types: Vec<Ty> = u.types.iter().map(ast_type_to_ty).collect();
            Ty::Union(types)
        }
        AstType::Intersection(_) => Ty::Mixed,
        AstType::List(l) => Ty::List(Box::new(ast_type_to_ty(&l.element_type))),
        AstType::Map(m) => Ty::Map(
            Box::new(ast_type_to_ty(&m.key_type)),
            Box::new(ast_type_to_ty(&m.value_type)),
        ),
        AstType::Tuple(t) => Ty::Tuple(t.element_types.iter().map(ast_type_to_ty).collect()),
        AstType::Result(r) => Ty::Result(
            Box::new(ast_type_to_ty(&r.ok_type)),
            Box::new(ast_type_to_ty(&r.err_type)),
        ),
        AstType::Function(f) => Ty::Function {
            params: f.param_types.iter().map(ast_type_to_ty).collect(),
            ret: Box::new(ast_type_to_ty(&f.return_type)),
        },
    }
}
```

- [ ] **Step 3: Update lib.rs to export new modules**

Add to `crates/coco_typeck/src/lib.rs`:
```rust
pub mod env;
pub mod convert;
```

- [ ] **Step 4: Verify build**

Run: `cargo build -p coco_typeck 2>&1`
Expected: success

- [ ] **Step 5: Commit**

```bash
git add crates/coco_typeck/src/
git commit -m "feat(typeck): add TypeEnv and AST-to-Ty conversion"
```

---

### Task 3: Expression type inference

**Files:**
- Create: `crates/coco_typeck/src/infer.rs`
- Create: `crates/coco_typeck/tests/infer_test.rs`

- [ ] **Step 1: Write tests for expression type inference**

Create `crates/coco_typeck/tests/infer_test.rs`:

```rust
use coco_parser::Parser;
use coco_typeck::{check, Ty};

fn check_source(src: &str) -> coco_typeck::TypeckResult {
    let mut parser = Parser::new(src);
    let program = parser.parse_program();
    check(&program)
}

#[test]
fn no_errors_on_untyped_code() {
    let result = check_source("fn add(a, b) { return a + b; }");
    assert!(!result.has_errors());
}

#[test]
fn no_errors_on_correct_typed_code() {
    let result = check_source("fn add(a: int, b: int): int { return a + b; }");
    assert!(!result.has_errors());
}

#[test]
fn error_on_type_mismatch_in_assignment() {
    let result = check_source("fn test() { let x: int = \"hello\"; }");
    assert!(result.has_errors());
    assert_eq!(result.errors[0].code, "T001");
}

#[test]
fn error_on_incompatible_arithmetic() {
    let result = check_source("fn test() { let x: int = 1 + \"two\"; }");
    assert!(result.has_errors());
    assert_eq!(result.errors[0].code, "T006");
}

#[test]
fn no_error_on_string_concat() {
    let result = check_source("fn test() { let x: string = \"a\" + \"b\"; }");
    assert!(!result.has_errors());
}

#[test]
fn error_on_wrong_return_type() {
    let result = check_source("fn test(): int { return \"hello\"; }");
    assert!(result.has_errors());
    assert_eq!(result.errors[0].code, "T001");
}

#[test]
fn no_error_on_float_promotion() {
    let result = check_source("fn test(): float { return 1 + 2.0; }");
    assert!(!result.has_errors());
}

#[test]
fn error_on_argument_type_mismatch() {
    let result = check_source("fn add(a: int, b: int): int { return a + b; }\nfn test() { add(1, \"x\"); }");
    assert!(result.has_errors());
    assert_eq!(result.errors[0].code, "T001");
}

#[test]
fn error_on_argument_count_mismatch() {
    let result = check_source("fn add(a: int, b: int): int { return a + b; }\nfn test() { add(1); }");
    assert!(result.has_errors());
    assert_eq!(result.errors[0].code, "T002");
}

#[test]
fn no_error_mixed_typed_untyped() {
    let result = check_source("fn untyped(x) { return x; }\nfn typed(a: int): int { return a + untyped(1); }");
    // untyped returns mixed — mixed + int is allowed (gradual)
    assert!(!result.has_errors());
}

#[test]
fn infer_list_element_type() {
    let result = check_source("fn test() { let x: list<int> = [1, 2, \"three\"]; }");
    assert!(result.has_errors());
    assert_eq!(result.errors[0].code, "T001");
}

#[test]
fn null_coalesce_strips_null() {
    let result = check_source("fn test(): int { let x: int|null = null; return x ?? 0; }");
    assert!(!result.has_errors());
}
```

- [ ] **Step 2: Create infer.rs — infer type of an expression**

```rust
use coco_syntax::*;
use crate::types::Ty;
use crate::env::TypeEnv;
use crate::convert::ast_type_to_ty;

pub fn infer_expr(expr: &Expr, env: &TypeEnv) -> Ty {
    match expr {
        Expr::Literal(lit) => infer_literal(lit),
        Expr::Ident(ident) => env.get(&ident.name).cloned().unwrap_or(Ty::Mixed),
        Expr::Binary(bin) => infer_binary(bin, env),
        Expr::Unary(un) => infer_unary(un, env),
        Expr::Call(call) => infer_call(call, env),
        Expr::Index(idx) => infer_index(idx, env),
        Expr::Member(m) => infer_member(m, env),
        Expr::Array(arr) => infer_array(arr, env),
        Expr::Object(obj) => infer_object(obj, env),
        Expr::Group(inner) => infer_expr(inner, env),
        Expr::NullCoalesce(nc) => {
            let left = infer_expr(&nc.left, env);
            let right = infer_expr(&nc.right, env);
            // ?? strips null from left, result is non-null type or right type
            if left.is_nullable() {
                right
            } else {
                left
            }
        }
        Expr::Ternary(t) => {
            let then_ty = infer_expr(&t.then_expr, env);
            let else_ty = infer_expr(&t.else_expr, env);
            if then_ty == else_ty { then_ty } else { Ty::Union(vec![then_ty, else_ty]) }
        }
        Expr::Elvis(e) => {
            let left = infer_expr(&e.left, env);
            let right = infer_expr(&e.right, env);
            if left == right { left } else { Ty::Union(vec![left, right]) }
        }
        Expr::Lambda(_) => Ty::Mixed,
        Expr::This(_) | Expr::Dollar(_) => Ty::Mixed,
        Expr::Pipe(p) => infer_expr(&p.right, env),
        _ => Ty::Mixed,
    }
}

fn infer_literal(lit: &Literal) -> Ty {
    match lit {
        Literal::Int(_, _) => Ty::Int,
        Literal::Float(_, _) => Ty::Float,
        Literal::String(_, _) => Ty::String,
        Literal::Char(_, _) => Ty::String,
        Literal::Bool(_, _) => Ty::Bool,
        Literal::Null(_) => Ty::Null,
    }
}

fn infer_binary(bin: &BinaryExpr, env: &TypeEnv) -> Ty {
    let left = infer_expr(&bin.left, env);
    let right = infer_expr(&bin.right, env);

    match bin.op {
        // Arithmetic
        BinaryOp::Add => {
            if left == Ty::String || right == Ty::String {
                Ty::String
            } else if left == Ty::Float || right == Ty::Float {
                Ty::Float
            } else if left == Ty::Int && right == Ty::Int {
                Ty::Int
            } else if left.is_mixed() || right.is_mixed() {
                Ty::Mixed
            } else {
                Ty::Mixed
            }
        }
        BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod | BinaryOp::Pow => {
            if left == Ty::Float || right == Ty::Float {
                Ty::Float
            } else if left == Ty::Int && right == Ty::Int {
                Ty::Int
            } else if left.is_mixed() || right.is_mixed() {
                Ty::Mixed
            } else {
                Ty::Mixed
            }
        }
        // Comparison
        BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Gt
        | BinaryOp::Le | BinaryOp::Ge | BinaryOp::Spaceship => Ty::Bool,
        // Logical
        BinaryOp::And | BinaryOp::Or => Ty::Bool,
        // Bitwise
        BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor
        | BinaryOp::Shl | BinaryOp::Shr => {
            if left == Ty::Bool && right == Ty::Bool { Ty::Bool } else { Ty::Int }
        }
        // Assignment
        BinaryOp::Assign | BinaryOp::AddAssign | BinaryOp::SubAssign
        | BinaryOp::MulAssign | BinaryOp::DivAssign | BinaryOp::ModAssign
        | BinaryOp::PowAssign | BinaryOp::ShlAssign | BinaryOp::ShrAssign
        | BinaryOp::BitAndAssign | BinaryOp::BitOrAssign | BinaryOp::BitXorAssign => {
            infer_expr(&bin.left, env)
        }
        // Range
        BinaryOp::Range | BinaryOp::RangeInclusive => Ty::List(Box::new(Ty::Int)),
        _ => Ty::Mixed,
    }
}

fn infer_unary(un: &UnaryExpr, env: &TypeEnv) -> Ty {
    let inner = infer_expr(&un.expr, env);
    match un.op {
        UnaryOp::Neg => {
            if inner == Ty::Float { Ty::Float } else { Ty::Int }
        }
        UnaryOp::Not => Ty::Bool,
        UnaryOp::BitNot => Ty::Int,
        _ => inner,
    }
}

fn infer_call(call: &CallExpr, env: &TypeEnv) -> Ty {
    if let Expr::Ident(ident) = &call.callee {
        if let Some(sig) = env.get_fn(&ident.name) {
            return sig.ret.clone();
        }
    }
    Ty::Mixed
}

fn infer_index(idx: &IndexExpr, env: &TypeEnv) -> Ty {
    let obj_ty = infer_expr(&idx.object, env);
    match obj_ty {
        Ty::List(elem) => *elem,
        Ty::Map(_, val) => *val,
        _ => Ty::Mixed,
    }
}

fn infer_member(m: &MemberExpr, env: &TypeEnv) -> Ty {
    let obj_ty = infer_expr(&m.object, env);
    match (&obj_ty, m.property.name.as_str()) {
        (Ty::String, "length") => Ty::Int,
        (Ty::List(_), "length") => Ty::Int,
        _ => Ty::Mixed,
    }
}

fn infer_array(arr: &ArrayLiteral, env: &TypeEnv) -> Ty {
    if arr.elements.is_empty() {
        return Ty::List(Box::new(Ty::Mixed));
    }
    let first = infer_expr(&arr.elements[0], env);
    Ty::List(Box::new(first))
}

fn infer_object(_obj: &ObjectLiteral, _env: &TypeEnv) -> Ty {
    Ty::Map(Box::new(Ty::String), Box::new(Ty::Mixed))
}
```

- [ ] **Step 3: Update lib.rs to export infer**

Add `pub mod infer;` to lib.rs.

- [ ] **Step 4: Verify build**

Run: `cargo build -p coco_typeck 2>&1`
Expected: success

- [ ] **Step 5: Commit**

```bash
git add crates/coco_typeck/
git commit -m "feat(typeck): add expression type inference"
```

---

### Task 4: Type checking logic — check_expr, check_stmt, check_item

**Files:**
- Create: `crates/coco_typeck/src/check_expr.rs`
- Create: `crates/coco_typeck/src/check_stmt.rs`
- Create: `crates/coco_typeck/src/check_item.rs`
- Create: `crates/coco_typeck/src/unify.rs`
- Modify: `crates/coco_typeck/src/lib.rs`

- [ ] **Step 1: Create unify.rs — type compatibility checking**

```rust
use crate::types::Ty;

pub fn is_assignable(target: &Ty, value: &Ty) -> bool {
    if target == value {
        return true;
    }
    if target.is_mixed() || value.is_mixed() {
        return true;
    }
    if *value == Ty::Unknown {
        return true;
    }
    // int assignable to float
    if *target == Ty::Float && *value == Ty::Int {
        return true;
    }
    // null assignable to nullable
    if *value == Ty::Null {
        return target.is_nullable();
    }
    // Union: value must be assignable to at least one member
    if let Ty::Union(targets) = target {
        return targets.iter().any(|t| is_assignable(t, value));
    }
    // Value is union: all members must be assignable to target
    if let Ty::Union(values) = value {
        return values.iter().all(|v| is_assignable(target, v));
    }
    // List covariance
    if let (Ty::List(target_elem), Ty::List(value_elem)) = (target, value) {
        return is_assignable(target_elem, value_elem);
    }
    // Map covariance
    if let (Ty::Map(tk, tv), Ty::Map(vk, vv)) = (target, value) {
        return is_assignable(tk, vk) && is_assignable(tv, vv);
    }
    false
}
```

- [ ] **Step 2: Create check_expr.rs — validate expression types**

```rust
use coco_syntax::*;
use crate::types::Ty;
use crate::env::TypeEnv;
use crate::errors::TypeckError;
use crate::infer::infer_expr;
use crate::unify::is_assignable;

pub fn check_expr(expr: &Expr, env: &TypeEnv, errors: &mut Vec<TypeckError>) -> Ty {
    match expr {
        Expr::Binary(bin) => check_binary(bin, env, errors),
        Expr::Call(call) => check_call(call, env, errors),
        _ => infer_expr(expr, env),
    }
}

fn check_binary(bin: &BinaryExpr, env: &TypeEnv, errors: &mut Vec<TypeckError>) -> Ty {
    let left = check_expr(&bin.left, env, errors);
    let right = check_expr(&bin.right, env, errors);

    // Skip checks if either side is mixed/unknown
    if left.is_mixed() || right.is_mixed() {
        return infer_expr(&Expr::Binary(Box::new(bin.clone())), env);
    }

    match bin.op {
        BinaryOp::Add => {
            if left == Ty::String && right == Ty::String {
                return Ty::String;
            }
            if left.is_numeric() && right.is_numeric() {
                return if left == Ty::Float || right == Ty::Float { Ty::Float } else { Ty::Int };
            }
            if left == Ty::String || right == Ty::String {
                return Ty::String;
            }
            errors.push(TypeckError::error(
                "T006",
                format!("cannot apply + to {} and {}", left, right),
                bin.span,
            ));
            Ty::Mixed
        }
        BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod | BinaryOp::Pow => {
            if !left.is_numeric() || !right.is_numeric() {
                errors.push(TypeckError::error(
                    "T006",
                    format!("operator requires numeric operands, got {} and {}", left, right),
                    bin.span,
                ));
                return Ty::Mixed;
            }
            if left == Ty::Float || right == Ty::Float { Ty::Float } else { Ty::Int }
        }
        BinaryOp::Eq | BinaryOp::Ne => Ty::Bool,
        BinaryOp::Lt | BinaryOp::Gt | BinaryOp::Le | BinaryOp::Ge => {
            if left != right && !(left.is_numeric() && right.is_numeric()) {
                errors.push(TypeckError::error(
                    "T006",
                    format!("cannot compare {} with {}", left, right),
                    bin.span,
                ));
            }
            Ty::Bool
        }
        _ => infer_expr(&Expr::Binary(Box::new(bin.clone())), env),
    }
}

fn check_call(call: &CallExpr, env: &TypeEnv, errors: &mut Vec<TypeckError>) -> Ty {
    if let Expr::Ident(ident) = &call.callee {
        if let Some(sig) = env.get_fn(&ident.name) {
            if !sig.is_fully_typed {
                return sig.ret.clone();
            }
            // Check argument count
            let expected = sig.params.len();
            let got = call.args.len();
            // Allow variadic for builtins with mixed params
            if expected != got && !(expected == 1 && sig.params[0].1 == Ty::Mixed) {
                errors.push(TypeckError::error(
                    "T002",
                    format!("expected {} arguments, got {}", expected, got),
                    call.span,
                ));
                return sig.ret.clone();
            }
            // Check argument types
            for (i, arg) in call.args.iter().enumerate() {
                if i >= sig.params.len() { break; }
                let param_ty = &sig.params[i].1;
                if param_ty.is_mixed() { continue; }
                let arg_ty = check_expr(&arg.value, env, errors);
                if !is_assignable(param_ty, &arg_ty) {
                    errors.push(TypeckError::error(
                        "T001",
                        format!("expected {}, got {}", param_ty, arg_ty),
                        arg.span,
                    ));
                }
            }
            return sig.ret.clone();
        }
    }
    // Check args even for unknown callees
    for arg in &call.args {
        check_expr(&arg.value, env, errors);
    }
    Ty::Mixed
}
```

- [ ] **Step 3: Create check_stmt.rs**

```rust
use coco_syntax::*;
use crate::types::Ty;
use crate::env::TypeEnv;
use crate::errors::TypeckError;
use crate::check_expr::check_expr;
use crate::convert::ast_type_to_ty;
use crate::unify::is_assignable;

pub fn check_stmt(stmt: &Stmt, env: &mut TypeEnv, ret_ty: &Option<Ty>, errors: &mut Vec<TypeckError>) {
    match stmt {
        Stmt::Item(item) => check_block_item(item, env, ret_ty, errors),
        Stmt::Expr(es) => { check_expr(&es.expr, env, errors); }
        Stmt::If(if_stmt) => {
            check_expr(&if_stmt.condition, env, errors);
            check_block(&if_stmt.then_block, env, ret_ty, errors);
            for ei in &if_stmt.else_ifs {
                check_expr(&ei.condition, env, errors);
                check_block(&ei.block, env, ret_ty, errors);
            }
            if let Some(eb) = &if_stmt.else_block {
                check_block(eb, env, ret_ty, errors);
            }
        }
        Stmt::For(for_stmt) => {
            check_expr(&for_stmt.iterable, env, errors);
            env.push_scope();
            env.define(&for_stmt.pattern.name, Ty::Mixed);
            check_block(&for_stmt.body, env, ret_ty, errors);
            env.pop_scope();
        }
        Stmt::While(while_stmt) => {
            check_expr(&while_stmt.condition, env, errors);
            check_block(&while_stmt.body, env, ret_ty, errors);
        }
        Stmt::Loop(loop_stmt) => {
            check_block(&loop_stmt.body, env, ret_ty, errors);
        }
        Stmt::Return(ret) => {
            if let Some(expected) = ret_ty {
                if !expected.is_mixed() {
                    if let Some(expr) = &ret.value {
                        let got = check_expr(expr, env, errors);
                        if !is_assignable(expected, &got) {
                            errors.push(TypeckError::error(
                                "T001",
                                format!("expected return type {}, got {}", expected, got),
                                ret.span,
                            ));
                        }
                    } else if *expected != Ty::Void {
                        errors.push(TypeckError::error(
                            "T005",
                            format!("expected return type {}, got void", expected),
                            ret.span,
                        ));
                    }
                }
            }
        }
        Stmt::Throw(t) => { check_expr(&t.value, env, errors); }
        Stmt::Try(try_stmt) => {
            check_block(&try_stmt.body, env, ret_ty, errors);
            for catch in &try_stmt.catches {
                env.push_scope();
                env.define(&catch.param.name, Ty::Mixed);
                check_block(&catch.body, env, ret_ty, errors);
                env.pop_scope();
            }
            if let Some(finally) = &try_stmt.finally {
                check_block(finally, env, ret_ty, errors);
            }
        }
        _ => {}
    }
}

pub fn check_block(block: &Block, env: &mut TypeEnv, ret_ty: &Option<Ty>, errors: &mut Vec<TypeckError>) {
    for stmt in &block.stmts {
        check_stmt(stmt, env, ret_ty, errors);
    }
}

fn check_block_item(item: &Item, env: &mut TypeEnv, ret_ty: &Option<Ty>, errors: &mut Vec<TypeckError>) {
    match item {
        Item::LetDecl(decl) => {
            let declared_ty = decl.type_ann.as_ref().map(ast_type_to_ty);
            if let Some(init) = &decl.value {
                let init_ty = check_expr(init, env, errors);
                if let Some(ref target) = declared_ty {
                    if !target.is_mixed() && !is_assignable(target, &init_ty) {
                        errors.push(TypeckError::error(
                            "T001",
                            format!("expected {}, got {}", target, init_ty),
                            decl.span,
                        ));
                    }
                }
                env.define(&decl.name.name, declared_ty.unwrap_or(init_ty));
            } else {
                env.define(&decl.name.name, declared_ty.unwrap_or(Ty::Mixed));
            }
        }
        Item::ConstDecl(decl) => {
            let declared_ty = decl.type_ann.as_ref().map(ast_type_to_ty);
            let init_ty = check_expr(&decl.value, env, errors);
            if let Some(ref target) = declared_ty {
                if !target.is_mixed() && !is_assignable(target, &init_ty) {
                    errors.push(TypeckError::error(
                        "T001",
                        format!("expected {}, got {}", target, init_ty),
                        decl.span,
                    ));
                }
            }
            env.define(&decl.name.name, declared_ty.unwrap_or(init_ty));
        }
        Item::FnDecl(fn_decl) => {
            crate::check_item::check_fn_body(fn_decl, env, errors);
        }
        _ => {}
    }
}
```

- [ ] **Step 4: Create check_item.rs — top-level declaration checking**

```rust
use coco_syntax::*;
use crate::types::Ty;
use crate::env::{TypeEnv, FnSig};
use crate::errors::TypeckError;
use crate::convert::ast_type_to_ty;
use crate::check_stmt::check_block;

pub fn collect_items(program: &Program, env: &mut TypeEnv) {
    for item in &program.items {
        if let Item::FnDecl(fn_decl) = item {
            let params: Vec<(String, Ty)> = fn_decl.params.iter().map(|p| {
                let ty = p.type_ann.as_ref().map(ast_type_to_ty).unwrap_or(Ty::Mixed);
                (p.name.name.clone(), ty)
            }).collect();
            let ret = fn_decl.return_type.as_ref().map(ast_type_to_ty).unwrap_or(Ty::Mixed);
            let is_fully_typed = fn_decl.params.iter().all(|p| p.type_ann.is_some())
                && fn_decl.return_type.is_some();
            env.define_fn(&fn_decl.name.name, FnSig { params, ret, is_fully_typed });
        }
    }
}

pub fn check_items(program: &Program, env: &mut TypeEnv, errors: &mut Vec<TypeckError>) {
    for item in &program.items {
        match item {
            Item::FnDecl(fn_decl) => check_fn_body(fn_decl, env, errors),
            Item::LetDecl(decl) => {
                let declared_ty = decl.type_ann.as_ref().map(ast_type_to_ty);
                if let Some(init) = &decl.value {
                    let init_ty = crate::check_expr::check_expr(init, env, errors);
                    if let Some(ref target) = declared_ty {
                        if !target.is_mixed() && !crate::unify::is_assignable(target, &init_ty) {
                            errors.push(TypeckError::error(
                                "T001",
                                format!("expected {}, got {}", target, init_ty),
                                decl.span,
                            ));
                        }
                    }
                    env.define(&decl.name.name, declared_ty.unwrap_or(init_ty));
                } else {
                    env.define(&decl.name.name, declared_ty.unwrap_or(Ty::Mixed));
                }
            }
            Item::ConstDecl(decl) => {
                let declared_ty = decl.type_ann.as_ref().map(ast_type_to_ty);
                let init_ty = crate::check_expr::check_expr(&decl.value, env, errors);
                if let Some(ref target) = declared_ty {
                    if !target.is_mixed() && !crate::unify::is_assignable(target, &init_ty) {
                        errors.push(TypeckError::error(
                            "T001",
                            format!("expected {}, got {}", target, init_ty),
                            decl.span,
                        ));
                    }
                }
                env.define(&decl.name.name, declared_ty.unwrap_or(init_ty));
            }
            _ => {}
        }
    }
}

pub fn check_fn_body(fn_decl: &FnDecl, env: &mut TypeEnv, errors: &mut Vec<TypeckError>) {
    let has_any_annotation = fn_decl.params.iter().any(|p| p.type_ann.is_some())
        || fn_decl.return_type.is_some();
    if !has_any_annotation {
        return;
    }

    env.push_scope();
    for param in &fn_decl.params {
        let ty = param.type_ann.as_ref().map(ast_type_to_ty).unwrap_or(Ty::Mixed);
        env.define(&param.name.name, ty);
    }
    let ret_ty = fn_decl.return_type.as_ref().map(ast_type_to_ty);
    check_block(&fn_decl.body, env, &ret_ty, errors);
    env.pop_scope();
}
```

- [ ] **Step 5: Wire up lib.rs with full check() implementation**

Replace the stub `check()` in lib.rs:

```rust
pub mod types;
pub mod errors;
pub mod env;
pub mod convert;
pub mod infer;
pub mod unify;
pub mod check_expr;
pub mod check_stmt;
pub mod check_item;

pub use types::Ty;
pub use errors::{TypeckError, Severity};

use coco_syntax::Program;
use env::TypeEnv;

pub struct TypeckResult {
    pub errors: Vec<TypeckError>,
    pub warnings: Vec<TypeckError>,
}

impl TypeckResult {
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

pub fn check(program: &Program) -> TypeckResult {
    let mut env = TypeEnv::new();
    let mut errors = Vec::new();

    // Pass 1: collect all function signatures
    check_item::collect_items(program, &mut env);

    // Pass 2: check bodies
    check_item::check_items(program, &mut env, &mut errors);

    let warnings = Vec::new();
    TypeckResult { errors, warnings }
}
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p coco_typeck 2>&1`
Expected: All tests pass

- [ ] **Step 7: Commit**

```bash
git add crates/coco_typeck/
git commit -m "feat(typeck): implement type checking with inference, unification, and error reporting"
```

---

### Task 5: CLI — file resolution helper

**Files:**
- Modify: `crates/coco_cli/src/main.rs`

- [ ] **Step 1: Add resolve_file function**

Add before `fn main()`:

```rust
fn resolve_file(path: &PathBuf) -> PathBuf {
    if path.exists() {
        return path.clone();
    }
    let candidates = [
        path.with_extension("co"),
        PathBuf::from("src").join(path),
        PathBuf::from("src").join(path).with_extension("co"),
    ];
    for candidate in &candidates {
        if candidate.exists() {
            return candidate.clone();
        }
    }
    path.clone()
}
```

- [ ] **Step 2: Update read_source to use resolve_file**

```rust
fn read_source(file: &PathBuf) -> Result<(String, PathBuf), String> {
    let resolved = resolve_file(file);
    let source = fs::read_to_string(&resolved)
        .map_err(|e| format!("error: cannot find '{}' (tried {}, {}.co, src/{}, src/{}.co)",
            file.display(), file.display(), file.display(), file.display(), file.display()))?;
    Ok((source, resolved))
}
```

- [ ] **Step 3: Update all command handlers to use new read_source signature**

Each `cmd_*` function needs to destructure `(source, resolved)` instead of just `source`. Use `resolved` for display in diagnostics.

- [ ] **Step 4: Verify it works**

Run: `cargo run -- run tests/01-hello-world` (no .co extension)
Expected: prints "Hello, World!"

- [ ] **Step 5: Commit**

```bash
git add crates/coco_cli/
git commit -m "feat(cli): add file resolution (optional .co extension, src/ fallback)"
```

---

### Task 6: CLI — typecheck command and run gating

**Files:**
- Modify: `crates/coco_cli/Cargo.toml`
- Modify: `crates/coco_cli/src/main.rs`

- [ ] **Step 1: Add coco_typeck dependency**

In `crates/coco_cli/Cargo.toml`:
```toml
coco_typeck = { workspace = true }
```

- [ ] **Step 2: Add Typecheck command and --no-check flag on Run**

```rust
    /// Type-check a .co file
    Typecheck {
        /// Path to the .co file
        file: PathBuf,
    },
    /// Run a .co file
    Run {
        /// Path to the .co file
        file: PathBuf,
        /// Skip type checking
        #[arg(long = "no-check")]
        no_check: bool,
    },
```

- [ ] **Step 3: Implement cmd_typecheck**

```rust
fn cmd_typecheck(file: &PathBuf) {
    let (source, resolved) = match read_source(file) {
        Ok(s) => s,
        Err(e) => { eprintln!("{}", e); std::process::exit(1); }
    };
    let mut parser = Parser::new(&source);
    let program = parser.parse_program();
    let result = coco_typeck::check(&program);

    if result.has_errors() {
        for err in &result.errors {
            eprintln!("{}[{}]: {}", "error", err.code, err.message);
        }
        eprintln!("\n{} error(s) found in {}", result.errors.len(), resolved.display());
        std::process::exit(1);
    } else {
        println!("{}: types OK", resolved.display());
    }
}
```

- [ ] **Step 4: Update cmd_run to typecheck first (unless --no-check)**

```rust
fn cmd_run(file: &PathBuf, no_check: bool) {
    let (source, resolved) = match read_source(file) {
        Ok(s) => s,
        Err(e) => { eprintln!("{}", e); std::process::exit(1); }
    };

    if !no_check {
        let mut parser = Parser::new(&source);
        let program = parser.parse_program();
        let result = coco_typeck::check(&program);
        if result.has_errors() {
            for err in &result.errors {
                eprintln!("{}[{}]: {}", "error", err.code, err.message);
            }
            eprintln!("\n{} type error(s). Use --no-check to skip.", result.errors.len());
            std::process::exit(1);
        }
    }

    let mut interp = Interpreter::new();
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

- [ ] **Step 5: Update command dispatch**

```rust
Commands::Typecheck { file } => cmd_typecheck(&file),
Commands::Run { file, no_check } => cmd_run(&file, no_check),
```

- [ ] **Step 6: Verify**

Run: `cargo run -- typecheck tests/01-hello-world.co`
Expected: "tests/01-hello-world.co: types OK"

Run: `cargo run -- run tests/01-hello-world.co`
Expected: "Hello, World!" (type checks pass, then runs)

- [ ] **Step 7: Commit**

```bash
git add crates/coco_cli/
git commit -m "feat(cli): add typecheck command and type-gate on run"
```

---

### Task 7: Type checker tests and integration verification

**Files:**
- Create: `tests/13-typed-code.co`
- Modify: `crates/coco_typeck/tests/infer_test.rs` (ensure all pass)

- [ ] **Step 1: Create a test .co file with typed code**

Create `tests/13-typed-code.co`:

```coco
// Typed functions — type checker validates these
fn add(a: int, b: int): int {
    return a + b;
}

fn greet(name: string): string {
    return "Hello, " + name;
}

fn isPositive(n: int): bool {
    return n > 0;
}

fn sumList(items: list<int>): int {
    let total: int = 0;
    for item in items {
        total += item;
    }
    return total;
}

// Untyped functions — type checker ignores these
fn untyped(x) {
    return x;
}

fn main() {
    const result: int = add(10, 20);
    print(result);
    print(greet("Coco"));
    print(isPositive(5));
    print(isPositive(-3));
    print(sumList([1, 2, 3, 4, 5]));
    return 0;
}
```

- [ ] **Step 2: Run typecheck on typed code**

Run: `cargo run -- typecheck tests/13-typed-code.co`
Expected: "tests/13-typed-code.co: types OK"

- [ ] **Step 3: Run the typed code**

Run: `cargo run -- run tests/13-typed-code.co`
Expected: prints 30, Hello Coco, true, false, 15

- [ ] **Step 4: Verify all existing tests still pass**

Run: `cargo test 2>&1`
Expected: All tests pass (92 existing + new typeck tests)

- [ ] **Step 5: Verify file resolution**

Run: `cargo run -- run tests/01-hello-world` (no .co)
Expected: "Hello, World!"

- [ ] **Step 6: Commit**

```bash
git add tests/13-typed-code.co crates/coco_typeck/tests/
git commit -m "test: add typed code test file and type checker tests"
```

---

### Task 8: Final verification and docs update

- [ ] **Step 1: Run full test suite**

Run: `cargo test 2>&1`
Expected: All pass

- [ ] **Step 2: Run clippy**

Run: `cargo clippy 2>&1`
Expected: No errors

- [ ] **Step 3: Test type error reporting**

Create a temp file with a type error and run typecheck:
```
echo 'fn test(): int { return "hello"; }' | cargo run -- typecheck /dev/stdin
```
Expected: error[T001] output

- [ ] **Step 4: Update CLAUDE.md**

- Update test count
- Add `coco_typeck` to architecture table
- Add `typecheck` to CLI commands
- Note `--no-check` flag on run
- Update roadmap to show Phase 4 complete

- [ ] **Step 5: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: update CLAUDE.md for Phase 4 type checker"
```
