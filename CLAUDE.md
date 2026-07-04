# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is Coco?

Coco is a memory-safe, gradually typed language for backend services, CLI tools, and automation. It targets JS/TS/PHP developers who want memory safety without Rust's learning curve.

**Current status:** Early implementation — syntax, parser, formatter, gradual type checker, safety analyzer, bytecode VM/gc runtime, and concurrency primitives are functional. The sole execution path is the bytecode VM.

**Core principles:**

- Practical memory safety
- Optional gradual typing
- Safe default concurrency model at runtime
- Reliability over low-level control

## Build & Development Commands

```bash
cargo build                         # Build all crates
cargo test                          # Run all tests
cargo test -p coco_lexer            # Test single crate
cargo clippy                        # Lint
cargo fmt --check                   # Check formatting
cargo run -- lex FILE.co            # Tokenize a .co file
cargo run -- parse FILE.co          # Parse and print AST
cargo run -- fmt FILE.co            # Format to stdout
cargo run -- fmt -w FILE.co         # Format in-place
cargo run -- check FILE.co          # Parse and report diagnostics
cargo run -- typecheck FILE.co      # Type-check a .co file
cargo run -- safety FILE.co         # Safety analysis only
cargo run -- run FILE.co            # Type-check + VM execute
cargo run -- run --no-check FILE.co # VM execute without checks
cargo run -- build FILE.co          # Serialize .cb bytecode artifact
cargo run -- build --disasm FILE.co # Bytecode disassembly
cargo run -- test                   # Run project test suite
```

Toolchain: Rust stable (see `rust-toolchain.toml`). Components: `rustfmt`, `clippy`.

## Compiler / Runtime Architecture

Pipeline, left to right:

```txt
coco_span -> coco_diagnostics -> coco_lexer -> coco_syntax -> coco_parser -> coco_formatter -> coco_cli
                                                                        -> coco_interpreter -------^
                                                                        -> coco_typeck ------------^
```

| Crate | Role |
|---|---|
| `coco_span` | `Span`, `Location`, `SourceFile`, `SourceMap` — byte-offset tracking |
| `coco_diagnostics` | `Diagnostic` struct + ariadne-based colored error reporting |
| `coco_lexer` | Tokenizer |
| `coco_syntax` | AST node definitions. Shared between parser and formatter |
| `coco_parser` | Recursive descent (declarations/statements) + Pratt parsing (expressions). Error recovery via sync points |
| `coco_formatter` | AST -> formatted source. 4-space indent, ~100-char width, idempotent |
| `coco_interpreter` | Bytecode compiler, stack VM, `.cb` serialization. No tree-walker |
| `coco_typeck` | Gradual type checker. `check(&Program)` validates annotated code and leaves unannotated code permissive |
| `coco_cli` | clap-based binary: `lex`, `parse`, `fmt`, `check`, `typecheck`, `safety`, `run`, `build`, `test` |
| `coco_safety` | Safety analysis |

Key types:
- `Token { kind, span, text }`
- `Program { items: Vec<Item>, span: Span }`
- `Item` enum: `FnDecl`, `ClassDecl`, `InterfaceDecl`, `TraitDecl`, `EnumDecl`, `ConstDecl`, `LetDecl`, `TypeAlias`, `Import`, `Export`, `ExprStmt`, `Stmt`

## Language Design Reference

- `docs/language-reference.md` - syntax overview
- `docs/grammar.ebnf` - formal grammar
- `docs/decisions/` - design decisions
- `docs/type-system.md` - gradual typing, primitives, unions
- `docs/concurrency.md` - async, parallel, channels
- `docs/safety-promise.md` - memory safety guarantees
- `docs/adaptive-numeric-tower.md` - numeric model, optional `apc-advisory`
- `docs/build-system.md` - project and CLI usage
- `docs/stdlib.md` -Builtin/stdlib module index
- `docs/vm-audit.md` - VM audit notes
- `examples/` - `.co` files showing syntax and features

## Key Language Syntax

- **Function keywords:** `function`, `fn`, `f`
- **Class `$`:** shorthand for `this` in class bodies only
- **Optional types:** gradual typing with annotation
- **Error handling:** `Result<T,E>` + `?`, plus exceptions
- **Concurrency:** `async` / `await`, `parallel`, `coro`, `chan`, `Atomic`, `select`
- **Pipe operators:** `|>`, `<|`, `$$`
- **Operators:** `&&`/`and`, `||`/`or`, `!`/`not`, `^`/`xor`, `??`, `?:`
- **stdlib modules:** see `docs/stdlib.md`

## Design Guidelines

1. Grammar is source of truth — `docs/grammar.ebnf`
2. Check ADRs before changing semantics — `docs/decisions/`
3. Safety invariant — new features must respect `docs/safety-promise.md`
4. Types remain optional — never force annotations

## Roadmap

Current: early implementation / behavioral stabilization.

Completed: lexer, parser, formatter, type checker, safety analyzer, bytecode VM/gc runtime, basic stdlib surface.

Near-term: concurrency safety refinements, async I/O completeness, stdlib/host integration, incremental compiler/codegen research.

Not planned: runtime libraries yet, hashbang support or self-hosted compiler.

## Non-Goals for v1

Browser/frontend, bare-metal, PHP syntax compatibility beyond ergonomics, manual memory management, JIT, WASM target, self-hosting, package registry.
