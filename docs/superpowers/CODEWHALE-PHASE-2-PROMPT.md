# CodeWhale Agent Prompt: Coco Phase 2 Implementation

## Mission

Implement the Coco language compiler Phase 2: **Lexer, Parser, and Formatter** in Rust.

You are building the foundational compilation pipeline that converts Coco source code (`.co` files) into Abstract Syntax Trees (ASTs) and formatted output. This phase does NOT include semantic analysis, type checking, or execution—only syntax processing.

---

## Context

**Project:** Coco programming language  
**Repository:** `/apps/coco-lang`  
**Phase:** 2 (Lexer, Parser, Formatter)  
**Language:** Rust (edition 2021)  
**Timeline:** 6-8 weeks  

**Key Documents:**
- Spec: `docs/superpowers/specs/2026-06-04-coco-phase-2-design.md`
- Plan: `docs/superpowers/plans/2026-06-04-phase-2-implementation.md`
- Grammar: `docs/grammar.ebnf`
- Language Reference: `docs/language-reference.md`

**Test Data:** 21 example `.co` files in `examples/` directory (01-hello.co through 21-pipe-operator.co)

---

## Architecture Overview

You will create a Rust workspace with 7 crates:

```
crates/
├── coco_span/         # Source location tracking (Span, Location, SourceMap)
├── coco_diagnostics/  # Error reporting with ariadne (beautiful colored errors)
├── coco_lexer/        # Tokenization (source → Token stream)
├── coco_syntax/       # AST node definitions (lossless, preserves comments/whitespace)
├── coco_parser/       # Parser (Token stream → AST via recursive descent + Pratt)
├── coco_formatter/    # Pretty-printer (AST → formatted source)
└── coco_cli/          # CLI tool (coco lex, parse, fmt, check commands)
```

### Data Flow

```
Source Code (.co)
    ↓ [Lexer]
Token Stream
    ↓ [Parser]
AST (Abstract Syntax Tree)
    ↓ [Formatter]
Formatted Source Code
```

---

## Implementation Phases

### Phase 1: Foundation (Week 1)
- **Workspace setup:** Cargo.toml, .gitignore, rust-toolchain.toml
- **coco_span:** Implement Span, Location, SourceFile, SourceMap
- **coco_diagnostics:** Integrate ariadne for beautiful errors

**Success:** Can emit colored error messages with source context

### Phase 2: Lexer (Week 2)
- **coco_lexer:** Tokenize Coco source code
- Keywords: `fn`, `class`, `const`, `let`, `if`, `for`, `async`, `await`, etc.
- Operators: `+`, `-`, `*`, `/`, `|>`, `<|`, `$$`, `?.`, `??`, `?:`, `<=>`, etc.
- Literals: integers (hex, binary, octal), floats, strings, chars, booleans, null
- Template literals: `` `Hello ${name}` ``
- Unicode identifiers (use `unicode-xid` crate)
- Comments: `//` line comments, `/* */` block comments

**Success:** All 21 examples tokenize without errors

### Phase 3: AST Definitions (Week 3)
- **coco_syntax:** Define all AST node types
- Program, Item (FnDecl, ClassDecl, etc.)
- Stmt (If, For, While, Match, Parallel, etc.)
- Expr (Binary, Unary, Call, Member, Lambda, etc.)
- Type (Primitive, Named, Union, List, Map, Result, etc.)
- **Lossless tokens:** Preserve whitespace and comments (trivia)

**Success:** AST types cover entire grammar from `docs/grammar.ebnf`

### Phase 4-5: Parser (Week 4-5)
- **coco_parser:** Parse tokens into AST
- **Recursive descent** for declarations and statements
- **Pratt parsing** for expressions (operator precedence)
- **Operator precedence table:**
  1. Assignment (`=`, `+=`, `-=`, ...)
  2. Elvis (`?:`)
  3. Null coalesce (`??`)
  4. Logical OR (`||`)
  5. Logical AND (`&&`)
  6. Bitwise OR (`|`)
  7. Bitwise XOR (`^`)
  8. Bitwise AND (`&`)
  9. Equality (`==`, `!=`)
  10. Comparison (`<`, `>`, `<=`, `>=`, `<=>`)
  11. Shift (`<<`, `>>`)
  12. Additive (`+`, `-`)
  13. Multiplicative (`*`, `/`, `%`)
  14. Exponentiation (`**`, right-associative)
  15. Pipe (`|>`, `<|`)
  16. Unary (`!`, `~`, `-`, `typeof`, `new`, `await`, `lazy`)
  17. Postfix (`.`, `?.`, `[]`, `()`, `!`, `?`, `++`, `--`)

