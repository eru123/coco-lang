# Coco Phase 0-1: Charter, Decisions & Grammar — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Produce all Phase 0-1 documentation artifacts — charter, safety promise, decision records, grammar, language reference, and 20 example programs.

**Architecture:** Pure documentation. No code compilation or runtime. Each task produces markdown or EBNF files that collectively define the Coco language specification. Tasks are ordered so later documents can reference earlier ones.

**Tech Stack:** Markdown, EBNF notation, Coco syntax examples (`.co` files)

---

## File Structure

```
docs/
├── charter.md                  — Language identity, goals, non-goals, principles
├── safety-promise.md           — Memory safety guarantees and mechanisms
├── concurrency.md              — Concurrency model, structured parallelism, safety rules
├── type-system.md              — Gradual typing, primitives, compounds, inference rules
├── grammar.ebnf                — Formal EBNF grammar for Phase 1 syntax
├── language-reference.md       — Human-readable syntax guide (~30 min read)
└── decisions/
    ├── 001-method-syntax.md
    ├── 002-string-concatenation.md
    ├── 003-result-type.md
    ├── 004-race-detection.md
    ├── 005-cycle-collection.md
    ├── 006-unsafe-dependencies.md
    ├── 007-magic-methods.md
    ├── 008-trait-state.md
    ├── 009-error-model.md
    ├── 010-async-execution.md
    ├── 011-coroutine-scoping.md
    ├── 012-first-backend.md
    ├── 013-gradual-typing.md
    ├── 014-php-compat-v1.md
    └── 015-runtime-magic.md
examples/
├── 01-hello.co
├── 02-variables.co
├── 03-functions.co
├── 04-classes.co
├── 05-collections.co
├── 06-error-handling.co
├── 07-null-safety.co
├── 08-async-basic.co
├── 09-parallel.co
├── 10-channels.co
├── 11-http-server.co
├── 12-traits.co
├── 13-generics.co
├── 14-magic-methods.co
├── 15-match-expressions.co
├── 16-enums.co
├── 17-iterators.co
├── 18-modules.co
├── 19-cli-tool.co
└── 20-queue-worker.co
```

---

## Task 1: Charter Document

**Files:**
- Create: `docs/charter.md`

- [ ] **Step 1: Write docs/charter.md**

```markdown
# Coco Language Charter

> Version: 1.0
> Date: 2026-06-03
> Status: Ratified

---

## Identity

Coco is a compiled, memory-safe programming language for backend applications, CLI tools, automation scripts, worker services, and high-performance APIs.

Coco feels like JavaScript, provides the practical backend features PHP developers rely on, and delivers memory-safe applications automatically without forcing developers to write ownership or borrow syntax.

---

## Target Users

Backend developers from JavaScript, TypeScript, and PHP backgrounds who want:
- Memory safety without Rust's learning curve
- Deployment simplicity of static binaries
- True multi-core concurrency without manual thread-safety types
- Familiar syntax they can read on day one

---

## Target Workloads

- HTTP APIs and microservices
- CLI applications
- Queue workers and job processors
- Automation and scripting
- Long-running services and daemons
- Internal tools and control-plane software

---

## Core Design Principles

1. **Simple surface, strict underneath.** Developer writes clean code. Compiler enforces safety invisibly.
2. **Automatic safety.** No manual ownership, borrowing, or lifetime annotations in application code.
3. **Gradual typing.** Types are optional and additive. Untyped code compiles. Typed code gets stronger guarantees.
4. **Trust the runtime.** Memory strategy is fully automatic. No developer knobs for GC tuning in normal code.
5. **Strict parallelism.** Cross-task mutable capture is always a compile error. No exceptions.
6. **Static deployment.** Final output is a single native binary (post-v1).

---

## Non-Goals for v1

These are explicitly out of scope for the first version:

- Browser or frontend target
- Bare-metal or embedded systems
- PHP syntax compatibility mode
- Manual memory management as default API
- JIT compilation
- WebAssembly target
- Self-hosting compiler
- Plugin or macro system
- Package registry
- Complex macros or procedural generation
- Higher-kinded types or advanced type-theory features
- Full ORM
- Hot module replacement

---

## Implementation Language

The Coco compiler, runtime, and tooling are implemented in **Rust**.

Rationale: Rust provides the performance, safety, and ecosystem needed to build a language runtime. It is not the language Coco exposes to users — it is the language Coco is built with.

---

## Syntax Direction

Coco syntax is closer to JavaScript/TypeScript than PHP:
- No `$` variables
- Dot notation for member access (not `->`)
- `this` not `$this`
- ES-style imports
- Template literals
- Arrow functions
- Constructor property promotion

PHP influence appears in features, not syntax:
- Named arguments
- Traits with state
- Magic methods
- Match expressions
- Enums
- Practical standard library
- Web-first ergonomics

---

## Safety Promise

In normal safe Coco code, the following are impossible:
- Use-after-free
- Dangling references
- Null pointer dereference (without explicit assertion)
- Buffer overflow
- Uninitialized memory reads
- Accidental data races
- Unsafe raw pointer access
- Iterator invalidation
- Mutation of shared state without synchronization
- Undefined behavior

Any operation violating these guarantees must be inside `unsafe { }`.

---

## Version History

| Version | Date | Change |
|---------|------|--------|
| 1.0 | 2026-06-03 | Initial ratified charter |
```

- [ ] **Step 2: Commit**

```bash
git add docs/charter.md
git commit -m "Add language charter document"
```

---

## Task 2: Safety Promise Document

**Files:**
- Create: `docs/safety-promise.md`

- [ ] **Step 1: Write docs/safety-promise.md**

```markdown
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
```

- [ ] **Step 2: Commit**

```bash
git add docs/safety-promise.md
git commit -m "Add safety promise document"
```

---

## Task 3: Concurrency Document

**Files:**
- Create: `docs/concurrency.md`

- [ ] **Step 1: Write docs/concurrency.md**

```markdown
# Coco Concurrency Model

> This document defines how Coco handles async operations, parallelism, and shared state.

---

## Overview

Coco provides safe real concurrency with multi-core execution. The developer-facing model is simple; the compiler and runtime enforce safety automatically.

Key primitives:
- `async fn` — asynchronous function
- `await` — collect async result
- `lazy` — defer async execution
- `parallel { run ... }` — structured multi-task execution
- `coro { ... }` — spawn coroutine
- `select { case ... }` — multiplex channels/events
- `chan<T>(capacity)` — typed channel
- `atomic(value)` — atomic operations
- `synchronized { ... }` — mutual exclusion block

---

## Async Functions

```coco
async fn fetchUser(id: int): Result<User, HttpError> {
    const response = await http.get(`/users/${id}`)?;
    return Ok(User.fromJson(response.body));
}
```

- `async fn` declares a function that may suspend
- Calling an async function **starts execution immediately** (eager)
- `await` collects the result and suspends until available
- `lazy asyncFn()` defers execution until awaited

```coco
const p = fetchUser(1);          // starts NOW
const task = lazy fetchUser(2);  // cold, does nothing
const user = await task;         // NOW starts and completes
```

---

## Structured Parallelism

```coco
const [user, posts, comments] = await parallel {
    run getUser(id);
    run getPosts(id);
    run getComments(id);
};
```

Rules:
- Child tasks are scoped to the `parallel` block
- Parent waits for all children to complete
- Cancellation propagates to children on error
- Errors from children propagate to parent
- Captured state is checked for safety at compile time

---

## Coroutines

```coco
// Structured (preferred):
await parallel {
    run { processItems(); }
}

// Unscoped (allowed with scrutiny):
coro { backgroundCleanup(); }
```

Unscoped `coro`:
- Allowed but generates compiler warnings
- Subject to lifetime analysis
- Leak detection in debug builds
- Should be used sparingly for fire-and-forget background work

---

## Channels

```coco
const jobs = chan<Job>(100);
const results = chan<JobResult>(100);

coro {
    for job in jobs {
        results.send(await processJob(job));
    }
}

jobs.send(new Job("task-1"));
const result = results.recv();
```

Channel operations:
- `chan<T>(capacity)` — create buffered channel
- `chan<T>()` — create unbuffered (rendezvous) channel
- `.send(value)` — send value (blocks if full)
- `.recv()` — receive value (blocks if empty)
- `.close()` — close channel
- Iterating a channel consumes until closed

---

## Select

```coco
select {
    case job = jobs.recv():
        await process(job);
    case _ = ctx.cancelled():
        return;
    case _ = timeout(5000):
        log("timeout waiting for job");
}
```

`select` multiplexes multiple channel operations. First ready case wins.

---

## Atomics

```coco
let counter = atomic(0);

await parallel {
    run { counter.add(1); }
    run { counter.add(1); }
}

print(counter.load()); // 2
```

Atomic operations: `.load()`, `.store(v)`, `.add(v)`, `.sub(v)`, `.compareAndSwap(old, new)`

---

## Synchronized Blocks

```coco
const cache = synchronizedMap<string, User>();

await parallel {
    run { cache.set("a", userA); }
    run { cache.set("b", userB); }
}
```

`synchronized { }` provides mutual exclusion for a block of code. The runtime manages the lock.

---

## Race Prevention Rules

**Compile-time rejection:** Any mutable capture of a local variable across `parallel` or `coro` boundaries is a compile error.

Rejected:
```coco
let total = 0;
await parallel {
    run { total += 1; } // ERROR: mutable capture across parallel boundary
}
```

Accepted alternatives:
```coco
// Atomics:
let total = atomic(0);
await parallel {
    run { total.add(expensiveA()); }
    run { total.add(expensiveB()); }
}

// Channels:
const results = chan<int>();
await parallel {
    run { results.send(expensiveA()); }
    run { results.send(expensiveB()); }
}
const total = results.recv() + results.recv();

// Collect from parallel:
const [a, b] = await parallel {
    run expensiveA();
    run expensiveB();
};
const total = a + b;
```

**Immutable sharing is always safe:** Reading `const` values from parallel tasks is allowed.

```coco
const config = loadConfig();
await parallel {
    run { useConfig(config); }  // OK: config is immutable
    run { useConfig(config); }  // OK: reading shared immutable data
}
```

---

## Cancellation

```coco
async fn worker(ctx: Context, jobs: Receiver<Job>): Result<void, Error> {
    loop {
        select {
            case job = jobs.recv():
                await process(job)?;
            case _ = ctx.cancelled():
                return Ok();
        }
    }
}
```

Cancellation primitives:
- `Context` — carries deadline, cancellation signal, and values
- `ctx.cancelled()` — channel that closes on cancellation
- `ctx.withTimeout(ms)` — derived context with deadline
- `ctx.withCancel()` — derived context with manual cancel function
```

- [ ] **Step 2: Commit**

```bash
git add docs/concurrency.md
git commit -m "Add concurrency model document"
```

---

## Task 4: Type System Document

