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

**Note:** Regular variables do NOT use PHP-style `$` prefix:

```coco
let count = 0;      // correct
let $count = 0;     // invalid - $ not used for variable names
```

---

## The `$` Shorthand (Class Context Only)

Inside classes, `$` is a shorthand for `this`:

```coco
class User {
    private name: string;

    setName(name: string): void {
        $.name = name;        // $ is shorthand for this
        this.name = name;     // equivalent
    }

    getName(): string {
        return $.name;        // can use $ or this interchangeably
    }
}
```

**Rules:**
- `$` only works inside class methods and constructors
- `$` is exactly equivalent to `this` — use whichever is clearer
- Both can be mixed freely in the same class
- Regular functions (not in classes) cannot use `$`

---

## Functions

Coco provides multiple ways to define functions:

### Named Functions

```coco
// Full keyword:
function greet(name: string): string {
    return `Hello, ${name}`;
}

// Short keyword:
fn hello(name: string): string {
    return `Hi, ${name}`;
}

// Shortest keyword:
f hey(name: string): string {
    return `Hey, ${name}`;
}

// Async variants:
async function fetchUser(id: int): Result<User, Error> { /* ... */ }
async fn getUser(id: int): Result<User, Error> { /* ... */ }
async f loadUser(id: int): Result<User, Error> { /* ... */ }
```

### Anonymous Functions

Anonymous functions can use any keyword or arrow syntax:

```coco
// With function keyword:
const greet = function(name: string): string {
    return `Hello, ${name}`;
};

// With fn keyword:
const hello = fn(name: string): string {
    return `Hi, ${name}`;
};

// With f keyword:
const hey = f(name: string): string {
    return `Hey, ${name}`;
};

// Arrow function (expression body):
const square = (n: int): int => n * n;

// Arrow function (block body):
const double = (n: int): int => {
    return n * 2;
};

// Anonymous async:
const fetchData = async function(url: string): Result<string, Error> { /* ... */ };
const getData = async fn(url: string): Result<string, Error> { /* ... */ };
const loadData = async f(url: string): Result<string, Error> { /* ... */ };
const requestData = async (url: string): Result<string, Error> => { /* ... */ };
```

### Function Declaration Rules

**For top-level and nested functions:**
- Use `function`, `fn`, or `f` with a name
- All three keywords are equivalent

**For anonymous functions (assigned to variables):**
- Can use `function`, `fn`, `f`, or arrow syntax `() => {}`
- All four forms are valid: `const x = function() {}`, `const x = fn() {}`, `const x = f() {}`, `const x = () => {}`

**Parameters and return types:**
- Optional (gradual typing)
- Can be fully typed, partially typed, or untyped

```coco
// Untyped:
fn add(a, b) { return a + b; }
const multiply = function(a, b) { return a * b; };

// Typed:
fn subtract(a: int, b: int): int { return a - b; }
const divide = fn(a: int, b: int): int => a / b;
```

---

## Classes

```coco
class User {
    constructor(
        public readonly id: int,
        public name: string,
        private email: string,
    ) {}

    // Method without keyword (direct method name):
    getDisplayName(): string {
        return this.name;
    }

    // Method with function keyword:
    function getEmail(): string {
        return $.email;
    }

    // Method with fn keyword:
    fn setEmail(email: string): void {
        $.email = email;
    }

    // Method with f keyword:
    f updateName(name: string): void {
        $.name = name;
    }

    // Static method:
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

**Class method declaration rules:**
- Methods can be declared with: direct name, `function`, `fn`, or `f`
- Arrow functions `() => {}` are **NOT allowed** for class methods
- All keyword forms are equivalent in behavior
- `this` or `$` for instance access (both are equivalent)
- `.` for member access
- `static` can prefix any method form
- Single inheritance with `extends`
- Named arguments at call site

**Invalid class method syntax:**
```coco
class Invalid {
    // ERROR: Arrow functions not allowed as class methods
    getName = (): string => {
        return this.name;
    };
}
```

**Constructor parameter properties:**
- `public`, `private`, `protected`, `readonly` modifiers create automatic properties

### Visibility Modifiers

```coco
class Account {
    public balance: int;           // accessible everywhere
    protected owner: string;       // accessible in class and subclasses
    private pin: string;           // accessible only in this class
    public readonly id: int;       // public but immutable after construction

    constructor(id: int, owner: string, pin: string) {
        $.id = id;
        $.owner = owner;
        $.pin = pin;
        $.balance = 0;
    }

