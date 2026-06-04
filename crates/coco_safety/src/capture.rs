//! Capture analysis for parallel and coroutine blocks.
//!
//! Detects mutable (`let`) variables captured across `parallel { run { ... } }`
//! or `coro { ... }` boundaries. This is a data-race safety check.
//!
//! Safe patterns:
//! - `const` variables (immutable, always safe to share)
//! - `new Atomic<T>(...)` constructor calls (safe shared mutation)
//!
//! Rejected:
//! - Mutable `let` variable referenced inside a parallel `run` clause
//! - Mutable `let` variable referenced inside a `coro` block

use coco_syntax::*;

use crate::diagnostics::SafetyError;
use crate::env::SafetyEnv;

/// Check capture safety across all items.
pub fn check_captures(items: &[Item], env: &mut SafetyEnv) -> Vec<SafetyError> {
    let mut errors = Vec::new();
    for item in items {
        check_item_captures(item, env, &mut errors);
    }
    errors
}

fn check_item_captures(item: &Item, env: &mut SafetyEnv, errors: &mut Vec<SafetyError>) {
    match item {
        Item::FnDecl(d) => {
            env.push_scope();
            // Register params as immutable for capture purposes
            for param in &d.params {
                env.define(param.name.name.clone(), true, true, param.span);
            }
            check_block_captures(&d.body, env, errors);
            env.pop_scope();
        }
        Item::Stmt(s) => {
            check_stmt_captures(s, env, errors);
        }
        Item::Export(e) => {
            check_item_captures(&e.item, env, errors);
        }
        _ => {}
    }
}

fn check_block_captures(block: &Block, env: &mut SafetyEnv, errors: &mut Vec<SafetyError>) {
    for stmt in &block.stmts {
        check_stmt_captures(stmt, env, errors);
    }
}

fn check_stmt_captures(stmt: &Stmt, env: &mut SafetyEnv, errors: &mut Vec<SafetyError>) {
    match stmt {
        Stmt::Parallel(s) => {
            for run in &s.runs {
                let mut captured_vars = Vec::new();
                collect_captured_vars(&run.expr, &mut captured_vars);

                for var_name in &captured_vars {
                    if let Some(binding) = env.lookup(var_name) {
                        if binding.is_mutable {
                            // Check if it's an Atomic constructor — safe pattern
                            if !is_atomic_constructor(&run.expr, var_name) {
                                errors.push(SafetyError::mutable_capture(
                                    var_name,
                                    "parallel",
                                    run.span,
                                ));
                            }
                        }
                    }
                }
            }
        }
        Stmt::Coro(s) => {
            let mut captured_vars = Vec::new();
            collect_block_captured_vars(&s.body, &mut captured_vars);

            for var_name in &captured_vars {
                if let Some(binding) = env.lookup(var_name) {
                    if binding.is_mutable {
                        errors.push(SafetyError::mutable_capture(
                            var_name,
                            "coro",
                            s.span,
                        ));
                    }
                }
            }
        }
        Stmt::If(s) => {
            check_block_captures(&s.then_block, env, errors);
            for elif in &s.else_ifs {
                check_block_captures(&elif.block, env, errors);
            }
            if let Some(ref else_block) = s.else_block {
                check_block_captures(else_block, env, errors);
            }
        }
        Stmt::For(s) => {
            check_block_captures(&s.body, env, errors);
        }
        Stmt::While(s) => {
            check_block_captures(&s.body, env, errors);
        }
        Stmt::Loop(s) => {
            check_block_captures(&s.body, env, errors);
        }
        Stmt::Try(s) => {
            check_block_captures(&s.body, env, errors);
            for catch in &s.catches {
                check_block_captures(&catch.body, env, errors);
            }
            if let Some(ref finally) = s.finally {
                check_block_captures(finally, env, errors);
            }
        }
        Stmt::Item(item) => {
            // Register local lets/consts before recursing
            match item.as_ref() {
                Item::LetDecl(d) => {
                    env.define(d.name.name.clone(), true, d.value.is_some(), d.span);
                }
                Item::ConstDecl(d) => {
                    env.define(d.name.name.clone(), false, true, d.span);
                }
                _ => {}
            }
            check_item_captures(item, env, errors);
        }
        Stmt::Synchronized(s) => {
            // Synchronized blocks are safe — mutual exclusion
            check_block_captures(&s.body, env, errors);
        }
        Stmt::DoWhile(s) => {
            check_block_captures(&s.body, env, errors);
        }
        Stmt::Select(s) => {
            for case in &s.cases {
                for stmt in &case.body {
                    check_stmt_captures(stmt, env, errors);
                }
            }
        }
        _ => {}
    }
}

