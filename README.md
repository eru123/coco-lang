# Coco

> JavaScript feel. PHP productivity. Memory-safe by default. Built for modern backends.

Coco is a planned compiled, memory-safe programming language for backend services, CLI tools, automation, and long-running worker applications.

It is designed to feel familiar to JavaScript and TypeScript developers while preserving the practical backend productivity PHP is known for. Coco aims to provide strict typing, automatic memory safety, safe multi-core concurrency, secure web defaults, and static binary deployment without forcing developers to manage low-level memory details manually.

Coco is currently in the planning and specification stage. This repository is expected to evolve from language design, to parser, to interpreter, to compiler, to runtime. In other words, do not deploy it to production unless your idea of production is “a document with ambition.”

## Project Status

Coco is not yet a stable language.

Current stage:

```txt
Phase 3 — Tree-walking interpreter (functional)
```

Planned stages:

```txt
language design
  → lexer and parser
  → formatter
  → interpreter
  → type checker
  → automatic memory safety analyzer
  → runtime
  → async/concurrency system
  → native compiler
  → package manager and tooling
```

## Why Coco?

Modern backend developers often have to choose between JavaScript/TypeScript, PHP, Go, Rust, and lower-level systems languages.

Coco aims for a different balance:

```txt
JavaScript-like syntax
PHP-like backend ergonomics
automatic memory safety
safe real concurrency
strict typing
single-binary deployment
```

Coco should let developers write code that feels simple while the compiler and runtime handle the difficult safety work in the background.

## Example

```coco
import { Server, Response } from "std/http";
import { db } from "./database";

class User {
    constructor(
        public readonly id: UserId,
        public name: string,
        private passwordHash: string,
    ) {}

    verify(password: string): bool {
        return checkHash(password, $.passwordHash);
    }

    static fromRow(row: DbRow): User {
        return new User(
            id: UserId(row["id"]),
            name: row["name"],
            passwordHash: row["password_hash"],
        );
    }
}

async fn main(): int {
    const app = new Server();

    app.get("/users/:id", async (req) => {
        const id = UserId.parse(req.params.id)?;
        const user = await db.users.find(id)?;

        return Response.json(user);
    });

    // Data transformation with pipe operators:
    app.get("/users", async (req) => {
        const activeUsers = await db.users.findAll()
            |> $$.filter(u => u.active)
            |> $$.map(u => ({ id: u.id, name: u.name }));

        return Response.json(activeUsers);
    });

    await app.listen(3000);
    return 0;
}
```

## Core Goals

1. JavaScript-like surface syntax.
2. PHP-inspired backend productivity.
3. Automatic memory safety.
4. Safe multi-core concurrency.
5. Static binary deployment.
6. Secure defaults.
7. Excellent developer experience.

## Syntax Philosophy

Coco should feel closer to JavaScript and TypeScript than PHP.

```coco
const appName = "Coco";
let counter = 0;

counter += 1;
```

Coco does not use PHP-style `$variables` in normal code.

```coco
let $counter = 0; // invalid in normal Coco
```

Functions can be defined with `function`, `fn`, `f`, or arrow syntax:

```coco
// Named functions:
function add(a: int, b: int): int { return a + b; }
fn subtract(a: int, b: int): int { return a - b; }
f multiply(a: int, b: int): int { return a * b; }

// Anonymous functions (all valid):
const divide = function(a: int, b: int): int { return a / b; };
const modulo = fn(a: int, b: int): int { return a % b; };
const power = f(a: int, b: int): int { return a ** b; };
const square = (a: int): int => a * a;
```

Class methods can use direct name, `function`, `fn`, or `f` (but NOT arrow functions):

```coco
class Math {
    calculate(): int { /* ... */ }           // direct name
    function compute(): int { /* ... */ }    // function keyword
    fn process(): int { /* ... */ }          // fn keyword
    f execute(): int { /* ... */ }           // f keyword
    // result = (): int => { /* ... */ };    // ERROR: no arrow functions in classes
}
```

Types use TypeScript-like annotations:

```coco
let count: int = 0;
let name: string = "Coco";
let user: User|null = null;
```

## Automatic Memory Safety

Coco aims to provide memory-safe application behavior automatically.

Developers should not have to write Rust-like ownership or borrowing syntax. Coco should not require normal application developers to choose between low-level memory wrappers just to build an API.

Developers write normal Coco:

```coco
class UserProfile {
    constructor(
        public user: User,
        public avatarUrl: string|null,
    ) {}

    updateAvatar(url: string): void {
        $.avatarUrl = url;  // $ is shorthand for this
    }
}

fn updateName(user: User, name: string): void {
    user.name = name;
}
```

Coco handles memory safety through compiler and runtime systems.

Planned safety mechanisms:

```txt
compiler lifetime analysis
escape analysis
automatic object lifetime management
copy-on-write values
safe object references
automatic cycle handling
null safety checks
bounds checking
safe coroutine capture analysis
runtime diagnostics in debug mode
restricted unsafe boundaries
```

## Safe Concurrency

Coco aims to support real multi-core concurrency without making developers manually manage thread-safety types.

```coco
const [user, posts, comments] = await parallel {
    run getUser(id);
    run getPosts(id);
    run getComments(id);
};
```

Unsafe shared mutation should be rejected:

```coco
let counter = 0;

await parallel {
    run {
        counter += 1;
    }

    run {
        counter += 1;
    }
}
```

Expected diagnostic:

```txt
Error: `counter` is mutated from multiple parallel tasks.
Help: use an atomic value, a channel, or a synchronized block.
```

Safe version:

```coco
const counter = new Atomic<int>(0);

await parallel {
    run {
        counter.add(1);
    }

    run {
        counter.add(1);
    }
}
```

## Roadmap

- Phase 0: Charter and language identity
- Phase 1: Grammar and specification
- Phase 2: Lexer, parser, formatter
- Phase 3: Tree-walking interpreter MVP
- Phase 4: Type checker
- Phase 5: Automatic memory safety analyzer
- Phase 6: Runtime memory system
- Phase 7: Intermediate representation and VM
- Phase 8: Automatic concurrency safety
- Phase 9: M:N scheduler and async runtime
- Phase 10: Async I/O and web standard library
- Phase 11: Native AOT backend
- Phase 12: Developer tooling
- Phase 13: PHP compatibility and migration tools
- Phase 14: Package registry and ecosystem
- Phase 15: Portability and advanced targets

## MVP Scope

The first serious MVP should prove this:

```txt
Coco can run and type-check a small memory-safe backend service using JS-like syntax.
```

MVP includes:

```txt
no $ variables
JS-like syntax
functions
classes
lists and maps
strict types
automatic memory safety checks
basic runtime memory management
result/null safety
basic HTTP server
basic async
formatter
CLI runner
```

MVP excludes:

```txt
PHP migration
package registry
HMR
JIT
WASM
bare-metal
old CPU support
complete ORM
plugin system
self-hosting
complex macros
advanced generics
```

## Summary

Coco is a planned language for developers who want the productivity of JavaScript and PHP, the safety expectations of modern systems languages, and the deployment simplicity of native binaries.

The goal is simple to say and difficult to build:

```txt
Write backend code that feels easy.
Get memory safety and safe concurrency automatically.
Ship it as a fast native binary.
```

Simple surface. Strict underneath. That is Coco.
