# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is Coco?

Coco is a compiled, memory-safe programming language for backend services, CLI tools, and automation. Targets JS/TS/PHP developers who want memory safety without Rust's learning curve.

**Current status:** Phase 4 - gradual type checker implemented. Lexer, parser, formatter, interpreter, and type checker all functional. Language design docs and grammar are the source of truth for what the compiler should accept.

**Core principles:**
- JavaScript-like syntax (not PHP `$variables`)
- Automatic memory safety (no manual ownership/borrowing)
- Safe multi-core concurrency (compile-time race prevention)
- Gradual typing (types optional and additive)
- Trust the runtime (automatic memory management)

## Build & Development Commands

```bash
cargo build                         # Build all crates
cargo test                          # Run all tests (currently 103 pass)
cargo test -p coco_lexer            # Test single crate
cargo clippy                        # Lint
cargo fmt --check                   # Check formatting
cargo run -- lex FILE.co            # Tokenize a .co file
cargo run -- parse FILE.co          # Parse and print AST summary
cargo run -- fmt FILE.co            # Format to stdout
cargo run -- fmt -w FILE.co         # Format in-place
cargo run -- check FILE.co          # Parse and report diagnostics
cargo run -- typecheck FILE.co      # Type-check a .co file
cargo run -- run FILE.co            # Type-check and execute a .co file
cargo run -- run --no-check FILE.co # Execute a .co file without type checking
```

Toolchain: Rust stable (see `rust-toolchain.toml`). Components: `rustfmt`, `clippy`.

Test suite: 103 tests across all crates (`cargo test`).

## Compiler Architecture (Rust workspace)

Pipeline flows left-to-right, each crate depends only on those to its left:

```txt
coco_span -> coco_diagnostics -> coco_lexer -> coco_syntax -> coco_parser -> coco_formatter -> coco_cli
                                                                     -> coco_interpreter -------^
                                                                     -> coco_typeck ------------^
```

| Crate | Role |
|-------|------|
| `coco_span` | `Span`, `Location`, `SourceFile`, `SourceMap` - byte-offset tracking |
| `coco_diagnostics` | `Diagnostic` struct + ariadne-based colored error reporting |
| `coco_lexer` | Tokenizer. `Lexer::new(&str)` -> call `next_token()` in loop until `Eof` |
| `coco_syntax` | AST node definitions (`ast.rs`). Shared between parser and formatter |
| `coco_parser` | Recursive descent (declarations/statements) + Pratt parsing (expressions). Error recovery via sync points |
| `coco_formatter` | AST -> formatted source. 4-space indent, ~100-char width, idempotent |
| `coco_interpreter` | Tree-walking interpreter. `Interpreter::new()` + `run_main(source)` |
| `coco_typeck` | Gradual type checker. `check(&Program)` validates annotated code and leaves unannotated code permissive |
| `coco_cli` | clap-based binary: `lex`, `parse`, `fmt`, `check`, `typecheck`, `run` subcommands |

Key types:
- `Token { kind: TokenKind, span: Span, text: String }` - lexer output
- `Program { items: Vec<Item>, span: Span }` - parser output (top-level AST)
- `Item` enum: `FnDecl`, `ClassDecl`, `InterfaceDecl`, `TraitDecl`, `EnumDecl`, `ConstDecl`, `LetDecl`, `TypeAlias`, `Import`, `Export`, `ExprStmt`, `Stmt`

## Language Design Reference

- `docs/language-reference.md` - complete syntax overview (read first)
- `docs/grammar.ebnf` - formal grammar the parser implements
- `docs/decisions/` - 16 ADRs (001-016) with ratified design decisions
- `docs/type-system.md` - gradual typing, primitives, generics, unions
- `docs/concurrency.md` - async, parallel, channels, race prevention
- `docs/safety-promise.md` - memory safety guarantees
- `examples/` - 20 `.co` files showing syntax

## Key Language Syntax Rules

**Function declaration:** `function`, `fn`, or `f` keywords. Arrow syntax for anonymous only.

**Class methods:** direct name, `function`, `fn`, `f` - NO arrow functions in class bodies.

**Instance access:** `this.x` or `$.x` (equivalent). `$` is shorthand for `this`, NOT a variable sigil.

**Type system:** gradual (optional). Non-null by default. `Result<T, E>` + `?` propagation. Primitives: `int`, `uint`, `float`, `bool`, `string`, `char`, `byte`, `null`, `void`, `never`, `mixed`.

**Error handling:** Result type for expected failures, exceptions for bugs/invariant violations.

**Pipe operators:** `|>` (pipe-right), `<|` (pipe-left), `$$` (pipe placeholder).

**Logical operators:** `&&`/`and`, `||`/`or`, `!`/`not`, `^`/`xor` - word forms are case-insensitive.

## Design Guidelines

When modifying the compiler or adding language features:
1. Grammar is source of truth - `docs/grammar.ebnf` defines what the parser accepts
2. Check ADRs before changing semantics - `docs/decisions/` has ratified decisions
3. Safety invariant - any new feature must respect `docs/safety-promise.md`
4. Types must remain optional - never force annotations

## Roadmap

Current: **Phase 4** (gradual type checker - implemented)

Completed: lexer, parser, formatter (Phase 2), interpreter (Phase 3), type checker (Phase 4)

Next phases: memory safety analyzer (5), runtime (6), bytecode VM (7), concurrency safety (8), async runtime (9), stdlib (10), native AOT compiler (11).

## Non-Goals for v1

Browser/frontend, bare-metal, PHP syntax compat, manual memory management, JIT, WASM target, self-hosting, package registry.
