# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is Coco?

Coco is a planned compiled, memory-safe programming language for backend services, CLI tools, and automation. It targets JavaScript/TypeScript/PHP developers who want memory safety without Rust's learning curve.

**Current status:** Design and specification phase. No implementation yet — this repository contains language design documents, specifications, examples, and editor support.

**Project philosophy:**
- JavaScript-like syntax (not PHP `$variables` syntax)
- Automatic memory safety (no manual ownership/borrowing annotations)
- Safe multi-core concurrency (compile-time race prevention)
- Gradual typing (types are optional and additive)
- Trust the runtime (automatic memory management, no GC tuning knobs)

## Repository Structure

```
/docs/                      Language design documents
  charter.md                Language identity and core principles
  language-reference.md     Complete syntax reference (read this first)
  type-system.md            Gradual typing, primitives, generics, unions
  concurrency.md            Async, parallel, channels, race prevention
  safety-promise.md         Memory safety guarantees and enforcement
  /decisions/               Architecture Decision Records (ADRs)
    001-*.md through 015-*.md

/examples/                  20 example .co files showing syntax
  01-hello.co               Hello world
  11-http-server.co         Backend server example
  19-cli-tool.co            CLI application
  20-queue-worker.co        Worker service

/editors/vscode/            VS Code extension (syntax highlighting, snippets)
/logo/                      Coco language logo assets
```

## Key Language Features

### Classes and Visibility
- **Visibility modifiers:** `public`, `protected`, `private`, `readonly`, `static`
- **Property defaults:** properties are `private` by default, methods are `public` by default
- **Instance access:** `this.property` or `$.property` (both equivalent, `$` is shorthand)
- **Constructor parameter properties:** modifiers in constructor create properties automatically

### Syntax Style
- **JavaScript-like:** `const`, `let`, arrow functions, template literals, dot notation
- **Function keywords:** `function`, `fn`, `f`, or arrow syntax `() => {}`
- **Instance member access:** `this.property` or `$.property` (both equivalent)
- **PHP-inspired features:** named arguments, traits with state, magic methods, `$` shorthand
- **No dollar signs for variables:** variables are `name`, not `$name` (but `$` is used as shorthand for `this` in classes)

### Type System
- Gradual typing — types optional, can mix typed/untyped in same file
- Non-null by default: `User|null` for nullable
- Result type for errors: `Result<T, E>`, propagate with `?`
- Primitives: `int`, `uint`, `float`, `bool`, `string`, `char`, `byte`, `null`, `void`, `never`, `mixed`

### Memory Safety (Automatic)
No manual ownership/borrowing syntax. Compiler and runtime enforce:
- Lifetime analysis (compile-time)
- Escape analysis
- Automatic memory management
- Copy-on-write collections
- Bounds checking
- Cycle collection
- Null safety via type narrowing

### Concurrency Safety
Strict compile-time enforcement:
```coco
let counter = 0;
await parallel {
    run { counter += 1; }  // COMPILE ERROR: mutable capture across parallel boundary
}
```

Safe alternatives: atomics, channels, or collect results from `parallel { run ... }`

Immutable sharing always safe:
```coco
const config = loadConfig();
await parallel {
    run { useConfig(config); }  // OK: config is immutable
}
```

### Error Handling Split
- **Result type** for expected failures: parsing, I/O, validation
- **Exceptions** for unexpected failures: bugs, invariant violations

## Documentation Guidelines

When working on language design or specification:

1. **Read first:** `docs/language-reference.md` is the 30-minute syntax overview
2. **Check ADRs:** `docs/decisions/` contains ratified design decisions
3. **Syntax consistency:** Follow JavaScript/TypeScript style, not PHP `$variable` style
4. **Safety first:** Any feature must respect the safety promise (docs/safety-promise.md)
5. **Gradual typing:** Types must be optional; never force type annotations

## Planned Roadmap

Current phase: **Phase 0-1** (design/specification)

Upcoming phases:
- Phase 2: Lexer, parser, formatter
- Phase 3: Tree-walking interpreter MVP
- Phase 4: Type checker
- Phase 5: Automatic memory safety analyzer
- Phase 6: Runtime memory system
- Phase 7: Bytecode VM
- Phase 8: Concurrency safety
- Phase 9: Async runtime
- Phase 10: Standard library
- Phase 11: Native AOT compiler

Implementation language: **Rust** (compiler, runtime, tooling)

## Example Code Patterns

### Basic HTTP Server
```coco
import { Server, Response } from "std/http";

async fn main(): int {
    const app = new Server();
    
    app.get("/users/:id", async (req) => {
        const id = UserId.parse(req.params.id)?;
        const user = await db.users.find(id)?;
        return Response.json(user);
    });
    
    await app.listen(3000);
    return 0;
}
```

### Safe Parallel Execution
```coco
const [user, posts, comments] = await parallel {
    run getUser(id);
    run getPosts(id);
    run getComments(id);
};
```

### Result Type Error Handling
```coco
fn processForm(data: FormData): Result<User, FormError> {
    const age = parseAge(data.get("age"))?;  // propagate error
    const name = data.get("name") ?: return Err(new FormError("name required"));
    return Ok(new User(name: name, age: age));
}
```

## Non-Goals for v1

Explicitly out of scope:
- Browser/frontend target
- Bare-metal/embedded systems
- PHP syntax compatibility mode
- Manual memory management as default
- JIT compilation
- WebAssembly target
- Self-hosting compiler
- Package registry (comes post-v1)
