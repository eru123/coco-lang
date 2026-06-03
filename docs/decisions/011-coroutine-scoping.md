# ADR-011: Both Scoped and Unscoped Coroutines

**Status:** Accepted
**Date:** 2026-06-03

## Context

Should all coroutines be scoped (structured concurrency only), or can developers spawn unscoped fire-and-forget tasks?

## Decision

Both allowed. Scoped is preferred and recommended. Unscoped `coro` gets extra compiler scrutiny:
- Compiler warning for unscoped usage
- Lifetime analysis on captured state
- Leak detection in debug builds

```coco
// Structured (preferred):
await parallel {
    run { doWork(); }
}

// Unscoped (allowed, with scrutiny):
coro { backgroundCleanup(); }
```

## Rationale

- Strict scoping prevents resource leaks
- Some patterns genuinely need fire-and-forget (background cleanup, metrics, logging)
- Warnings + debug detection balance flexibility with safety
- Developers making conscious choice to use unscoped get informed of risks

## Consequences

- Unscoped `coro` produces a compiler warning (suppressible with annotation)
- Debug runtime tracks unscoped coroutine lifetimes
- Unscoped coroutines that outlive their spawning context are reported
- Documentation strongly recommends structured patterns
