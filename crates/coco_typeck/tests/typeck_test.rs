use coco_parser::Parser;
use coco_typeck::{check, TypeckResult};

fn check_source(src: &str) -> TypeckResult {
    let mut parser = Parser::new(src);
    let program = parser.parse_program();
    assert!(
        parser.diagnostics().is_empty(),
        "parser diagnostics: {:?}",
        parser.diagnostics()
    );
    check(&program)
}

fn error_codes(result: &TypeckResult) -> Vec<&'static str> {
    result.errors.iter().map(|error| error.code).collect()
}

#[test]
fn unannotated_code_is_not_type_checked() {
    let result = check_source("fn test(a, b) { return a + b; }");
    assert!(!result.has_errors(), "{:?}", result.errors);
}

#[test]
fn annotated_assignment_rejects_wrong_literal_type() {
    let result = check_source("fn test() { let x: int = \"hello\"; }");
    assert!(result.has_errors());
    assert_eq!(error_codes(&result), vec!["T001"]);
}

#[test]
fn annotated_return_rejects_wrong_type() {
    let result = check_source("fn test(): int { return \"hello\"; }");
    assert!(result.has_errors());
    assert_eq!(error_codes(&result), vec!["T001"]);
}

#[test]
fn incompatible_arithmetic_reports_operand_error() {
    let result = check_source("fn test(): int { return 1 + \"two\"; }");
    assert!(result.has_errors());
    assert_eq!(error_codes(&result), vec!["T006"]);
}

#[test]
fn string_concatenation_is_allowed() {
    let result = check_source("fn test(): string { return \"a\" + \"b\"; }");
    assert!(!result.has_errors(), "{:?}", result.errors);
}

#[test]
fn int_promotes_to_float_return() {
    let result = check_source("fn test(): float { return 1 + 2.0; }");
    assert!(!result.has_errors(), "{:?}", result.errors);
}

#[test]
fn function_call_rejects_wrong_argument_count() {
    let result =
        check_source("fn add(a: int, b: int): int { return a + b; } fn test() { add(1); }");
    assert!(result.has_errors());
    assert_eq!(error_codes(&result), vec!["T002"]);
}

#[test]
fn function_call_rejects_wrong_argument_type() {
    let result =
        check_source("fn add(a: int, b: int): int { return a + b; } fn test() { add(1, \"x\"); }");
    assert!(result.has_errors());
    assert_eq!(error_codes(&result), vec!["T001"]);
}

#[test]
fn list_annotation_rejects_incompatible_element() {
    let result = check_source("fn test() { let nums: list<int> = [1, 2, \"three\"]; }");
    assert!(result.has_errors());
    assert_eq!(error_codes(&result), vec!["T001"]);
}

#[test]
fn null_coalesce_allows_nullable_fallback() {
    let result = check_source("fn test(): int { let x: int|null = null; return x ?? 0; }");
    assert!(!result.has_errors(), "{:?}", result.errors);
}
