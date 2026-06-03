# Coco Safety Promise

> This document defines what Coco guarantees about memory safety and what mechanisms enforce those guarantees.

---

## The Promise

In safe Coco code (any code not inside an `unsafe` block), the following are impossible at runtime:

| Guarantee | Description |
|-----------|-------------|
| No use-after-free | Objects cannot be accessed after their memory is reclaimed |
| No dangling references | References always point to valid, live objects |
| No null dereference | Null access requires explicit `!` assertion or narrowing |
| No buffer overflow | Array/list access is bounds-checked |
| No uninitialized reads | All variables must be assigned before use |
| No data races | Cross-task mutable capture is a compile error |
| No raw pointer access | Pointers do not exist in safe code |
| No iterator invalidation | Mutating a collection while iterating is prevented |
| No unsynchronized mutation | Shared mutable state requires explicit synchronization |
| No undefined behavior | Safe code has fully defined semantics |

---

## How Coco Enforces Safety

### Compiler Analysis (Static)

The compiler performs invisible safety analysis:

- **Lifetime analysis:** Tracks object lifetimes without developer annotation. Rejects code where an object could be used after it becomes invalid.
- **Escape analysis:** Determines whether values escape their defining scope. Informs allocation strategy (stack vs heap).
- **Null flow analysis:** Tracks nullable types through control flow. Narrows `T|null` to `T` after checks.
- **Capture analysis:** In `parallel` and `coro` blocks, detects mutable captures of local variables. Rejects unsafe sharing.
- **Bounds analysis:** Where possible, proves array access is within bounds at compile time.

### Runtime Mechanisms (Dynamic)

The runtime provides safety where static analysis cannot:

- **Automatic memory management:** Objects are reclaimed automatically when no longer reachable. Strategy chosen by runtime (generational GC, concurrent marking, ref-counting — implementation detail).
- **Cycle collection:** Circular reference graphs are detected and collected. No silent leaks from parent-child cycles.
- **Copy-on-write:** Collections and strings use CoW semantics. Assignment is cheap; mutation creates a private copy when shared.
- **Bounds checking:** Array/list access is bounds-checked at runtime where compile-time proof is impossible.
- **Debug diagnostics:** Debug builds include leak detection, race detection, and lifetime violation reporting.

### Unsafe Boundary

Code inside `unsafe { }` may violate safety guarantees. Rules:

- `unsafe` blocks are syntactically visible
- Packages using `unsafe` must declare it in their manifest
- Application safety mode blocks unsafe dependencies by default
- Safe code cannot call unsafe internals without crossing an explicit boundary
- Tooling reports all unsafe usage in a project

---

## Safety Modes

Configured in `coco.toml`:

### `application` (default)

No unsafe code in application source. Unsafe dependencies blocked unless explicitly allowlisted.

```toml
[safety]
mode = "application"
allow_unsafe_dependencies = false
```

### `library`

Allows controlled unsafe internals with public safe APIs. Useful for performance-critical libraries.

```toml
[safety]
mode = "library"
```

### `systems`

Allows unsafe blocks and low-level APIs. For FFI bindings, custom allocators, and platform-specific code.

```toml
[safety]
mode = "systems"
```

---

## What This Does NOT Promise

- **Deterministic destruction timing.** Objects are reclaimed "promptly" but exact timing is runtime-dependent.
- **Zero-cost safety.** Bounds checks, cycle collection, and CoW copies have measurable cost. Coco prioritizes safety over maximum throughput.
- **Logic bug prevention.** Coco prevents memory bugs, not business logic bugs. A safe program can still compute wrong answers.
- **Deadlock prevention.** Coco prevents data races but does not prevent deadlocks from synchronized blocks or channel usage.
