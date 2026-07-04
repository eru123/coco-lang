//! Integration tests for the `.cb` bytecode artifact path.
//!
//! These build a `.co` source to a `.cb` artifact via `coco build`, then run
//! the artifact via `coco run prog.cb`, and assert the output and exit code
//! match the source-run path. They guard the serialize/deserialize round-trip
//! end-to-end (including `BigInt` constants, nested `FnObj`s, and exit codes).

use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("coco_cli crate should live under crates/")
        .to_path_buf()
}

/// Path to the built `coco` binary. Tests run via `cargo test -p coco_cli`,
/// which builds the bin into the workspace target dir.
fn coco_bin() -> PathBuf {
    // CARGO_BIN_EXE_coco is set by cargo for integration tests and points at
    // the compiled binary.
    PathBuf::from(env!("CARGO_BIN_EXE_coco"))
}

/// Run `coco <args>` from the repo root. Returns (stdout, stderr, status_code).
fn coco(args: &[&str]) -> (String, String, Option<i32>) {
    let root = repo_root();
    let output = Command::new(coco_bin())
        .args(args)
        .current_dir(&root)
        .output()
        .expect("failed to spawn coco");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
        output.status.code(),
    )
}

/// Build a temp `.co` source, build it to `.cb`, and run the `.cb`.
/// Returns (cb_run_stdout, cb_run_exit_code).
fn build_and_run_cb(source: &str, slug: &str) -> (String, Option<i32>) {
    let dir = std::env::temp_dir().join(format!("coco-cb-test-{}-{}", std::process::id(), slug));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let co_path = dir.join("prog.co");
    let cb_path = dir.join("prog.cb");
    std::fs::write(&co_path, source).expect("write source");

    // Build the .cb artifact.
    let (out, err, code) = coco(&["build", &co_path.to_string_lossy()]);
    assert!(
        code == Some(0),
        "coco build failed (code {:?}):\nstdout:\n{}\nstderr:\n{}",
        code,
        out,
        err
    );
    assert!(cb_path.exists(), ".cb artifact was not written");

    // Run the .cb artifact.
    let (out, err, code) = coco(&["run", &cb_path.to_string_lossy()]);
    assert!(
        code.is_some(),
        "coco run prog.cb did not exit:\nstdout:\n{}\nstderr:\n{}",
        out,
        err
    );

    let _ = std::fs::remove_dir_all(&dir);
    (out, code)
}

#[test]
fn cb_runs_hello_world() {
    let source = r#"fn main(): int {
    print("Hello, World!");
    return 0;
}"#;
    let (out, code) = build_and_run_cb(source, "hello");
    assert_eq!(out, "Hello, World!\n");
    assert_eq!(code, Some(0));
}

#[test]
fn cb_propagates_nonzero_exit_code() {
    // main returns 42; the .cb path must propagate it as the process exit code.
    let source = "fn main(): int { return 42; }";
    let (_out, code) = build_and_run_cb(source, "exit42");
    assert_eq!(code, Some(42));
}

#[test]
fn cb_roundtrips_bigint_arithmetic() {
    // Exercises BigInt constants and arithmetic through serialize/deserialize:
    // the result exceeds i64, so it only works if bignum limbs round-trip.
    let source = r#"fn main(): int {
    let a = 1000000000000000000;
    let b = 1000000000000000000;
    print(a * b);
    return 0;
}"#;
    let (out, code) = build_and_run_cb(source, "bigint");
    assert_eq!(out, "1000000000000000000000000000000000000\n");
    assert_eq!(code, Some(0));
}

#[test]
fn cb_runs_functions_and_loops() {
    // Nested FnObj constants (each function is a constant) must round-trip.
    let source = r#"fn fib(n: int): int {
    if n < 2 { return n; }
    return fib(n - 1) + fib(n - 2);
}
fn main(): int {
    let r = fib(10);
    print(r);
    return r;
}"#;
    let (out, code) = build_and_run_cb(source, "fib");
    assert_eq!(out, "55\n");
    assert_eq!(code, Some(55));
}

#[test]
fn cb_output_matches_source_run() {
    // The .cb run path and the .co run path must produce identical output.
    let source = r#"fn main(): int {
    let xs = [1, 2, 3, 4, 5];
    let sum = 0;
    for x in xs { sum = sum + x; }
    print(sum);
    print(xs.length);
    return 0;
}"#;
    let dir = std::env::temp_dir().join(format!("coco-cb-match-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let co_path = dir.join("prog.co");
    let cb_path = dir.join("prog.cb");
    std::fs::write(&co_path, source).expect("write source");

    // Source run (skip checks so the test focuses on execution parity).
    let (src_out, _, src_code) = coco(&["run", "--no-check", &co_path.to_string_lossy()]);

    // Build + artifact run.
    let (_out, _err, build_code) = coco(&["build", &co_path.to_string_lossy()]);
    assert_eq!(build_code, Some(0), "build failed");
    let (cb_out, _, cb_code) = coco(&["run", &cb_path.to_string_lossy()]);

    assert_eq!(src_out, cb_out, "source run and .cb run output differ");
    assert_eq!(src_code, cb_code, "exit codes differ");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cb_rejects_garbage_input() {
    // Running a non-.cb file as if it were an artifact should fail clearly.
    let dir = std::env::temp_dir().join(format!("coco-cb-garbage-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let fake = dir.join("not-a.cb");
    std::fs::write(&fake, b"this is definitely not a coco artifact").expect("write fake");

    let (out, err, code) = coco(&["run", &fake.to_string_lossy()]);
    assert_ne!(code, Some(0), "should have failed on garbage input");
    // Either stdout or stderr should mention the deserialization failure.
    let combined = format!("{}\n{}", out, err);
    assert!(
        combined.contains("cb") || combined.contains("magic") || combined.contains("deserialize"),
        "expected a .cb/deserialize error message, got: {}",
        combined
    );

    let _ = std::fs::remove_dir_all(&dir);
}