    // Visibility with direct method name:
    public deposit(amount: int): void {
        this.balance += amount;
    }

    // Visibility with function keyword:
    protected function validateOwner(name: string): bool {
        return $.owner == name;
    }

    // Visibility with fn keyword:
    private fn checkPin(pin: string): bool {
        return this.pin == pin;
    }

    // Visibility with f keyword:
    public f withdraw(amount: int): void {
        $.balance -= amount;
    }

    // Static with visibility:
    static fromData(data): Account {
        return new Account(data.id, data.owner, data.pin);
    }
}
```

**Visibility rules:**
- `public` — accessible from anywhere (default for methods)
- `protected` — accessible in class and subclasses only
- `private` — accessible only within the class
- `readonly` — can only be assigned in constructor
- `static` — belongs to class, not instances

**Property defaults:**
- Properties without modifier are `private` by default
- Methods without modifier are `public` by default
- Constructor parameters with modifiers become properties

**Instance member access:**
- `this.property` — standard reference to instance member
- `$.property` — shorthand reference to instance member (equivalent to `this`)
- Both forms are interchangeable; use whichever is clearer

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
        $.updatedAt = DateTime.now();
    }
}

trait SoftDelete {
    deletedAt: DateTime|null = null;

    softDelete(): void {
        $.deletedAt = DateTime.now();
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
- Trait methods can use `this` or `$` for member access

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

Coco provides two mechanisms for error handling: `Result` types for expected failures and exceptions for unexpected failures.

### Result Type

The `Result<T, E>` type represents success (`Ok`) or failure (`Err`):

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
    const age = parseAge(data.get("age"))?;  // unwraps Ok, propagates Err
    const name = data.get("name") ?: return Err(new FormError("name required"));
    return Ok(new User(name: name, age: age));
}
```

**Explicit Result Handling:**

```coco
// Pattern 1: Match expression
const result = parseAge("25");
const message = match result {
    is Ok(age) => `Valid age: ${age}`,
    is Err(e) => `Error: ${e.message}`,
};

// Pattern 2: If-else with type narrowing
const result = parseAge("25");
if result.isOk() {
    const age = result.unwrap();  // safe after isOk()
    print(`Age: ${age}`);
} else {
    const error = result.unwrapErr();
    log.error(error.message);
}

// Pattern 3: Chaining operations
fn validateUser(data: FormData): Result<User, ValidationError> {
    const age = parseAge(data.get("age"))?;
    const email = parseEmail(data.get("email"))?;
    const username = parseUsername(data.get("username"))?;
    
    return Ok(new User(
        age: age,
        email: email,
        username: username,
    ));
}

// Pattern 4: Transforming results
fn getUserAge(userId: int): Result<int, DbError> {
    const user = db.findUser(userId)?;  // propagate DbError
    return Ok(user.age);
}

// Pattern 5: Default values on error
const age = parseAge(input).unwrapOr(0);  // use 0 if parsing fails
const config = loadConfig().unwrapOrElse(() => defaultConfig());
```

### Exceptions

Use exceptions for unexpected failures (programming errors, invariant violations):

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

**Exception Handling Patterns:**

```coco
// Pattern 1: Specific exception types
try {
    const config = loadConfig();
    connectDatabase(config);
} catch (e: FileNotFoundError) {
    log.error("Config file missing");
} catch (e: NetworkError) {
    log.error("Cannot reach database");
} catch (e: Error) {
    log.error("Unexpected error: " + e.message);
}

// Pattern 2: Re-throwing with context
fn processFile(path: string): void {
    try {
        const data = readFile(path);
        parseData(data);
    } catch (e: ParseError) {
        throw new ProcessingError(`Failed to process ${path}: ${e.message}`);
    }
}

// Pattern 3: Converting exceptions to Results
fn safeOperation(): Result<Data, Error> {
    try {
        const data = riskyThirdPartyApi();  // might throw
        return Ok(data);
    } catch (e: Error) {
        return Err(e);
    }
}
```

### Error Handling Split Rule

Choose the right mechanism for your error:

| Use `Result<T, E>` for... | Use exceptions for... |
|---------------------------|----------------------|
| Parsing failures | Array index out of bounds |
| File not found | Null pointer access |
| Network timeouts | Divide by zero (bug) |
| Validation errors | Invariant violations |
| Database constraint violations | Out of memory |
| Authentication failures | Stack overflow |

**Rule of thumb:** If the error is part of normal program flow and the caller should handle it, use `Result`. If the error indicates a bug or impossible state, use exceptions.

