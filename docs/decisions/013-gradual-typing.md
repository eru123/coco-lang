# ADR-013: Gradual Typing

**Status:** Accepted
**Date:** 2026-06-03

## Context

Should Coco require full type annotations (strict), allow fully untyped code (gradual), or use inference-only?

## Decision

Gradual typing. Types are optional and can be added progressively. Typed and untyped code can coexist in the same file.

```coco
fn add(a, b) { return a + b; }              // untyped
fn addTyped(a: int, b: int): int { return a + b; }  // typed
```

## Rationale

- Lowers barrier to entry for scripting use cases
- Matches PHP/JS developer expectations (types are additive, not required)
- Typed code gets stronger guarantees (compile-time checking)
- Untyped code still gets runtime safety (memory safety, bounds checks)
- Progressive typing encourages adoption without forcing it

## Consequences

- Type checker must handle `mixed` (unknown type) boundaries
- Untyped → typed boundaries require runtime checks
- Memory safety still holds for untyped code (runtime enforcement)
- Some compiler optimizations only available for fully typed code
- Linter can recommend adding types (optional strictness level)
