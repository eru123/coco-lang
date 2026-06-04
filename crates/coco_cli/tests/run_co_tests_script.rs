use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("coco_cli crate should live under crates/")
        .to_path_buf()
}

#[test]
fn run_co_tests_lists_requested_files() {
    let root = repo_root();
    let output = Command::new("bash")
        .arg(root.join("scripts/run-co-tests.sh"))
        .arg("--list")
        .arg("--pattern")
        .arg("tests/01-hello-world.co")
        .current_dir(&root)
        .output()
        .expect("failed to run scripts/run-co-tests.sh");

    assert!(
        output.status.success(),
        "script failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "tests/01-hello-world.co"
    );
}