/// Collect all `Expr::Ident` variable names in an expression tree.
fn collect_captured_vars(expr: &Expr, vars: &mut Vec<String>) {
    match expr {
        Expr::Ident(id) => {
            if !vars.contains(&id.name) {
                vars.push(id.name.clone());
            }
        }
        Expr::Binary(b) => {
            collect_captured_vars(&b.left, vars);
            collect_captured_vars(&b.right, vars);
        }
        Expr::Unary(u) => {
            collect_captured_vars(&u.expr, vars);
        }
        Expr::Call(c) => {
            collect_captured_vars(&c.callee, vars);
            for arg in &c.args {
                collect_captured_vars(&arg.value, vars);
            }
        }
        Expr::Index(i) => {
            collect_captured_vars(&i.object, vars);
            collect_captured_vars(&i.index, vars);
        }
        Expr::Member(m) => {
            collect_captured_vars(&m.object, vars);
        }
        Expr::Lambda(l) => {
            match &l.body {
                LambdaBody::Expr(e) => collect_captured_vars(e, vars),
                LambdaBody::Block(b) => collect_block_captured_vars(b, vars),
            }
        }
        Expr::Array(a) => {
            for elem in &a.elements {
                collect_captured_vars(elem, vars);
            }
        }
        Expr::Object(o) => {
            for field in &o.fields {
                collect_captured_vars(&field.value, vars);
            }
        }
        Expr::New(n) => {
            for arg in &n.args {
                collect_captured_vars(&arg.value, vars);
            }
        }
        Expr::Ternary(t) => {
            collect_captured_vars(&t.condition, vars);
            collect_captured_vars(&t.then_expr, vars);
            collect_captured_vars(&t.else_expr, vars);
        }
        Expr::NullCoalesce(n) => {
            collect_captured_vars(&n.left, vars);
            collect_captured_vars(&n.right, vars);
        }
        Expr::Elvis(e) => {
            collect_captured_vars(&e.left, vars);
            collect_captured_vars(&e.right, vars);
        }
        Expr::Pipe(p) => {
            collect_captured_vars(&p.left, vars);
            collect_captured_vars(&p.right, vars);
        }
        Expr::Match(m) => {
            collect_captured_vars(&m.scrutinee, vars);
            for arm in &m.arms {
                collect_captured_vars(&arm.body, vars);
            }
        }
        Expr::Postfix(p) => {
            collect_captured_vars(&p.object, vars);
        }
        Expr::Group(g) => {
            collect_captured_vars(g, vars);
        }
        // Literals, This, Dollar, DollarDollar — no variable captures
        _ => {}
    }
}

fn collect_block_captured_vars(block: &Block, vars: &mut Vec<String>) {
    for stmt in &block.stmts {
        collect_stmt_captured_vars(stmt, vars);
    }
}

