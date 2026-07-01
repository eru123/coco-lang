//! Build script for coco_codegen.
//!
//! When the `native` feature is enabled, ensures LLVM 18 is available for
//! `llvm-sys`/`inkwell` to link against. Resolution order:
//!   1. `LLVM_SYS_180_PREFIX` env var (if set and valid).
//!   2. `llvm-config-18` on PATH (system install).
//!   3. A prebuilt `clang+llvm-18.1.8-{target}.tar.xz` downloaded from the
//!      LLVM releases on GitHub, extracted to `target/llvm-18/`.
//!
//! When the resolved prefix is the vendored download, sets
//! `cargo:rustc-env=LLVM_SYS_180_PREFIX=<abs-path>` so llvm-sys finds it.
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

    // 1. Honour an explicit LLVM_SYS_180_PREFIX if it points at a real install.
    if let Some(prefix) = env::var_os("LLVM_SYS_180_PREFIX") {
        let prefix = PathBuf::from(&prefix);
        if prefix.join("bin/llvm-config").exists() || prefix.join("bin/llvm-config-18").exists() {
            println!("cargo:rustc-env=LLVM_SYS_180_PREFIX={}", prefix.display());
            println!("cargo:warning=coco_codegen: using system LLVM at {}", prefix.display());
            return;
        }
    }

    // 2. Try llvm-config-18 / llvm-config on PATH.
    if let Some(prefix) = find_via_llvm_config() {
        println!("cargo:rustc-env=LLVM_SYS_180_PREFIX={}", prefix.display());
        println!("cargo:warning=coco_codegen: using LLVM from llvm-config at {}", prefix.display());
        return;
    }

    // 3. Download and vendor a prebuilt LLVM.
    match vendor_llvm() {
        Ok(prefix) => {
            println!("cargo:rustc-env=LLVM_SYS_180_PREFIX={}", prefix.display());
            println!("cargo:warning=coco_codegen: vendored LLVM 18 at {}", prefix.display());
        }
        Err(e) => {
            // Don't hard-fail the build script — emit a clear warning so the
            // subsequent llvm-sys error is contextualized.
            println!("cargo:warning=coco_codegen: could not locate or vendor LLVM 18: {}", e);
            println!("cargo:warning=coco_codegen: install LLVM 18 (e.g. `apt install llvm-18`) or set LLVM_SYS_180_PREFIX");
        }
    }
}

/// Run `llvm-config-18 --prefix` (falling back to `llvm-config`) to find a
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
        // Verify the version is 18.x.
        let ver = Command::new(tool).arg("--version").output().ok()?;
        let ver = String::from_utf8_lossy(&ver.stdout).trim().to_string();
        if ver.starts_with("18.") {
            return Some(PathBuf::from(prefix));
        }
    }
    None
}

/// Download the prebuilt clang+llvm tarball for the host target and extract
/// it to `target/llvm-18/`. Returns the prefix path on success.
fn vendor_llvm() -> Result<PathBuf, String> {
    let target = env::var("TARGET").map_err(|_| "TARGET not set".to_string())?;
    let tarball_name = llvm_tarball_name(&target)
        .ok_or_else(|| format!("no prebuilt LLVM tarball for target {}", target))?;

    // Out dir: OUT_DIR is like target/<profile>/build/<hash>; use the target
    // dir two levels up to host a shared llvm-18/ across builds.
    let out_dir = env::var_os("OUT_DIR")
        .map(PathBuf::from)
        .ok_or_else(|| "OUT_DIR not set".to_string())?;
    // target dir = OUT_DIR/../../../..
    let target_dir = out_dir
        .ancestors()
        .nth(3)
        .ok_or_else(|| "could not determine target dir".to_string())?
        .to_path_buf();
    let llvm_dir = target_dir.join("llvm-18");

    // The extracted top-level dir is clang+llvm-18.1.8-<target>; symlink or
    // treat it as the prefix.
    let prefix = llvm_dir.join(format!("clang+llvm-{}-{}", LLVM_VERSION, llvm_tarball_target(&target)));
    if prefix.join("bin/llvm-config").exists() {
        return Ok(prefix);
    }

    // Download using curl (available on most CI runners).
    let url = format!(
        "https://github.com/llvm/llvm-project/releases/download/llvmorg-{}/{}",
        LLVM_VERSION, tarball_name
    );
    let tarball_path = llvm_dir.join(tarball_name);
    std::fs::create_dir_all(&llvm_dir).map_err(|e| e.to_string())?;

    if !tarball_path.exists() {
        let status = Command::new("curl")
            .args(["-L", "-o"])
            .arg(&tarball_path)
            .arg(&url)
            .status()
            .map_err(|e| format!("curl failed to start: {}", e))?;
        if !status.success() {
            return Err(format!("curl download failed for {}", url));
        }
    }

    // Extract with tar (supports .tar.xz).
    let status = Command::new("tar")
        .args(["-xf"])
        .arg(&tarball_path)
        .arg("-C")
        .arg(&llvm_dir)
        .status()
        .map_err(|e| format!("tar failed to start: {}", e))?;
    if !status.success() {
        return Err("tar extraction failed".to_string());
    }

    if prefix.join("bin/llvm-config").exists() {
        Ok(prefix)
    } else {
        Err("extraction did not produce bin/llvm-config".to_string())
    }
}

/// Map a Rust target triple to the LLVM prebuilt tarball's target suffix.
fn llvm_tarball_target(target: &str) -> String {
    if target.contains("linux") && target.contains("x86_64") {
        "x86_64-linux-gnu-ubuntu-18.04".to_string()
    } else if target.contains("linux") && target.contains("aarch64") {
        "aarch64-linux-gnu-ubuntu-18.04".to_string()
    } else if target.contains("darwin") && target.contains("aarch64") {
        "arm64-apple-darwin".to_string()
    } else if target.contains("darwin") && target.contains("x86_64") {
        "x86_64-apple-darwin".to_string()
    } else if target.contains("windows") && target.contains("x86_64") {
        "pc-windows-msvc".to_string()
    } else {
        target.to_string()
    }
}

/// The full tarball filename for a target.
fn llvm_tarball_name(target: &str) -> Option<String> {
    let suffix = llvm_tarball_target(target);
    Some(format!("clang+llvm-{}-{}.tar.xz", LLVM_VERSION, suffix))
}
