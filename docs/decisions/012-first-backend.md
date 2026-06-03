# ADR-012: Tree-Walking Interpreter First

**Status:** Accepted
**Date:** 2026-06-03

## Context

The first execution backend could be an interpreter, bytecode VM, or native compiler (Cranelift/LLVM).

## Decision

Tree-walking interpreter first. VM and native backends come in later phases.

## Rationale

- Fastest path to running Coco programs
- Language semantics can be iterated quickly
- No LLVM/Cranelift build complexity in early phases
- Bugs in language design are cheaper to fix in an interpreter
- Performance is not the Phase 3 goal — correctness is

## Consequences

- Early Coco programs will be slow (interpreted)
- Phase 7 adds bytecode VM for better performance
- Phase 11 adds native compilation (LLVM or Cranelift)
- Interpreter remains useful as reference implementation and debugging tool
