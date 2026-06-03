# Coco Language — Phase 0-1 Design Spec

> Date: 2026-06-03
> Scope: Charter, grammar specification, decision records, examples
> Status: Approved

---

## 1. Overview

Phase 0-1 establishes Coco's identity, resolves all open design questions, produces a formal EBNF grammar, and validates the grammar with 20+ example programs. No implementation code is written in this phase.

---

## 2. Language Identity

**What Coco is:** A compiled, memory-safe language for backend applications. JS-like syntax, PHP-inspired ergonomics, automatic memory safety, safe real concurrency, static binary deployment.

**Target users:** Backend developers from JS/TS/PHP backgrounds who want safety and deployment simplicity without Rust's learning curve.

**Target workloads:**
- HTTP APIs
- CLI tools
- Queue workers
- Automation scripts
- Microservices
- Long-running services
- Internal tools
- Control-plane tools

**Implementation language:** Rust (compiler, runtime, tooling).

---

## 3. Non-Goals for v1

- Browser/frontend target
- Bare-metal/embedded
- PHP syntax compatibility mode
- Manual memory management as default
- JIT compilation
- WASM target
- Self-hosting compiler
- Plugin/macro system
- Package registry
- Complex macros
- Advanced generics (HKT)
- ORM
- HMR

---

## 4. Core Principles

1. Simple surface, strict underneath
2. Compiler handles safety — developer writes clean code
3. Gradual typing — types optional but encouraged
4. Full runtime magic — developer trusts runtime for memory strategy
5. Strict parallelism safety — no shared mutable state across tasks
6. Static binary output (eventually)

---

## 5. Resolved Design Decisions

### Decision 1: Class Method Syntax

**Both** `method(): T { }` and `fn method(): T { }` are valid inside classes. Developers choose style.

```coco
class User {
    getName(): string { return this.name; }
    fn getEmail(): string { return this.email; }
}
```

### Decision 2: String Concatenation

`+` concatenates when at least one operand is a string. Non-string operands get type-descriptive stringification:
- Primitives render their value: `"hello " + 1` → `"hello 1"`
- Booleans render type tag: `"hello " + false` → `"hello [bool]"`
- Objects render type tag: `"hello " + user` → `"hello [User]"`

No compile error. JS-like ergonomics.

```coco
const msg = "count: " + 42;       // "count: 42"
const bad = "flag: " + true;      // "flag: [bool]"
const obj = "user: " + user;      // "user: [User]"
```

### Decision 3: Result Type

`Result<T, E>` is a language-level builtin. No import needed. `?` operator is compiler-intrinsic.

```coco
fn parse(s: string): Result<int, ParseError> { ... }
const val = parse(input)?;
```

### Decision 4: Race Detection Strictness

**Strict.** All cross-task mutable captures rejected at compile time. Must use atomics, channels, or synchronized blocks.

```coco
// REJECTED:
let x = 0;
await parallel { run { x += 1; } }

// REQUIRED:
let x = atomic(0);
await parallel { run { x.add(1); } }
```

### Decision 5: Cycle Collection

Ships in v1. Runtime includes production cycle collector. Tree/graph structures with parent references do not leak.

### Decision 6: Unsafe Dependencies

Blocked by default in `application` safety mode. Must explicitly allowlist in `coco.toml`.

```toml
[safety]
mode = "application"
allow_unsafe_dependencies = false

[safety.allow]
coco-ffi-png = "audited"
```

### Decision 7: Magic Methods

PHP-style magic methods (`__toString`, `__get`, `__set`, `__call`, `__invoke`, etc.) are first-class in normal Coco. No Symbol ceremony.

```coco
class Money {
    __toString(): string {
        return `$${this.cents / 100}`;
    }
}
```

### Decision 8: Trait State

Traits can hold both methods and properties with defaults.

```coco
trait Timestamps {
    createdAt: DateTime|null = null;
    updatedAt: DateTime|null = null;

    touch(): void {
        this.updatedAt = DateTime.now();
    }
}
```

### Decision 9: Error Model

Both exceptions and Result coexist with clear split:
- **Exceptions:** unexpected/fatal errors (bugs, OOM, assertion failures)
- **Result:** expected failures (parse, I/O, validation, database, network)

Stdlib uses both appropriately.

### Decision 10: Async Execution

Eager by default. `lazy` keyword available for deferred execution.

