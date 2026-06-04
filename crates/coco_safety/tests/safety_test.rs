//! Integration tests for the coco_safety crate.
//!
//! Tests the full `analyze()` pipeline end-to-end.

use coco_parser::Parser;
use coco_safety::analyze;

/// Parse and analyze source, returning the SafetyResult.
fn check(src: &str) -> coco_safety::SafetyResult {
    let mut parser = Parser::new(src);
    let program = parser.parse_program();
    analyze(&program)
}

// ============================================================
// Clean programs should pass
// ============================================================

#[test]
fn clean_hello_world() {
    let result = check("fn main(): int { return 0; }");
    assert!(
        result.is_ok(),
        "hello world should pass: {:?}",
        result.errors
    );
    assert!(result.warnings.is_empty());
}

#[test]
fn clean_variables() {
    let result = check(
        "fn main(): int {
            const pi = 3.14;
            let counter = 0;
            counter = counter + 1;
            return counter;
        }",
    );
    assert!(
        result.is_ok(),
        "clean variables should pass: {:?}",
        result.errors
    );
}

#[test]
fn clean_functions() {
    let result = check(
        "fn add(a: int, b: int): int { return a + b; }
         fn main(): int { return add(1, 2); }",
    );
    assert!(
        result.is_ok(),
        "clean functions should pass: {:?}",
        result.errors
    );
}

#[test]
fn clean_const_capture_in_parallel() {
    let result = check(
        "async fn main(): int {
            const config = 42;
            await parallel {
                run config + 1;
            };
            return 0;
        }",
    );
    assert!(
        result.is_ok(),
        "const capture in parallel should pass: {:?}",
        result.errors
    );
}

// ============================================================
// Errors should be detected
// ============================================================

#[test]
fn error_uninitialized_var() {
    let result = check("fn main(): int { let x: int; return x; }");
    assert!(result.has_errors());
    assert!(result.errors.iter().any(|e| e.code == "S001"));
}

#[test]
fn error_mutable_capture_parallel() {
    let result = check(
        "fn main(): int {
            let counter = 0;
            parallel {
                run counter += 1;
            };
            return 0;
        }",
    );
    assert!(result.has_errors());
    assert!(result.errors.iter().any(|e| e.code == "S002"));
}

#[test]
fn warning_unsafe_block() {
    let result = check(
        "fn main(): int {
            unsafe { hack(); }
            return 0;
        }",
    );
    assert!(result.has_warnings());
    assert!(result.warnings.iter().any(|e| e.code == "S003"));
}

#[test]
fn warning_iterator_invalidation() {
    let result = check(
        "fn main(): int {
            let list = [1, 2, 3];
            for x in list {
                list.push(4);
            }
            return 0;
        }",
    );
    assert!(result.has_warnings());
    assert!(result.warnings.iter().any(|e| e.code == "S004"));
}

// ============================================================
// Result API tests
// ============================================================

#[test]
fn result_counts() {
    let result = check(
        "fn main(): int {
            let a: int;
            let b: int;
            return a + b;
        }",
    );
    assert_eq!(result.error_count(), 2);
    assert_eq!(result.warning_count(), 0);
    assert_eq!(result.diagnostic_count(), 2);
}
