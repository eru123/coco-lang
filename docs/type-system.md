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
| `mixed` | **Any type (escape hatch)** | dynamic value, opt-out of type checking |

---

## The `mixed` Type: Your Escape Hatch

The `mixed` type is Coco's explicit escape hatch from the type system. Use it when you need dynamic typing or when types are genuinely unknowable at compile time.

### When to Use `mixed`

**✅ Good use cases:**

```coco
// JSON parsing — structure unknown until runtime
fn parseJson(input: string): mixed {
    return JSON.parse(input);
}

// Dynamic configuration values
class Config {
    private data: map<string, mixed>;
    
    get(key: string): mixed {
        return $.data[key];
    }
}

// Logging/debugging utilities
fn debugDump(label: string, value: mixed): void {
    console.log(`${label}: ${JSON.stringify(value)}`);
}

// FFI/interop with untyped systems
fn callJavaScript(fn: string, args: list<mixed>): mixed {
    return jsRuntime.invoke(fn, args);
}

// Generic container for heterogeneous data
class EventPayload {
    constructor(public data: mixed) {}
}
```

**❌ Anti-patterns (use better alternatives):**

```coco
// DON'T use mixed to avoid thinking about types:
fn processUser(user: mixed): void {  // ❌ User has a known shape
    // ...
}

// DO use proper types:
fn processUser(user: User): void {  // ✅
    // ...
}

// DON'T use mixed for unions:
fn format(value: mixed): string {  // ❌
    // ...
}

// DO use union types:
fn format(value: string | int | bool): string {  // ✅
    // ...
}
```

### Working with `mixed`

When you have a `mixed` value, you must narrow it before use:

```coco
fn handle(value: mixed): void {
    // Type guards narrow mixed to specific types:
    if value is string {
        print(value.toUpperCase());  // value is string here
    } else if value is int {
        print(value * 2);  // value is int here
    } else if value is list {
        print(`List with ${value.length} items`);
    } else {
        print("Unknown type");
    }
}

// Pattern matching with mixed:
fn describe(value: mixed): string {
    return match value {
        is string => `String: ${value}`,
        is int => `Number: ${value}`,
        is bool => `Boolean: ${value}`,
        is null => "Null",
        _ => "Unknown type",
    };
}
```

### `mixed` vs Untyped Parameters

There's a subtle difference:

```coco
// Untyped parameter (gradual typing):
fn addUntyped(a, b) {
    return a + b;
}

// Explicit mixed (opt-out):
fn addMixed(a: mixed, b: mixed): mixed {
    return a + b;
}
```

Both are dynamically checked, but:
- **Untyped parameters** are a gradual typing feature — you might add types later
- **`mixed` annotation** is an explicit declaration that this value is intentionally dynamic

