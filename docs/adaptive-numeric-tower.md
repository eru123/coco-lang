# The Adaptive Numeric Tower

Coco's arithmetic is *magical*: the user writes `a + b` and never thinks about
numeric representation. The compiler and runtime together automatically pick
the **fastest tier whose result is still exact**, specializing the operation
statically when operand types are known and guarding/escalating dynamically
otherwise.

> **Definition.** An *adaptive numeric tower* is an arithmetic system that
> represents every value in the fastest tier whose result is still exact,
> specializing the operation statically when operand types are known and
> guarding/escalating dynamically otherwise.

This concept is, as far as we know, novel as a single system. It sits at the
intersection of three existing ideas:

| Lineage | What it contributes | What it lacks |
|---|---|---|
| **Numeric tower** (Scheme/R⁵RS) | Correctness tiers + exactness contagion | No performance specialization |
| **Adaptive precision** (Shewchuk) | Correctness-gated performance escalation | Scoped to geometric sign predicates |
| **Speculative specialization** (Julia/V8/TruffleRuby) | Type-specialized fast codegen | Wraps/demotes rather than preserving exactness |

Coco combines all three: the tower's correctness, Shewchuk's "escalate only
when correctness demands," and the JIT lineage's static specialization —
generalized to general-purpose arithmetic.

Short alias: **tiered arithmetic**.

## The tiers

In target order (fastest first), each tier preserves exactness for the
operands it handles:

| Tier | Representation | When used | Exact? |
|---|---|---|---|
| **0 — i64 fast path** | native `int64_t` | `int + int` that fits in 64 bits | ✅ exact, fastest |
| **1 — bignum escalation** | sign-magnitude big integer | i64 op would overflow | ✅ exact |
| **2 — f64** | native `double` | `float` involved | ⚠️ may be inexact (IEEE) |
| **3 — string** | refcounted UTF-8 | `string + string` | ✅ exact |
| **fallback — dynamic tag dispatch** | runtime tag check | operand types unknown (`mixed`) | delegates to 0–3 |

Tiers deliberately deferred (structure prepared, not yet built): a
rational/decimal tier for exact `0.1 + 0.2`, and optimized (Karatsuba) bignum.

## How dispatch works

There are two dispatch points, one static and one dynamic:

### Static (compile time)

The type checker (`coco_typeck`) infers a `Ty` for every expression and records
it in a span-keyed `TypeMap` (`crates/coco_typeck/src/typemap.rs`). The bytecode
compiler consults this map in `compile_binary` (`crates/coco_interpreter/src/compiler.rs`)
via `typed_arith_op`:

- Both operands `Ty::Int`/`Ty::Uint` → emit `OP_ADD_I` (etc.) — an int-specialized
  opcode that skips runtime tag dispatch and goes straight to the i64 fast path
  with overflow→BigInt escalation (Tier 0 + Tier 1).
- Either operand `Ty::Float` (and the other numeric) → emit `OP_ADD_F` (etc.) —
  a float-specialized opcode doing a native f64 op (Tier 2).
- Either operand `Ty::Unknown`/`Ty::Mixed` (untyped code) → emit the generic
  `OP_ADD`, which dispatches on the value's runtime tag (fallback).

Because Coco is gradually typed, unannotated code (e.g. `fn add(a, b)`) still
compiles — it just takes the dynamic fallback. Annotated code
(`fn add(a: int, b: int): int`) gets the specialized fast path. The user pays
for abstraction only when they don't annotate. Inference also flows from
literals, so `let x = 1; x + 2` (unannotated but inferable) gets `OP_ADD_I`.

The `TypeMap` is plumbed from `coco_typeck::check` (run in the CLI's `run`,
`build`, and `test` paths) into `Compiler::with_types`, and propagates into
function bodies (each `compile_function_body` clones the map; spans are stable
program-wide).

### Dynamic (runtime)

The VM's value layer (`crates/coco_interpreter/src/vm.rs`) implements the tower
in `int_binop` (the core) and the `vm_add`/`vm_sub`/... handlers. Each generic
arithmetic opcode dispatches on the operands' runtime tags:

