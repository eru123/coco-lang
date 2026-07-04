# Adaptive Numeric Tower

This document describes Coco’s numeric model and the optional APC advisory behavior. The implementation focuses on VM correctness and predictable runtime behavior.

## Goals

- default runtime behavior should remain stable and narrow
- overflow behavior should escalate to a wider representation instead of silent truncation in advisory mode
- type-specialized arithmetic should remain a compiler optimization surface, not a separate code generation path

## Current implementation

The sole execution path is the bytecode VM. Arithmetic specialization is controlled at compile time, then executed in VM opcodes. There is no LLVM/AOT backend.

Default mode:
- integer literals use 64-bit integers
- overflow path escalates to a bignum-style representation when needed
- binary/bitwise/shift operators behave consistently with numeric escalation rules

Advisory mode (`apc-advisory` feature):
- runtime can advise or enforce overflow escalation policies
- this remains optional and feature-gated
- default builds have no advisory behavior

## Type checker interaction

The type checker records inferred types keyed by AST span. The bytecode compiler uses these spans to choose narrower arithmetic opcodes when both operands are statically known. Unresolved operands fall back to runtime dispatch.

## Future considerations

- exact rational/decimal exact tier is deferred
- improved bignum performance is deferred