**Files:**
- Create: `docs/type-system.md`

- [ ] **Step 1: Write docs/type-system.md**

```markdown
# Coco Type System

> This document defines Coco's gradual type system — how types work, when they're required, and what guarantees they provide.

---

## Gradual Typing

Coco uses gradual typing. Types are optional and additive:

```coco
// Untyped — compiles, types checked at runtime:
fn add(a, b) { return a + b; }

// Typed — compile-time type checking:
fn addTyped(a: int, b: int): int { return a + b; }

// Mixed in same file — allowed:
const x = add(1, 2);
const y: int = addTyped(1, 2);
```

Rules:
- Untyped parameters accept any value
- Untyped functions have return type inferred where possible, otherwise `mixed`
- Typed code gets full compile-time checking
- Mixing typed and untyped code in one file is allowed
- The boundary between typed and untyped is explicit (presence or absence of annotation)

---

## Primitive Types

| Type | Description | Example |
|------|-------------|---------|
| `int` | Signed integer (64-bit) | `42`, `-1`, `0` |
| `uint` | Unsigned integer (64-bit) | `0`, `255` |
| `float` | IEEE 754 double (64-bit) | `3.14`, `-0.5` |
| `bool` | Boolean | `true`, `false` |
| `string` | UTF-8 string | `"hello"`, `` `tmpl` `` |
| `char` | Single Unicode codepoint | `'a'`, `'☺'` |
| `byte` | Unsigned 8-bit integer | `0x0F` |
| `null` | Null value | `null` |
| `void` | No return value | function returns nothing |
| `never` | Function never returns | always throws or loops forever |
| `mixed` | Any type (opt-out of checking) | dynamic value |

---

## Compound Types

```coco
list<int>               // ordered collection
map<string, User>       // key-value mapping
tuple<int, string>      // fixed-size heterogeneous
Result<User, DbError>   // success or failure (builtin)
User|null               // union: User or null
Countable & Iterable    // intersection: must satisfy both
```

---

## Nullability

Non-null by default. `T|null` makes a type nullable.

```coco
let user: User|null = null;

// Must narrow before use:
if user != null {
    print(user.name); // OK: narrowed to User
}

// Optional chaining:
const name = user?.name;           // string|null

// Null coalescing:
const name = user?.name ?? "anon"; // string

// Elvis (truthy coalescing):
const name = user?.name ?: "anon"; // string (also handles empty string)

// Non-null assertion (runtime risk):
const name = user!.name;           // string (throws if null)
```

---

## Type Inference

Local variables infer their type from the assigned value:

```coco
const x = 42;          // int
const name = "Coco";   // string
const list = [1, 2, 3]; // list<int>
const map = { "a": 1 }; // map<string, int>
```

Function parameters and return types:
- If annotated: compile-time checked
- If omitted: treated as `mixed` (gradual boundary)

---

## Generics

```coco
class Stack<T> {
    private items: list<T> = [];

    push(item: T): void {
        this.items.push(item);
    }

    pop(): T|null {
        return this.items.pop();
    }
}

fn identity<T>(value: T): T {
    return value;
}
```

Generic constraints (where needed):

```coco
fn max<T: Comparable>(a: T, b: T): T {
    return (a <=> b) >= 0 ? a : b;
}
```

---

## Union Types

```coco
type StringOrNumber = string | int;

fn format(value: StringOrNumber): string {
    match value {
        is string => value.toUpperCase(),
        is int => value.toString(),
    }
}
```

---

## Intersection Types

```coco
interface Countable {
    count(): int;
}

interface Iterable<T> {
    iterator(): Iterator<T>;
}

fn process(collection: Countable & Iterable<int>): void {
    print(collection.count());
    for item in collection {
        print(item);
    }
}
```

---

## Type Narrowing

The compiler narrows types after checks:

```coco
fn handle(value: string | int | null): string {
    if value == null {
        return "nothing";
    }
    // value is now string | int

    if value is string {
        return value.toUpperCase();
    }
    // value is now int

    return value.toString();
}
```

Narrowing triggers:
- `!= null` / `== null`
- `is Type`
- `match` arms
- Truthiness checks (for `T|null`)

---

## Result Type (Builtin)

```coco
// Language-level, no import needed:
fn divide(a: int, b: int): Result<int, MathError> {
    if b == 0 {
        return Err(new MathError("division by zero"));
    }
    return Ok(a / b);
}

// Propagation:
fn compute(): Result<int, MathError> {
    const x = divide(10, 2)?;  // unwraps or propagates error
    return Ok(x * 2);
}
```

`Result<T, E>` has two variants: `Ok(T)` and `Err(E)`.
The `?` operator propagates `Err` to the caller.

---

## String Concatenation Type Rules

The `+` operator is overloaded:
- Both numeric → arithmetic addition
- At least one string → string concatenation

Stringification rules for non-string operands in concatenation:
| Type | Stringified as |
|------|---------------|
| int, uint, float | Value representation (`"42"`, `"3.14"`) |
| bool | `[bool]` |
| null | `[null]` |
| object | `[ClassName]` |

```coco
"count: " + 42      // "count: 42"
"flag: " + true     // "flag: [bool]"
"val: " + null      // "val: [null]"
"user: " + user     // "user: [User]"
```
```

- [ ] **Step 2: Commit**

```bash
git add docs/type-system.md
git commit -m "Add type system document"
```

---

## Task 5: Decision Records (ADRs 001-005)

**Files:**
- Create: `docs/decisions/001-method-syntax.md`
- Create: `docs/decisions/002-string-concatenation.md`
- Create: `docs/decisions/003-result-type.md`
- Create: `docs/decisions/004-race-detection.md`
- Create: `docs/decisions/005-cycle-collection.md`

- [ ] **Step 1: Write docs/decisions/001-method-syntax.md**

```markdown
# ADR-001: Class Method Syntax

**Status:** Accepted
**Date:** 2026-06-03

## Context

Classes need a method declaration syntax. Options: JS-style `method(): T {}`, explicit `fn method(): T {}`, or both.

## Decision

Both syntaxes are valid inside classes:

```coco
class User {
    getName(): string { return this.name; }
    fn getEmail(): string { return this.email; }
}
```

## Rationale

- JS-style feels natural for TypeScript developers
- `fn` prefix provides explicit clarity for developers who prefer it
- No ambiguity: both forms are unambiguous in class body context
- Formatter does not enforce one style over the other

## Consequences

- Parser must handle both forms in class bodies
- Linter may offer a consistency rule (optional, not default)
- Documentation examples may use either style
```

- [ ] **Step 2: Write docs/decisions/002-string-concatenation.md**

```markdown
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
```

- [ ] **Step 3: Write docs/decisions/003-result-type.md**

```markdown
# ADR-003: Result Type is Language-Level

**Status:** Accepted
**Date:** 2026-06-03

## Context

`Result<T, E>` could be a stdlib type, a compiler-recognized stdlib type, or a language builtin.

## Decision

`Result<T, E>` is a language-level builtin. No import required. The `?` operator is compiler-intrinsic and only works with `Result`.

## Rationale

- Always available without imports
- Compiler can optimize Result handling (no indirection)
- `?` operator has guaranteed semantics
- Consistent with Result being a core error-handling mechanism alongside exceptions

## Consequences

- `Result`, `Ok`, `Err` are reserved identifiers
- `?` operator is not generalizable to other types
- Cannot be redefined or shadowed by user code
```

- [ ] **Step 4: Write docs/decisions/004-race-detection.md**

```markdown
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
```

- [ ] **Step 5: Write docs/decisions/005-cycle-collection.md**

```markdown
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
```

- [ ] **Step 6: Commit**

```bash
git add docs/decisions/001-method-syntax.md docs/decisions/002-string-concatenation.md docs/decisions/003-result-type.md docs/decisions/004-race-detection.md docs/decisions/005-cycle-collection.md
git commit -m "Add decision records 001-005"
```

---

## Task 6: Decision Records (ADRs 006-010)

**Files:**
- Create: `docs/decisions/006-unsafe-dependencies.md`
- Create: `docs/decisions/007-magic-methods.md`
- Create: `docs/decisions/008-trait-state.md`
- Create: `docs/decisions/009-error-model.md`
- Create: `docs/decisions/010-async-execution.md`

- [ ] **Step 1: Write docs/decisions/006-unsafe-dependencies.md**

```markdown
# ADR-006: Unsafe Dependencies Blocked by Default

**Status:** Accepted
**Date:** 2026-06-03

## Context

In application safety mode, should packages that use `unsafe` internally be allowed as dependencies?

## Decision

Blocked by default in `application` mode. Must be explicitly allowlisted in `coco.toml`.

```toml
[safety]
mode = "application"
allow_unsafe_dependencies = false

[safety.allow]
coco-ffi-png = "audited"
```

## Rationale

- Forces awareness of which dependencies use unsafe code
- Application developers should not unknowingly depend on unsafe code
- Allowlisting creates an audit trail
- Library authors are incentivized to minimize unsafe usage

## Consequences

- Some packages will be blocked until allowlisted
- Package metadata must declare unsafe usage
- First-time setup may require allowlisting common packages
- `coco build` clearly reports which packages are blocked and why
```

- [ ] **Step 2: Write docs/decisions/007-magic-methods.md**

```markdown
# ADR-007: PHP-Style Magic Methods

**Status:** Accepted
**Date:** 2026-06-03

## Context

Classes need special behavior hooks (string conversion, property access, invocation). Options: JS Symbol-style protocols, PHP magic methods, or no magic.

## Decision

PHP-style magic methods (`__toString`, `__get`, `__set`, `__call`, `__invoke`, `__compare`) are first-class in normal Coco.

```coco
class Money {
    __toString(): string {
        return `$${this.cents / 100}`;
    }

    __compare(other: Money): int {
        return this.cents <=> other.cents;
    }
}
```

## Rationale

- Familiar to PHP developers (target audience)
- Simpler than JS Symbol ceremony
- Clear naming convention (double underscore = magic)
- Easy to grep for and audit

## Consequences

- Double-underscore prefix is reserved for magic methods
- Users cannot name regular methods with `__` prefix
- Compiler must recognize and validate magic method signatures
- Defined set of magic methods (not arbitrary)
```

- [ ] **Step 3: Write docs/decisions/008-trait-state.md**

```markdown
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
```

- [ ] **Step 4: Write docs/decisions/009-error-model.md**

```markdown
# ADR-009: Dual Error Model (Exceptions + Result)

**Status:** Accepted
**Date:** 2026-06-03

## Context

Error handling could use exceptions only, Result only, or both with a clear split.

## Decision

Both coexist with a clear semantic split:
- **Exceptions:** unexpected/fatal errors (bugs, OOM, assertion failures, invariant violations)
- **Result<T, E>:** expected failures (parsing, validation, I/O, database, network)

## Rationale

- Expected failures should be in the type signature (Result)
- Unexpected failures should not pollute every return type (exceptions)
- Stdlib uses both: `db.query()` → Result, `assert()` → throws
- Developers can choose based on error nature, not language limitation

## Consequences

- Functions that can fail expectedly should return Result
- `?` propagates Result errors
- `try/catch` handles exceptions
- A function can both return Result AND throw (throws = bugs only)
- Stdlib documentation clearly labels which functions use which model
```

