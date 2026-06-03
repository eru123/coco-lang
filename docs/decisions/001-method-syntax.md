# ADR-001: Class Method Syntax

**Status:** Accepted
**Date:** 2026-06-03

## Context

Classes need a method declaration syntax. Options: JS-style `method(): T {}`, explicit `fn method(): T {}`, or both.

## Decision

Both syntaxes are valid inside classes:

```coco
class User {
    getName(): string { return this.name; }
    fn getEmail(): string { return this.email; }
}
```

## Rationale

- JS-style feels natural for TypeScript developers
- `fn` prefix provides explicit clarity for developers who prefer it
- No ambiguity: both forms are unambiguous in class body context
- Formatter does not enforce one style over the other

## Consequences

- Parser must handle both forms in class bodies
- Linter may offer a consistency rule (optional, not default)
- Documentation examples may use either style
