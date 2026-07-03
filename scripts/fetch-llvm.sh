#!/usr/bin/env bash
# Fetch a prebuilt LLVM 18 for building Coco's native (LLVM) codegen, for
# machines without a system LLVM 18 install.
#
# This exists because a Cargo build script CANNOT propagate an env var to its
# dependencies' build scripts — so coco_codegen/build.rs cannot tell llvm-sys
# where a downloaded LLVM lives. The fix is to download LLVM to a stable path
# HERE, then export LLVM_SYS_180_PREFIX into the REAL shell environment before
# running `cargo build --features native`.
#
# Usage:
#   scripts/fetch-llvm.sh                       # downloads to vendor/llvm-18
#   eval "$(scripts/fetch-llvm.sh --env)"       # prints the export line
#
# Then: cargo build -p coco_cli --features native
set -euo pipefail

cd "$(dirname "$0")/.."
LLVM_VERSION="18.1.8"
DEST="${COCO_LLVM_DEST:-vendor/llvm-18}"

# Print-only mode: emit the export line for eval.
if [ "${1:-}" = "--env" ]; then
  echo "export LLVM_SYS_180_PREFIX=\"$(pwd)/$DEST\""
  exit 0
fi

# Map the Rust target triple to the LLVM prebuilt tarball suffix.
target="$(rustc -vV 2>/dev/null | sed -n 's/^host: //p' || true)"
if [ -z "$target" ]; then
  echo "error: could not determine host target via rustc" >&2; exit 1
fi
case "$target" in
  *linux*x86_64*)  suffix="x86_64-linux-gnu-ubuntu-18.04" ;;
  *linux*aarch64*) suffix="aarch64-linux-gnu-ubuntu-18.04" ;;
  *darwin*aarch64*) suffix="arm64-apple-darwin" ;;
  *darwin*x86_64*) suffix="x86_64-apple-darwin" ;;
  *windows*x86_64*) suffix="pc-windows-msvc" ;;
  *) suffix="$target" ;;
esac

tarball="clang+llvm-${LLVM_VERSION}-${suffix}.tar.xz"
url="https://github.com/llvm/llvm-project/releases/download/llvmorg-${LLVM_VERSION}/${tarball}"
prefix="$DEST/clang+llvm-${LLVM_VERSION}-${suffix}"

if [ -x "$prefix/bin/llvm-config" ]; then
  echo "LLVM $LLVM_VERSION already present at $prefix"
else
  echo "Downloading $url ..."
  mkdir -p "$DEST"
  if [ ! -f "$DEST/$tarball" ]; then
    curl -L --fail -o "$DEST/$tarball" "$url"
  fi
  echo "Extracting ..."
  tar -xf "$DEST/$tarball" -C "$DEST"
fi

echo
echo "Done. LLVM $LLVM_VERSION is at:"
echo "  $prefix"
echo
echo "Before building Coco with --features native, run:"
echo "  export LLVM_SYS_180_PREFIX=\"$(pwd)/$prefix\""
echo "  eval \"\$(scripts/fetch-llvm.sh --env)\"   # shorthand"
echo "  cargo build -p coco_cli --features native"