### Common Error Handling Anti-Patterns

**❌ Don't ignore errors:**
```coco
const result = parseAge(input);  // Result unused!
```

**✅ Handle or propagate:**
```coco
const age = parseAge(input)?;  // propagate
// OR
const age = parseAge(input).unwrapOr(0);  // handle with default
```

**❌ Don't use exceptions for control flow:**
```coco
try {
    const user = findUser(id);  // throws if not found
    return user;
} catch {
    return null;  // BAD: expected case, should use Result
}
```

**✅ Use Result for expected failures:**
```coco
fn findUser(id: int): Result<User, NotFoundError> {
    // ...
}
```

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
| `\|>` `->` | Pipe (left-to-right) |
| `<\|` `<-` | Pipe (right-to-left) |

---

## Pipe Operator

The pipe operator (inspired by HHVM) allows threading values through a sequence of function calls, making data transformations more readable:

### Left-to-Right Piping

```coco
// Using |> operator:
const result = [1, 2, 3]
    |> (a) => a.map(n => n * 2)
    |> (a) => a.filter(n => n > 4);

// Using -> operator (alternative syntax):
const result = [1, 2, 3]
    -> (a) => a.map(n => n * 2)
    -> (a) => a.filter(n => n > 4);

// Both are equivalent to:
const result = [1, 2, 3].map(n => n * 2).filter(n => n > 4);
```

### Right-to-Left Piping

```coco
// Using <| operator:
const result = (a) => a.filter(n => n > 4)
    <| (a) => a.map(n => n * 2)
    <| [1, 2, 3];

// Using <- operator (alternative syntax):
const result = (a) => a.filter(n => n > 4)
    <- (a) => a.map(n => n * 2)
    <- [1, 2, 3];
```

### Pipe Operator Rules

**Valid syntax:**
```coco
// Same direction throughout:
x |> f |> g |> h    // ✅ All left-to-right
x -> f -> g -> h    // ✅ All left-to-right
h <| g <| f <| x    // ✅ All right-to-left
h <- g <- f <- x    // ✅ All right-to-left
```

**Invalid syntax (mixing directions):**
```coco
x |> f <- g         // ❌ Error: Cannot mix |> and <-
x -> f <| g         // ❌ Error: Cannot mix -> and <|
x |> f -> g         // ❌ Error: Cannot mix |> and ->
x <- f <| g         // ❌ Error: Cannot mix <- and <|
```

**Error message:**
```
SyntaxError: Cannot mix pipe operator directions in the same expression.
Use either left-to-right (|>, ->) or right-to-left (<|, <-), not both.
```

### When to Use Pipe Operators

**Use pipes when:**
- Chaining multiple transformations makes a long method chain
- The intermediate steps benefit from explicit parameter names
- You want to emphasize the data flow direction

**Prefer method chaining when:**
- The chain is short (2-3 calls)
- Methods are well-named and self-documenting
- No intermediate transformation logic needed

**Example with complex transformations:**
```coco
// Without pipes (nested):
const result = filterInvalid(
    sortByDate(
        mapToUsers(
            fetchData(apiUrl)
        )
    )
);

// With pipes (clearer data flow):
const result = fetchData(apiUrl)
    |> (data) => mapToUsers(data)
    |> (users) => sortByDate(users)
    |> (sorted) => filterInvalid(sorted);

// Or concise with method chaining:
const result = fetchData(apiUrl)
    .then(mapToUsers)
    .then(sortByDate)
    .then(filterInvalid);
```

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
    private cents: int;

    constructor(cents: int) {
        $.cents = cents;
    }

    __toString(): string {
        return `$${$.cents / 100}`;
    }

    __compare(other: Money): int {
        return this.cents <=> other.cents;
    }
}

