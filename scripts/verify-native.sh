#!/usr/bin/env bash
# Verify the native (LLVM) codegen path end-to-end.
#
# Prereq: LLVM 18 installed. On Debian/Ubuntu:
#   sudo apt-get install -y llvm-18-dev
# (provides llvm-config-18 and the libLLVM-18.so / static libs llvm-sys needs)
#
# Usage: scripts/verify-native.sh
set -euo pipefail

cd "$(dirname "$0")/.."

echo "=== 1. Check LLVM 18 is available ==="
if command -v llvm-config-18 >/dev/null 2>&1; then
    echo "llvm-config-18: $(llvm-config-18 --version) at $(llvm-config-18 --prefix)"
elif [ -n "${LLVM_SYS_180_PREFIX:-}" ]; then
    echo "Using LLVM_SYS_180_PREFIX=$LLVM_SYS_180_PREFIX"
else
    echo "ERROR: llvm-config-18 not found and LLVM_SYS_180_PREFIX not set."
    echo "Install LLVM 18:  sudo apt-get install -y llvm-18-dev"
    exit 1
fi

echo
echo "=== 2. Build the CLI with the native feature ==="
cargo build -p coco_cli --features native 2>&1 | tail -5

echo
echo "=== 3. Native-compile a test .co file ==="
cat > /tmp/native_test.co <<'EOF'
fn add(a: int, b: int): int {
    return a + b;
}
fn main(): int {
    return add(2, 3);
}
EOF
./target/debug/coco build --native /tmp/native_test.co 2>&1 || {
    echo "Native build failed (see output above)."
    exit 1
}

echo
echo "=== 4. Run the native binary and check the exit code ==="
BIN=/tmp/native_test
if [ -x "$BIN" ]; then
    "$BIN"; rc=$?
    echo "native binary exit code: $rc (expected 5)"
    [ "$rc" -eq 5 ] && echo "PASS: native codegen produces a working binary" || echo "FAIL: expected exit 5"
else
    echo "Native binary not produced at $BIN"
    exit 1
fi