- `int + int` (no overflow) → native i64 add via `int_binop`'s fast path (Tier 0)
- `int + int` (overflow) → BigInt escalation, exact (Tier 1)
- any `float` operand → f64 (Tier 2)
- `string + string` (for `+` only) → concatenation (Tier 3)
- `bigint` involved → BigInt (Tier 1)

`int_binop` tries the i64 op with `checked_add`/`checked_mul`/etc.; on overflow
or when an operand is a BigInt, it escalates to BigInt and `normalize_int`
shrinks the result back to `Int64` if it fits (so the fast-path representation
is "sticky" across compound expressions). The int-specialized `OP_ADD_I`/
`vm_add_i` handlers call `int_binop` directly (skipping the tag-dispatch match);
the float-specialized `OP_ADD_F`/`vm_add_f` handlers do native f64 with
int→float promotion.

## Edge-case decisions

These are deliberate choices, documented as the spec the VM implements against:

- **Integer overflow → bignum, never wrap.** `INT64_MAX + 1` is `9223372036854775808`, exact. (Python/Scheme lineage, not Julia/Rust.)
- **`int + float` → `float`.** The int is promoted to f64; this is lossy for ints beyond 2⁵³, matching most languages. (A future rational/decimal tier could avoid this.)
- **Division by zero:** integer `÷0` aborts with a diagnostic; float `/0` yields IEEE `inf`/`nan`.
- **Modulo** is truncated toward zero; the remainder takes the sign of the dividend (C semantics).
- **Equality is type-strict:** `1 == 1.0` is `false`, `1 == "1"` is `false` (via `value_eq`).
- **Comparisons** promote `int`→`float` when mixed; `string < string` is lexicographic by bytes.
- **`+` is overloaded** for string concatenation, but only when *both* operands are strings (no JS-style `"a" + 1` coercion).
- **`-INT64_MIN`** escalates to bignum (would overflow i64).
- **Bitwise ops** (`& | ^ << >> ~`) use the i64 fast path with BigInt fallback (same `int_binop` pattern); `(Bool, Bool)` returns a `Bool` so `xor` stays logical.

## Value model

Every value is a `Value` enum (`crates/coco_interpreter/src/value.rs`):

```rust
pub enum Value {
    Int64(i64),                            // Tier 0: i64 fast path, no allocation
    Int(BigInt),                           // Tier 1: bignum (overflow escalation)
    Float(f64),                            // Tier 2: f64
    String(String),                        // Tier 3: UTF-8
    Bool(bool),
    Null,
    List(Arc<CoW<Vec<Value>>>),            // refcounted, copy-on-write
    Map(Arc<CoW<HashMap<String, Value>>>), // string keys
    // ... FnObj, TaskHandle, Channel, Atomic, Ok/Err
}
```

`Int64` and `Int` are semantically identical (`Int64(1) == Int(BigInt::from(1))`);
the dual representation is purely a performance optimization. List/Map are
`Arc<CoW<...>>` (refcounted, copy-on-write) — no tracing GC. `int_binop` and
`normalize_int` move values between `Int64` and `Int` as overflow demands.

## Building / running

Coco's sole execution model is the bytecode VM (the AOT/LLVM codegen was
removed). Build and run:

```
cargo build -p coco_cli
./target/debug/coco run program.co        # parse -> typeck -> compile -> VM
./target/debug/coco build --disasm program.co   # show bytecode (incl. _I/_F opcodes)
```

The type checker runs as part of `run`/`build`/`test` to produce the `TypeMap`
for specialization (and as a hard-check gate on `run`, skippable with
`--no-check`).

## What's implemented vs. deferred

**Implemented:** tiers 0–3 + dynamic fallback; i64/f64/string/bigint with
overflow escalation; static opcode specialization (`_I`/`_F`) via the TypeMap;
i64 fast path for bitops; lists/indexing/`.length`/mutation (write-back for
local targets); control flow; the full builtin library (fs/net/db/regex/json);
async/parallel/channels/atomics (see `docs/vm-audit.md` for the audit).

**Deferred:** rational/decimal tier (exact `0.1 + 0.2`); Karatsuba bignum;
BigInt literals (bignum currently arises only from overflow escalation);
compile-time constant folding; full blocking channel semantics; real upvalue
capture; `io_wait` task suspension. See `docs/vm-audit.md`.
