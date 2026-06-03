# ADR-015: Full Runtime Magic

**Status:** Accepted
**Date:** 2026-06-03

## Context

How much should the runtime decide automatically about memory management strategy? Options: predictable with opt-in, full magic, or minimal with manual opt-in.

## Decision

Full magic. The runtime chooses optimal memory strategy automatically. No developer control knobs for GC tuning in normal code.

## Rationale

- "Trust the runtime" is a core principle
- Backend developers should not tune GC parameters for CRUD APIs
- Runtime can make better decisions than most developers (generational heuristics, allocation patterns)
- Simpler developer experience — just write code
- Matches the "automatic safety" promise

## Consequences

- Latency is less predictable than manual control (acceptable for target workloads)
- No `gc.disable()` or tuning knobs in safe code
- Runtime may use generational GC, concurrent marking, or hybrid strategies
- Profiling tools can observe runtime behavior (read-only)
- Systems-mode code may eventually get lower-level access (post-v1)
- If a workload needs predictable latency, Coco may not be the right choice (and that's fine)
