//! Definite assignment analysis.
//!
//! Walks all function bodies and statement sequences to verify that every
//! `let` and `const` variable is initialized before its first read.
//!
//! Strategy:
//! - Walk statements in order, tracking initialization.
//! - On assignment (`x = expr`), mark `x` as initialized.
//! - On `Ident` reference, check if the variable is initialized.
//! - For `if/else`, a variable is initialized after the branch only if
//!   initialized in both arms.
//! - For loops (`for`, `while`, `loop`), assume the body may execute zero
//!   times — don't propagate initialization out of the loop body.

use std::collections::HashMap;

use coco_syntax::*;

use crate::diagnostics::SafetyError;
use crate::env::{Binding, SafetyEnv};

/// Check definite assignment for an entire program.
pub fn check_def_assign(items: &[Item], env: &mut SafetyEnv) -> Vec<SafetyError> {
    let mut errors = Vec::new();
    for item in items {
        check_item_def_assign(item, env, &mut errors);
    }
    errors
}

fn check_item_def_assign(item: &Item, env: &mut SafetyEnv, errors: &mut Vec<SafetyError>) {
    match item {
        Item::FnDecl(d) => {
            env.push_scope();
            // Register parameters as initialized
            for param in &d.params {
                env.define(
                    param.name.name.clone(),
                    true, // params are mutable in Coco
                    true, // params are initialized at entry
                    param.span,
                );
            }
            check_block(&d.body, env, errors);
            env.pop_scope();
        }
        Item::LetDecl(d) => {
            let has_value = d.value.is_some();
            if let Some(ref value) = d.value {
                check_expr(value, env, errors);
            }
            env.define(d.name.name.clone(), true, has_value, d.span);
        }
        Item::ConstDecl(d) => {
            check_expr(&d.value, env, errors);
            env.mark_initialized(&d.name.name);
        }
        Item::ExprStmt(e) => {
            check_expr(&e.expr, env, errors);
        }
        Item::Stmt(s) => {
            check_stmt(s, env, errors);
        }
        Item::Export(e) => {
            check_item_def_assign(&e.item, env, errors);
        }
        _ => {}
    }
}

fn check_block(block: &Block, env: &mut SafetyEnv, errors: &mut Vec<SafetyError>) {
    for stmt in &block.stmts {
        check_stmt(stmt, env, errors);
    }
}