Use `mixed` when the type is genuinely unknowable (JSON, FFI, dynamic config). Use untyped parameters during prototyping or in scripts where typing overhead isn't worth it.

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
        $.items.push(item);  // $ is shorthand for this
    }

    pop(): T|null {
        return $.items.pop();
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

---

## Troubleshooting Type Errors

This section covers common type errors and how to fix them.

### Error: "Type 'X' is not assignable to type 'Y'"

**Problem:** You're trying to assign a value of one type to a variable/parameter of an incompatible type.

```coco
let age: int = "25";  // ❌ Type 'string' is not assignable to type 'int'
```

**Solutions:**

```coco
// Solution 1: Parse/convert the value
let age: int = int.parse("25").unwrap();

// Solution 2: Change the type annotation
let age: string = "25";

// Solution 3: Use a union type if both are valid
let age: int | string = "25";

// Solution 4: Use mixed as an escape hatch (last resort)
let age: mixed = "25";
```

---

### Error: "Cannot access property 'X' on nullable type 'T|null'"

**Problem:** You're trying to access a property on a value that might be null.

```coco
let user: User|null = findUser(id);
print(user.name);  // ❌ Cannot access property 'name' on nullable type 'User|null'
```

**Solutions:**

```coco
// Solution 1: Optional chaining
print(user?.name);  // string|null

// Solution 2: Null coalescing with default
print(user?.name ?? "Unknown");

// Solution 3: Narrow with null check
if user != null {
    print(user.name);  // OK: narrowed to User
}

// Solution 4: Non-null assertion (dangerous!)
print(user!.name);  // throws if user is null

// Solution 5: Early return pattern
if user == null {
    return;
}
print(user.name);  // OK: user narrowed to User after null check
```

---

### Error: "Property 'X' does not exist on type 'Y'"

**Problem:** You're trying to access a property that doesn't exist on the given type.

```coco
class User {
    name: string;
}

const user = new User(name: "Alice");
print(user.email);  // ❌ Property 'email' does not exist on type 'User'
```

**Solutions:**

```coco
// Solution 1: Add the property to the class
class User {
    name: string;
    email: string;  // ✅
}

// Solution 2: Use optional property
class User {
    name: string;
    email: string|null = null;  // ✅
}

// Solution 3: Check if you have the right type
const user = findUser(id);  // might be different type than expected
print(user);  // inspect what you actually have

// Solution 4: Use a map for dynamic properties
class User {
    name: string;
    private metadata: map<string, mixed> = {};
    
    getMeta(key: string): mixed {
        return $.metadata[key];
    }
}
```

---

### Error: "Type 'mixed' has no properties"

**Problem:** You're trying to use a `mixed` value without narrowing it first.

```coco
fn handle(value: mixed): void {
    print(value.name);  // ❌ Type 'mixed' has no properties
}
```

**Solutions:**

```coco
// Solution 1: Narrow with type guard
fn handle(value: mixed): void {
    if value is User {
        print(value.name);  // ✅ value is User here
    }
}

// Solution 2: Use pattern matching
fn handle(value: mixed): void {
    match value {
        is User => print(value.name),
        _ => print("Not a user"),
    }
}

// Solution 3: Cast (unsafe, but sometimes necessary)
fn handle(value: mixed): void {
    const user = value as User;
    print(user.name);  // compiles, but throws if value isn't User
}

// Solution 4: Rethink your types — should this really be mixed?
fn handle(value: User | Guest): void {  // ✅ Better: use union
    match value {
        is User => print(value.name),
        is Guest => print(value.guestId),
    }
}
```

---

### Error: "Function with return type 'Result<T, E>' must return a Result"

**Problem:** You're returning a plain value from a function declared to return `Result`.

```coco
fn getUser(id: int): Result<User, DbError> {
    return db.findUser(id);  // ❌ if findUser returns User, not Result<User, DbError>
}
```

**Solutions:**

```coco
// Solution 1: Wrap the value in Ok()
fn getUser(id: int): Result<User, DbError> {
    const user = db.findUser(id);
    return Ok(user);  // ✅
}

// Solution 2: Propagate existing Result
fn getUser(id: int): Result<User, DbError> {
    return db.findUser(id);  // ✅ if findUser returns Result<User, DbError>
}

// Solution 3: Handle errors and wrap
fn getUser(id: int): Result<User, DbError> {
    try {
        const user = db.findUser(id);  // might throw
        return Ok(user);
    } catch (e: DbException) {
        return Err(new DbError(e.message));
    }
}

// Solution 4: Change return type
fn getUser(id: int): User {  // ✅ if errors are truly impossible
    return db.findUser(id);
}
```

---

### Error: "Cannot call method 'X' on type 'Result<T, E>'"

**Problem:** You're trying to use a `Result` value without unwrapping it.

```coco
const result = parseAge("25");
print(result.toString());  // ❌ Cannot call 'toString' on Result<int, ParseError>
```

**Solutions:**

```coco
// Solution 1: Unwrap with ?
fn process(): Result<void, Error> {
    const age = parseAge("25")?;  // unwraps or propagates error
    print(age.toString());  // ✅ age is int
    return Ok(void);
}

// Solution 2: Match on the Result
const result = parseAge("25");
match result {
    is Ok(age) => print(age.toString()),  // ✅
    is Err(e) => print(`Error: ${e.message}`),
}

// Solution 3: Unwrap with default
const age = parseAge("25").unwrapOr(0);
print(age.toString());  // ✅

// Solution 4: Check before unwrap
const result = parseAge("25");
if result.isOk() {
    print(result.unwrap().toString());  // ✅
}
```

---

### Error: "Type mismatch in match arms"

**Problem:** Different arms of a match expression return different types.

```coco
const result = match status {
    HttpStatus.Ok => "success",
    HttpStatus.NotFound => 404,  // ❌ returns int, but first arm returns string
};
```

**Solutions:**

```coco
// Solution 1: Make all arms return the same type
const result = match status {
    HttpStatus.Ok => "success",
    HttpStatus.NotFound => "not found",  // ✅ both string
};

// Solution 2: Use explicit types that accommodate both
const result: string | int = match status {
    HttpStatus.Ok => "success",
    HttpStatus.NotFound => 404,  // ✅ union type
};

// Solution 3: Convert values to common type
const result = match status {
    HttpStatus.Ok => "success",
    HttpStatus.NotFound => 404.toString(),  // ✅ both string
};

// Solution 4: Use mixed (escape hatch)
const result: mixed = match status {
    HttpStatus.Ok => "success",
    HttpStatus.NotFound => 404,  // ✅ mixed accepts anything
};
```

---

### Error: "Missing return statement in function with return type 'T'"

**Problem:** Not all code paths return a value.

```coco
fn getStatus(code: int): string {
    if code == 200 {
        return "OK";
    }
    // ❌ missing return for other cases
}
```

**Solutions:**

```coco
// Solution 1: Add return for all paths
fn getStatus(code: int): string {
    if code == 200 {
        return "OK";
    }
    return "Error";  // ✅
}

// Solution 2: Use match (exhaustive)
fn getStatus(code: int): string {
    return match code {
        200 => "OK",
        404 => "Not Found",
        _ => "Error",  // ✅ catch-all ensures all cases covered
    };
}

// Solution 3: Change return type to allow null
fn getStatus(code: int): string|null {
    if code == 200 {
        return "OK";
    }
    return null;  // ✅
}
```

---

### Quick Reference: Common Type Fixes

| Error Pattern | Quick Fix |
|--------------|-----------|
| Can't use nullable value | Add `?.` optional chaining or `!= null` check |
| Can't assign type X to Y | Parse/convert, or use union `X\|Y`, or use `mixed` |
| Can't use mixed value | Add `is Type` guard or pattern match |
| Can't use Result | Unwrap with `?`, `unwrap()`, or `match` |
| Function missing return | Add return to all paths, use `match`, or allow `null` |
| Type mismatch in match | Make all arms return same type or use union/`mixed` |
| Property doesn't exist | Add to class, make optional, or check your type |

---

## When to Use Each Type Feature

**Decision tree for choosing the right type approach:**

1. **Is the type structure known at compile time?**
   - Yes → Use explicit types (`User`, `int`, unions, etc.)
   - No → Use `mixed` and narrow at runtime

2. **Can the value be null?**
   - Yes → Use union with null: `User|null`
   - No → Use non-null type: `User`

3. **Can an operation fail in an expected way?**
   - Yes → Use `Result<T, E>`
   - No, but might throw → Use exceptions
   - No → Use plain return type `T`

4. **Do you need multiple possible types?**
   - Known set → Use union: `string | int | bool`
   - Unknown/dynamic → Use `mixed`

5. **Are you prototyping or in production?**
   - Prototyping → Untyped parameters OK
   - Production → Add explicit types for safety and documentation