- **Pipe operator validation:**
  - `|>` (forward pipe): data flows left-to-right, `$$` allowed in right operand
  - `<|` (backward pipe): data flows right-to-left, `$$` allowed in left operand
  - `$$` scope tracking: only valid inside pipe expressions, compilation error otherwise
  - Track pipe direction to validate `$$` placement

- **Error recovery:** On syntax error, skip to sync point (`;`, `}`, keyword), emit diagnostic, continue parsing

**Success:** All 21 examples parse into valid ASTs

### Phase 6-7: Formatter (Week 6-7)
- **coco_formatter:** Pretty-print AST back to source
- **Style:** 4-space indent, 100-char max line, trailing commas on multiline
- **Idempotence:** `coco fmt | coco fmt` produces identical output
- **Comment preservation:** Attach comments to AST nodes via trivia
- Line-breaking algorithm (start with greedy, optimize later if needed)

**Success:** All 21 examples format correctly and idempotently

### Phase 8: CLI and Integration (Week 8)
- **coco_cli:** Command-line interface using `clap`
  - `coco lex <file>` — Tokenize and print tokens
  - `coco parse <file>` — Parse and print AST (debug format)
  - `coco fmt <file>` — Format and print to stdout
  - `coco fmt -w <file>` — Format and write in-place
  - `coco check <file>` — Parse and report diagnostics
- Integration tests on all 21 examples
- Performance benchmarks (lex < 1ms, parse < 5ms, fmt < 10ms per 1000 lines)

**Success:** All CLI commands work, tests pass, benchmarks met

---

## Key Requirements

### 1. Lossless AST
The AST must preserve ALL source information:
- Whitespace between tokens (leading/trailing trivia)
- Line comments (`// ...`)
- Block comments (`/* ... */`)
- Exact token spans (byte offsets)

**Why:** The formatter needs this to preserve comments and reproduce source layout.

### 2. Pipe Operator Handling
Special attention to pipe operators (`|>`, `<|`, `$$`):
- `|>` pipes data forward: `value |> transform($$, arg)` → `transform(value, arg)`
- `<|` pipes data backward: `transform(arg, $$) <| value` → `transform(arg, value)`
- `$$` is ONLY valid inside pipe expressions
- Nested pipes: track direction, validate `$$` scope correctly
- See ADR-016 (`docs/decisions/016-pipe-operator.md`) for full spec

### 3. Error Recovery
Parser must continue after errors:
- Emit diagnostic with helpful message (use ariadne)
- Skip to synchronization point (`;`, `}`, `fn`, `class`, etc.)
- Produce partial AST where possible (mark errors)
- Allow multiple errors in one pass

### 4. Beautiful Diagnostics
Use `ariadne` for error messages:
```
error: expected ';' after variable declaration
  ┌─ examples/02-variables.co:5:20
  │
5 │     let counter = 0
  │                    ^ expected ';' here
  │
  = help: add a semicolon to complete the statement
```

### 5. Unicode Support
- Identifiers: Use `unicode-xid` crate for XID_Start and XID_Continue
- String literals: Full UTF-8 support, escape sequences (`\n`, `\t`, `\x##`, `\u{...}`)
- Source files: UTF-8 encoded

### 6. Testing
- **Unit tests:** Each crate has `tests/` directory
- **Integration tests:** All 21 examples must pass
- **Golden output tests:** Compare formatted output to expected
- **Fuzzing:** Lexer and parser should not panic on invalid input
- **Coverage:** >90% target

---

## Implementation Guidelines

### DO:
- ✅ Follow the task-by-task plan in `docs/superpowers/plans/2026-06-04-phase-2-implementation.md`
- ✅ Write tests FIRST for each component (TDD approach recommended)
- ✅ Commit after each completed task (atomic commits)
- ✅ Run `cargo test` frequently
- ✅ Run `cargo clippy` to catch common mistakes
- ✅ Run `cargo fmt` to maintain consistent style
- ✅ Reference grammar in `docs/grammar.ebnf` for correctness
- ✅ Test on example files in `examples/` directory
- ✅ Document public APIs with rustdoc comments (`///`)

