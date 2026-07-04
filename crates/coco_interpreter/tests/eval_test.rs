//! Ported eval tests — now use the bytecode VM (compile + run).
//! Each test wraps its source in `fn main() { ... }` so the VM can find and
//! call main(). The VM is the sole dev runtime.

use coco_interpreter::compiler::Compiler;
use coco_interpreter::vm::Vm;
use coco_interpreter::Value;
use coco_parser::Parser;
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
    assert_eq!(run_src("42").as_i64(), Some(42));
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
    assert!(matches!(run_src("\"hello\""), Value::String(s) if s == "hello"));
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
    assert_eq!(run_src("1 + 2").as_i64(), Some(3));
}
#[test]
fn subtraction() {
    assert_eq!(run_src("10 - 3").as_i64(), Some(7));
}
#[test]
fn multiplication() {
    assert_eq!(run_src("4 * 5").as_i64(), Some(20));
}
#[test]
fn division() {
    assert_eq!(run_src("10 / 3").as_i64(), Some(3));
}
#[test]
fn modulo() {
    assert_eq!(run_src("10 % 3").as_i64(), Some(1));
}
#[test]
fn power() {
    assert_eq!(run_src("2 ** 10").as_i64(), Some(1024));
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
fn logical_and_truthy() {
    // Regression: the truthy path of `&&` left the left operand on the stack,
    // causing "true is not callable" when used in a call argument.
    assert!(matches!(run_src("true && true"), Value::Bool(true)));
}
#[test]
fn logical_and_word_form() {
    assert!(matches!(run_src("true and true"), Value::Bool(true)));
    assert!(matches!(run_src("true and false"), Value::Bool(false)));
}
#[test]
fn logical_or() {
    assert!(matches!(run_src("false || true"), Value::Bool(true)));
}
#[test]
fn logical_xor_bool() {
    // Per grammar, `xor` (word-form of `^`) operates on bools as logical XOR.
    assert!(matches!(run_src("true xor false"), Value::Bool(true)));
    assert!(matches!(run_src("true xor true"), Value::Bool(false)));
    assert!(matches!(run_src("false xor false"), Value::Bool(false)));
}
#[test]
fn bitwise_xor_int() {
    // `^`/`xor` on ints remains bitwise.
    assert_eq!(run_src("5 ^ 3").as_i64(), Some(6));
}
#[test]
fn unary_neg() {
    assert_eq!(run_src("-5").as_i64(), Some(-5));
}
#[test]
fn deep_equals_builtin() {
    // Structural equality: type-strict, deep, order-independent for maps.
    assert!(matches!(run_src("deepEquals(1, 1)"), Value::Bool(true)));
    assert!(matches!(
        run_src("deepEquals(1, \"1\")"),
        Value::Bool(false)
    ));
    assert!(matches!(
        run_src("deepEquals([1,2,3], [1,2,3])"),
        Value::Bool(true)
    ));
    assert!(matches!(
        run_src("deepEquals([1,2], [1,2,3])"),
        Value::Bool(false)
    ));
    // Map order independence — the bug that toString comparison had.
    assert!(matches!(
        run_src("deepEquals({\"a\":1,\"b\":2}, {\"b\":2,\"a\":1})"),
        Value::Bool(true)
    ));
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
    assert_eq!(run_src("2 + 3 * 4").as_i64(), Some(14));
}
#[test]
fn grouping() {
    assert_eq!(run_src("(2 + 3) * 4").as_i64(), Some(20));
}
#[test]
fn let_binding() {
    assert_eq!(
        run_src("fn main() { let x = 42; return x; }").as_i64(),
        Some(42)
    );
}
#[test]
fn const_binding() {
    assert_eq!(
        run_src("fn main() { const y = 99; return y; }").as_i64(),
        Some(99)
    );
}
#[test]
fn reassignment() {
    assert_eq!(
        run_src("fn main() { let x = 1; x = 2; return x; }").as_i64(),
        Some(2)
    );
}
#[test]
fn add_assign() {
    assert_eq!(
        run_src("fn main() { let x = 10; x += 5; return x; }").as_i64(),
        Some(15)
    );
}
#[test]
fn sub_assign() {
    assert_eq!(
        run_src("fn main() { let x = 10; x -= 3; return x; }").as_i64(),
        Some(7)
    );
}
#[test]
fn if_true() {
    assert_eq!(
        run_src("fn main() { let x = 0; if true { x = 1; } return x; }").as_i64(),
        Some(1)
    );
}
#[test]
fn if_false() {
    assert_eq!(
        run_src("fn main() { let x = 0; if false { x = 1; } return x; }").as_i64(),
        Some(0)
    );
}
#[test]
fn if_else() {
    assert_eq!(
        run_src("fn main() { let x = 0; if false { x = 1; } else { x = 2; } return x; }").as_i64(),
        Some(2)
    );
}
#[test]
fn while_loop() {
    assert_eq!(
        run_src("fn main() { let x = 0; while x < 5 { x += 1; } return x; }").as_i64(),
        Some(5)
    );
}
#[test]
fn for_loop() {
    assert_eq!(
        run_src("fn main() { let s = 0; for n in [1, 2, 3] { s += n; } return s; }").as_i64(),
        Some(6)
    );
}
#[test]
fn for_in_map() {
    // Regression: `for k in map` failed because maps had no `length` member and
    // could not be indexed by int. Maps now expose `.length` and int-indexing
    // yields the key at that position.
    assert_eq!(run_src("fn main() { const m = {\"a\": 1, \"b\": 2}; let count = 0; for k in m { count += m[k]; } return count; }").as_i64(), Some(3));
}
#[test]
fn loop_break() {
    assert_eq!(
        run_src("fn main() { let x = 0; loop { x += 1; if x == 3 { break; } } return x; }")
            .as_i64(),
        Some(3)
    );
}
#[test]
fn function_call() {
    assert_eq!(
        run_src("fn add(a, b) { return a + b; } fn main() { return add(2, 3); }").as_i64(),
        Some(5)
    );
}
#[test]
fn arrow_expr_body_returns_value() {
    assert_eq!(
        run_src("fn main() { const x = () => 1; return x(); }").as_i64(),
        Some(1)
    );
}
#[test]
fn recursion() {
    assert_eq!(run_src("fn fib(n) { if n <= 1 { return n; } return fib(n - 1) + fib(n - 2); } fn main() { return fib(10); }").as_i64(), Some(55));
}
#[test]
fn nested_function_declaration() {
    // Regression: nested `fn` declarations inside another function body were
    // skipped by the compiler, so calling them yielded "null is not callable".
    assert_eq!(
        run_src("fn main() { fn helper() { return 42; } return helper(); }").as_i64(),
        Some(42)
    );
}
#[test]
fn nested_function_with_let_binding() {
    // Regression: a spurious OP_POP after STORE_LOCAL corrupted the caller's
    // stack, so binding the result of a nested-function call failed with
    // "local N out of bounds".
    assert_eq!(
        run_src("fn main() { fn helper() { return 7; } let r = helper(); return r; }").as_i64(),
        Some(7)
    );
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
    assert_eq!(
        run_src("fn main() { const a = [10, 20, 30]; return a[1]; }").as_i64(),
        Some(20)
    );
}
#[test]
fn list_length() {
    assert_eq!(
        run_src("fn main() { const a = [1, 2, 3, 4]; return a.length; }").as_i64(),
        Some(4)
    );
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
    assert_eq!(
        run_src("fn main() { const m = {\"a\": 100}; return m[\"a\"]; }").as_i64(),
        Some(100)
    );
}