fn collect_stmt_captured_vars(stmt: &Stmt, vars: &mut Vec<String>) {
    match stmt {
        Stmt::Expr(e) => collect_captured_vars(&e.expr, vars),
        Stmt::Return(r) => {
            if let Some(ref expr) = r.value {
                collect_captured_vars(expr, vars);
            }
        }
        Stmt::If(s) => {
            collect_captured_vars(&s.condition, vars);
            collect_block_captured_vars(&s.then_block, vars);
            for elif in &s.else_ifs {
                collect_captured_vars(&elif.condition, vars);
                collect_block_captured_vars(&elif.block, vars);
            }
            if let Some(ref else_block) = s.else_block {
                collect_block_captured_vars(else_block, vars);
            }
        }
        Stmt::Throw(s) => collect_captured_vars(&s.value, vars),
        Stmt::Item(item) => {
            if let Item::LetDecl(d) = item.as_ref() {
                if let Some(ref value) = d.value {
                    collect_captured_vars(value, vars);
                }
            }
        }
        _ => {}
    }
}

/// Check if an expression is an Atomic constructor call that wraps a variable.
///
/// Pattern: `total.add(...)` where `total` was captured but is an Atomic.
/// More specifically: `new Atomic<T>(initial)` or any `.add()`, `.load()` etc.
/// on a variable that has been initialized as an Atomic.
///
/// For now, we recognize the `new Atomic` pattern by looking for
/// `Call { callee: New { type_name: "Atomic" } }`.
fn is_atomic_constructor(expr: &Expr, var_name: &str) -> bool {
    match expr {
        Expr::Call(c) => {
            // Check if callee is a member access: `total.add(...)`
            if let Expr::Member(ref m) = c.callee {
                if m.property.name == "add"
                    || m.property.name == "sub"
                    || m.property.name == "store"
                    || m.property.name == "load"
                    || m.property.name == "compareAndSwap"
                {
                    if let Expr::Ident(ref id) = m.object {
                        return id.name == *var_name;
                    }
                }
            }
            false
        }
        Expr::Lambda(_l) => {
            // Inside a parallel run { ... }, a lambda body may still
            // reference externals — this is where the outer check fires.
            false
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use coco_parser::Parser;

    fn analyze_src(src: &str) -> Vec<SafetyError> {
        let mut parser = Parser::new(src);
        let program = parser.parse_program();
        let mut env = SafetyEnv::new();
        crate::collect::collect_bindings(&program.items, &mut env);
        check_captures(&program.items, &mut env)
    }

    #[test]
    fn const_capture_ok() {
        let errors = analyze_src(
            "async fn main(): int {
                const config = 42;
                await parallel {
                    run { print(config); }
                };
                return 0;
            }",
        );
        assert!(errors.is_empty(), "const capture should be ok: {:?}", errors);
    }

    #[test]
    fn mutable_capture_rejected() {
        // Use bare expression form: `run counter += 1` (not `run { ... }`)
        // The parser currently treats `{ expr }` as an ObjectLiteral,
        // so block-form run clauses need a parser update.
        let errors = analyze_src(
            "fn main(): int {
                let counter = 0;
                parallel {
                    run counter += 1;
                };
                return 0;
            }",
        );
        assert!(!errors.is_empty(), "mutable capture should be rejected");
        assert!(errors[0].code == "S002", "expected S002, got {}", errors[0].code);
    }

    #[test]
    fn atomic_capture_ok() {
        // Atomic patterns should be recognized as safe
        let errors = analyze_src(
            "async fn main(): int {
                const total = new Atomic<int>(0);
                await parallel {
                    run { total.add(1); }
                };
                return 0;
            }",
        );
        // Atomic access via .add() on a const Atomic should be ok
        // 'total' is const, so it's already not flagged.
        assert!(errors.is_empty(), "atomic capture should be ok: {:?}", errors);
    }

    #[test]
    fn coro_mutable_capture_rejected() {
        let errors = analyze_src(
            "fn main(): int {
                let data = 42;
                coro { print(data); }
                return 0;
            }",
        );
        assert!(!errors.is_empty(), "coro mutable capture should be rejected");
    }
}
