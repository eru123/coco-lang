# Phase 4: Type Checker Design Spec

## Overview

Gradual type checker for Coco. Strict where annotated, permissive where not. Produces diagnostics that block execution for annotated type errors. Unannotated code runs unchecked.

## Enforcement Model

| Code pattern | Behavior |
|---|---|
| `fn add(a: int, b: int): int` | Fully checked — type errors are hard errors |
| `fn add(a, b)` | No checking — params are `mixed`, anything goes |
| `fn add(a: int, b)` | Partially checked — `a` enforced, `b` is `mixed` |
| Annotated calls unannotated | Return treated as `mixed` |
| Unannotated calls annotated | Args validated at typecheck time if inferable |

## Type System Features

### 1. Primitives + Inference

Infer types from literals and assignments:

```coco
const x = 42;          // inferred: int
const y = 3.14;        // inferred: float
const s = "hello";     // inferred: string
const b = true;        // inferred: bool
const n = null;        // inferred: null
```

Arithmetic operators require numeric operands:
- `int + int` → `int`
- `float + float` → `float`
- `int + float` → `float` (promotion)
- `string + string` → `string` (concat)
- `int + string` → ERROR

Comparison operators require compatible types:
- `int < int` → `bool`
- `string == string` → `bool`
- `int < string` → ERROR

### 2. Function Signatures

When annotated, validate:
- Argument count matches parameter count
- Argument types match parameter types
- Return expression matches declared return type
- All code paths return compatible type

Infer return type when not annotated (from return statements):
```coco
fn double(x: int) {        // inferred return: int
    return x * 2;
}
```

### 3. Nullability + Narrowing

Non-null by default. `T|null` makes nullable.

```coco
let user: User|null = null;
user.name;                  // ERROR: user might be null
if user != null {
    user.name;              // OK: narrowed to User
}
user?.name;                 // OK: optional chain returns string|null
user?.name ?? "anon";       // OK: null coalesce returns string
```

Type narrowing after checks:
- `if x != null` → narrows to non-null
- `if x is int` → narrows to int
- `if typeof x == "string"` → narrows to string

### 4. Generics

Validate element types in generic containers:

```coco
const nums: list<int> = [1, 2, 3];
nums[0];                    // type: int
const m: map<string, int> = {"a": 1};
m["a"];                     // type: int

// Type error:
const nums: list<int> = [1, "two", 3];  // ERROR: "two" is not int
```

## Architecture

### New crate: `coco_typeck`

```
crates/coco_typeck/
├── Cargo.toml
├── src/
│   ├── lib.rs          — public API: check(Program) → TypeckResult
│   ├── types.rs        — Type enum, TypeId, type representations
│   ├── env.rs          — TypeEnv (symbol table with type bindings)
│   ├── infer.rs        — Type inference from expressions
│   ├── check_expr.rs   — Expression type checking
│   ├── check_stmt.rs   — Statement type checking
│   ├── check_item.rs   — Declaration type checking (fn signatures, etc.)
│   ├── unify.rs        — Type unification and compatibility
│   └── errors.rs       — TypeckError with spans and messages
```

### Type Representation

```rust
enum Type {
    Int,
    Float,
    String,
    Bool,
    Null,
    Void,
    Never,
    Mixed,                          // escape hatch / untyped
    List(Box<Type>),
    Map(Box<Type>, Box<Type>),
    Tuple(Vec<Type>),
    Union(Vec<Type>),               // T | U
    Intersection(Vec<Type>),        // T & U
    Function(Vec<Type>, Box<Type>), // (params) → return
    Named(String),                  // user-defined class/interface
    Result(Box<Type>, Box<Type>),   // Result<T, E>
    Generic(String),                // type variable T
    Unknown,                        // not yet inferred
}
```

### Two-pass algorithm

**Pass 1: Collect**
- Walk all top-level items
- Register function signatures in symbol table
- Register class/interface/enum shapes
- Don't check bodies yet (allows forward references)

**Pass 2: Check**
- Walk each function body
- Infer expression types bottom-up
- Validate constraints (assignments, calls, returns)
- Track narrowing in control flow branches
- Emit errors with span info

### Public API

```rust
pub struct TypeckResult {
    pub errors: Vec<TypeckError>,
    pub warnings: Vec<TypeckError>,
}

pub fn check(program: &Program) -> TypeckResult;
```

## CLI Integration

### New command: `typecheck`

```
cargo run -- typecheck FILE.co
```

Reports type errors. Exit 0 if clean, exit 1 if errors.

### Gate on `run`

```
cargo run -- run FILE.co           # type-checks first, blocks on error
cargo run -- run --no-check FILE.co  # skip type checking
```

### File Resolution (all commands)

When a path argument doesn't exist as given, try in order:
1. `{path}` (exact)
2. `{path}.co`
3. `src/{path}`
4. `src/{path}.co`

First match wins. Error with helpful message if none found.

Applies to: `run`, `typecheck`, `lex`, `parse`, `fmt`, `check`.

## Error Format

```
error[T001]: type mismatch
 --> src/main.co:5:12
  |
5 |     let x: int = "hello";
  |                  ^^^^^^^ expected int, found string
```

Error codes:
- T001: type mismatch
- T002: argument count mismatch
- T003: undefined variable
- T004: null access without check
- T005: missing return value
- T006: incompatible operands
- T007: property not found on type

## What's NOT in Phase 4

- Class type checking (inheritance, method resolution) — Phase 4b or later
- Trait/interface satisfaction checking
- Generic constraint validation (just basic `list<T>` usage)
- Flow-sensitive typing beyond null checks
- Type exports/imports across modules
- Exhaustiveness checking for match

## Success Criteria

1. `cargo run -- typecheck` reports correct errors for annotated type mismatches
2. Unannotated code passes without errors
3. `cargo run -- run` blocks execution on type errors (unless `--no-check`)
4. File resolution works without `.co` extension
5. All existing 92+ tests still pass
6. Type errors include file, line, column, and helpful message
