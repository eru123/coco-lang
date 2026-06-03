# ADR-010: Eager Async with Lazy Opt-In

**Status:** Accepted
**Date:** 2026-06-03

## Context

When an async function is called, should it start executing immediately (eager, JS-style) or return a cold task (lazy, Rust-style)?

## Decision

Eager by default. `lazy` keyword available for deferred execution.

```coco
const p = fetchUser(1);          // starts immediately
const task = lazy fetchUser(2);  // cold until awaited
const user = await task;         // NOW runs
```

## Rationale

- JS developers expect eager execution (familiar mental model)
- Eager is more intuitive for most backend patterns
- `lazy` provides control when deferred execution is intentional
- Structured parallelism (`parallel { run ... }`) handles the "start many at once" pattern

## Consequences

- Calling async fn without await starts work (side effects begin)
- `lazy` is a keyword that wraps the call in a deferred task
- Lazy tasks are cold — no execution until awaited or explicitly started
- Resource management requires awareness that eager calls begin immediately