class Config {
    private data: map<string, string>;

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

Magic methods can use `this` or `$` for member access.

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

---

## Troubleshooting Common Errors

### Null Safety Errors

**Problem:** Accessing properties on potentially null values

```coco
let user: User|null = findUser(id);
print(user.name);  // ❌ Error: Cannot access 'name' on User|null
```

**Fix with optional chaining:**
```coco
print(user?.name);  // ✅ Returns string|null
print(user?.name ?? "Unknown");  // ✅ Returns string with fallback
```

**Fix with null check:**
```coco
if user != null {
    print(user.name);  // ✅ Type narrowed to User
}
```

---

### Result Handling Errors

**Problem:** Using Result values without unwrapping

```coco
const result = parseAge("25");
const doubled = result * 2;  // ❌ Error: Cannot multiply Result<int, Error>
```

**Fix by propagating with ?:**
```coco
fn process(): Result<int, Error> {
    const age = parseAge("25")?;  // ✅ Unwraps or propagates error
    return Ok(age * 2);
}
```

**Fix with pattern matching:**
```coco
const result = parseAge("25");
match result {
    is Ok(age) => print(age * 2),  // ✅
    is Err(e) => print(`Error: ${e.message}`),
}
```

**Fix with default value:**
```coco
const age = parseAge("25").unwrapOr(0);  // ✅ Use 0 if error
const doubled = age * 2;
```

---

### Type Mismatch Errors

**Problem:** Assigning incompatible types

```coco
let count: int = "42";  // ❌ Error: string not assignable to int
```

**Fix by parsing:**
```coco
let count: int = int.parse("42").unwrap();  // ✅
```

**Fix with union type:**
```coco
let count: int | string = "42";  // ✅ If both types are valid
```

**Fix with `mixed` (escape hatch):**
```coco
let count: mixed = "42";  // ✅ Opt out of type checking
```

---

### Function Return Type Errors

**Problem:** Missing return in all code paths

```coco
fn getStatus(ok: bool): string {
    if ok {
        return "success";
    }
    // ❌ Error: Not all paths return a value
}
```

**Fix with explicit return:**
```coco
fn getStatus(ok: bool): string {
    if ok {
        return "success";
    }
    return "failure";  // ✅
}
```

**Fix with match (exhaustive):**
```coco
fn getStatus(ok: bool): string {
    return match ok {
        true => "success",
        false => "failure",
    };  // ✅ Match is exhaustive
}
```

---

### Concurrency Safety Errors

**Problem:** Mutable data races across parallel boundaries

```coco
let counter = 0;
await parallel {
    run { counter += 1; }  // ❌ Error: Mutable capture in parallel block
}
```

**Fix with atomics:**
```coco
const counter = new Atomic<int>(0);
await parallel {
    run { counter.add(1); }  // ✅ Atomic operations are safe
}
```

**Fix by collecting results:**
```coco
const results = await parallel {
    run { return 1; };
    run { return 1; };
};
const counter = results[0] + results[1];  // ✅ No shared mutation
```

**Fix with channels:**
```coco
const ch = chan<int>(10);
await parallel {
    run { ch.send(1); };
    run { ch.send(1); };
};
ch.close();
let counter = 0;
for value in ch {
    counter += value;  // ✅ Sequential consumption
}
```

---

### Working with `mixed` Type

**Problem:** Can't access properties on `mixed`

```coco
fn handle(data: mixed): void {
    print(data.name);  // ❌ Error: mixed has no properties
}
```

**Fix with type guard:**
```coco
fn handle(data: mixed): void {
    if data is User {
        print(data.name);  // ✅ Narrowed to User
    }
}
```

**Fix with pattern matching:**
```coco
fn handle(data: mixed): void {
    match data {
        is User => print(data.name),
        is Admin => print(data.adminName),
        _ => print("Unknown type"),
    }  // ✅
}
```

**Better: Avoid `mixed` when possible:**
```coco
fn handle(data: User | Admin): void {  // ✅ Use union instead
    match data {
        is User => print(data.name),
        is Admin => print(data.adminName),
    }
}
```

---

### Match Expression Type Errors

**Problem:** Match arms return different types

```coco
const result = match value {
    is string => value.toUpperCase(),  // returns string
    is int => value * 2,                // ❌ returns int
};
```

**Fix by converting to common type:**
```coco
const result = match value {
    is string => value.toUpperCase(),
    is int => (value * 2).toString(),  // ✅ Both return string
};
```

**Fix with union type:**
```coco
const result: string | int = match value {
    is string => value.toUpperCase(),
    is int => value * 2,  // ✅ Union allows both
};
```

---

## Quick Tips

**When to use each feature:**
- **`const`/`let`** — Always start with `const`, only use `let` when mutation is needed
- **Type annotations** — Optional, but add for function signatures and complex types
- **`Result<T, E>`** — For expected failures (parsing, I/O, validation)
- **Exceptions** — For unexpected failures (bugs, invariant violations)
- **`T|null`** — When a value can legitimately be absent
- **`mixed`** — Only for truly dynamic data (JSON, FFI, dynamic config)
- **Union types** — When you have a known set of possible types
- **Match expressions** — For exhaustive handling of enums, unions, and conditionals
