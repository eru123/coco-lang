# ADR-004: Strict Compile-Time Race Detection

**Status:** Accepted
**Date:** 2026-06-03

## Context

Parallel tasks may capture mutable local variables, creating data races. Options: reject all, reject only parallel (allow sequential async), or detect at runtime.

## Decision

Strict policy: all cross-task mutable captures are rejected at compile time. Developers must use atomics, channels, or synchronized blocks.

## Rationale

- Parallelism is complex; silent races are catastrophic
- Compile-time rejection prevents entire class of production bugs
- Explicit synchronization makes data flow visible
- Immutable sharing remains free and easy

## Consequences

- Some patterns require restructuring (channels instead of shared counters)
- Atomics needed for simple counters in parallel contexts
- Sequential async can still mutate locals (no parallel boundary crossed)
- Error messages must clearly suggest alternatives (atomics, channels, parallel return values)
