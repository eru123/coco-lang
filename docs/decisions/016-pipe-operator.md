# ADR 016: Pipe Operator

**Status:** Accepted  
**Date:** 2026-06-04  
**Deciders:** Language Design Team

## Context

Data transformation pipelines are common in backend services and CLI tools. Coco targets developers who work with data processing, API responses, and stream transformations. These operations often involve chaining multiple function calls, which can become difficult to read when:

1. **Nested function calls** create deeply nested parentheses
2. **Long method chains** require all operations to be methods on the object
3. **Intermediate variables** clutter the scope with single-use bindings

HHVM (Hack) introduced a pipe operator (`|>`) that threads values through function calls in a readable left-to-right or right-to-left flow. This pattern has proven useful for complex transformations.

## Decision

We will add **pipe operators** to Coco with the following design:

### Syntax

Four operator forms, two directions:

**Left-to-right:**
- `|>` — pipe operator (primary)
- `->` — alternative pipe operator (familiar to users of other languages)

**Right-to-left:**
- `<|` — reverse pipe operator
- `<-` — alternative reverse pipe operator

### The `$$` Pipe Placeholder

Within pipe expressions, `$$` represents the value being passed through the pipe. This allows for more concise code:

```coco
// Concise with $$:
const greeting = "world" |> sayHello($$);

function sayHello(name: string): string {
    return `Hello, ${name}!`;
}

// Method calls:
const result = "hello"
    |> $$.toUpperCase()
    |> $$.split("");

// With functions:
const processed = data
    |> parse($$)
    |> validate($$)
    |> transform($$);
```

**Scope restriction:** `$$` only works inside pipe expressions:

```coco
const x = sayHello($$);  // ❌ Error: $$ can only be used in pipe expressions
```

You can mix `$$` and lambda functions in the same pipeline:

```coco
const result = data
    |> parse($$)                    // concise
    |> (parsed) => {                 // explicit when needed
        log.info("Parsed:", parsed);
        return validate(parsed);
    }
    |> transform($$);               // back to concise
```

### Semantics

```coco
// With $$ (concise):
x |> process($$) |> transform($$)
// Equivalent to: transform(process(x))

// With lambda functions (explicit):
x |> (v) => process(v) |> (v) => transform(v)
// Equivalent to: transform(process(x))

// Right-to-left:
transform($$) <| process($$) <| x
// Equivalent to: transform(process(x))
```

Each operator takes the expression on one side and passes it to the function on the other side. With `$$`, the value replaces the placeholder in the expression.

### Mixing Restriction

**It is a syntax error to mix different pipe operator directions in the same expression:**

```coco
x |> f -> g     // ❌ Error: Cannot mix |> and ->
x |> f <- g     // ❌ Error: Cannot mix |> and <-
x -> f <| g     // ❌ Error: Cannot mix -> and <|
x <- f <| g     // ❌ Error: Cannot mix <- and <|
```

This restriction prevents ambiguous parsing and confusing data flow.

**Valid uniform usage:**
```coco
x |> f |> g |> h    // ✅ All left-to-right
x -> f -> g -> h    // ✅ All left-to-right
h <| g <| f <| x    // ✅ All right-to-left
h <- g <- f <- x    // ✅ All right-to-left
```

### Precedence

Pipe operators have **lower precedence** than most other operators, allowing natural composition:

```coco
// Arithmetic and comparison happen first:
numbers |> (a) => a.filter(n => n > 10)

// Function calls happen first:
data |> parse |> validate
```

### Use Cases

**1. Array transformations with $$:**
```coco
const result = [1, 2, 3, 4, 5]
    |> $$.map(n => n * 2)
    |> $$.filter(n => n > 5)
    |> $$.reduce((sum, n) => sum + n, 0);
```

**2. Text processing with $$:**
```coco
const formatted = rawInput
    |> $$.trim()
    |> $$.toLowerCase()
    |> $$.replace(/[^a-z0-9]/g, "-");
```

**3. API response processing with $$:**
```coco
const users = await fetchUsers()
    |> parse<list<User>>($$)?
    |> $$.filter(u => u.active)
    |> $$.map(u => u.name);
```

**4. Function composition (right-to-left) with $$:**
```coco
const process = $$.trim()
    <- $$.toLowerCase()
    <- $$.split(" ");
```

**5. Named functions with $$:**
```coco
const result = data
    |> parse($$)
    |> validate($$)
    |> transform($$);
```

## Rationale

### Why Add Pipe Operators?

1. **Improved readability** — Data flow is explicit and linear
2. **Reduced nesting** — Eliminates deeply nested function calls
3. **Flexibility** — Works with any function, not just methods
4. **Concise with `$$`** — No need for verbose lambda functions in simple cases
5. **Familiar** — Borrowed from HHVM/Hack, F#, Elixir, and other languages
6. **Optional** — Developers can use method chaining when appropriate

### Why the `$$` Placeholder?

1. **Conciseness** — `data |> parse($$)` is cleaner than `data |> (x) => parse(x)`
2. **Flexibility** — Can use `$$` for simple cases, lambdas for complex logic
3. **Clear scope** — Only valid in pipe expressions, preventing accidental misuse
4. **Natural positioning** — `$$` can appear anywhere in the expression: `parse($$, config)` or `$$.method()`
5. **Familiar pattern** — Similar to shell pipes and functional programming placeholders

### Why Four Operators?

- `|>` and `<|` are the primary forms (familiar from functional languages)
- `->` and `<-` provide alternatives that may feel more natural to some developers
- Having both directions supports different mental models (data flow vs function composition)