### DON'T:
- ❌ Skip tests — tests are NOT optional
- ❌ Ignore clippy warnings
- ❌ Implement semantic analysis (type checking, name resolution) — that's Phase 4
- ❌ Implement interpreter or execution — that's Phase 3
- ❌ Implement code generation — that's Phase 7+
- ❌ Add dependencies not listed in the spec (ask first)
- ❌ Change the grammar without updating `docs/grammar.ebnf`

---

## Success Criteria

Phase 2 is complete when ALL of the following are true:

1. ✅ All 21 example programs parse without errors
2. ✅ `coco fmt` is idempotent (format twice = same output)
3. ✅ Error messages are helpful with source context (ariadne)
4. ✅ Performance targets met:
   - Lex 1000 lines: < 1ms
   - Parse 1000 lines: < 5ms
   - Format 1000 lines: < 10ms
5. ✅ Test coverage > 90%
6. ✅ All clippy warnings resolved
7. ✅ Documentation complete (rustdoc for public APIs)
8. ✅ CI pipeline passes (build, test, clippy, fmt)

---

## Quick Start Commands

```bash
# Setup workspace
cd /apps/coco-lang

# Create workspace structure
mkdir -p crates/{coco_span,coco_diagnostics,coco_lexer,coco_syntax,coco_parser,coco_formatter,coco_cli}

# Run tests
cargo test

# Run clippy
cargo clippy --all-targets --all-features

# Format code
cargo fmt --all

# Build CLI
cargo build --release --bin coco

# Test CLI on examples
./target/release/coco lex examples/01-hello.co
./target/release/coco parse examples/01-hello.co
./target/release/coco fmt examples/01-hello.co
./target/release/coco check examples/01-hello.co
```

---

## Resources

### Documentation
- **Phase 2 Spec:** `docs/superpowers/specs/2026-06-04-coco-phase-2-design.md`
- **Implementation Plan:** `docs/superpowers/plans/2026-06-04-phase-2-implementation.md`
- **Grammar:** `docs/grammar.ebnf` (formal EBNF)
- **Language Reference:** `docs/language-reference.md` (human-readable guide)
- **Pipe Operator ADR:** `docs/decisions/016-pipe-operator.md`

### External Crates
- **ariadne:** Beautiful error reporting (https://docs.rs/ariadne)
- **unicode-xid:** Unicode identifier validation (https://docs.rs/unicode-xid)
- **clap:** CLI argument parsing (https://docs.rs/clap)

### Example Files
All 21 `.co` files in `examples/` directory are your test suite:
- `01-hello.co` through `20-queue-worker.co` (Phase 0-1 examples)
- `21-pipe-operator.co` (pipe operator demonstrations)

---

## Communication Protocol

### Questions
If you encounter ambiguity or need clarification:
1. Check the spec (`docs/superpowers/specs/2026-06-04-coco-phase-2-design.md`)
2. Check the grammar (`docs/grammar.ebnf`)
3. Check ADRs (`docs/decisions/*.md`)
4. If still unclear, ask the user before proceeding

### Progress Updates
After each major milestone, report:
- ✅ What was completed
- 📊 Test results (pass/fail count)
- 🐛 Any issues encountered
- ⏭️ Next steps

### Blockers
If blocked:
- State the blocker clearly
- Provide options for resolution
- Ask for guidance

---

## Starting Point

Begin with **Task 1: Workspace Setup** from the implementation plan:
1. Create `Cargo.toml` (workspace manifest)
2. Create `.gitignore` and `rust-toolchain.toml`
3. Create `crates/` directory structure
4. Commit: `feat(phase-2): setup Rust workspace structure`

Then proceed sequentially through the plan. Follow the checkboxes `- [ ]` to track progress.

---

## Final Notes

- **Phase 2 is syntax only** — no semantics, no execution, no type checking
- **Quality over speed** — correct, well-tested code is more valuable than fast, buggy code
- **Tests are mandatory** — every component must have tests
- **The 21 examples are your contract** — if they all parse and format correctly, you're done

Good luck! 🚀
