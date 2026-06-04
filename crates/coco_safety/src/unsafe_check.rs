//! Unsafe block reporting.
//!
//! Walks the AST to find all `unsafe { }` blocks and reports them as
//! warnings (S003). Unsafe blocks are unchecked territory — the safety
//! analyzer does not recurse into them.

use coco_syntax::*;

use crate::diagnostics::SafetyError;

/// Find and report all unsafe blocks in the program.
pub fn check_unsafe_blocks(items: &[Item]) -> Vec<SafetyError> {
    let mut errors = Vec::new();
    for item in items {
        check_item_unsafe(item, &mut errors);
    }
    errors
}

fn check_item_unsafe(item: &Item, errors: &mut Vec<SafetyError>) {
    match item {
        Item::FnDecl(d) => {
            check_block_unsafe(&d.body, errors);
        }
        Item::Export(e) => {
            check_item_unsafe(&e.item, errors);
        }
        _ => {}
    }
}

fn check_block_unsafe(block: &Block, errors: &mut Vec<SafetyError>) {
    for stmt in &block.stmts {
        check_stmt_unsafe(stmt, errors);
    }
}

fn check_stmt_unsafe(stmt: &Stmt, errors: &mut Vec<SafetyError>) {
    match stmt {
        Stmt::Unsafe(s) => {
            errors.push(SafetyError::unsafe_block_used(s.span));
            // Do NOT recurse into unsafe block bodies.
        }
        Stmt::If(s) => {
            check_block_unsafe(&s.then_block, errors);
            for elif in &s.else_ifs {
                check_block_unsafe(&elif.block, errors);
            }
            if let Some(ref else_block) = s.else_block {
                check_block_unsafe(else_block, errors);
            }
        }
        Stmt::For(s) => {
            check_block_unsafe(&s.body, errors);
        }
        Stmt::While(s) => {
            check_block_unsafe(&s.body, errors);
        }
        Stmt::DoWhile(s) => {
            check_block_unsafe(&s.body, errors);
        }
        Stmt::Loop(s) => {
            check_block_unsafe(&s.body, errors);
        }
        Stmt::Try(s) => {
            check_block_unsafe(&s.body, errors);
            for catch in &s.catches {
                check_block_unsafe(&catch.body, errors);
            }
            if let Some(ref finally) = s.finally {
                check_block_unsafe(finally, errors);
            }
        }
        Stmt::Parallel(s) => {
            for run in &s.runs {
                // The run expression could be a lambda containing unsafe
                if let Expr::Lambda(l) = &run.expr {
                    match &l.body {
                        LambdaBody::Block(b) => check_block_unsafe(b, errors),
                        _ => {}
                    }
                }
            }
        }
        Stmt::Coro(s) => {
            check_block_unsafe(&s.body, errors);
        }
        Stmt::Synchronized(s) => {
            check_block_unsafe(&s.body, errors);
        }
        Stmt::Select(s) => {
            for case in &s.cases {
                for stmt in &case.body {
                    check_stmt_unsafe(stmt, errors);
                }
            }
        }
        Stmt::Item(item) => {
            check_item_unsafe(item, errors);
        }
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
        check_unsafe_blocks(&program.items)
    }

    #[test]
    fn no_unsafe_blocks() {
        let errors = analyze_src("fn main(): int { return 0; }");
        assert!(errors.is_empty());
    }

    #[test]
    fn unsafe_block_reported() {
        let errors = analyze_src(
            "fn main(): int {
                unsafe { doSomething(); }
                return 0;
            }",
        );
        assert!(!errors.is_empty(), "unsafe block should be reported");
        assert!(errors[0].code == "S003", "expected S003, got {}", errors[0].code);
    }

    #[test]
    fn nested_unsafe_reported() {
        let errors = analyze_src(
            "fn main(): int {
                if true {
                    unsafe { hack(); }
                }
                return 0;
            }",
        );
        assert_eq!(errors.len(), 1);
    }
}
