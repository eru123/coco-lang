//! Build script for coco_codegen.
//!
//! When the `native` feature is enabled, ensures LLVM 18 is available for
//! `llvm-sys`/`inkwell` to link against.
//!
//! IMPORTANT: a crate's build script CANNOT propagate environment variables to
//! its dependencies' build scripts. In particular, emitting
//! `cargo:rustc-env=LLVM_SYS_180_PREFIX=...` here does NOT make `llvm-sys`'s
//! own build script see that variable — `llvm-sys` reads it from its own
//! process environment. So this script does NOT attempt to vendor LLVM itself
//! (that path was broken). Instead it resolves LLVM one of two ways and, if
//! neither works, emits a clear error telling you how to fix it:
//!
//!   1. `LLVM_SYS_180_PREFIX` already set in the real environment (shell,
//!      CI, or `.cargo/config.toml [env]`). `llvm-sys` sees it directly.
//!   2. `llvm-config-18` (or `llvm-config`) on PATH — `llvm-sys` finds this
//!      itself, no action needed here.
//!
//! To use a vendored LLVM on a machine without a system install, run
//! `scripts/fetch-llvm.sh` first, which downloads LLVM 18 to a stable path and
//! prints the `LLVM_SYS_180_PREFIX=...` line to export into your shell before
//! invoking `cargo build --features native`.
//!
//! When the `native` feature is NOT enabled, this script does nothing — the
//! crate compiles to an empty stub and no LLVM is required.

use std::env;
use std::path::PathBuf;
use std::process::Command;

const LLVM_VERSION: &str = "18.1.8";

fn main() {
    // Only act when the `native` feature is enabled.
    if env::var_os("CARGO_FEATURE_NATIVE").is_none() {
        return;
    }

    // 1. Honour an explicit LLVM_SYS_180_PREFIX in the real environment.
    if let Some(prefix) = env::var_os("LLVM_SYS_180_PREFIX") {
        let prefix = PathBuf::from(&prefix);
        if prefix.join("bin/llvm-config").exists() || prefix.join("bin/llvm-config-18").exists() {
            println!(
                "cargo:warning=coco_codegen: using LLVM at {} (via LLVM_SYS_180_PREFIX)",
                prefix.display()
            );
            return;
        }
        // Set but invalid — fall through to the llvm-config probe, then error.
        println!(
            "cargo:warning=coco_codegen: LLVM_SYS_180_PREFIX is set but no bin/llvm-config found at {}",
            prefix.display()
        );
    }

    // 2. Probe llvm-config-18 / llvm-config on PATH. If present, llvm-sys will
    //    find LLVM on its own; nothing more to do here.
    if find_via_llvm_config().is_some() {
        println!("cargo:warning=coco_codegen: using LLVM found via llvm-config on PATH");
        return;
    }

    // 3. Nothing found. Emit a clear, actionable error. We do NOT hard-fail the
    //    build script (that produces an opaque message); instead we let the
    //    build continue so llvm-sys's own, well-known error is the primary
    //    diagnostic, augmented by these hints.
    println!("cargo:warning=coco_codegen: LLVM 18 was not found.");
    println!("cargo:warning=coco_codegen: either install it (e.g. `apt-get install llvm-18-dev libpolly-18-dev`),");
    println!("cargo:warning=coco_codegen: or run `scripts/fetch-llvm.sh` and export the LLVM_SYS_180_PREFIX it prints,");
    println!("cargo:warning=coco_codegen: or set LLVM_SYS_180_PREFIX=<llvm-18-prefix> before building with --features native.");
    let _ = LLVM_VERSION; // referenced for future version-specific messaging
}

/// Run `llvm-config-18 --prefix` (falling back to `llvm-config`) to detect a
/// system install. Returns the prefix if the version matches 18.x.
fn find_via_llvm_config() -> Option<PathBuf> {
    for tool in ["llvm-config-18", "llvm-config"] {
        let output = Command::new(tool).arg("--prefix").output().ok()?;
        if !output.status.success() {
            continue;
        }
        let prefix = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if prefix.is_empty() {
            continue;
        }
        let ver = Command::new(tool).arg("--version").output().ok()?;
        let ver = String::from_utf8_lossy(&ver.stdout).trim().to_string();
        if ver.starts_with("18.") {
            return Some(PathBuf::from(prefix));
        }
    }
    None
}
