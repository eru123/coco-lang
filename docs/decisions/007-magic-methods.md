# ADR-007: PHP-Style Magic Methods

**Status:** Accepted
**Date:** 2026-06-03

## Context

Classes need special behavior hooks (string conversion, property access, invocation). Options: JS Symbol-style protocols, PHP magic methods, or no magic.

## Decision

PHP-style magic methods (`__toString`, `__get`, `__set`, `__call`, `__invoke`, `__compare`) are first-class in normal Coco.

```coco
class Money {
    __toString(): string {
        return `$${this.cents / 100}`;
    }

    __compare(other: Money): int {
        return this.cents <=> other.cents;
    }
}
```

## Rationale

- Familiar to PHP developers (target audience)
- Simpler than JS Symbol ceremony
- Clear naming convention (double underscore = magic)
- Easy to grep for and audit

## Consequences

- Double-underscore prefix is reserved for magic methods
- Users cannot name regular methods with `__` prefix
- Compiler must recognize and validate magic method signatures
- Defined set of magic methods (not arbitrary)
