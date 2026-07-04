//! Interpreter edge-case tests targeting VM boundaries, numeric limits, and
//! APC-influenced paths. These are Rust-level assertions around `Vm::run(...)`
//! so we can validate exact returned values without string-scanning `print`.

use coco_interpreter::compiler::Compiler;
use coco_interpreter::vm::Vm;
use coco_interpreter::Value;
use num_bigint::BigInt;

fn run(src: &str) -> Value {
    let wrapped = if src.contains("fn main") {
        src.to_string()
    } else {
        format!("fn main() {{ return {} }}", src.trim_end_matches(';'))
    };
    let mut parser = coco_parser::Parser::new(&wrapped);
    let program = parser.parse_program();
    let mut compiler = Compiler::new();
    let chunk = compiler.compile_script(&program).expect("compile error");
    let mut vm = Vm::new();
    vm.run(&chunk).expect("VM error")
}

#[test]
fn integer_overflow_escalates_to_bigint() {
    let max = i64::MAX as u64;
    let src = format!("fn main() {{ return {} + 1; }}", max);
    let val = run(&src);
    match val {
        Value::Int(n) => assert_eq!(n, BigInt::from(max as u128 + 1)),
        Value::Int64(n) => panic!("expected BigInt escalation, got Int64 {}", n),
        other => panic!("unexpected value {:?}", other),
    }
}

#[test]
fn i64_min_negation_stays_fit_or_escalates_at_runtime() {
    // When compiling a negative i64 literal, the parser/compiler currently
    // preserves the value as an Int64 literal if it fits; this validates the
    // stable default path. The dynamic overflow escalation path is exercised by
    // `integer_overflow_escalates_to_bigint` and runtime arithmetic cases.
    let src = "fn main() { return -9223372036854775808; }";
    let val = run(src);
    // Default behavior is stable Int64 representation here; if runtime
    // representation changes later, this should be updated to explicit BigInt.
    assert_eq!(val.as_i64(), Some(-9223372036854775808_i64));
}

#[test]
fn division_by_zero_returns_runtime_error() {
    let src = "fn main() { return 1 / 0; }";
    let mut parser = coco_parser::Parser::new(src);
    let program = parser.parse_program();
    let mut compiler = Compiler::new();
    let chunk = compiler.compile_script(&program).expect("compile error");
    let mut vm = Vm::new();
    let result = vm.run(&chunk);
    assert!(result.is_err(), "expected runtime error for /0");
}

#[test]
fn modulo_by_zero_returns_runtime_error() {
    let src = "fn main() { return 7 % 0; }";
    let mut parser = coco_parser::Parser::new(src);
    let program = parser.parse_program();
    let mut compiler = Compiler::new();
    let chunk = compiler.compile_script(&program).expect("compile error");
    let mut vm = Vm::new();
    let result = vm.run(&chunk);
    assert!(result.is_err(), "expected runtime error for %0");
}

#[test]
fn hang_and_improved_re_abuse() {
    // A deep recursion on fib should still return after reasonable work.
    let src =
        "fn fib(n) { if n <= 1 { return n; } return fib(n - 1) + fib(n - 2); } fn main() { return fib(20); }";
    let val = run(src);
    assert_eq!(val.as_i64(), Some(6765));

    // Many short-lived allocations in a loop to stress paths touched by APC refactors.
    let src =
        "fn main() { let sum = 0; let i = 0; while i < 500 { sum += i; i = i + 1; } return sum; }";
    let val = run(src);
    assert_eq!(val.as_i64(), Some(124750));
}

#[test]
fn apc_advisory_feature_does_not_change_default_runtime() {
    // Default features must not require `coco_num` to compile/run basic arithmetic.
    let src = "fn main() { return 1 + 2; }";
    let mut parser = coco_parser::Parser::new(src);
    let program = parser.parse_program();
    let mut compiler = Compiler::new();
    let chunk = compiler
        .compile_script(&program)
        .expect("compile default-feature program");
    let mut vm = Vm::new();
    let val = vm.run(&chunk).expect("run default-feature program");
    assert_eq!(val.as_i64(), Some(3));
}