fn check_stmt(stmt: &Stmt, env: &mut SafetyEnv, errors: &mut Vec<SafetyError>) {
    match stmt {
        Stmt::Expr(expr) => check_expr(&expr.expr, env, errors),
        Stmt::Item(item) => check_item_def_assign(item, env, errors),
        Stmt::Return(s) => {
            if let Some(ref expr) = s.value {
                check_expr(expr, env, errors);
            }
        }
        Stmt::If(s) => {
            check_expr(&s.condition, env, errors);
            // Snapshot state before any branch for later restoration.
            let before_branches = env.snapshot();
            check_block(&s.then_block, env, errors);
            let after_then = env.snapshot();

            // Process else-if chain: each elif accumulates into the model
            // that the "else" path (including elifs + final else) could
            // execute. We treat elifs as an extended else chain — only
            // variables initialized in ALL paths through the chain
            // (including the final else) are considered initialized.
            if s.else_ifs.is_empty() && s.else_block.is_none() {
                // No else branch at all — restore pre-if state (conservative).
                env.restore(&before_branches);
            } else {
                // We have else-ifs and/or an else block. Accumulate
                // initialization from all alternative paths.
                let mut accumulated_else: Option<Vec<HashMap<String, Binding>>> = None;

                for elif in &s.else_ifs {
                    env.restore(&before_branches);
                    check_expr(&elif.condition, env, errors);
                    check_block(&elif.block, env, errors);
                    let after_elif = env.snapshot();

                    accumulated_else = Some(match accumulated_else {
                        None => after_elif,
                        Some(prev) => {
                            // Intersection merge: keep only vars initialized
                            // in BOTH the accumulated else path AND this elif.
                            let mut merged_env = SafetyEnv::new();
                            merged_env.restore(&prev);
                            merged_env.merge_initialized(&prev, &after_elif);
                            merged_env.snapshot()
                        }
                    });
                }

                if let Some(ref else_block) = s.else_block {
                    env.restore(&before_branches);
                    check_block(else_block, env, errors);
                    let after_else = env.snapshot();

                    accumulated_else = Some(match accumulated_else {
                        None => after_else,
                        Some(prev) => {
                            let mut merged_env = SafetyEnv::new();
                            // Start from the accumulated else state and merge with after_else
                            merged_env.restore(&prev);
                            merged_env.merge_initialized(&prev, &after_else);
                            merged_env.snapshot()
                        }
                    });
                }

                // Merge then-branch with accumulated else path
                if let Some(else_snap) = accumulated_else {
                    env.restore(&before_branches);
                    env.merge_initialized(&after_then, &else_snap);
                } else {
                    env.restore(&before_branches);
                }
            }
        }
        Stmt::For(s) => {
            check_expr(&s.iterable, env, errors);
            env.push_scope();
            env.define(s.pattern.name.clone(), true, true, s.pattern.span);
            check_block(&s.body, env, errors);
            env.pop_scope();
        }
        Stmt::While(s) => {
            check_expr(&s.condition, env, errors);
            check_block(&s.body, env, errors);
            // Loop body may execute zero times — don't propagate
        }
        Stmt::Loop(s) => {
            check_block(&s.body, env, errors);
        }
        Stmt::Throw(s) => {
            check_expr(&s.value, env, errors);
        }
        Stmt::Try(s) => {
            check_block(&s.body, env, errors);
            for catch in &s.catches {
                env.push_scope();
                env.define(catch.param.name.clone(), true, true, catch.param.span);
                check_block(&catch.body, env, errors);
                env.pop_scope();
            }
            if let Some(ref finally) = s.finally {
                check_block(finally, env, errors);
            }
        }
        Stmt::Parallel(s) => {
            for run in &s.runs {
                check_expr(&run.expr, env, errors);
            }
        }
        Stmt::Coro(s) => {
            check_block(&s.body, env, errors);
        }
        Stmt::Unsafe(s) => {
            // Don't recurse into unsafe blocks — unchecked territory
            let _ = s;
        }
        Stmt::Synchronized(s) => {
            check_block(&s.body, env, errors);
        }
        Stmt::Select(s) => {
            for case in &s.cases {
                check_expr(&case.expr, env, errors);
                for stmt in &case.body {
                    check_stmt(stmt, env, errors);
                }
            }
        }
        Stmt::DoWhile(s) => {
            check_block(&s.body, env, errors);
            check_expr(&s.condition, env, errors);
        }
        // Break, Continue — no expressions to check
        Stmt::Break(_) | Stmt::Continue(_) => {} // Item was handled above
    }
}

