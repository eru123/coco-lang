# ADR-008: Traits Can Hold State

**Status:** Accepted
**Date:** 2026-06-03

## Context

Should traits be limited to method definitions, or can they include properties with defaults?

## Decision

Traits can hold both methods and properties with defaults.

```coco
trait Timestamps {
    createdAt: DateTime|null = null;
    updatedAt: DateTime|null = null;

    touch(): void {
        this.updatedAt = DateTime.now();
    }
}

class User {
    use Timestamps;
}
```

## Rationale

- PHP traits have state — familiar to target audience
- Practical for common patterns (timestamps, soft-delete, sluggable)
- Reduces boilerplate for cross-cutting concerns
- Properties must have defaults (no uninitialized state from traits)

## Consequences

- Diamond problem for properties: compile error if two traits define same property name
- Trait properties must always have default values
- Classes can override trait properties
- Memory layout must account for trait-provided fields
