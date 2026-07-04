//! Bake the workspace root path into the `coco` binary at compile time.
//!
//! The release binary reads `COCO_WORKSPACE_ROOT` at runtime so it can locate
//! packaged runtime data compiled from the workspace. There is no `coco build
//! --binary` output anymore; the VM is the sole execution path and releases
//! carry `.cb` artifacts or source, not a generated native crate.
//!
//! We compute the workspace root from `CARGO_MANIFEST_DIR` and expose it via
//! the `COCO_WORKSPACE_ROOT` env var so `main.rs` can read it with `env!`.

use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // crates/coco_cli -> crates/ -> workspace root (two levels up).
    let workspace_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("CARGO_MANIFEST_DIR has fewer than two parents");
    println!(
        "cargo:rustc-env=COCO_WORKSPACE_ROOT={}",
        workspace_root.display()
    );
}