```coco
const p = fetchUser(1);          // starts immediately
const task = lazy fetchUser(2);  // cold until awaited
const user = await task;         // NOW runs
```

### Decision 11: Coroutine Scoping

Both scoped and unscoped `coro` allowed. Unscoped gets extra compiler scrutiny — warnings, lifetime analysis, leak detection in debug mode.

```coco
// Structured (preferred):
await parallel { run { doWork(); } }

// Unscoped (allowed, with scrutiny):
coro { backgroundCleanup(); }
```

### Decision 12: First Backend

Tree-walking interpreter first. Fastest iteration on language semantics. VM and native backend come in later phases.

### Decision 13: Type System

Gradual typing. Types are optional and can be added progressively. Untyped and typed code can mix in the same file.

```coco
fn add(a, b) { return a + b; }              // untyped
fn addTyped(a: int, b: int): int { return a + b; }  // typed
```

### Decision 14: PHP Compatibility in v1

None. v1 is pure Coco. PHP migration tools are post-v1.

### Decision 15: Runtime Magic Level

Full magic. Runtime chooses optimal memory strategy automatically (generational GC, concurrent marking, etc.). No developer control needed. Simpler code, less latency control.

---

## 6. Grammar Scope

### 6.1 Declarations

- `let`, `const` bindings (typed and untyped)
- `fn` named functions
- Arrow functions / closures
- `class` with constructor property promotion
- `trait` with state
- `interface`
- `enum`
- `import` / `export` (ES-style)

### 6.2 Types (optional in gradual mode)

- Primitives: `int`, `uint`, `float`, `bool`, `string`, `char`, `byte`, `null`, `void`, `never`, `mixed`
- Compounds: `list<T>`, `map<K,V>`, `tuple<...>`, union `A|B`, intersection `A & B`
- Nullable: `T|null`
- Generics: `<T>`, `<T, E>`
- `Result<T, E>` builtin

### 6.3 Expressions

- Arithmetic: `+`, `-`, `*`, `/`, `%`, `**`
- Comparison: `==`, `!=`, `<`, `>`, `<=`, `>=`
- Logical: `&&`, `||`, `!`
- Bitwise: `&`, `|`, `^`, `~`, `<<`, `>>`
- Assignment: `=`, `+=`, `-=`, `*=`, `/=`, `%=`, `**=`
- String concatenation via `+` (one operand string → concat)
- Template literals: `` `Hello, ${name}` ``
- Optional chaining: `?.`
- Null coalescing: `??`
- Elvis operator: `?:` (truthy coalescing)
- Non-null assertion: `!`
- Spaceship operator: `<=>` (three-way comparison, returns -1/0/1)
- Result propagation: `?`
- `match` expressions
- `new` for class instantiation
- Named arguments
- Object/list/map literals
- `async` / `await`
- `lazy` prefix for deferred async

### 6.4 Statements

- `if` / `else if` / `else`
- `for` / `for...in` / `for...of`
- `while` / `do...while`
- `loop`
- `break` / `continue` / `return`
- `throw` / `try` / `catch` / `finally`
- `await`, `async fn`
- `parallel { run ... }`
- `coro { ... }`
- `select { case ... }`
- `unsafe { ... }`
- `synchronized { ... }`

### 6.5 Class Features

- Constructor with property promotion (`public`, `private`, `protected`, `readonly`)
- Methods: both `method(): T { }` and `fn method(): T { }` syntax
- Static methods/properties
- Magic methods: `__toString`, `__get`, `__set`, `__call`, `__invoke`, `__compare`
- `use Trait`
- `implements Interface`
- `extends` (single inheritance)

### 6.6 Module System

```coco
import { Server } from "std/http";
import { User } from "./models/user";
import * as crypto from "std/crypto";
export class MyService { ... }
export fn helper(): void { ... }
```

---

## 7. Deliverables

