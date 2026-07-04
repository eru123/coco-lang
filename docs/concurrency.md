# Coco Concurrency Model

This document defines runtime concurrency behavior in the current interpreter.

## Implementation Status

| Feature | Notes |
|---|---|
| `async fn` / `await` | Implemented |
| `lazy async fn()` | Compiles to cold async lambda |
| `parallel { run ... }` | Single-threaded VM path; structured parallelism semantics where possible |
| `coro { ... }` | Fire-and-forget task |
| `select { case ... }` | Channel/event multiplexing in VM |
| `chan<T>` | Managed channel primitive |
| `Atomic<T>` | Managed atomic primitive |
| `synchronized { }` | Scoped mutual exclusion |
| Real multi-threading | Not implemented |
| Async I/O event loop | Partially implemented; some I/O primitives report readiness without guaranteed task suspension yet |

## Compile-time race prevention

Mutable capture of local variables across `parallel` or `coro` boundaries is rejected by the safety layer.

Preferred patterns:
- atomics for shared counters
- channels for producer/consumer boundaries
- `parallel { run ... }` for scoped result collection

## Runtime obligation

Runtime checks reject unsafe operations distinctly so that invalid state surfaces as an error rather than silent corruption.

## Cancellation

`Context` carries cancellation, deadline, and per-request values. Derived contexts may be created with timeouts or cancellation functions.
