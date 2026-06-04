//! Iterator invalidation detection.
//!
//! Walks `for-in` loops and checks if the iterated collection is mutated
//! inside the loop body. This is a best-effort analysis — it catches
//! direct mutations of the same variable.
//!
//! Patterns detected:
//! - `x = expr` where `x` is the iterated variable
//! - `x.push(...)`, `x.remove(...)`, `x.set(...)`, `x.insert(...)` mutations
//! - `x += ...`, `x -= ...` etc. on the iterated variable

use coco_syntax::*;

use crate::diagnostics::SafetyError;

/// Check all items for iterator invalidation.
pub fn check_iterator_invalidation(items: &[Item]) -> Vec<SafetyError> {
    let mut errors = Vec::new();
    for item in items {
        check_item_iterator(item, &mut errors);
    }
    errors
}

fn check_item_iterator(item: &Item, errors: &mut Vec<SafetyError>) {
    match item {
        Item::FnDecl(d) => {
            check_block_iterator(&d.body, errors);
        }
        Item::Export(e) => {
            check_item_iterator(&e.item, errors);
        }
        _ => {}
    }
}

fn check_block_iterator(block: &Block, errors: &mut Vec<SafetyError>) {
    for stmt in &block.stmts {
        check_stmt_iterator(stmt, errors);
    }
}

fn check_stmt_iterator(stmt: &Stmt, errors: &mut Vec<SafetyError>) {
    match stmt {
        Stmt::For(s) => {
            // Get the iterated variable name
            if let Expr::Ident(ref id) = s.iterable {
                let iter_var = &id.name;
                // Check the loop body for mutations of the same variable
                check_block_for_mutation(&s.body, iter_var, errors);
            }
        }
        Stmt::If(s) => {
            check_block_iterator(&s.then_block, errors);
            for elif in &s.else_ifs {
                check_block_iterator(&elif.block, errors);
            }
            if let Some(ref else_block) = s.else_block {
                check_block_iterator(else_block, errors);
            }
        }
        Stmt::While(s) => {
            check_block_iterator(&s.body, errors);
        }
        Stmt::DoWhile(s) => {
            check_block_iterator(&s.body, errors);
        }
        Stmt::Loop(s) => {
            check_block_iterator(&s.body, errors);
        }
        Stmt::Try(s) => {
            check_block_iterator(&s.body, errors);
            for catch in &s.catches {
                check_block_iterator(&catch.body, errors);
            }
            if let Some(ref finally) = s.finally {
                check_block_iterator(finally, errors);
            }
        }
        Stmt::Item(item) => {
            check_item_iterator(item, errors);
        }
        _ => {}
    }
}

/// Check a block for mutations of a specific variable.
fn check_block_for_mutation(block: &Block, var: &str, errors: &mut Vec<SafetyError>) {
    for stmt in &block.stmts {
        check_stmt_for_mutation(stmt, var, errors);
    }
}

fn check_stmt_for_mutation(stmt: &Stmt, var: &str, errors: &mut Vec<SafetyError>) {
    match stmt {
        Stmt::Expr(e) => {
            check_expr_for_mutation(&e.expr, var, errors);
        }
        Stmt::If(s) => {
            check_block_for_mutation(&s.then_block, var, errors);
            for elif in &s.else_ifs {
                check_block_for_mutation(&elif.block, var, errors);
            }
            if let Some(ref else_block) = s.else_block {
                check_block_for_mutation(else_block, var, errors);
            }
        }
        Stmt::For(s) => {
            // Don't recurse into nested for loops with same iterator pattern
            // but check the body for mutations
            check_block_for_mutation(&s.body, var, errors);
        }
        Stmt::While(s) => {
            check_block_for_mutation(&s.body, var, errors);
        }
        Stmt::Loop(s) => {
            check_block_for_mutation(&s.body, var, errors);
        }
        _ => {}
    }
}

fn check_expr_for_mutation(expr: &Expr, var: &str, errors: &mut Vec<SafetyError>) {
    match expr {
        Expr::Binary(b) => {
            // Check if this is an assignment to the iterated variable
            if matches!(
                b.op,
                BinaryOp::Assign
                    | BinaryOp::AddAssign
                    | BinaryOp::SubAssign
                    | BinaryOp::MulAssign
                    | BinaryOp::DivAssign
                    | BinaryOp::ModAssign
            ) && is_var_ref(&b.left, var)
            {
                errors.push(SafetyError::iterator_invalidation(var, b.span));
            }
        }
        Expr::Call(c) => {
            // Check for mutation methods: x.push(...), x.remove(...), x.set(...), x.insert(...)
            if let Expr::Member(ref m) = c.callee {
                if (m.property.name == "push"
                    || m.property.name == "remove"
                    || m.property.name == "set"
                    || m.property.name == "insert"
                    || m.property.name == "pop"
                    || m.property.name == "shift"
                    || m.property.name == "unshift"
                    || m.property.name == "splice"
                    || m.property.name == "clear")
                    && is_var_ref(&m.object, var)
                {
                    errors.push(SafetyError::iterator_invalidation(var, m.span));
                }
            }
        }
        _ => {}
    }
}

/// Check if an expression is a reference to a specific variable.
fn is_var_ref(expr: &Expr, name: &str) -> bool {
    matches!(expr, Expr::Ident(id) if id.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use coco_parser::Parser;

    fn analyze_src(src: &str) -> Vec<SafetyError> {
        let mut parser = Parser::new(src);
        let program = parser.parse_program();
        check_iterator_invalidation(&program.items)
    }

    #[test]
    fn safe_for_loop() {
        let errors = analyze_src(
            "fn main(): int {
                const items = [1, 2, 3];
                for item in items {
                    print(item);
                }
                return 0;
            }",
        );
        assert!(errors.is_empty(), "safe loop should be ok: {:?}", errors);
    }

    #[test]
    fn mutation_during_iteration_warning() {
        let errors = analyze_src(
            "fn main(): int {
                const list = [1, 2, 3];
                for x in list {
                    list.push(4);
                }
                return 0;
            }",
        );
        assert!(!errors.is_empty(), "mutation during iteration should warn");
        assert!(
            errors[0].code == "S004",
            "expected S004, got {}",
            errors[0].code
        );
    }

    #[test]
    fn assignment_during_iteration_warning() {
        let errors = analyze_src(
            "fn main(): int {
                let items = [1, 2, 3];
                for x in items {
                    items = [4, 5, 6];
                }
                return 0;
            }",
        );
        assert!(
            !errors.is_empty(),
            "assignment during iteration should warn"
        );
    }

    #[test]
    fn unrelated_mutation_ok() {
        let errors = analyze_src(
            "fn main(): int {
                const list = [1, 2, 3];
                let other = [4, 5, 6];
                for x in list {
                    other.push(7);
                }
                return 0;
            }",
        );
        assert!(
            errors.is_empty(),
            "unrelated mutation should be ok: {:?}",
            errors
        );
    }
}
