use coco_formatter::Formatter;
use coco_parser::Parser;

fn format(src: &str) -> String {
    let mut parser = Parser::new(src);
    let program = parser.parse_program();
    let mut formatter = Formatter::new();
    formatter.format(&program)
}

fn assert_idempotent(src: &str) {
    let first = format(src);
    let second = format(&first);
    assert_eq!(
        first, second,
        "Formatter is not idempotent for input:\n{}\nFirst:\n{}\nSecond:\n{}",
        src, first, second
    );
}

#[test]
fn format_fn_decl() {
    let output = format("fn add(x: int, y: int): int { return x + y; }");
    assert!(
        output.contains("fn add"),
        "Expected 'fn add' in output: {}",
        output
    );
    assert!(
        output.contains("return"),
        "Expected 'return' in output: {}",
        output
    );
}

#[test]
fn format_idempotent_fn() {
    assert_idempotent("fn add(x: int, y: int): int {\n    return x + y;\n}\n");
}

#[test]
fn format_class_decl() {
    let output = format("class Dog { name: string; fn bark() { } }");
    assert!(
        output.contains("class Dog"),
        "Expected 'class Dog' in output: {}",
        output
    );
}

#[test]
fn format_let_const() {
    let output = format("let x: int = 42;");
    assert!(
        output.contains("let x"),
        "Expected 'let x' in output: {}",
        output
    );
}

#[test]
fn format_enum() {
    let output = format("enum Color { Red, Green, Blue }");
    assert!(
        output.contains("enum Color"),
        "Expected 'enum Color' in output: {}",
        output
    );
    assert!(
        output.contains("Red"),
        "Expected 'Red' in output: {}",
        output
    );
}

#[test]
fn format_binary_expr() {
    let output = format("let x = 1 + 2 * 3;");
    assert!(
        !output.is_empty(),
        "Expected non-empty output for binary expression"
    );
}

#[test]
fn format_pipe_expr() {
    let output = format("let x = data |> transform;");
    assert!(output.contains("|>"), "Expected '|>' in output: {}", output);
}