- [ ] **Step 5: Write docs/decisions/010-async-execution.md**

```markdown
# ADR-010: Eager Async with Lazy Opt-In

**Status:** Accepted
**Date:** 2026-06-03

## Context

When an async function is called, should it start executing immediately (eager, JS-style) or return a cold task (lazy, Rust-style)?

## Decision

Eager by default. `lazy` keyword available for deferred execution.

```coco
const p = fetchUser(1);          // starts immediately
const task = lazy fetchUser(2);  // cold until awaited
const user = await task;         // NOW runs
```

## Rationale

- JS developers expect eager execution (familiar mental model)
- Eager is more intuitive for most backend patterns
- `lazy` provides control when deferred execution is intentional
- Structured parallelism (`parallel { run ... }`) handles the "start many at once" pattern

## Consequences

- Calling async fn without await starts work (side effects begin)
- `lazy` is a keyword that wraps the call in a deferred task
- Lazy tasks are cold — no execution until awaited or explicitly started
- Resource management requires awareness that eager calls begin immediately
```

- [ ] **Step 6: Commit**

```bash
git add docs/decisions/006-unsafe-dependencies.md docs/decisions/007-magic-methods.md docs/decisions/008-trait-state.md docs/decisions/009-error-model.md docs/decisions/010-async-execution.md
git commit -m "Add decision records 006-010"
```

---

## Task 7: Decision Records (ADRs 011-015)

**Files:**
- Create: `docs/decisions/011-coroutine-scoping.md`
- Create: `docs/decisions/012-first-backend.md`
- Create: `docs/decisions/013-gradual-typing.md`
- Create: `docs/decisions/014-php-compat-v1.md`
- Create: `docs/decisions/015-runtime-magic.md`

- [ ] **Step 1: Write docs/decisions/011-coroutine-scoping.md**

```markdown
# ADR-011: Both Scoped and Unscoped Coroutines

**Status:** Accepted
**Date:** 2026-06-03

## Context

Should all coroutines be scoped (structured concurrency only), or can developers spawn unscoped fire-and-forget tasks?

## Decision

Both allowed. Scoped is preferred and recommended. Unscoped `coro` gets extra compiler scrutiny:
- Compiler warning for unscoped usage
- Lifetime analysis on captured state
- Leak detection in debug builds

```coco
// Structured (preferred):
await parallel {
    run { doWork(); }
}

// Unscoped (allowed, with scrutiny):
coro { backgroundCleanup(); }
```

## Rationale

- Strict scoping prevents resource leaks
- Some patterns genuinely need fire-and-forget (background cleanup, metrics, logging)
- Warnings + debug detection balance flexibility with safety
- Developers making conscious choice to use unscoped get informed of risks

## Consequences

- Unscoped `coro` produces a compiler warning (suppressible with annotation)
- Debug runtime tracks unscoped coroutine lifetimes
- Unscoped coroutines that outlive their spawning context are reported
- Documentation strongly recommends structured patterns
```

- [ ] **Step 2: Write docs/decisions/012-first-backend.md**

```markdown
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
```

- [ ] **Step 3: Write docs/decisions/013-gradual-typing.md**

```markdown
# ADR-013: Gradual Typing

**Status:** Accepted
**Date:** 2026-06-03

## Context

Should Coco require full type annotations (strict), allow fully untyped code (gradual), or use inference-only?

## Decision

Gradual typing. Types are optional and can be added progressively. Typed and untyped code can coexist in the same file.

```coco
fn add(a, b) { return a + b; }              // untyped
fn addTyped(a: int, b: int): int { return a + b; }  // typed
```

## Rationale

- Lowers barrier to entry for scripting use cases
- Matches PHP/JS developer expectations (types are additive, not required)
- Typed code gets stronger guarantees (compile-time checking)
- Untyped code still gets runtime safety (memory safety, bounds checks)
- Progressive typing encourages adoption without forcing it

## Consequences

- Type checker must handle `mixed` (unknown type) boundaries
- Untyped → typed boundaries require runtime checks
- Memory safety still holds for untyped code (runtime enforcement)
- Some compiler optimizations only available for fully typed code
- Linter can recommend adding types (optional strictness level)
```

- [ ] **Step 4: Write docs/decisions/014-php-compat-v1.md**

```markdown
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
```

- [ ] **Step 5: Write docs/decisions/015-runtime-magic.md**

```markdown
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
```

- [ ] **Step 6: Commit**

```bash
git add docs/decisions/011-coroutine-scoping.md docs/decisions/012-first-backend.md docs/decisions/013-gradual-typing.md docs/decisions/014-php-compat-v1.md docs/decisions/015-runtime-magic.md
git commit -m "Add decision records 011-015"
```

---

## Task 8: EBNF Grammar

**Files:**
- Create: `docs/grammar.ebnf`

- [ ] **Step 1: Write docs/grammar.ebnf**

