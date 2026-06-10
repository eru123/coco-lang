//! Ported eval tests — now use the bytecode VM (compile + run).
//! Each test wraps its source in `fn main() { ... }` so the VM can find and
//! call main(). The VM is the sole dev runtime.

use coco_parser::Parser;
use coco_interpreter::compiler::Compiler;
use coco_interpreter::vm::Vm;
use coco_interpreter::Value;
use num_bigint::BigInt;

fn run_src(src: &str) -> Value {
    let wrapped = if src.contains("fn main") {
        src.to_string()
    } else {
        format!("fn main() {{ return {} }}", src.trim_end_matches(';'))
    };
    let mut parser = Parser::new(&wrapped);
    let program = parser.parse_program();
    let mut compiler = Compiler::new();
    let chunk = compiler.compile_script(&program).expect("compile error");
    let mut vm = Vm::new();
    vm.run(&chunk).expect("VM error")
}

#[test]
fn int_literal() {
    assert!(matches!(run_src("42"), Value::Int(n) if n == BigInt::from(42)));
}
#[test]
fn float_literal() {
    match run_src("3.14") {
        Value::Float(f) => assert!((f - 3.14).abs() < 0.001),
        _ => panic!(),
    }
}
#[test]
fn string_literal() {
    assert!(matches!(run_src("\"hello\""), Value::String(ref s) if s == "hello"));
}
#[test]
fn bool_literal() {
    assert!(matches!(run_src("true"), Value::Bool(true)));
}
#[test]
fn null_literal() {
    assert!(matches!(run_src("null"), Value::Null));
}
#[test]
fn addition() {
    assert!(matches!(run_src("1 + 2"), Value::Int(n) if n == BigInt::from(3)));
}
#[test]
fn subtraction() {
    assert!(matches!(run_src("10 - 3"), Value::Int(n) if n == BigInt::from(7)));
}
#[test]
fn multiplication() {
    assert!(matches!(run_src("4 * 5"), Value::Int(n) if n == BigInt::from(20)));
}
#[test]
fn division() {
    assert!(matches!(run_src("10 / 3"), Value::Int(n) if n == BigInt::from(3)));
}
#[test]
fn modulo() {
    assert!(matches!(run_src("10 % 3"), Value::Int(n) if n == BigInt::from(1)));
}
#[test]
fn power() {
    assert!(matches!(run_src("2 ** 10"), Value::Int(n) if n == BigInt::from(1024)));
}
#[test]
fn comparison() {
    assert!(matches!(run_src("1 < 2"), Value::Bool(true)));
}
#[test]
fn equality() {
    assert!(matches!(run_src("1 == 1"), Value::Bool(true)));
}
#[test]
fn logical_and() {
    assert!(matches!(run_src("true && false"), Value::Bool(false)));
}
#[test]
fn logical_or() {
    assert!(matches!(run_src("false || true"), Value::Bool(true)));
}
#[test]
fn unary_neg() {
    assert!(matches!(run_src("-5"), Value::Int(n) if n == BigInt::from(-5)));
}
#[test]
fn unary_not() {
    assert!(matches!(run_src("!true"), Value::Bool(false)));
}
#[test]
fn string_concat() {
    match run_src("\"a\" + \"b\"") {
        Value::String(s) => assert_eq!(s, "ab"),
        _ => panic!(),
    }
}
#[test]
fn precedence() {
    assert!(matches!(run_src("2 + 3 * 4"), Value::Int(n) if n == BigInt::from(14)));
}
#[test]
fn grouping() {
    assert!(matches!(run_src("(2 + 3) * 4"), Value::Int(n) if n == BigInt::from(20)));
}
#[test]
fn let_binding() {
    assert!(matches!(run_src("fn main() { let x = 42; return x; }"), Value::Int(n) if n == BigInt::from(42)));
}
#[test]
fn const_binding() {
    assert!(matches!(run_src("fn main() { const y = 99; return y; }"), Value::Int(n) if n == BigInt::from(99)));
}
#[test]
fn reassignment() {
    assert!(matches!(run_src("fn main() { let x = 1; x = 2; return x; }"), Value::Int(n) if n == BigInt::from(2)));
}
#[test]
fn add_assign() {
    assert!(matches!(run_src("fn main() { let x = 10; x += 5; return x; }"), Value::Int(n) if n == BigInt::from(15)));
}
#[test]
fn sub_assign() {
    assert!(matches!(run_src("fn main() { let x = 10; x -= 3; return x; }"), Value::Int(n) if n == BigInt::from(7)));
}
#[test]
fn if_true() {
    assert!(matches!(
        run_src("fn main() { let x = 0; if true { x = 1; } return x; }"),
        Value::Int(n) if n == BigInt::from(1)
    ));
}
#[test]
fn if_false() {
    assert!(matches!(
        run_src("fn main() { let x = 0; if false { x = 1; } return x; }"),
        Value::Int(n) if n == BigInt::from(0)
    ));
}
#[test]
fn if_else() {
    assert!(matches!(
        run_src("fn main() { let x = 0; if false { x = 1; } else { x = 2; } return x; }"),
        Value::Int(n) if n == BigInt::from(2)
    ));
}
#[test]
fn while_loop() {
    assert!(matches!(
        run_src("fn main() { let x = 0; while x < 5 { x += 1; } return x; }"),
        Value::Int(n) if n == BigInt::from(5)
    ));
}
#[test]
fn for_loop() {
    assert!(matches!(
        run_src("fn main() { let s = 0; for n in [1, 2, 3] { s += n; } return s; }"),
        Value::Int(n) if n == BigInt::from(6)
    ));
}
#[test]
fn loop_break() {
    assert!(matches!(
        run_src("fn main() { let x = 0; loop { x += 1; if x == 3 { break; } } return x; }"),
        Value::Int(n) if n == BigInt::from(3)
    ));
}
#[test]
fn function_call() {
    assert!(matches!(
        run_src("fn add(a, b) { return a + b; } fn main() { return add(2, 3); }"),
        Value::Int(n) if n == BigInt::from(5)
    ));
}
#[test]
fn arrow_expr_body_returns_value() {
    assert!(matches!(run_src("fn main() { const x = () => 1; return x(); }"), Value::Int(n) if n == BigInt::from(1)));
}
#[test]
fn recursion() {
    assert!(matches!(
        run_src("fn fib(n) { if n <= 1 { return n; } return fib(n - 1) + fib(n - 2); } fn main() { return fib(10); }"),
        Value::Int(n) if n == BigInt::from(55)
    ));
}
#[test]
fn list_literal() {
    match run_src("fn main() { return [1, 2, 3]; }") {
        Value::List(l) => assert_eq!(l.data.len(), 3),
        _ => panic!(),
    }
}
#[test]
fn list_index() {
    assert!(matches!(
        run_src("fn main() { const a = [10, 20, 30]; return a[1]; }"),
        Value::Int(n) if n == BigInt::from(20)
    ));
}
#[test]
fn list_length() {
    assert!(matches!(
        run_src("fn main() { const a = [1, 2, 3, 4]; return a.length; }"),
        Value::Int(n) if n == BigInt::from(4)
    ));
}
#[test]
fn map_literal() {
    match run_src("fn main() { return {\"x\": 1, \"y\": 2}; }") {
        Value::Map(m) => assert_eq!(m.data.len(), 2),
        _ => panic!(),
    }
}
#[test]
fn map_index() {
    assert!(matches!(
        run_src("fn main() { const m = {\"a\": 100}; return m[\"a\"]; }"),
        Value::Int(n) if n == BigInt::from(100)
    ));
}
