use coco_interpreter::{Interpreter, Value};

fn eval(src: &str) -> Value {
    let mut interp = Interpreter::new();
    interp.eval_source(src).unwrap()
}

#[test]
fn int_literal() {
    assert!(matches!(eval("42;"), Value::Int(42)));
}
#[test]
fn float_literal() {
    match eval("3.14;") {
        Value::Float(f) => assert!((f - 3.14).abs() < 0.001),
        _ => panic!(),
    }
}
#[test]
fn string_literal() {
    assert!(matches!(eval("\"hello\";"), Value::String(ref s) if s == "hello"));
}
#[test]
fn bool_literal() {
    assert!(matches!(eval("true;"), Value::Bool(true)));
}
#[test]
fn null_literal() {
    assert!(matches!(eval("null;"), Value::Null));
}
#[test]
fn addition() {
    assert!(matches!(eval("1 + 2;"), Value::Int(3)));
}
#[test]
fn subtraction() {
    assert!(matches!(eval("10 - 3;"), Value::Int(7)));
}
#[test]
fn multiplication() {
    assert!(matches!(eval("4 * 5;"), Value::Int(20)));
}
#[test]
fn division() {
    assert!(matches!(eval("10 / 3;"), Value::Int(3)));
}
#[test]
fn modulo() {
    assert!(matches!(eval("10 % 3;"), Value::Int(1)));
}
#[test]
fn power() {
    assert!(matches!(eval("2 ** 10;"), Value::Int(1024)));
}
#[test]
fn comparison() {
    assert!(matches!(eval("1 < 2;"), Value::Bool(true)));
}
#[test]
fn equality() {
    assert!(matches!(eval("1 == 1;"), Value::Bool(true)));
}
#[test]
fn logical_and() {
    assert!(matches!(eval("true && false;"), Value::Bool(false)));
}
#[test]
fn logical_or() {
    assert!(matches!(eval("false || true;"), Value::Bool(true)));
}
#[test]
fn unary_neg() {
    assert!(matches!(eval("-5;"), Value::Int(-5)));
}
#[test]
fn unary_not() {
    assert!(matches!(eval("!true;"), Value::Bool(false)));
}
#[test]
fn string_concat() {
    match eval("\"a\" + \"b\";") {
        Value::String(s) => assert_eq!(s, "ab"),
        _ => panic!(),
    }
}
#[test]
fn precedence() {
    assert!(matches!(eval("2 + 3 * 4;"), Value::Int(14)));
}
#[test]
fn grouping() {
    assert!(matches!(eval("(2 + 3) * 4;"), Value::Int(20)));
}
#[test]
fn let_binding() {
    assert!(matches!(eval("let x = 42; x;"), Value::Int(42)));
}
#[test]
fn const_binding() {
    assert!(matches!(eval("const y = 99; y;"), Value::Int(99)));
}
#[test]
fn reassignment() {
    assert!(matches!(eval("let x = 1; x = 2; x;"), Value::Int(2)));
}
#[test]
fn add_assign() {
    assert!(matches!(eval("let x = 10; x += 5; x;"), Value::Int(15)));
}
#[test]
fn sub_assign() {
    assert!(matches!(eval("let x = 10; x -= 3; x;"), Value::Int(7)));
}
#[test]
fn if_true() {
    assert!(matches!(
        eval("let x = 0; if true { x = 1; } x;"),
        Value::Int(1)
    ));
}
#[test]
fn if_false() {
    assert!(matches!(
        eval("let x = 0; if false { x = 1; } x;"),
        Value::Int(0)
    ));
}
#[test]
fn if_else() {
    assert!(matches!(
        eval("let x = 0; if false { x = 1; } else { x = 2; } x;"),
        Value::Int(2)
    ));
}
#[test]
fn while_loop() {
    assert!(matches!(
        eval("let x = 0; while x < 5 { x += 1; } x;"),
        Value::Int(5)
    ));
}
#[test]
fn for_loop() {
    assert!(matches!(
        eval("let s = 0; for n in [1, 2, 3] { s += n; } s;"),
        Value::Int(6)
    ));
}
#[test]
fn loop_break() {
    assert!(matches!(
        eval("let x = 0; loop { x += 1; if x == 3 { break; } } x;"),
        Value::Int(3)
    ));
}
#[test]
fn function_call() {
    assert!(matches!(
        eval("fn add(a, b) { return a + b; } add(2, 3);"),
        Value::Int(5)
    ));
}
#[test]
fn arrow_expr_body_returns_value() {
    assert!(matches!(eval("const x = () => 1; x();"), Value::Int(1)));
}
#[test]
fn recursion() {
    assert!(matches!(
        eval("fn fib(n) { if n <= 1 { return n; } return fib(n - 1) + fib(n - 2); } fib(10);"),
        Value::Int(55)
    ));
}
#[test]
fn list_literal() {
    match eval("[1, 2, 3];") {
        Value::List(l) => assert_eq!(l.data.len(), 3),
        _ => panic!(),
    }
}
#[test]
fn list_index() {
    assert!(matches!(
        eval("const a = [10, 20, 30]; a[1];"),
        Value::Int(20)
    ));
}
#[test]
fn list_length() {
    assert!(matches!(
        eval("const a = [1, 2, 3, 4]; a.length;"),
        Value::Int(4)
    ));
}
#[test]
fn map_literal() {
    match eval("{\"x\": 1, \"y\": 2};") {
        Value::Map(m) => assert_eq!(m.data.len(), 2),
        _ => panic!(),
    }
}
#[test]
fn null_coalesce() {
    assert!(matches!(eval("null ?? 42;"), Value::Int(42)));
}
#[test]
fn ternary() {
    assert!(matches!(eval("let x = true ? 1 : 2; x;"), Value::Int(1)));
}
#[test]
fn run_main() {
    let mut interp = Interpreter::new();
    let result = interp.run_main("fn main() { return 0; }").unwrap();
    assert!(matches!(result, Value::Int(0)));
}
#[test]
fn run_main_with_print() {
    let mut interp = Interpreter::new();
    let result = interp
        .run_main("fn main() { print(\"hi\"); return 0; }")
        .unwrap();
    assert!(matches!(result, Value::Int(0)));
}