```ebnf
(* Coco Language Grammar — Phase 1 *)
(* Version: 1.0 *)
(* Date: 2026-06-03 *)

(* === Top Level === *)

program = { import_decl | export_decl | declaration | statement } ;

(* === Imports / Exports === *)

import_decl = "import" , ( named_import | namespace_import ) , "from" , string_literal , ";" ;
named_import = "{" , identifier , { "," , identifier } , [ "," ] , "}" ;
namespace_import = "*" , "as" , identifier ;

export_decl = "export" , ( class_decl | fn_decl | interface_decl | trait_decl | enum_decl | const_decl ) ;

(* === Declarations === *)

declaration = const_decl
            | let_decl
            | fn_decl
            | class_decl
            | interface_decl
            | trait_decl
            | enum_decl
            | type_alias ;

const_decl = "const" , identifier , [ ":" , type_expr ] , "=" , expression , ";" ;
let_decl = "let" , identifier , [ ":" , type_expr ] , [ "=" , expression ] , ";" ;

fn_decl = [ "async" ] , "fn" , identifier , [ type_params ] , "(" , [ param_list ] , ")" , [ ":" , type_expr ] , block ;

type_alias = "type" , identifier , [ type_params ] , "=" , type_expr , ";" ;

(* === Parameters === *)

param_list = param , { "," , param } , [ "," ] ;
param = identifier , [ ":" , type_expr ] , [ "=" , expression ] ;

(* === Classes === *)

class_decl = "class" , identifier , [ type_params ] , [ extends_clause ] , [ implements_clause ] , class_body ;
extends_clause = "extends" , type_expr ;
implements_clause = "implements" , type_expr , { "," , type_expr } ;

class_body = "{" , { class_member } , "}" ;
class_member = constructor_decl
             | method_decl
             | property_decl
             | static_member
             | use_trait ;

constructor_decl = "constructor" , "(" , [ constructor_param_list ] , ")" , block ;
constructor_param_list = constructor_param , { "," , constructor_param } , [ "," ] ;
constructor_param = { modifier } , identifier , [ ":" , type_expr ] , [ "=" , expression ] ;

modifier = "public" | "private" | "protected" | "readonly" ;

method_decl = [ "async" ] , [ "fn" ] , identifier , [ type_params ] , "(" , [ param_list ] , ")" , [ ":" , type_expr ] , block ;

property_decl = { modifier } , identifier , ":" , type_expr , [ "=" , expression ] , ";" ;

static_member = "static" , ( method_decl | property_decl ) ;

use_trait = "use" , identifier , { "," , identifier } , ";" ;

(* === Interfaces === *)

interface_decl = "interface" , identifier , [ type_params ] , [ extends_clause ] , interface_body ;
interface_body = "{" , { interface_member } , "}" ;
interface_member = method_signature | property_signature ;
method_signature = [ "async" ] , [ "fn" ] , identifier , [ type_params ] , "(" , [ param_list ] , ")" , ":" , type_expr , ";" ;
property_signature = identifier , ":" , type_expr , ";" ;

(* === Traits === *)

trait_decl = "trait" , identifier , [ type_params ] , trait_body ;
trait_body = "{" , { trait_member } , "}" ;
trait_member = method_decl | property_decl | method_signature ;

(* === Enums === *)

enum_decl = "enum" , identifier , [ ":" , type_expr ] , enum_body ;
enum_body = "{" , enum_variant , { "," , enum_variant } , [ "," ] , "}" ;
enum_variant = identifier , [ "(" , type_expr , { "," , type_expr } , ")" ] , [ "=" , expression ] ;

(* === Type Expressions === *)

type_expr = union_type ;
union_type = intersection_type , { "|" , intersection_type } ;
intersection_type = primary_type , { "&" , primary_type } ;

primary_type = "int" | "uint" | "float" | "bool" | "string" | "char" | "byte"
             | "null" | "void" | "never" | "mixed"
             | named_type
             | list_type
             | map_type
             | tuple_type
             | result_type
             | "(" , type_expr , ")"
             | fn_type ;

named_type = identifier , [ "<" , type_expr , { "," , type_expr } , ">" ] ;
list_type = "list" , "<" , type_expr , ">" ;
map_type = "map" , "<" , type_expr , "," , type_expr , ">" ;
tuple_type = "tuple" , "<" , type_expr , { "," , type_expr } , ">" ;
result_type = "Result" , "<" , type_expr , "," , type_expr , ">" ;
fn_type = "(" , [ type_expr , { "," , type_expr } ] , ")" , "=>" , type_expr ;

type_params = "<" , type_param , { "," , type_param } , ">" ;
type_param = identifier , [ ":" , type_expr ] ;

(* === Statements === *)

statement = expression_stmt
          | if_stmt
          | for_stmt
          | while_stmt
          | do_while_stmt
          | loop_stmt
          | return_stmt
          | throw_stmt
          | try_stmt
          | break_stmt
          | continue_stmt
          | parallel_stmt
          | coro_stmt
          | select_stmt
          | unsafe_stmt
          | synchronized_stmt
          | block ;

expression_stmt = expression , ";" ;

if_stmt = "if" , expression , block , { "else" , "if" , expression , block } , [ "else" , block ] ;

for_stmt = "for" , identifier , "in" , expression , block
         | "for" , "(" , [ declaration | expression_stmt ] , [ expression ] , ";" , [ expression ] , ")" , block ;

while_stmt = "while" , expression , block ;
do_while_stmt = "do" , block , "while" , expression , ";" ;
loop_stmt = "loop" , block ;

return_stmt = "return" , [ expression ] , ";" ;
throw_stmt = "throw" , expression , ";" ;
break_stmt = "break" , ";" ;
continue_stmt = "continue" , ";" ;

try_stmt = "try" , block , { catch_clause } , [ finally_clause ] ;
catch_clause = "catch" , "(" , identifier , [ ":" , type_expr ] , ")" , block ;
finally_clause = "finally" , block ;

parallel_stmt = "await" , "parallel" , "{" , { run_clause } , "}" ;
run_clause = "run" , ( expression , ";" | block ) ;

coro_stmt = "coro" , block ;

select_stmt = "select" , "{" , { case_clause } , "}" ;
case_clause = "case" , identifier , "=" , expression , ":" , { statement } ;

unsafe_stmt = "unsafe" , block ;
synchronized_stmt = "synchronized" , block ;

block = "{" , { declaration | statement } , "}" ;

(* === Expressions === *)

expression = assignment_expr ;

assignment_expr = conditional_expr , [ assignment_op , assignment_expr ] ;
assignment_op = "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "**=" | "<<=" | ">>=" | "&=" | "|=" | "^=" ;

conditional_expr = elvis_expr , [ "?" , expression , ":" , expression ] ;

elvis_expr = null_coalesce_expr , [ "?:" , elvis_expr ] ;

null_coalesce_expr = logical_or_expr , [ "??" , null_coalesce_expr ] ;

logical_or_expr = logical_and_expr , { "||" , logical_and_expr } ;
logical_and_expr = bitwise_or_expr , { "&&" , bitwise_or_expr } ;
bitwise_or_expr = bitwise_xor_expr , { "|" , bitwise_xor_expr } ;
bitwise_xor_expr = bitwise_and_expr , { "^" , bitwise_and_expr } ;
bitwise_and_expr = equality_expr , { "&" , equality_expr } ;
equality_expr = comparison_expr , { ( "==" | "!=" ) , comparison_expr } ;
comparison_expr = shift_expr , { ( "<" | ">" | "<=" | ">=" | "<=>" ) , shift_expr } ;
shift_expr = additive_expr , { ( "<<" | ">>" ) , additive_expr } ;
additive_expr = multiplicative_expr , { ( "+" | "-" ) , multiplicative_expr } ;
multiplicative_expr = exponent_expr , { ( "*" | "/" | "%" ) , exponent_expr } ;
exponent_expr = unary_expr , [ "**" , exponent_expr ] ;

unary_expr = ( "!" | "~" | "-" | "typeof" | "new" | "await" | "lazy" ) , unary_expr
           | postfix_expr ;

postfix_expr = primary_expr , { postfix_op } ;
postfix_op = "." , identifier
           | "?." , identifier
           | "[" , expression , "]"
           | "(" , [ argument_list ] , ")"
           | "!"
           | "?"
           | "++"
           | "--" ;

argument_list = argument , { "," , argument } , [ "," ] ;
argument = [ identifier , ":" ] , expression ;

primary_expr = identifier
             | literal
             | "this"
             | "(" , expression , ")"
             | array_literal
             | object_literal
             | arrow_fn
             | match_expr
             | "fn" , "(" , [ param_list ] , ")" , [ ":" , type_expr ] , ( "=>" , expression | block ) ;

arrow_fn = "(" , [ param_list ] , ")" , [ ":" , type_expr ] , "=>" , ( expression | block )
         | identifier , "=>" , ( expression | block ) ;

match_expr = "match" , expression , "{" , { match_arm } , "}" ;
match_arm = pattern , "=>" , expression , ","
          | pattern , "=>" , block , [ "," ] ;

pattern = literal
        | identifier
        | "is" , type_expr
        | "_" ;

(* === Literals === *)

literal = int_literal
        | float_literal
        | string_literal
        | template_literal
        | char_literal
        | bool_literal
        | null_literal ;

int_literal = digit , { digit | "_" }
            | "0x" , hex_digit , { hex_digit | "_" }
            | "0b" , bin_digit , { bin_digit | "_" }
            | "0o" , oct_digit , { oct_digit | "_" } ;

float_literal = digit , { digit } , "." , digit , { digit } , [ exponent_part ]
              | digit , { digit } , exponent_part ;
exponent_part = ( "e" | "E" ) , [ "+" | "-" ] , digit , { digit } ;

string_literal = '"' , { string_char } , '"'
               | "'" , { char_content } , "'" ;
string_char = ? any character except '"' and '\' ?
            | escape_sequence ;

template_literal = "`" , { template_char | template_expr } , "`" ;
template_char = ? any character except '`', '\', and '${' ? | escape_sequence ;
template_expr = "${" , expression , "}" ;

char_literal = "'" , char_content , "'" ;
char_content = ? any single character except "'" and '\' ? | escape_sequence ;

escape_sequence = "\\" | "\n" | "\r" | "\t" | "\0" | "\'" | '\"' | "\$"
               | "\x" , hex_digit , hex_digit
               | "\u{" , hex_digit , { hex_digit } , "}" ;

bool_literal = "true" | "false" ;
null_literal = "null" ;

array_literal = "[" , [ expression , { "," , expression } , [ "," ] ] , "]" ;
object_literal = "{" , [ object_field , { "," , object_field } , [ "," ] ] , "}" ;
object_field = ( identifier | string_literal ) , ":" , expression ;

(* === Lexical === *)

identifier = ( letter | "_" ) , { letter | digit | "_" } ;
letter = "a" | "b" | ... | "z" | "A" | "B" | ... | "Z" ;
digit = "0" | "1" | ... | "9" ;
hex_digit = digit | "a" | ... | "f" | "A" | ... | "F" ;
bin_digit = "0" | "1" ;
oct_digit = "0" | "1" | ... | "7" ;

(* === Comments === *)

comment = line_comment | block_comment ;
line_comment = "//" , { ? any character except newline ? } ;
block_comment = "/*" , { ? any character ? } , "*/" ;

(* === Reserved Words === *)

(* async, await, break, case, catch, class, const, continue, coro, do, else,
   enum, export, extends, false, finally, fn, for, if, implements, import,
   in, interface, is, lazy, let, loop, match, new, null, of, parallel,
   private, protected, public, readonly, return, run, select, static,
   synchronized, this, throw, trait, true, try, type, typeof, unsafe,
   use, void, while, Ok, Err, Result *)
```

- [ ] **Step 2: Commit**

```bash
git add docs/grammar.ebnf
git commit -m "Add formal EBNF grammar"
```

---

## Task 9: Language Reference

**Files:**
- Create: `docs/language-reference.md`

- [ ] **Step 1: Write docs/language-reference.md**

```markdown
# Coco Language Reference

> A human-readable guide to Coco syntax. Designed to be read in 30 minutes by a JavaScript or PHP developer.

---

## Variables

```coco
const pi = 3.14159;          // immutable binding
let counter = 0;             // mutable binding
let name: string = "Coco";   // explicit type annotation

counter += 1;                // mutation allowed for let
```

- `const` — immutable. Cannot be reassigned.
- `let` — mutable. Can be reassigned.
- Type annotations are optional (gradual typing).
- Shadowing is allowed but warned by default.

---

## Functions

```coco
fn greet(name: string): string {
    return `Hello, ${name}`;
}

// Arrow function:
const square = (n: int): int => n * n;

// Async function:
async fn fetchData(url: string): Result<string, HttpError> {
    const response = await http.get(url)?;
    return Ok(response.body);
}

// Untyped (gradual):
fn add(a, b) { return a + b; }
```

- `fn` keyword for named functions
- Arrow functions for callbacks and closures
- `async fn` for asynchronous functions
- Parameters and return types are optional

---

## Classes

```coco
class User {
    constructor(
        public readonly id: int,
        public name: string,
        private email: string,
    ) {}

    getDisplayName(): string {
        return this.name;
    }

    fn setEmail(email: string): void {
        this.email = email;
    }

    static create(name: string, email: string): User {
        return new User(
            id: nextId(),
            name: name,
            email: email,
        );
    }
}

const user = new User(id: 1, name: "Jericho", email: "j@ex.com");
```

- Constructor parameter properties (`public`, `private`, `protected`, `readonly`)
- Methods: both `method()` and `fn method()` syntax valid
- `this` for instance access
- `.` for member access
- `static` for class-level members
- Single inheritance with `extends`
- Named arguments at call site

---

## Interfaces

```coco
interface Serializable {
    serialize(): string;
    deserialize(data: string): void;
}

class Config implements Serializable {
    serialize(): string { return JSON.stringify(this); }
    deserialize(data: string): void { /* ... */ }
}
```

---

## Traits

```coco
trait Timestamps {
    createdAt: DateTime|null = null;
    updatedAt: DateTime|null = null;

    touch(): void {
        this.updatedAt = DateTime.now();
    }
}

trait SoftDelete {
    deletedAt: DateTime|null = null;

    softDelete(): void {
        this.deletedAt = DateTime.now();
    }

    isDeleted(): bool {
        return this.deletedAt != null;
    }
}

class Post {
    use Timestamps, SoftDelete;

    constructor(public title: string, public body: string) {}
}
```

- Traits can have properties with defaults
- Traits can have method implementations
- Multiple traits via `use Trait1, Trait2`

---

## Enums

```coco
enum Direction {
    North,
    South,
    East,
    West,
}

enum HttpStatus: int {
    Ok = 200,
    NotFound = 404,
    ServerError = 500,
}

enum Shape {
    Circle(float),
    Rectangle(float, float),
    Point,
}
```

---

## Collections

```coco
// Lists:
const numbers: list<int> = [1, 2, 3];
numbers.push(4);
const first = numbers[0];

// Maps:
const config: map<string, string> = {
    "env": "production",
    "region": "asia",
};
const env = config["env"];

// Tuples:
const pair: tuple<string, int> = ("hello", 42);
```

---

## Error Handling

### Result Type

```coco
fn parseAge(input: string): Result<int, ParseError> {
    if input.isEmpty() {
        return Err(new ParseError("empty input"));
    }
    const n = int.parse(input) ?: return Err(new ParseError("not a number"));
    if n < 0 {
        return Err(new ParseError("age cannot be negative"));
    }
    return Ok(n);
}

// Propagation with ?:
fn processForm(data: FormData): Result<User, FormError> {
    const age = parseAge(data.get("age"))?;
    const name = data.get("name") ?: return Err(new FormError("name required"));
    return Ok(new User(name: name, age: age));
}
```

### Exceptions

```coco
fn riskyOperation(): void {
    throw new RuntimeError("something went wrong");
}

try {
    riskyOperation();
} catch (e: RuntimeError) {
    log.error(e.message);
} finally {
    cleanup();
}
```

### Split Rule

- **Result** for expected failures (parsing, I/O, validation)
- **Exceptions** for unexpected failures (bugs, invariant violations)

---

## Null Safety

```coco
let user: User|null = findUser(id);

// Optional chaining:
const avatar = user?.profile?.avatar?.url;

// Null coalescing:
const name = user?.name ?? "Anonymous";

// Elvis (truthy coalescing):
const display = user?.nickname ?: "No nickname";

// Non-null assertion (throws if null):
const email = user!.email;

// Narrowing:
if user != null {
    print(user.name);  // user is User here, not User|null
}
```

---

## Match Expressions

```coco
const result = match status {
    HttpStatus.Ok => "success",
    HttpStatus.NotFound => "not found",
    HttpStatus.ServerError => "error",
    _ => "unknown",
};

const description = match shape {
    is Shape.Circle(r) => `circle with radius ${r}`,
    is Shape.Rectangle(w, h) => `${w}x${h} rectangle`,
    is Shape.Point => "point",
};
```

---

## Async and Concurrency

### Async/Await

```coco
async fn getUser(id: int): Result<User, DbError> {
    return await db.users.find(id);
}

const user = await getUser(1)?;
```

### Parallel Execution

```coco
const [user, posts] = await parallel {
    run getUser(id);
    run getPosts(id);
};
```

### Channels

```coco
const ch = chan<string>(10);

coro {
    ch.send("hello");
    ch.send("world");
    ch.close();
}

for msg in ch {
    print(msg);
}
```

### Select

```coco
select {
    case msg = inbox.recv():
        handle(msg);
    case _ = timeout(5000):
        print("timed out");
}
```

---

## Operators

| Operator | Description |
|----------|-------------|
| `+` `-` `*` `/` `%` `**` | Arithmetic |
| `==` `!=` `<` `>` `<=` `>=` | Comparison |
| `<=>` | Spaceship (three-way comparison) |
| `&&` `\|\|` `!` | Logical |
| `&` `\|` `^` `~` `<<` `>>` | Bitwise |
| `?.` | Optional chaining |
| `??` | Null coalescing |
| `?:` | Elvis (truthy coalescing) |
| `!` (postfix) | Non-null assertion |
| `?` (postfix) | Result propagation |
| `+` (string) | Concatenation (when one operand is string) |

---

## Modules

```coco
// Importing:
import { Server, Response } from "std/http";
import { readFile } from "std/fs";
import * as crypto from "std/crypto";
import { User } from "./models/user";

// Exporting:
export class ApiServer { /* ... */ }
export fn createApp(): Server { /* ... */ }
```

---

## Unsafe

```coco
unsafe {
    const lib = ffi.load("libcrypto.so");
    const encrypt = lib.fn("crypto_encrypt");
}
```

- Only for FFI, raw memory, systems work
- Blocked in `application` safety mode
- Visible in source and tooling reports
```

- [ ] **Step 2: Commit**

```bash
git add docs/language-reference.md
git commit -m "Add language reference document"
```

---

## Task 10: Example Programs (01-05)

**Files:**
- Create: `examples/01-hello.co`
- Create: `examples/02-variables.co`
- Create: `examples/03-functions.co`
- Create: `examples/04-classes.co`
- Create: `examples/05-collections.co`

- [ ] **Step 1: Write examples/01-hello.co**

```coco
// Hello World in Coco

fn main(): int {
    print("Hello, World!");
    return 0;
}
```

- [ ] **Step 2: Write examples/02-variables.co**

```coco
// Variables and bindings in Coco

fn main(): int {
    // Immutable binding:
    const pi = 3.14159;
    const language = "Coco";

    // Mutable binding:
    let counter = 0;
    counter += 1;
    counter += 1;

    // Typed bindings:
    let age: int = 28;
    let name: string = "Jericho";
    let active: bool = true;

    // Untyped (gradual):
    let anything = "could be anything";
    anything = 42;  // allowed in untyped mode

    // Null:
    let user: string|null = null;
    user = "found";

    print(`${language} counter: ${counter}`);
    print(`${name}, age ${age}, active: ${active}`);

    return 0;
}
```

- [ ] **Step 3: Write examples/03-functions.co**

```coco
// Functions in Coco

// Named function with types:
fn add(a: int, b: int): int {
    return a + b;
}

// Untyped function (gradual):
fn multiply(a, b) {
    return a * b;
}

// Arrow function:
const square = (n: int): int => n * n;

// Multi-line arrow:
const clamp = (value: int, min: int, max: int): int => {
    if value < min { return min; }
    if value > max { return max; }
    return value;
};

// Named arguments:
fn createUser(name: string, age: int, email: string): string {
    return `${name} (${age}) <${email}>`;
}

// Default parameters:
fn greet(name: string, greeting: string = "Hello"): string {
    return `${greeting}, ${name}!`;
}

// Returning Result:
fn divide(a: int, b: int): Result<int, string> {
    if b == 0 {
        return Err("division by zero");
    }
    return Ok(a / b);
}

fn main(): int {
    print(add(2, 3));
    print(multiply(4, 5));
    print(square(6));
    print(clamp(15, 0, 10));

    // Named arguments at call site:
    print(createUser(name: "Jericho", age: 28, email: "j@ex.com"));

    // Default parameter:
    print(greet("World"));
    print(greet("Coco", greeting: "Hey"));

    // Result handling:
    const result = divide(10, 3)?;
    print(result);

    return 0;
}
```

- [ ] **Step 4: Write examples/04-classes.co**

```coco
// Classes in Coco

class Animal {
    constructor(
        public readonly species: string,
        public name: string,
        private sound: string,
    ) {}

    speak(): string {
        return `${this.name} says ${this.sound}`;
    }

    fn rename(name: string): void {
        this.name = name;
    }

    static dog(name: string): Animal {
        return new Animal(species: "Dog", name: name, sound: "woof");
    }

    static cat(name: string): Animal {
        return new Animal(species: "Cat", name: name, sound: "meow");
    }
}

class Pet extends Animal {
    constructor(
        species: string,
        name: string,
        sound: string,
        public owner: string,
    ) {
        super(species, name, sound);
    }

    introduce(): string {
        return `${this.name} belongs to ${this.owner}`;
    }
}

interface Feedable {
    feed(food: string): void;
    isHungry(): bool;
}

class HomePet extends Pet implements Feedable {
    private hunger: int = 50;

    feed(food: string): void {
        this.hunger -= 20;
        print(`${this.name} eats ${food}`);
    }

    isHungry(): bool {
        return this.hunger > 70;
    }
}

fn main(): int {
    const dog = Animal.dog("Rex");
    print(dog.speak());

    dog.rename("Max");
    print(dog.speak());

    const pet = new Pet(
        species: "Bird",
        name: "Tweety",
        sound: "chirp",
        owner: "Jericho",
    );
    print(pet.introduce());
    print(pet.speak());

    return 0;
}
```

- [ ] **Step 5: Write examples/05-collections.co**

```coco
// Collections in Coco

fn main(): int {
    // Lists:
    const numbers: list<int> = [1, 2, 3, 4, 5];
    let fruits = ["apple", "banana", "cherry"];

    fruits.push("date");
    const first = fruits[0];
    const length = fruits.length;

    // Map/filter/reduce:
    const doubled = numbers.map((n) => n * 2);
    const evens = numbers.filter((n) => n % 2 == 0);
    const sum = numbers.reduce(0, (acc, n) => acc + n);

    print(`Doubled: ${doubled}`);
    print(`Evens: ${evens}`);
    print(`Sum: ${sum}`);

    // Maps:
    const scores: map<string, int> = {
        "alice": 95,
        "bob": 87,
        "carol": 92,
    };

    let config = {
        "host": "localhost",
        "port": "8080",
    };

    config["debug"] = "true";

    const host = config["host"];
    const hasAlice = scores.has("alice");

    print(`Host: ${host}`);
    print(`Has alice: ${hasAlice}`);

    // Iterating:
    for fruit in fruits {
        print(`Fruit: ${fruit}`);
    }

    for (key, value) in scores {
        print(`${key}: ${value}`);
    }

    // Tuples:
    const point: tuple<int, int> = (10, 20);
    const record: tuple<string, int, bool> = ("item", 42, true);

    // Copy-on-write behavior:
    const original = [1, 2, 3];
    let copy = original;
    copy.push(4);
    // original is still [1, 2, 3]
    // copy is [1, 2, 3, 4]

    print(`Original: ${original}`);
    print(`Copy: ${copy}`);

    return 0;
}
```

- [ ] **Step 6: Commit**

```bash
git add examples/01-hello.co examples/02-variables.co examples/03-functions.co examples/04-classes.co examples/05-collections.co
git commit -m "Add example programs 01-05"
```

---

## Task 11: Example Programs (06-10)

**Files:**
- Create: `examples/06-error-handling.co`
- Create: `examples/07-null-safety.co`
- Create: `examples/08-async-basic.co`
- Create: `examples/09-parallel.co`
- Create: `examples/10-channels.co`

- [ ] **Step 1: Write examples/06-error-handling.co**

```coco
// Error handling in Coco

class ValidationError {
    constructor(public field: string, public message: string) {}

    __toString(): string {
        return `${this.field}: ${this.message}`;
    }
}

fn validateAge(input: string): Result<int, ValidationError> {
    if input.isEmpty() {
        return Err(new ValidationError("age", "cannot be empty"));
    }

    const age = int.parse(input) ?: return Err(
        new ValidationError("age", "must be a number")
    );

    if age < 0 || age > 150 {
        return Err(new ValidationError("age", "must be between 0 and 150"));
    }

    return Ok(age);
}

fn validateName(input: string): Result<string, ValidationError> {
    if input.isEmpty() {
        return Err(new ValidationError("name", "cannot be empty"));
    }
    if input.length < 2 {
        return Err(new ValidationError("name", "must be at least 2 characters"));
    }
    return Ok(input.trim());
}

fn processForm(name: string, ageStr: string): Result<string, ValidationError> {
    const validName = validateName(name)?;
    const validAge = validateAge(ageStr)?;
    return Ok(`Welcome, ${validName} (age ${validAge})`);
}

// Exceptions for unexpected errors:
fn loadConfig(path: string): map<string, string> {
    if !fs.exists(path) {
        throw new RuntimeError(`config file not found: ${path}`);
    }
    return fs.readJson(path);
}

fn main(): int {
    // Result handling:
    match processForm("Jericho", "28") {
        is Ok(msg) => print(msg),
        is Err(e) => print(`Error: ${e}`),
    };

    // Chained propagation:
    const result = processForm("", "abc");
    if result is Err(e) {
        print(`Validation failed: ${e}`);
    }

    // Exception handling:
    try {
        const config = loadConfig("/etc/app/config.json");
        print(`Loaded ${config.size()} settings`);
    } catch (e: RuntimeError) {
        print(`Fatal: ${e.message}`);
    } finally {
        print("Config loading attempted");
    }

    return 0;
}
```

- [ ] **Step 2: Write examples/07-null-safety.co**

```coco
// Null safety in Coco

class Profile {
    constructor(
        public displayName: string|null = null,
        public bio: string|null = null,
        public avatarUrl: string|null = null,
    ) {}
}

class User {
    constructor(
        public name: string,
        public profile: Profile|null = null,
    ) {}
}

fn getAvatarUrl(user: User|null): string {
    // Optional chaining through multiple levels:
    return user?.profile?.avatarUrl ?? "/default-avatar.png";
}

fn getDisplayName(user: User|null): string {
    // Elvis operator (truthy coalescing - also handles empty string):
    return user?.profile?.displayName ?: user?.name ?: "Anonymous";
}

fn processUser(user: User|null): void {
    // Narrowing with if:
    if user == null {
        print("No user provided");
        return;
    }

    // user is now User (narrowed)
    print(`User: ${user.name}`);

    if user.profile != null {
        // user.profile is now Profile (narrowed)
        print(`Bio: ${user.profile.bio ?? "No bio"}`);
    }
}

fn main(): int {
    const userWithProfile = new User(
        name: "Jericho",
        profile: new Profile(
            displayName: "J",
            bio: "Coco creator",
            avatarUrl: "/img/jericho.png",
        ),
    );

    const userNoProfile = new User(name: "Guest");
    const nullUser: User|null = null;

    print(getAvatarUrl(userWithProfile));  // "/img/jericho.png"
    print(getAvatarUrl(userNoProfile));    // "/default-avatar.png"
    print(getAvatarUrl(nullUser));         // "/default-avatar.png"

    print(getDisplayName(userWithProfile)); // "J"
    print(getDisplayName(userNoProfile));   // "Guest"
    print(getDisplayName(nullUser));        // "Anonymous"

    processUser(userWithProfile);
    processUser(nullUser);

    // Non-null assertion (use sparingly):
    const definitelyExists: User = userWithProfile!;
    print(definitelyExists.name);

    return 0;
}
```

- [ ] **Step 3: Write examples/08-async-basic.co**

```coco
// Basic async in Coco

import { http, Response } from "std/http";

class ApiClient {
    constructor(private baseUrl: string) {}

    async fn get(path: string): Result<string, HttpError> {
        const response = await http.get(`${this.baseUrl}${path}`)?;
        return Ok(response.body);
    }

    async fn post(path: string, body: string): Result<string, HttpError> {
        const response = await http.post(`${this.baseUrl}${path}`, body)?;
        return Ok(response.body);
    }
}

async fn fetchUserData(id: int): Result<string, HttpError> {
    const client = new ApiClient("https://api.example.com");
    const data = await client.get(`/users/${id}`)?;
    return Ok(data);
}

async fn main(): int {
    // Eager execution (starts immediately):
    const userPromise = fetchUserData(1);

    // Do other work while fetch is in flight:
    print("Fetching user data...");

    // Collect result:
    const result = await userPromise;
    match result {
        is Ok(data) => print(`Got: ${data}`),
        is Err(e) => print(`Failed: ${e.message}`),
    };

    // Lazy execution (deferred):
    const lazyFetch = lazy fetchUserData(2);
    print("Lazy fetch created but not started");

    // NOW it starts:
    const user2 = await lazyFetch;
    print("Lazy fetch completed");

    return 0;
}
```

- [ ] **Step 4: Write examples/09-parallel.co**

```coco
// Parallel execution in Coco

import { http } from "std/http";
import { db } from "std/db";

async fn getUser(id: int): Result<string, Error> {
    return await db.query("SELECT name FROM users WHERE id = ?", [id]);
}

async fn getPosts(userId: int): Result<list<string>, Error> {
    return await db.query("SELECT title FROM posts WHERE user_id = ?", [userId]);
}

async fn getNotifications(userId: int): Result<int, Error> {
    return await db.query("SELECT COUNT(*) FROM notifications WHERE user_id = ?", [userId]);
}

// Structured parallel execution:
async fn getUserDashboard(userId: int): Result<string, Error> {
    const [user, posts, notifCount] = await parallel {
        run getUser(userId);
        run getPosts(userId);
        run getNotifications(userId);
    };

    const name = user?;
    const postList = posts?;
    const count = notifCount?;

    return Ok(`${name}: ${postList.length} posts, ${count} notifications`);
}

// Parallel with atomics (safe shared counter):
async fn countParallel(): int {
    let total = atomic(0);

    await parallel {
        run { total.add(computePartA()); }
        run { total.add(computePartB()); }
        run { total.add(computePartC()); }
    }

    return total.load();
}

fn computePartA(): int { return 10; }
fn computePartB(): int { return 20; }
fn computePartC(): int { return 30; }

// This would be REJECTED by the compiler:
// async fn unsafeExample(): void {
//     let counter = 0;
//     await parallel {
//         run { counter += 1; }  // ERROR: mutable capture
//     }
// }

async fn main(): int {
    const dashboard = await getUserDashboard(1);
    match dashboard {
        is Ok(msg) => print(msg),
        is Err(e) => print(`Error: ${e.message}`),
    };

    const total = await countParallel();
    print(`Total: ${total}`);  // 60

    return 0;
}
```

- [ ] **Step 5: Write examples/10-channels.co**

```coco
// Channels in Coco

class Job {
    constructor(public id: int, public payload: string) {}
}

class JobResult {
    constructor(public jobId: int, public output: string) {}
}

async fn worker(id: int, jobs: Receiver<Job>, results: Sender<JobResult>): void {
    for job in jobs {
        print(`Worker ${id} processing job ${job.id}`);
        const output = `processed: ${job.payload}`;
        results.send(new JobResult(jobId: job.id, output: output));
    }
    print(`Worker ${id} done`);
}

async fn producer(jobs: Sender<Job>, count: int): void {
    for i in 0..count {
        jobs.send(new Job(id: i, payload: `task-${i}`));
    }
    jobs.close();
}

async fn main(): int {
    const jobs = chan<Job>(10);
    const results = chan<JobResult>(10);

    const numWorkers = 3;
    const numJobs = 10;

    // Spawn workers:
    await parallel {
        run producer(jobs, numJobs);

        run worker(1, jobs, results);
        run worker(2, jobs, results);
        run worker(3, jobs, results);
    }

    results.close();

    // Collect results:
    let processed = 0;
    for result in results {
        print(`Result: job ${result.jobId} -> ${result.output}`);
        processed += 1;
    }

    print(`Processed ${processed} jobs with ${numWorkers} workers`);

    return 0;
}
```

- [ ] **Step 6: Commit**

```bash
git add examples/06-error-handling.co examples/07-null-safety.co examples/08-async-basic.co examples/09-parallel.co examples/10-channels.co
git commit -m "Add example programs 06-10"
```

---

## Task 12: Example Programs (11-15)

**Files:**
- Create: `examples/11-http-server.co`
- Create: `examples/12-traits.co`
- Create: `examples/13-generics.co`
- Create: `examples/14-magic-methods.co`
- Create: `examples/15-match-expressions.co`

- [ ] **Step 1: Write examples/11-http-server.co**

```coco
// HTTP server in Coco

import { Server, Request, Response, Middleware } from "std/http";
import { db } from "./database";

class User {
    constructor(
        public readonly id: int,
        public name: string,
        public email: string,
    ) {}

    static fromRow(row): User {
        return new User(
            id: row["id"],
            name: row["name"],
            email: row["email"],
        );
    }
}

// Middleware:
fn logger(): Middleware {
    return (req, next) => {
        const start = DateTime.now();
        const response = next(req);
        const duration = DateTime.now() - start;
        print(`${req.method} ${req.path} ${response.status} ${duration}ms`);
        return response;
    };
}

async fn main(): int {
    const app = new Server();

    app.use(logger());

    app.get("/", (req) => {
        return Response.json({ "message": "Welcome to Coco API" });
    });

    app.get("/users", async (req) => {
        const users = await db.query("SELECT * FROM users")?;
        return Response.json(users);
    });

    app.get("/users/:id", async (req) => {
        const id = int.parse(req.params.id) ?: return Response.badRequest("invalid id");
        const row = await db.queryOne("SELECT * FROM users WHERE id = ?", [id])?;

        if row == null {
            return Response.notFound("user not found");
        }

        return Response.json(User.fromRow(row));
    });

    app.post("/users", async (req) => {
        const body = req.json()?;
        const name = body["name"] ?: return Response.badRequest("name required");
        const email = body["email"] ?: return Response.badRequest("email required");

        const id = await db.insert("INSERT INTO users (name, email) VALUES (?, ?)", [name, email])?;

        return Response.created(User.fromRow({ "id": id, "name": name, "email": email }));
    });

    print("Server listening on :3000");
    await app.listen(3000);

    return 0;
}
```

- [ ] **Step 2: Write examples/12-traits.co**

```coco
// Traits in Coco

trait Timestamps {
    createdAt: DateTime|null = null;
    updatedAt: DateTime|null = null;

    touch(): void {
        this.updatedAt = DateTime.now();
    }

    wasModified(): bool {
        return this.updatedAt != null;
    }
}

trait SoftDelete {
    deletedAt: DateTime|null = null;

    softDelete(): void {
        this.deletedAt = DateTime.now();
    }

    restore(): void {
        this.deletedAt = null;
    }

    isDeleted(): bool {
        return this.deletedAt != null;
    }
}

trait Sluggable {
    slug: string = "";

    generateSlug(source: string): void {
        this.slug = source.toLowerCase().replace(" ", "-").replace("[^a-z0-9-]", "");
    }
}

class Article {
    use Timestamps, SoftDelete, Sluggable;

    constructor(
        public readonly id: int,
        public title: string,
        public body: string,
    ) {
        this.generateSlug(title);
        this.createdAt = DateTime.now();
    }

    update(title: string, body: string): void {
        this.title = title;
        this.body = body;
        this.generateSlug(title);
        this.touch();
    }
}

fn main(): int {
    const article = new Article(
        id: 1,
        title: "Hello Coco World",
        body: "This is an article about Coco.",
    );

    print(`Slug: ${article.slug}`);         // "hello-coco-world"
    print(`Modified: ${article.wasModified()}`); // false

    article.update("Updated Title", "New body");
    print(`Modified: ${article.wasModified()}`); // true
    print(`New slug: ${article.slug}`);          // "updated-title"

    article.softDelete();
    print(`Deleted: ${article.isDeleted()}`);    // true

    article.restore();
    print(`Deleted: ${article.isDeleted()}`);    // false

    return 0;
}
```

- [ ] **Step 3: Write examples/13-generics.co**

```coco
// Generics in Coco

class Stack<T> {
    private items: list<T> = [];

    push(item: T): void {
        this.items.push(item);
    }

    pop(): T|null {
        return this.items.pop();
    }

    peek(): T|null {
        if this.items.isEmpty() { return null; }
        return this.items[this.items.length - 1];
    }

    size(): int {
        return this.items.length;
    }

    isEmpty(): bool {
        return this.items.length == 0;
    }
}

class Pair<A, B> {
    constructor(public first: A, public second: B) {}

    swap(): Pair<B, A> {
        return new Pair(first: this.second, second: this.first);
    }
}

// Generic functions:
fn identity<T>(value: T): T {
    return value;
}

fn max<T: Comparable>(a: T, b: T): T {
    return (a <=> b) >= 0 ? a : b;
}

fn filter<T>(items: list<T>, predicate: (T) => bool): list<T> {
    let result: list<T> = [];
    for item in items {
        if predicate(item) {
            result.push(item);
        }
    }
    return result;
}

fn map<T, U>(items: list<T>, transform: (T) => U): list<U> {
    let result: list<U> = [];
    for item in items {
        result.push(transform(item));
    }
    return result;
}

fn main(): int {
    // Stack usage:
    const stack = new Stack<int>();
    stack.push(1);
    stack.push(2);
    stack.push(3);
    print(`Top: ${stack.peek()}`);  // 3
    print(`Pop: ${stack.pop()}`);   // 3
    print(`Size: ${stack.size()}`); // 2

    // Pair:
    const pair = new Pair(first: "hello", second: 42);
    const swapped = pair.swap();
    print(`${swapped.first}, ${swapped.second}`); // 42, hello

    // Generic functions:
    const bigger = max(10, 20);
    print(`Max: ${bigger}`); // 20

    const names = ["Alice", "Bob", "Charlie", "Dave"];
    const long = filter(names, (n) => n.length > 3);
    print(`Long names: ${long}`); // ["Alice", "Charlie", "Dave"]

    const lengths = map(names, (n) => n.length);
    print(`Lengths: ${lengths}`); // [5, 3, 7, 4]

    return 0;
}
```

- [ ] **Step 4: Write examples/14-magic-methods.co**

```coco
// Magic methods in Coco

class Money {
    constructor(private cents: int) {}

    static php(amount: float): Money {
        return new Money(cents: (amount * 100) as int);
    }

    static fromCents(cents: int): Money {
        return new Money(cents: cents);
    }

    __toString(): string {
        const whole = this.cents / 100;
        const frac = (this.cents % 100).toString().padStart(2, "0");
        return `₱${whole}.${frac}`;
    }

    __compare(other: Money): int {
        return this.cents <=> other.cents;
    }

    add(other: Money): Money {
        return Money.fromCents(this.cents + other.cents);
    }

    subtract(other: Money): Money {
        return Money.fromCents(this.cents - other.cents);
    }
}

class DynamicConfig {
    private data: map<string, string> = {};

    __get(key: string): string|null {
        return this.data[key] ?? null;
    }

    __set(key: string, value: string): void {
        this.data[key] = value;
    }

    __invoke(key: string): bool {
        return this.data.has(key);
    }
}

class Handler {
    __call(method: string, args: list<mixed>): mixed {
        print(`Called: ${method} with ${args.length} args`);
        return null;
    }
}

fn main(): int {
    // __toString:
    const price = Money.php(49.99);
    print(`Price: ${price}`);           // "Price: ₱49.99"
    print("Total: " + price);           // "Total: ₱49.99"

    // __compare (spaceship):
    const cheap = Money.php(10.00);
    const expensive = Money.php(99.99);
    print(cheap <=> expensive);         // -1
    print(expensive <=> cheap);         // 1

    const total = cheap.add(expensive);
    print(`Total: ${total}`);           // "Total: ₱109.99"

    // __get / __set:
    const config = new DynamicConfig();
    config.host = "localhost";           // calls __set
    config.port = "8080";               // calls __set
    print(config.host);                  // calls __get, prints "localhost"

    // __invoke:
    print(config("host"));              // calls __invoke, prints true
    print(config("missing"));           // calls __invoke, prints false

    // __call:
    const handler = new Handler();
    handler.anyMethod("arg1", "arg2");  // "Called: anyMethod with 2 args"

    return 0;
}
```

- [ ] **Step 5: Write examples/15-match-expressions.co**

```coco
// Match expressions in Coco

enum Color {
    Red,
    Green,
    Blue,
    Custom(int, int, int),
}

enum Shape {
    Circle(float),
    Rectangle(float, float),
    Triangle(float, float, float),
    Point,
}

fn colorToHex(color: Color): string {
    return match color {
        Color.Red => "#FF0000",
        Color.Green => "#00FF00",
        Color.Blue => "#0000FF",
        Color.Custom(r, g, b) => `#${r.toHex()}${g.toHex()}${b.toHex()}`,
    };
}

fn area(shape: Shape): float {
    return match shape {
        Shape.Circle(r) => 3.14159 * r * r,
        Shape.Rectangle(w, h) => w * h,
        Shape.Triangle(a, b, c) => {
            const s = (a + b + c) / 2.0;
            return (s * (s - a) * (s - b) * (s - c)).sqrt();
        },
        Shape.Point => 0.0,
    };
}

fn describeNumber(n: int): string {
    return match n {
        0 => "zero",
        1 => "one",
        is int if n < 0 => "negative",
        is int if n > 100 => "large",
        _ => "some number",
    };
}

fn httpMessage(status: int): string {
    return match status {
        200 => "OK",
        201 => "Created",
        301 => "Moved Permanently",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => `Status ${status}`,
    };
}

fn main(): int {
    // Enum matching:
    print(colorToHex(Color.Red));                    // "#FF0000"
    print(colorToHex(Color.Custom(128, 64, 255)));   // "#8040FF"

    // Shape areas:
    print(area(Shape.Circle(5.0)));                  // 78.53975
    print(area(Shape.Rectangle(3.0, 4.0)));          // 12.0
    print(area(Shape.Point));                        // 0.0

    // Guard patterns:
    print(describeNumber(0));    // "zero"
    print(describeNumber(-5));   // "negative"
    print(describeNumber(200));  // "large"
    print(describeNumber(42));   // "some number"

    // Match as value:
    const message = httpMessage(404);
    print(message); // "Not Found"

    return 0;
}
```

- [ ] **Step 6: Commit**

```bash
git add examples/11-http-server.co examples/12-traits.co examples/13-generics.co examples/14-magic-methods.co examples/15-match-expressions.co
git commit -m "Add example programs 11-15"
```

---

## Task 13: Example Programs (16-20)

**Files:**
- Create: `examples/16-enums.co`
- Create: `examples/17-iterators.co`
- Create: `examples/18-modules.co`
- Create: `examples/19-cli-tool.co`
- Create: `examples/20-queue-worker.co`

- [ ] **Step 1: Write examples/16-enums.co**

```coco
// Enums in Coco

// Simple enum:
enum Direction {
    North,
    South,
    East,
    West,
}

// Backed enum (with values):
enum Planet: string {
    Mercury = "mercury",
    Venus = "venus",
    Earth = "earth",
    Mars = "mars",
}

// Enum with associated data:
enum Token {
    Number(float),
    String(string),
    Identifier(string),
    Operator(string),
    LeftParen,
    RightParen,
    Eof,
}

// Enum with methods:
enum Suit {
    Hearts,
    Diamonds,
    Clubs,
    Spades,

    fn isRed(): bool {
        return match this {
            Suit.Hearts => true,
            Suit.Diamonds => true,
            _ => false,
        };
    }

    fn symbol(): string {
        return match this {
            Suit.Hearts => "♥",
            Suit.Diamonds => "♦",
            Suit.Clubs => "♣",
            Suit.Spades => "♠",
        };
    }
}

fn navigate(dir: Direction, x: int, y: int): tuple<int, int> {
    return match dir {
        Direction.North => (x, y + 1),
        Direction.South => (x, y - 1),
        Direction.East => (x + 1, y),
        Direction.West => (x - 1, y),
    };
}

fn tokenize(input: string): list<Token> {
    let tokens: list<Token> = [];
    // Simplified tokenizer:
    for char in input {
        match char {
            '(' => tokens.push(Token.LeftParen),
            ')' => tokens.push(Token.RightParen),
            '+' => tokens.push(Token.Operator("+")),
            '-' => tokens.push(Token.Operator("-")),
            _ => {},
        };
    }
    tokens.push(Token.Eof);
    return tokens;
}

fn main(): int {
    // Direction:
    const pos = navigate(Direction.North, 0, 0);
    print(`Position: ${pos}`); // (0, 1)

    // Backed enum:
    print(`Planet: ${Planet.Earth}`); // "earth"

    // Enum methods:
    const suit = Suit.Hearts;
    print(`${suit.symbol()} red: ${suit.isRed()}`); // "♥ red: true"

    // Tokens:
    const tokens = tokenize("(a + b)");
    for token in tokens {
        match token {
            Token.LeftParen => print("("),
            Token.RightParen => print(")"),
            Token.Operator(op) => print(`op: ${op}`),
            Token.Eof => print("EOF"),
            _ => {},
        };
    }

    return 0;
}
```

- [ ] **Step 2: Write examples/17-iterators.co**

```coco
// Iterators in Coco

interface Iterator<T> {
    next(): T|null;
    hasNext(): bool;
}

interface Iterable<T> {
    iterator(): Iterator<T>;
}

class RangeIterator implements Iterator<int> {
    private current: int;

    constructor(private start: int, private end: int, private step: int = 1) {
        this.current = start;
    }

    next(): int|null {
        if this.current >= this.end { return null; }
        const value = this.current;
        this.current += this.step;
        return value;
    }

    hasNext(): bool {
        return this.current < this.end;
    }
}

class Range implements Iterable<int> {
    constructor(
        private start: int,
        private end: int,
        private step: int = 1,
    ) {}

    iterator(): Iterator<int> {
        return new RangeIterator(this.start, this.end, this.step);
    }
}

// Fibonacci iterator:
class Fibonacci implements Iterable<int> {
    constructor(private limit: int) {}

    iterator(): Iterator<int> {
        return new FibIterator(this.limit);
    }
}

class FibIterator implements Iterator<int> {
    private a: int = 0;
    private b: int = 1;
    private count: int = 0;

    constructor(private limit: int) {}

    next(): int|null {
        if this.count >= this.limit { return null; }
        const value = this.a;
        const temp = this.b;
        this.b = this.a + this.b;
        this.a = temp;
        this.count += 1;
        return value;
    }

    hasNext(): bool {
        return this.count < this.limit;
    }
}

fn main(): int {
    // Range iteration:
    const range = new Range(0, 10, 2);
    for n in range {
        print(`${n}`); // 0, 2, 4, 6, 8
    }

    // Fibonacci:
    const fib = new Fibonacci(10);
    let fibNumbers: list<int> = [];
    for n in fib {
        fibNumbers.push(n);
    }
    print(`Fib: ${fibNumbers}`); // [0, 1, 1, 2, 3, 5, 8, 13, 21, 34]

    // Built-in list iteration with chaining:
    const numbers = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    const result = numbers
        .filter((n) => n % 2 == 0)
        .map((n) => n * n)
        .filter((n) => n > 10);

    print(`Result: ${result}`); // [16, 36, 64, 100]

    // for...in with index:
    const fruits = ["apple", "banana", "cherry"];
    for (index, fruit) in fruits.entries() {
        print(`${index}: ${fruit}`);
    }

    return 0;
}
```

- [ ] **Step 3: Write examples/18-modules.co**

```coco
// Module system in Coco

// --- File: src/models/user.co ---

export class User {
    constructor(
        public readonly id: int,
        public name: string,
        public email: string,
    ) {}

    fn displayName(): string {
        return this.name;
    }
}

export type UserId = int;

export fn createUser(name: string, email: string): User {
    return new User(id: generateId(), name: name, email: email);
}

fn generateId(): int {
    return Math.randomInt(1, 999999);
}

// --- File: src/services/user-service.co ---

import { User, UserId, createUser } from "../models/user";
import { db } from "std/db";
import * as validate from "../utils/validate";

export class UserService {
    async fn find(id: UserId): Result<User|null, DbError> {
        const row = await db.queryOne("SELECT * FROM users WHERE id = ?", [id])?;
        if row == null { return Ok(null); }
        return Ok(new User(id: row["id"], name: row["name"], email: row["email"]));
    }

    async fn create(name: string, email: string): Result<User, ValidationError> {
        validate.notEmpty(name, "name")?;
        validate.email(email)?;

        const user = createUser(name, email);
        await db.insert("INSERT INTO users (id, name, email) VALUES (?, ?, ?)",
            [user.id, user.name, user.email])?;

        return Ok(user);
    }

    async fn list(limit: int = 50, offset: int = 0): Result<list<User>, DbError> {
        const rows = await db.query(
            "SELECT * FROM users LIMIT ? OFFSET ?",
            [limit, offset],
        )?;

        return Ok(rows.map((row) => new User(
            id: row["id"],
            name: row["name"],
            email: row["email"],
        )));
    }
}

// --- File: src/main.co ---

import { UserService } from "./services/user-service";
import { Server } from "std/http";

async fn main(): int {
    const users = new UserService();
    const app = new Server();

    app.get("/users", async (req) => {
        const list = await users.list()?;
        return Response.json(list);
    });

    await app.listen(3000);
    return 0;
}
```

- [ ] **Step 4: Write examples/19-cli-tool.co**

```coco
// CLI tool in Coco

import { args, exit } from "std/process";
import { readFile, writeFile, exists } from "std/fs";
import { Path } from "std/path";

enum Command {
    Init(string),
    Add(string, string),
    List,
    Remove(string),
    Help,
}

fn parseArgs(argv: list<string>): Result<Command, string> {
    if argv.length < 2 {
        return Ok(Command.Help);
    }

    return match argv[1] {
        "init" => {
            const name = argv[2] ?? "project";
            Ok(Command.Init(name));
        },
        "add" => {
            if argv.length < 4 {
                return Err("usage: add <key> <value>");
            }
            Ok(Command.Add(argv[2], argv[3]));
        },
        "list" => Ok(Command.List),
        "remove" => {
            if argv.length < 3 {
                return Err("usage: remove <key>");
            }
            Ok(Command.Remove(argv[2]));
        },
        "help" => Ok(Command.Help),
        _ => Err(`unknown command: ${argv[1]}`),
    };
}

fn printHelp(): void {
    print("coco-kv — a simple key-value store CLI");
    print("");
    print("Commands:");
    print("  init [name]     Initialize a new store");
    print("  add <k> <v>     Add a key-value pair");
    print("  list            List all entries");
    print("  remove <key>    Remove an entry");
    print("  help            Show this message");
}

const STORE_FILE = ".kv-store.json";

fn loadStore(): Result<map<string, string>, string> {
    if !exists(STORE_FILE) {
        return Err("store not initialized. Run 'init' first.");
    }
    const content = readFile(STORE_FILE)?;
    return Ok(JSON.parse(content));
}

fn saveStore(store: map<string, string>): Result<void, string> {
    return writeFile(STORE_FILE, JSON.stringify(store, indent: 2));
}

fn main(): int {
    const cmd = parseArgs(args());
    if cmd is Err(msg) {
        print(`Error: ${msg}`);
        return 1;
    }

    match cmd.unwrap() {
        Command.Init(name) => {
            if exists(STORE_FILE) {
                print("Store already exists.");
                return 1;
            }
            saveStore({}) ?: {
                print("Failed to create store");
                return 1;
            };
            print(`Initialized store '${name}'`);
        },
        Command.Add(key, value) => {
            let store = loadStore() ?: { print("Error loading store"); return 1; };
            store[key] = value;
            saveStore(store)?;
            print(`Set ${key} = ${value}`);
        },
        Command.List => {
            const store = loadStore() ?: { print("Error loading store"); return 1; };
            if store.isEmpty() {
                print("(empty)");
            } else {
                for (key, value) in store {
                    print(`  ${key} = ${value}`);
                }
            }
        },
        Command.Remove(key) => {
            let store = loadStore() ?: { print("Error loading store"); return 1; };
            if !store.has(key) {
                print(`Key '${key}' not found`);
                return 1;
            }
            store.remove(key);
            saveStore(store)?;
            print(`Removed '${key}'`);
        },
        Command.Help => printHelp(),
    };

    return 0;
}
```

- [ ] **Step 5: Write examples/20-queue-worker.co**

```coco
// Queue worker in Coco

import { db } from "std/db";
import { sleep } from "std/time";
import { Context } from "std/context";
import { signal } from "std/process";

enum JobStatus: string {
    Pending = "pending",
    Processing = "processing",
    Completed = "completed",
    Failed = "failed",
}

class Job {
    constructor(
        public readonly id: int,
        public type: string,
        public payload: string,
        public status: JobStatus = JobStatus.Pending,
        public attempts: int = 0,
        public maxAttempts: int = 3,
        public error: string|null = null,
    ) {}

    static fromRow(row): Job {
        return new Job(
            id: row["id"],
            type: row["type"],
            payload: row["payload"],
            status: JobStatus.from(row["status"]),
            attempts: row["attempts"],
            maxAttempts: row["max_attempts"],
            error: row["error"],
        );
    }
}

class Worker {
    constructor(
        private id: int,
        private pollInterval: int = 1000,
    ) {}

    async fn run(ctx: Context): void {
        print(`Worker ${this.id} started`);

        loop {
            select {
                case _ = ctx.cancelled():
                    print(`Worker ${this.id} shutting down`);
                    return;
                case _ = sleep(this.pollInterval):
                    await this.poll();
            }
        }
    }

    private async fn poll(): void {
        const job = await this.claimJob();
        if job == null { return; }

        print(`Worker ${this.id}: processing job ${job.id} (${job.type})`);

        const result = await this.process(job);
        match result {
            is Ok(_) => {
                await this.markCompleted(job);
                print(`Worker ${this.id}: job ${job.id} completed`);
            },
            is Err(e) => {
                await this.markFailed(job, e);
                print(`Worker ${this.id}: job ${job.id} failed: ${e}`);
            },
        };
    }

    private async fn claimJob(): Job|null {
        const row = await db.queryOne(
            "UPDATE jobs SET status = ?, attempts = attempts + 1 WHERE status = ? LIMIT 1 RETURNING *",
            [JobStatus.Processing, JobStatus.Pending],
        )?;
        if row == null { return null; }
        return Job.fromRow(row);
    }

    private async fn process(job: Job): Result<void, string> {
        return match job.type {
            "email" => await this.sendEmail(job.payload),
            "report" => await this.generateReport(job.payload),
            "cleanup" => await this.runCleanup(job.payload),
            _ => Err(`unknown job type: ${job.type}`),
        };
    }

    private async fn sendEmail(payload: string): Result<void, string> {
        await sleep(100); // simulate work
        return Ok();
    }

    private async fn generateReport(payload: string): Result<void, string> {
        await sleep(500); // simulate work
        return Ok();
    }

    private async fn runCleanup(payload: string): Result<void, string> {
        await sleep(200); // simulate work
        return Ok();
    }

    private async fn markCompleted(job: Job): void {
        await db.execute(
            "UPDATE jobs SET status = ? WHERE id = ?",
            [JobStatus.Completed, job.id],
        );
    }

    private async fn markFailed(job: Job, error: string): void {
        const newStatus = job.attempts >= job.maxAttempts
            ? JobStatus.Failed
            : JobStatus.Pending;

        await db.execute(
            "UPDATE jobs SET status = ?, error = ? WHERE id = ?",
            [newStatus, error, job.id],
        );
    }
}

async fn main(): int {
    const ctx = Context.withCancel();
    const numWorkers = 4;

    // Graceful shutdown on SIGINT/SIGTERM:
    signal.on("SIGINT", () => {
        print("\nShutting down gracefully...");
        ctx.cancel();
    });

    print(`Starting ${numWorkers} workers...`);

    await parallel {
        run new Worker(1).run(ctx);
        run new Worker(2).run(ctx);
        run new Worker(3).run(ctx);
        run new Worker(4).run(ctx);
    }

    print("All workers stopped. Goodbye.");
    return 0;
}
```

- [ ] **Step 6: Commit**

```bash
git add examples/16-enums.co examples/17-iterators.co examples/18-modules.co examples/19-cli-tool.co examples/20-queue-worker.co
git commit -m "Add example programs 16-20"
```

---

## Task 14: Final Review and Validation

**Files:**
- All files from Tasks 1-13

- [ ] **Step 1: Cross-reference grammar against examples**

Verify each example program uses only syntax defined in `docs/grammar.ebnf`:
- Check all keywords used in examples appear in reserved words list
- Check all operator precedences match grammar rules
- Check class/trait/enum/interface syntax matches productions
- Check import/export syntax matches grammar

- [ ] **Step 2: Cross-reference decisions against reference**

Verify `docs/language-reference.md` is consistent with all 15 ADRs:
- Method syntax allows both forms (ADR-001)
- String concat rules match (ADR-002)
- Result is used without import (ADR-003)
- Parallel blocks reject mutable captures (ADR-004)
- Magic methods shown (ADR-007)
- Traits have state (ADR-008)
- Both exceptions and Result shown (ADR-009)
- Eager async with lazy keyword (ADR-010)
- Gradual typing demonstrated (ADR-013)

- [ ] **Step 3: Verify completeness against spec**

Check deliverables against `docs/superpowers/specs/2026-06-03-coco-phase-0-1-design.md`:
- [ ] charter.md exists and covers identity/goals/non-goals
- [ ] safety-promise.md covers all 10 guarantees
- [ ] concurrency.md covers all primitives
- [ ] type-system.md covers gradual typing + all types
- [ ] grammar.ebnf covers all listed constructs
- [ ] language-reference.md is readable in 30 minutes
- [ ] 15 ADRs exist with context/decision/consequences
- [ ] 20 example programs exist and demonstrate different features

- [ ] **Step 4: Final commit if any fixes were needed**

```bash
git add -A
git commit -m "Fix review findings in Phase 0-1 docs"
```
