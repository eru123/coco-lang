//! Bake the workspace root path into the `coco` binary at compile time.
//!
//! `coco build --binary` generates a small Rust crate that depends on
//! `coco_interpreter` by path. That path must point at the in-tree
//! `crates/coco_interpreter` directory, which lives one level up from this
//! crate. We compute it from `CARGO_MANIFEST_DIR` here and expose it via the
//! `COCO_WORKSPACE_ROOT` env var so `main.rs` can read it with `env!`.

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
