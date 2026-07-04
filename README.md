# Coco

**Status:** Early implementation — syntax, type checker, bytecode VM runtime, concurrency primitives, and stdlib/IO modules are functional.

**Current execution model:** Bytecode VM only. `coco run` type-checks and executes source via the VM path. `coco build` emits a `.cb` artifact via `coco_interpreter::serialize_chunk`; `coco build --disasm` prints bytecode disassembly. The old AOT/LLVM backend and `coco build --binary` are not present.

## What is Coco?

Coco is a memory-safe, gradually typed language for backend services, CLI tools, and automation. It targets JS/TS/PHP developers who want memory safety without Rust's learning curve.

## Quick start

```bash
cargo build
cargo run -- hello.co
cargo test
```

## Project structure

- `crates/coco_cli`: CLI binary
- `crates/coco_parser`: parser
- `crates/coco_typeck`: gradual type checker
- `crates/coco_interpreter`: bytecode compiler, stack VM, `.cb` serialization
- `crates/coco_formatter`: formatter
- `crates/coco_syntax`: AST definitions
- `tests/`: `.co` integration tests

## Build

```bash
cargo build
```

## Run

```bash
cargo run -- hello.co
```

## Test

```bash
cargo test
```

Single crate tests:

```bash
cargo test -p coco_interpreter
cargo test -p coco_typeck
```

## CLI reference

| Command | Notes |
|--------|-------|
| `coco lex FILE.co` | Tokenize |
| `coco parse FILE.co` | Print AST |
| `coco fmt FILE.co` | Format |
| `coco fmt -w FILE.co` | Format in-place |
| `coco check FILE.co` | Parse + diagnostics |
| `coco typecheck FILE.co` | Type checking |
| `coco safety FILE.co` | Safety analysis |
| `coco run FILE.co` | Type-check + VM execute |
| `coco run --no-check FILE.co` | VM execute without checks |
| `cargo run -- build FILE.co` | Serialize `.cb` |
| `cargo run -- build --disasm FILE.co` | Bytecode disassembly |

## Design goals

- Memory safety by default
- Optional gradual typing
- Safe concurrency primitives
- Practical bytecode VM performance
- Explicit future compatibility; no backward-compat hacks

## Roadmap

Near-term:

- Finalize safe error-runtime behavior on VM paths
- Complete project graph resolution for multi-file builds
- Stabilize stdlib/host integration
- Add APC advisory coverage where `apc-advisory` is enabled

Deferred:

- Runtime package management surface
- Self-hosting compiler
- JIT/WASM output
- AOT/native-backend research is explicit, tracked, and not currently enabled

## Contributing

See `CONTRIBUTING.md`.