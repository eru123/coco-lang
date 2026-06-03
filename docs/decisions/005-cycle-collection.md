# ADR-005: Cycle Collection in v1

**Status:** Accepted
**Date:** 2026-06-03

## Context

Reference-counted memory management leaks circular references. Options: ship cycle collector in v1, debug-only detection, or defer entirely.

## Decision

Production cycle collector ships in v1. Tree/graph structures with parent-child references do not silently leak.

## Rationale

- Tree structures (DOM-like, AST, parent pointers) are common in backend code
- Silent leaks in production are unacceptable for long-running services
- "Trust the runtime" principle means developers shouldn't worry about cycles
- Cost is acceptable for backend workloads (not targeting bare-metal)

## Consequences

- Runtime includes cycle detection overhead (low-priority background sweep)
- Memory management is not purely reference-counting
- Some latency jitter from cycle collection (acceptable for target workloads)
- Debug builds can report cycle frequency for optimization
