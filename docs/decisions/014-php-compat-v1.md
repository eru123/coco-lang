# ADR-014: No PHP Compatibility in v1

**Status:** Accepted
**Date:** 2026-06-03

## Context

Should v1 include any PHP compatibility features (aliases, migration tools, syntax bridges)?

## Decision

None. v1 is pure Coco. PHP migration tools are post-v1 work.

## Rationale

- PHP compat would bloat v1 scope
- Normal Coco syntax must be clean and uncompromised
- Migration tools are better built once the language is stable
- PHP influence is in features (traits, named args, magic methods), not syntax compatibility

## Consequences

- PHP developers must learn Coco syntax (minimal gap due to JS-like surface)
- No `$variables`, `->`, or PHP array functions in v1
- Post-v1: `coco migrate-php` tool and compat module
- Post-v1: documentation for PHP-to-Coco migration patterns
