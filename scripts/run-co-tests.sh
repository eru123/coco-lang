#!/usr/bin/env bash
set -uo pipefail

usage() {
    cat <<'USAGE'
Usage: scripts/run-co-tests.sh [OPTIONS] [FILES_OR_GLOBS...]

Build the Coco CLI once, then run a command against each .co file.
Defaults to tests/*.co in run mode, skipping run-time type/safety gates.

Options:
  -m, --mode MODE       Command to run: run, typecheck, safety, parse, check (default: run)
                        check runs typecheck and safety as failing gates
  -p, --pattern GLOB    Add a glob to discover files (default: tests/*.co when no files are given)
      --release         Use target/release/coco instead of target/debug/coco
      --no-build        Do not run cargo build before executing files
      --checked         Do not pass --no-check to `coco run`
      --no-check        Pass --no-check to `coco run` (default)
      --coco PATH       Use an existing coco binary path
      --list            Print discovered files and exit
  -v, --verbose         Print each command and successful command output
  -h, --help            Show this help text

Examples:
  scripts/run-co-tests.sh
  scripts/run-co-tests.sh --mode typecheck 'tests/*.co'
  scripts/run-co-tests.sh --release --verbose tests/01-hello-world.co
USAGE
}

die() {
    printf 'error: %s\n' "$*" >&2
    exit 2
}

repo_root() {
    local script_dir
    script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
    cd -- "$script_dir/.." && pwd
}

MODE="run"
PROFILE="debug"
BUILD=1
NO_CHECK=1
LIST_ONLY=0
VERBOSE=0
COCO_BIN=""
PATTERNS=()
SPECS=()

while (($#)); do
    case "$1" in
        -m|--mode)
            [[ $# -ge 2 ]] || die "$1 requires a mode"
            MODE="$2"
            shift 2
            ;;
        --mode=*)
            MODE="${1#*=}"
            shift
            ;;
        -p|--pattern)
            [[ $# -ge 2 ]] || die "$1 requires a glob"
            PATTERNS+=("$2")
            shift 2
            ;;
        --pattern=*)
            PATTERNS+=("${1#*=}")
            shift
            ;;
        --release)
            PROFILE="release"
            shift
            ;;
        --no-build)
            BUILD=0
            shift
            ;;
        --checked)
            NO_CHECK=0
            shift
            ;;
        --no-check)
            NO_CHECK=1
            shift
            ;;
        --coco)
            [[ $# -ge 2 ]] || die "$1 requires a path"
            COCO_BIN="$2"
            BUILD=0
            shift 2
            ;;
        --coco=*)
            COCO_BIN="${1#*=}"
            BUILD=0
            shift
            ;;
        --list)
            LIST_ONLY=1
            BUILD=0
            shift
            ;;
        -v|--verbose)
            VERBOSE=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        --)
            shift
            while (($#)); do
                SPECS+=("$1")
                shift
            done
            ;;
        -*)
            die "unknown option: $1"
            ;;
        *)
            SPECS+=("$1")
            shift
            ;;
    esac
done

case "$MODE" in
    run|typecheck|safety|parse|check) ;;
    *) die "unsupported mode '$MODE' (expected run, typecheck, safety, parse, or check)" ;;
esac

ROOT="$(repo_root)"
cd -- "$ROOT" || exit 2

if [[ ${#SPECS[@]} -eq 0 && ${#PATTERNS[@]} -eq 0 ]]; then
    PATTERNS=("tests/*.co")
fi

FILES=()
add_matches() {
    local spec="$1"
    local matches=()

    if [[ -f "$spec" ]]; then
        FILES+=("$spec")
        return
    fi

    shopt -s nullglob
    matches=( $spec )
    shopt -u nullglob

    if [[ ${#matches[@]} -eq 0 ]]; then
        die "no files matched '$spec'"
    fi

    local match
    for match in "${matches[@]}"; do
        [[ -f "$match" ]] || continue
        FILES+=("$match")
    done
}

for pattern in "${PATTERNS[@]}"; do
    add_matches "$pattern"
done
for spec in "${SPECS[@]}"; do
    add_matches "$spec"
done

if [[ ${#FILES[@]} -eq 0 ]]; then
    die "no .co files found"
fi

mapfile -t FILES < <(printf '%s\n' "${FILES[@]}" | sort -u)

if ((LIST_ONLY)); then
    printf '%s\n' "${FILES[@]}"
    exit 0
fi

if [[ -z "$COCO_BIN" ]]; then
    COCO_BIN="$ROOT/target/$PROFILE/coco"
fi

if ((BUILD)); then
    cargo_args=(build --quiet --bin coco)
    if [[ "$PROFILE" == "release" ]]; then
        cargo_args+=(--release)
    fi
    printf 'Building coco (%s)...\n' "$PROFILE"
    cargo "${cargo_args[@]}" || exit 1
fi

[[ -x "$COCO_BIN" ]] || die "coco binary is not executable: $COCO_BIN"

total=${#FILES[@]}
passed=0
failed=0
failures=()

printf 'Running %s on %d file(s)...\n' "$MODE" "$total"

print_command() {
    local file="$1"
    if [[ "$MODE" == "check" ]]; then
        printf '+ %q typecheck %q && %q safety %q\n' "$COCO_BIN" "$file" "$COCO_BIN" "$file"
        return
    fi

    local cmd=("$COCO_BIN" "$MODE" "$file")
    if [[ "$MODE" == "run" && "$NO_CHECK" -eq 1 ]]; then
        cmd+=("--no-check")
    fi
    printf '+'
    printf ' %q' "${cmd[@]}"
    printf '\n'
}

run_file() {
    local file="$1"
    if [[ "$MODE" == "check" ]]; then
        "$COCO_BIN" typecheck "$file" && "$COCO_BIN" safety "$file"
        return
    fi

    local cmd=("$COCO_BIN" "$MODE" "$file")
    if [[ "$MODE" == "run" && "$NO_CHECK" -eq 1 ]]; then
        cmd+=("--no-check")
    fi
    "${cmd[@]}"
}

for file in "${FILES[@]}"; do
    if ((VERBOSE)); then
        print_command "$file"
    fi

    output_file="$(mktemp)"
    if run_file "$file" >"$output_file" 2>&1; then
        ((passed += 1))
        printf 'PASS %s\n' "$file"
        if ((VERBOSE)); then
            sed 's/^/  /' "$output_file"
        fi
    else
        status=$?
        ((failed += 1))
        failures+=("$file")
        printf 'FAIL %s (exit %d)\n' "$file" "$status"
        sed 's/^/  /' "$output_file"
    fi
    rm -f "$output_file"
done

printf '\nSummary: %d passed, %d failed, %d total\n' "$passed" "$failed" "$total"

if ((failed > 0)); then
    printf 'Failed files:\n' >&2
    printf '  %s\n' "${failures[@]}" >&2
    exit 1
fi