### Why Forbid Mixing?

Allowing mixed directions in one expression would create ambiguous parsing and confusing data flow:

```coco
// What does this mean?
x |> f <- g |> h

// Is it:
h(g(f(x)))?  // All forward?
h(f(g(x)))?  // Mixed?
g(h(f(x)))?  // Reverse middle?
```

By enforcing uniform direction, the parser is unambiguous and the data flow is crystal clear to readers.

### Why Not Just Use Method Chaining?

Method chaining works well when:
- All operations are methods on the object
- The object type supports the needed operations
- You're working within a well-designed fluent API

Pipe operators work when:
- Functions are standalone (not methods)
- You need intermediate transformations with custom logic
- You want to compose operations from different modules
- You're working with primitive types or third-party libraries

Both patterns coexist — use whichever fits the situation.

## Alternatives Considered

### 1. **No pipe operator** (stick with method chaining and nested calls)

**Rejected:** While method chaining handles many cases, it doesn't solve the problem of standalone function composition or deeply nested calls.

### 2. **Only left-to-right** (`|>` only)

**Rejected:** Right-to-left is useful for function composition patterns and matches how developers think about building complex transformations.

### 3. **Allow mixing operators freely**

**Rejected:** Creates parsing ambiguity and makes code harder to read. The error messages would be confusing, and the behavior would be surprising.

### 4. **Use `_` as placeholder** (like `|> _ + 1`)

**Rejected in favor of `$$`:** While `_` is common in some languages, `$$` is more distinctive and less likely to conflict with variable names. It also parallels the `$` shorthand for `this` in classes, creating a consistent pattern for special placeholders in Coco.

### 5. **Thread-first/thread-last macros** (like Clojure)

**Rejected:** Coco is not a macro-based language. Built-in operators are more discoverable and better integrated with tooling.

## Consequences

### Positive

- **Clearer data pipelines** in backend services and CLI tools
- **Less nesting** in complex transformations
- **Familiar syntax** for developers from HHVM, F#, Elixir, or Rust
- **Flexible composition** without requiring method chaining
- **Editor support** will autocomplete and format pipe chains

### Negative

- **Another way to do things** (but method chaining remains preferred when available)
- **Learning curve** for developers unfamiliar with pipe operators
- **Parser complexity** (needs to track pipe direction and forbid mixing)

### Neutral

- **Code style debates** about when to use pipes vs. method chaining (style guides will address this)

## Implementation Notes

### Parser Changes

1. Add four binary operators: `|>`, `->`, `<|`, `<-`
2. Add special identifier `$$` that's only valid in pipe expressions
3. Track "active pipe direction" in the current expression
4. Emit syntax error if direction changes mid-expression
5. Emit syntax error if `$$` is used outside pipe expressions
6. Reset direction tracking at statement boundaries

### `$$` Placeholder Implementation

When parsing a pipe expression:
1. Set a context flag `inPipeExpression = true`
2. Parse the right-hand side expression
3. If `$$` appears, treat it as a placeholder token
4. Reset `inPipeExpression = false` at the end of the pipe chain

During codegen:
```coco
x |> process($$)
// Generates: process(x)

x |> $$.method()
// Generates: x.method()

x |> transform($$, config)
// Generates: transform(x, config)
```

### Error Messages

```
SyntaxError: Cannot mix pipe operator directions in the same expression.
Use either left-to-right (|>, ->) or right-to-left (<|, <-), not both.

  5 | const result = [1, 2, 3]
  6 |     |> $$.map(n => n * 2)
  7 |     <- $$.filter(n => n > 4);
    |     ^^ pipe direction changed from |> to <-
```

```
SyntaxError: $$ can only be used in pipe expressions.

  3 | fn example(): void {
  4 |     const greeting = sayHello($$);
    |                               ^^ $$ is not valid here
  5 | }

Help: Use $$ only with pipe operators: value |> sayHello($$)
```

### Type Checking

The type checker must verify:
1. With `$$`: The piped value type is compatible with where `$$` appears
2. With lambdas: Left side produces a value compatible with the function parameter
3. Infer the return type from the final function in the chain
4. Propagate Result types when `?` is used in the pipeline
5. Handle `$$` in both function arguments and method calls

### Formatter

The formatter should:
1. Break pipe chains at the operator for readability
2. Indent continued lines consistently
3. Align arrows vertically for long chains (optional style)

```coco
// Formatter output:
const result = data
    |> (d) => parse(d)
    |> (parsed) => validate(parsed)
    |> (valid) => transform(valid);
```

## Related Decisions

- **ADR 003: Gradual Typing** — Pipe operators work with both typed and untyped code
- **ADR 007: Function Syntax** — Pipe operators accept any function form (named, anonymous, arrow)
- **ADR 009: Result Type** — Pipe operators compose naturally with `?` propagation

## References

- [HHVM Pipe Operator](https://docs.hhvm.com/hack/expressions-and-operators/pipe)
- [F# Pipeline Operator](https://learn.microsoft.com/en-us/dotnet/fsharp/language-reference/symbol-and-operator-reference/)
- [Elixir Pipe Operator](https://elixir-lang.org/getting-started/enumerables-and-streams.html#the-pipe-operator)
- [Rust RFC: Try Operator](https://github.com/rust-lang/rfcs/blob/master/text/0243-trait-based-exception-handling.md)

## Examples

See `/examples/21-pipe-operator.co` for comprehensive usage examples.
