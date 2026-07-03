#!/usr/bin/env bash
# Verify the native (LLVM) codegen path end-to-end.
#
# Prereq: LLVM 18 installed. On Debian/Ubuntu:
#   sudo apt-get install -y llvm-18-dev
# (provides llvm-config-18 and the libLLVM-18.so / static libs llvm-sys needs)
# Also needs libzstd (the runtime .so is usually present; if linking fails
# with "-lzstd not found", install libzstd-dev or create a libzstd.so symlink).
#
# Usage: scripts/verify-native.sh
set -uo pipefail

cd "$(dirname "$0")/.."
PASS=0; FAIL=0

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
cargo build -p coco_cli --features native 2>&1 | tail -3 || { echo "BUILD FAILED"; exit 1; }

# Helper: native-compile a .co snippet and check its exit code.
check() {
    local name="$1" src="$2" expected="$3"
    local file="/tmp/native_$name.co"
    printf '%s\n' "$src" > "$file"
    if ./target/debug/coco build --native "$file" 2>/tmp/native_$name.err; then
        local bin="/tmp/native_$name"
        "$bin"; local rc=$?
        if [ "$rc" -eq "$expected" ]; then
            echo "  PASS  $name (exit $rc)"; PASS=$((PASS+1))
        else
            echo "  FAIL  $name (exit $rc, expected $expected)"; FAIL=$((FAIL+1))
        fi
    else
        echo "  FAIL  $name (compile error)"; cat /tmp/native_$name.err; FAIL=$((FAIL+1))
    fi
}

echo
echo "=== 3. Native codegen tests ==="
check add      'fn add(a: int, b: int): int { return a + b; } fn main(): int { return add(2, 3); }' 5
check sub      'fn main(): int { return 10 - 4; }' 6
check mul      'fn main(): int { return 6 * 7; }' 42
check fib      'fn fib(n: int): int { if n <= 1 { return n; } return fib(n - 1) + fib(n - 2); } fn main(): int { return fib(10); }' 55
check ternary  'fn main(): int { const x = 10; return x > 5 ? 100 : 200; }' 100
check nullco   'fn main(): int { const x = null; return x ?? 42; }' 42
check cmp      'fn main(): int { if 3 < 5 { return 7; } return 0; }' 7
check while    'fn main(): int { let i = 0; let s = 0; while i < 5 { s = s + i; i = i + 1; } return s; }' 10

echo
echo "=== Summary: $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ]
