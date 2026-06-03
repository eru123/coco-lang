# ADR-003: Result Type is Language-Level

**Status:** Accepted
**Date:** 2026-06-03

## Context

`Result<T, E>` could be a stdlib type, a compiler-recognized stdlib type, or a language builtin.

## Decision

`Result<T, E>` is a language-level builtin. No import required. The `?` operator is compiler-intrinsic and only works with `Result`.

## Rationale

- Always available without imports
- Compiler can optimize Result handling (no indirection)
- `?` operator has guaranteed semantics
- Consistent with Result being a core error-handling mechanism alongside exceptions

## Consequences

- `Result`, `Ok`, `Err` are reserved identifiers
- `?` operator is not generalizable to other types
- Cannot be redefined or shadowed by user code