fn check_expr(expr: &Expr, env: &mut SafetyEnv, errors: &mut Vec<SafetyError>) {
    match expr {
        Expr::Ident(id) => {
            if let Some(binding) = env.lookup(&id.name) {
                if !binding.initialized {
                    errors.push(SafetyError::uninitialized_var(&id.name, id.span));
                }
            }
        }
        Expr::Binary(b) => {
            // For assignment operators, mark the target as initialized
            // after checking the value (right side).
            let is_assign = matches!(
                b.op,
                BinaryOp::Assign
                    | BinaryOp::AddAssign
                    | BinaryOp::SubAssign
                    | BinaryOp::MulAssign
                    | BinaryOp::DivAssign
                    | BinaryOp::ModAssign
                    | BinaryOp::PowAssign
                    | BinaryOp::ShlAssign
                    | BinaryOp::ShrAssign
                    | BinaryOp::BitAndAssign
                    | BinaryOp::BitOrAssign
                    | BinaryOp::BitXorAssign
            );
            if is_assign {
                check_expr(&b.right, env, errors);
                mark_target_initialized(&b.left, env);
            } else {
                check_expr(&b.left, env, errors);
                check_expr(&b.right, env, errors);
            }
        }
        Expr::Unary(u) => {
            check_expr(&u.expr, env, errors);
        }
        Expr::Call(c) => {
            check_expr(&c.callee, env, errors);
            for arg in &c.args {
                check_expr(&arg.value, env, errors);
            }
        }
        Expr::Index(i) => {
            check_expr(&i.object, env, errors);
            check_expr(&i.index, env, errors);
        }
        Expr::Member(m) => {
            check_expr(&m.object, env, errors);
        }
        Expr::Assignment(a) => {
            // Check the value expression first
            check_expr(&a.value, env, errors);
            // Mark the target as initialized
            mark_target_initialized(&a.target, env);
        }
        Expr::Ternary(t) => {
            check_expr(&t.condition, env, errors);
            check_expr(&t.then_expr, env, errors);
            check_expr(&t.else_expr, env, errors);
        }
        Expr::NullCoalesce(n) => {
            check_expr(&n.left, env, errors);
            check_expr(&n.right, env, errors);
        }
        Expr::Elvis(e) => {
            check_expr(&e.left, env, errors);
            check_expr(&e.right, env, errors);
        }
        Expr::Pipe(p) => {
            check_expr(&p.left, env, errors);
            check_expr(&p.right, env, errors);
        }
        Expr::Match(m) => {
            check_expr(&m.scrutinee, env, errors);
            for arm in &m.arms {
                check_expr(&arm.body, env, errors);
            }
        }
        Expr::Lambda(l) => {
            env.push_scope();
            for param in &l.params {
                env.define(param.name.name.clone(), true, true, param.span);
            }
            match &l.body {
                LambdaBody::Expr(e) => check_expr(e, env, errors),
                LambdaBody::Block(b) => check_block(b, env, errors),
            }
            env.pop_scope();
        }
        Expr::Array(a) => {
            for elem in &a.elements {
                check_expr(elem, env, errors);
            }
        }
        Expr::Object(o) => {
            for field in &o.fields {
                check_expr(&field.value, env, errors);
            }
        }
        Expr::New(n) => {
            for arg in &n.args {
                check_expr(&arg.value, env, errors);
            }
        }
        Expr::Postfix(p) => {
            check_expr(&p.object, env, errors);
        }
        Expr::Group(g) => {
            check_expr(g, env, errors);
        }
        // Literals, This, Dollar, DollarDollar — no reads
        _ => {}
    }
}

/// Recursively mark the target of an assignment as initialized.
fn mark_target_initialized(target: &Expr, env: &mut SafetyEnv) {
    match target {
        Expr::Ident(id) => {
            env.mark_initialized(&id.name);
        }
        Expr::Member(m) => {
            // `x.y = ...` marks `x` as initialized if it was previously uninitialized
            // But don't mark `x` initialized just from property assignment
            // — only simple variable assigment does definite assignment.
            let _ = m;
        }
        Expr::Index(i) => {
            let _ = i;
        }
        // Nested destructuring not handled yet
        _ => {}
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
        check_def_assign(&program.items, &mut env)
    }

    #[test]
    fn const_initialized_ok() {
        let errors = analyze_src("fn main(): int { const x = 1; return x; }");
        assert!(
            errors.is_empty(),
            "const should be initialized: {:?}",
            errors
        );
    }

    #[test]
    fn let_initialized_ok() {
        let errors = analyze_src("fn main(): int { let x = 1; return x; }");
        assert!(
            errors.is_empty(),
            "let with value should be initialized: {:?}",
            errors
        );
    }

    #[test]
    fn uninitialized_var_error() {
        let errors = analyze_src("fn main(): int { let x: int; return x; }");
        assert!(!errors.is_empty(), "uninitialized let should produce error");
        assert!(
            errors[0].code == "S001",
            "expected S001, got {}",
            errors[0].code
        );
    }

    #[test]
    fn assign_then_read_ok() {
        let errors = analyze_src("fn main(): int { let x: int; x = 5; return x; }");
        assert!(
            errors.is_empty(),
            "assign-then-read should be ok: {:?}",
            errors
        );
    }

    #[test]
    fn param_initialized_on_entry() {
        let errors = analyze_src("fn add(a: int, b: int): int { return a + b; }");
        assert!(
            errors.is_empty(),
            "params should be initialized: {:?}",
            errors
        );
    }
}
