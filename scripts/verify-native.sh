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

# Helper: a .co snippet that MUST fail to compile (unsupported construct).
# `pattern` is grepped in the error output to confirm the right error fired.
check_err() {
    local name="$1" src="$2" pattern="$3"
    local file="/tmp/native_$name.co"
    printf '%s\n' "$src" > "$file"
    if ./target/debug/coco build --native "$file" 2>/tmp/native_$name.err; then
        echo "  FAIL  $name (compiled but should have errored)"; FAIL=$((FAIL+1))
    elif grep -qi "$pattern" /tmp/native_$name.err; then
        echo "  PASS  $name (rejected: $(grep -i "$pattern" /tmp/native_$name.err | head -1 | sed 's/^Codegen error: //'))"; PASS=$((PASS+1))
    else
        echo "  FAIL  $name (errored but wrong message)"; cat /tmp/native_$name.err; FAIL=$((FAIL+1))
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
echo "=== 4. Control flow (break/continue/loops/for) ==="
check break      'fn main(): int { let i = 0; let s = 0; while true { if i >= 5 { break; } s = s + i; i = i + 1; } return s; }' 10
check continue   'fn main(): int { let s = 0; for i in 0..5 { if i == 2 { continue; } s = s + i; } return s; }' 8
check loop_stmt  'fn main(): int { let i = 0; loop { if i >= 3 { break; } i = i + 1; } return i; }' 3
check dowhile    'fn main(): int { let i = 5; do { i = i + 1; } while i < 0; return i; }' 6
check forrange   'fn main(): int { let s = 0; for x in 1..5 { s = s + x; } return s; }' 10
check forincl    'fn main(): int { let s = 0; for x in 1..=5 { s = s + x; } return s; }' 15
check elseif     'fn main(): int { let x = 2; if x == 1 { return 10; } else if x == 2 { return 20; } else { return 30; } }' 20

echo
echo "=== 5. Operators (logical/mod/bitwise/shift) ==="
check andop      'fn main(): int { if 1 && 0 { return 1; } return 7; }' 7
check andtrue    'fn main(): int { if 1 && 1 { return 7; } return 1; }' 7
check orop       'fn main(): int { if 0 || 1 { return 7; } return 1; }' 7
check orfalse    'fn main(): int { if 0 || 0 { return 7; } return 1; }' 1
check modop      'fn main(): int { return 17 % 5; }' 2
check bitand     'fn main(): int { return 12 & 10; }' 8
check bitor      'fn main(): int { return 12 | 10; }' 14
check bitxor     'fn main(): int { return 12 ^ 10; }' 6
check bitnot     'fn main(): int { return ~0; }' 255
check shl        'fn main(): int { return 1 << 4; }' 16
check shr        'fn main(): int { return 256 >> 4; }' 16
check compound   'fn main(): int { let i = 10; i += 5; i -= 2; i *= 2; i %= 7; return i; }' 5

echo
echo "=== 6. Unsupported constructs must error (not silently no-op) ==="
check_err err_throw    'fn main(): int { throw 1; return 0; }' 'throw statement'
check_err err_break    'fn main(): int { break; return 0; }' 'break outside loop'
check_err err_array    'fn main(): int { let a = [1, 2, 3]; return 0; }' 'array literal'
check_err err_index    'fn main(): int { let a = 1; return a[0]; }' 'indexing'
check_err err_member   'fn main(): int { let a = 1; return a.x; }' 'member access'
check_err err_undefvar 'fn main(): int { return x; }' 'undefined variable'
check_err err_unkcall  'fn main(): int { return nope(); }' 'unknown function'
check_err err_forlist  'fn main(): int { for x in someList { } return 0; }' 'non-range'

echo
echo "=== Summary: $PASS passed, $FAIL failed ==="
[ "$FAIL" -eq 0 ]
