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
