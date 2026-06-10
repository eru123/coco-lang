# Contributing to Coco

## Commit Format

Use Conventional Commits:
```
type(scope): description
```

Types: `feat`, `fix`, `refactor`, `test`, `docs`, `ci`

Examples:
```
feat(parser): parse do-while statement
fix(vm): implement structural equality for lists
refactor(arch): remove tree-walking interpreter
test(vm): add bytecode compiler snapshot tests
docs: add BUILDING.md
```

Author: `eru123 <jericho@skiddph.com>`. Max 150 chars. No co-authors.

## Adding a Language Feature

A new language feature typically touches these crates, in order:

1. **`coco_lexer`** — Add any new token kinds needed.
2. **`coco_syntax`** — Add AST node types (`Stmt`, `Expr`, etc.) and span methods.
3. **`coco_parser`** — Parse the new syntax into AST nodes.
4. **`coco_typeck`** — Add type checking rules in `check_expr.rs` or `check_stmt.rs`. Add inference in `infer.rs`.
5. **`coco_safety`** — Add safety analysis rules (capture, def-assign, iterator, unsafe).
6. **`coco_interpreter/compiler.rs`** — Add bytecode compilation for the new AST node.
7. **`coco_interpreter/vm.rs`** — Add VM runtime support if new opcodes are needed.
8. **`coco_formatter`** — Add formatting logic.
9. **`coco_cli`** — Update CLI if new subcommands or flags are needed.

## Test Requirements

- **Parser tests**: Add to `crates/coco_parser/tests/`.
- **Type checker tests**: Add to `crates/coco_typeck/tests/typeck_test.rs`.
- **VM tests**: Add to `crates/coco_interpreter/src/vm.rs` (test module at bottom) or `crates/coco_interpreter/tests/eval_test.rs`.
- **Integration tests**: Add `.co` files to `tests/` directory and run with `scripts/run-co-tests.sh`.

Run `cargo test` before submitting changes.

## Code Style

- Follow existing patterns in the codebase.
- Rust edition 2021.
- Use `cargo fmt` before committing.
- Public APIs should be documented with `///` comments.

## Current Priorities

See [TASKS.md](TASKS.md) for the current task list.
