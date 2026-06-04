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

#[test]
fn result_reports_counts_and_primary_error() {
    let result = check_source("fn test(): int { return \"nope\"; }");
    assert!(!result.is_ok());
    assert_eq!(result.error_count(), 1);
    assert_eq!(result.warning_count(), 0);
    assert_eq!(result.diagnostic_count(), 1);
    assert_eq!(result.primary_error().unwrap().code, "T001");
    assert!(result
        .primary_error()
        .unwrap()
        .message
        .contains("expected `int`"));
}

#[test]
fn class_property_default_rejects_wrong_type() {
    let result = check_source("class User { name: string = 42; }");
    assert!(result.has_errors());
    assert_eq!(error_codes(&result), vec!["T001"]);
}

#[test]
fn class_member_access_uses_property_type() {
    let result = check_source(
        "class User { name: string; } fn display(user: User): string { return user.name; }",
    );
    assert!(!result.has_errors(), "{:?}", result.errors);
}

#[test]
fn class_member_access_reports_missing_property() {
    let result = check_source(
        "class User { name: string; } fn display(user: User): string { return user.email; }",
    );
    assert!(result.has_errors());
    assert_eq!(error_codes(&result), vec!["T007"]);
}

#[test]
fn class_method_assignment_checks_property_type() {
    let result = check_source(
        "class User { name: string; fn rename(value: int): void { this.name = value; } }",
    );
    assert!(result.has_errors());
    assert_eq!(error_codes(&result), vec!["T001"]);
}

#[test]
fn new_expression_checks_constructor_arguments() {
    let result = check_source(
        "class User { constructor(name: string, age: int) { } } fn make(): User { return new User(\"Ada\", \"old\"); }",
    );
    assert!(result.has_errors());
    assert_eq!(error_codes(&result), vec!["T001"]);
}

#[test]
fn class_implements_interface_checks_members() {
    let result = check_source(
        "interface Named { name: string; label(): string; } class User implements Named { name: int; fn label(): string { return \"user\"; } }",
    );
    assert!(result.has_errors());
    assert_eq!(error_codes(&result), vec!["T001"]);
}

#[test]
fn trait_members_are_available_on_using_class() {
    let result = check_source(
        "trait Tagged { tag: string; } class Post { use Tagged; } fn label(post: Post): string { return post.tag; }",
    );
    assert!(!result.has_errors(), "{:?}", result.errors);
}

#[test]
fn map_literal_checks_value_types() {
    let result =
        check_source("fn test() { let scores: map<string, int> = {\"ada\": 10, bob: \"x\"}; }");
    assert!(result.has_errors());
    assert_eq!(error_codes(&result), vec!["T001"]);
}

#[test]
fn map_literal_checks_key_types() {
    let result = check_source("fn test() { let scores: map<int, string> = {\"ada\": \"ok\"}; }");
    assert!(result.has_errors());
    assert_eq!(error_codes(&result), vec!["T001"]);
}

#[test]
fn nullable_member_access_requires_optional_chain_or_narrowing() {
    let result = check_source(
        "class User { name: string; } fn display(user: User|null): string { return user.name; }",
    );
    assert!(result.has_errors());
    assert_eq!(error_codes(&result), vec!["T004"]);
}

#[test]
fn optional_member_access_returns_nullable_property() {
    let result = check_source(
        "class User { name: string; } fn display(user: User|null): string|null { return user?.name; }",
    );
    assert!(!result.has_errors(), "{:?}", result.errors);
}

#[test]
fn null_check_narrows_union_inside_then_block() {
    let result = check_source(
        "class User { name: string; } fn display(user: User|null): string { if user != null { return user.name; } return \"anon\"; }",
    );
    assert!(!result.has_errors(), "{:?}", result.errors);
}
