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

## Magic Methods

```coco
class Money {
    __toString(): string {
        return `$${this.cents / 100}`;
    }

    __compare(other: Money): int {
        return this.cents <=> other.cents;
    }
}

class Config {
    __get(key: string): string|null { /* ... */ }
    __set(key: string, value: string): void { /* ... */ }
    __invoke(key: string): bool { /* ... */ }
    __call(method: string, args: list<mixed>): mixed { /* ... */ }
}
```

Available magic methods:
- `__toString` — string conversion
- `__get` — property read interception
- `__set` — property write interception
- `__call` — method call interception
- `__invoke` — callable object
- `__compare` — spaceship operator overload

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
