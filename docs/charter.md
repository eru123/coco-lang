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
- No `$` variables (regular variables use `name`, not `$name`)
- Dot notation for member access (not `->`)
- `this` or `$` for instance member access (both equivalent, `$` is shorthand)
- Multiple function keywords: `function`, `fn`, `f`, or arrow syntax `() => {}`
- ES-style imports
- Template literals
- Constructor property promotion

PHP influence appears in features, not syntax:
- Named arguments
- Traits with state
- Magic methods
- Match expressions
- Enums
- `$` as shorthand for `this` in classes
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
