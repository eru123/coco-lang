# ADR-002: String Concatenation

**Status:** Accepted
**Date:** 2026-06-03

## Context

How should the `+` operator behave with mixed types? Options: strict (error on non-string), JS-style (implicit coercion), or type-descriptive stringification.

## Decision

`+` concatenates when at least one operand is a string. Non-string operands are stringified:
- Numeric primitives: value representation (`42` → `"42"`)
- `bool`: `[bool]`
- `null`: `[null]`
- Objects: `[ClassName]`

No compile error. No implicit arithmetic coercion of strings.

## Rationale

- JS-like ergonomics for string building
- Type-descriptive output prevents confusion (unlike JS where `"5" + 3 = "53"`)
- Booleans and objects show type tags rather than values to avoid PHP-style truthiness surprises
- Template literals remain the preferred approach for complex formatting

## Consequences

- `+` is overloaded: arithmetic when both numeric, concat when one is string
- Non-string + non-string remains arithmetic (or compile error if non-numeric)
- Objects can override stringification via `__toString` magic method