```
coco-lang/
├── docs/
│   ├── charter.md
│   ├── safety-promise.md
│   ├── concurrency.md
│   ├── type-system.md
│   ├── grammar.ebnf
│   ├── language-reference.md
│   └── decisions/
│       ├── 001-method-syntax.md
│       ├── 002-string-concatenation.md
│       ├── 003-result-type.md
│       ├── 004-race-detection.md
│       ├── 005-cycle-collection.md
│       ├── 006-unsafe-dependencies.md
│       ├── 007-magic-methods.md
│       ├── 008-trait-state.md
│       ├── 009-error-model.md
│       ├── 010-async-execution.md
│       ├── 011-coroutine-scoping.md
│       ├── 012-first-backend.md
│       ├── 013-gradual-typing.md
│       ├── 014-php-compat-v1.md
│       └── 015-runtime-magic.md
├── examples/
│   ├── 01-hello.co
│   ├── 02-variables.co
│   ├── 03-functions.co
│   ├── 04-classes.co
│   ├── 05-collections.co
│   ├── 06-error-handling.co
│   ├── 07-null-safety.co
│   ├── 08-async-basic.co
│   ├── 09-parallel.co
│   ├── 10-channels.co
│   ├── 11-http-server.co
│   ├── 12-traits.co
│   ├── 13-generics.co
│   ├── 14-magic-methods.co
│   ├── 15-match-expressions.co
│   ├── 16-enums.co
│   ├── 17-iterators.co
│   ├── 18-modules.co
│   ├── 19-cli-tool.co
│   └── 20-queue-worker.co
└── README.md
```

---

## 8. Exit Criteria

Phase 0-1 is complete when:

1. Charter locks identity — no unresolved philosophical conflicts
2. All 15 decisions documented as ADRs with rationale
3. Safety promise defines exactly what safe Coco prevents
4. EBNF grammar has no known ambiguity in defined scope
5. 20 example programs parse correctly against grammar (manual verification)
6. Language reference readable by a JS/PHP dev in 30 minutes
7. No implementation code written — Phase 2 starts parser

---

## 9. Grammar Non-Scope

The Phase 1 grammar does NOT cover:
- PHP compatibility syntax
- Macro system
- Higher-kinded types or advanced generics
- WASM/embedded-specific constructs
- Decorator/annotation syntax (deferred)
- Pattern matching beyond `match` expressions

---

## 10. Operator Precedence (Draft)

From lowest to highest:

1. `=`, `+=`, `-=`, etc. (assignment)
2. `?:` (elvis)
3. `??` (null coalescing)
4. `||` (logical or)
5. `&&` (logical and)
6. `|` (bitwise or)
7. `^` (bitwise xor)
8. `&` (bitwise and)
9. `==`, `!=` (equality)
10. `<`, `>`, `<=`, `>=`, `<=>` (comparison/spaceship)
11. `<<`, `>>` (shift)
12. `+`, `-` (additive / string concat)
13. `*`, `/`, `%` (multiplicative)
14. `**` (exponentiation, right-associative)
15. `!`, `~`, `-` (unary prefix)
16. `?.`, `!`, `?` (postfix: optional chain, non-null assert, propagation)
17. `.`, `()`, `[]` (member access, call, index)

---

## 11. String Concatenation Rules

| Left | Right | Result |
|------|-------|--------|
| string | string | concatenation |
| string | int/uint/float | string + value repr |
| string | bool | string + `[bool]` |
| string | null | string + `[null]` |
| string | object | string + `[ClassName]` |
| int | string | value repr + string |
| non-string | non-string | compile error (+ is arithmetic) |

The `+` operator is overloaded: arithmetic when both numeric, concatenation when at least one string.

---

## 12. Async/Concurrency Model Summary

- `async fn` declares async function
- Calling async fn starts execution immediately (eager)
- `lazy asyncFn()` defers execution until awaited
- `await` collects async result
- `parallel { run ... }` structured multi-task execution
- `coro { ... }` spawns coroutine (scoped preferred, unscoped allowed with scrutiny)
- `select { case ... }` multiplexes channels/events
- `synchronized { ... }` ensures mutual exclusion
- `atomic(value)` provides atomic operations
- `chan<T>(capacity)` creates typed channel
- Cross-task mutable capture is a compile error

---

## 13. Memory Safety Model Summary

Developer-facing guarantees (in safe code):
- No use-after-free
- No dangling references
- No null pointer dereference (without assertion)
- No buffer overflow
- No uninitialized memory reads
- No accidental data races
- No unsafe raw pointer access
- No iterator invalidation
- No mutation of shared state without synchronization
- No undefined behavior

Implementation strategy:
- Full runtime magic (generational GC, concurrent marking, etc.)
- Cycle collector in production
- Copy-on-write for collections and strings
- Escape analysis for allocation decisions
- Stack allocation where possible
- Compiler lifetime analysis (invisible to developer)
- `unsafe` blocks for FFI/systems work, isolated and reported
