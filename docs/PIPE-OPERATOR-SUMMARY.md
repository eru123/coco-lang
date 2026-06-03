# Pipe Operator Implementation Summary

**Date:** 2026-06-04  
**Feature:** Pipe operators (`|>`, `->`, `<|`, `<-`) for left-to-right and right-to-left data flow

## Overview

Added HHVM-inspired pipe operators to Coco, enabling cleaner data transformation pipelines and function composition.

## Syntax

### Left-to-Right (Data Flow)
```coco
value |> fn1 |> fn2 |> fn3
value -> fn1 -> fn2 -> fn3
```

### Right-to-Left (Function Composition)
```coco
fn3 <| fn2 <| fn1 <| value
fn3 <- fn2 <- fn1 <- value
```

### Mixing Restriction
**Syntax error** to mix directions in one expression:
```coco
x |> f -> g     // ❌ Error
x |> f <- g     // ❌ Error
x -> f <| g     // ❌ Error
```

## Changes Made

### 1. Documentation
- **`docs/language-reference.md`** — Added pipe operator section with syntax, rules, and examples
- **`docs/decisions/016-pipe-operator.md`** — Complete ADR documenting the design decision, rationale, and implementation notes
- **`docs/PIPE-OPERATOR-SUMMARY.md`** — This summary document

### 2. Examples
- **`examples/21-pipe-operator.co`** — Comprehensive examples covering:
  - Basic left-to-right piping with `|>` and `->`
  - Right-to-left piping with `<|` and `<-`
  - Complex data processing (API responses, log files)
  - Comparison with nested calls and method chaining
  - Async/await integration
  - Result propagation with `?`

### 3. Editor Support
- **`editors/vscode/syntaxes/coco.tmLanguage.json`** — Added syntax highlighting for all four pipe operators

### 4. README
- **`README.md`** — Updated main example to demonstrate pipe operator usage in HTTP server

## Key Design Decisions

1. **Four operators** — Two syntaxes (`|>` vs `->`, `<|` vs `<-`) for user preference
2. **Direction enforcement** — Cannot mix left-to-right and right-to-left in same expression
3. **Lower precedence** — Pipe operators bind less tightly than arithmetic/comparison
4. **Type-safe** — Full type checking through the pipeline
5. **Composable** — Works naturally with Result propagation (`?`) and async/await

## Use Cases

**Best for:**
- Multi-step data transformations
- API response processing
- Collection pipelines (filter, map, reduce chains)
- Text processing workflows
- Function composition

**Prefer method chaining when:**
- Chain is short (2-3 calls)
- Methods are self-documenting
- No complex intermediate logic needed

## Example Comparison

### Nested (Hard to Read)
```coco
const result = sum(filter(map(data, n => n * 2), n => n > 4));
```

### Piped (Clear Data Flow)
```coco
const result = data
    |> (a) => map(a, n => n * 2)
    |> (a) => filter(a, n => n > 4)
    |> (a) => sum(a);
```

### Method Chaining (Cleanest When Available)
```coco
const result = data
    .map(n => n * 2)
    .filter(n => n > 4)
    .reduce((s, n) => s + n, 0);
```

## Implementation Status

**Phase 0-1 (Design/Specification):** ✅ **Complete**

The pipe operator is fully specified and documented. Implementation will occur in:
- **Phase 2:** Lexer and parser (tokenize and parse pipe operators)
- **Phase 4:** Type checker (verify type compatibility across pipeline)
- **Phase 11:** Compiler backend (lower to function calls)

## Next Steps

1. **Phase 2 (Parser):**
   - Add tokens for `|>`, `->`, `<|`, `<-`
   - Parse binary expressions with pipe operators
   - Track and enforce direction uniformity
   - Emit helpful error messages on direction mismatch

2. **Phase 4 (Type Checker):**
   - Verify left expression type matches right function parameter type
   - Infer result type from final function in chain
   - Handle Result propagation (`?`) in pipelines

3. **Phase 7 (Codegen):**
   - Lower pipe chains to nested function calls
   - Optimize away intermediate allocations when possible

## References

- HHVM Pipe Operator: https://docs.hhvm.com/hack/expressions-and-operators/pipe
- F# Pipeline: https://learn.microsoft.com/en-us/dotnet/fsharp/language-reference/symbol-and-operator-reference/
- Elixir Pipe: https://elixir-lang.org/getting-started/enumerables-and-streams.html#the-pipe-operator
