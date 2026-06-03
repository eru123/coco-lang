# ADR-009: Dual Error Model (Exceptions + Result)

**Status:** Accepted
**Date:** 2026-06-03

## Context

Error handling could use exceptions only, Result only, or both with a clear split.

## Decision

Both coexist with a clear semantic split:
- **Exceptions:** unexpected/fatal errors (bugs, OOM, assertion failures, invariant violations)
- **Result<T, E>:** expected failures (parsing, validation, I/O, database, network)

## Rationale

- Expected failures should be in the type signature (Result)
- Unexpected failures should not pollute every return type (exceptions)
- Stdlib uses both: `db.query()` → Result, `assert()` → throws
- Developers can choose based on error nature, not language limitation

## Consequences

- Functions that can fail expectedly should return Result
- `?` propagates Result errors
- `try/catch` handles exceptions
- A function can both return Result AND throw (throws = bugs only)
- Stdlib documentation clearly labels which functions use which model
