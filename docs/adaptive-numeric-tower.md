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
it in a span-keyed `TypeMap` (`crates/coco_typeck/src/typemap.rs`). The native
codegen consults this map in `compile_binary` (`crates/coco_codegen/src/lib.rs`):

- Both operands `Ty::Int`/`Ty::Uint` → emit a native i64 op **with an overflow
  guard** that, on overflow, branches to a runtime call (Tier 1). This is the
  Shewchuk-style escalation, generalized: the fast path runs first, and the
  exact-but-slow path runs only when the fast path can't stay correct.
- Both operands `Ty::Float` → emit a native f64 op (Tier 2).
- Either operand `Ty::Unknown`/`Ty::Mixed` (untyped code) → emit a runtime
  `coco_add`/`coco_sub`/... call that dispatches on the value's tag at runtime
  (fallback).

Because Coco is gradually typed, unannotated code (e.g. `fn add(a, b)`) still
compiles — it just takes the dynamic fallback. Annotated code (`fn add(a: int,
b: int): int`) gets the native fast path. The user pays for abstraction only
when they don't annotate.

### Dynamic (runtime)

The C runtime (`crates/coco_rt/c/arith.c`) implements `coco_add`/`coco_sub`/
`coco_mul`/`coco_div`/`coco_mod`, each dispatching on the operands' tags:

- `int + int` (no overflow) → native i64 add (Tier 0)
- `int + int` (overflow) → bignum (Tier 1, exact)
- any `float` operand → f64 (Tier 2)
- `string + string` (for `+` only) → concatenation (Tier 3)
- `bigint` involved → bignum (Tier 1)

The runtime is the single source of truth for tier selection; the codegen's
static specialization is an optimization that bypasses it when types are known.

## Edge-case decisions

These are deliberate choices, documented as the spec the codegen and runtime
implement against:

- **Integer overflow → bignum, never wrap.** `INT64_MAX + 1` is `9223372036854775808`, exact. (Python/Scheme lineage, not Julia/Rust.)
- **`int + float` → `float`.** The int is promoted to f64; this is lossy for ints beyond 2⁵³, matching most languages. (A future rational/decimal tier could avoid this.)
- **Division by zero:** integer `÷0` aborts with a diagnostic; float `/0` yields IEEE `inf`/`nan`.
- **Modulo** is truncated toward zero; the remainder takes the sign of the dividend (C semantics, matching the interpreter).
- **Equality is type-strict:** `1 == 1.0` is `false`, `1 == "1"` is `false` (matches the interpreter's `value_eq`).
- **Comparisons** promote `int`→`float` when mixed; `string < string` is lexicographic by bytes.
- **`+` is overloaded** for string concatenation, but only when *both* operands are strings (no JS-style `"a" + 1` coercion).
- **`-INT64_MIN`** escalates to bignum (would overflow i64).

## Value model

Every value is a heap-allocated, refcounted, tagged `coco_val` (see
`crates/coco_rt/c/coco_rt.h`):

```c
typedef struct coco_val {
    coco_tag tag;       // INT | FLOAT | BIGINT | STRING | BOOL | NULL | LIST | MAP
    int refcount;       // refcounted (matches interpreter's Arc model; no tracing GC)
    union {
        int64_t i;          // COCO_INT
        double f;           // COCO_FLOAT
        coco_bigint *bi;    // COCO_BIGINT
        coco_str *s;        // COCO_STRING
        bool b;             // COCO_BOOL
        coco_list *l;       // COCO_LIST
        coco_map *m;        // COCO_MAP
    } u;
} coco_val;
```

Refcounting (not tracing GC) matches the interpreter, where `List`/`Map` are
`Arc`-backed and `gc_ref()` returns `None`. Map keys are strings only.

## Building native code

The native codegen requires LLVM 18. Provide it one of these ways (see
`.cargo/config.toml` and `scripts/fetch-llvm.sh`):

1. **System LLVM** — `apt-get install llvm-18-dev libpolly-18-dev` (puts
   `llvm-config-18` on PATH; `llvm-sys` finds it automatically).
2. **Vendored** — `scripts/fetch-llvm.sh` downloads a prebuilt LLVM 18; export
   the `LLVM_SYS_180_PREFIX` it prints before building.

Then:

```
cargo build -p coco_cli --features native
./target/debug/coco build --native program.co   # produces ./program
```

The runtime (`libcoco_rt.a`) is compiled from C by `coco_rt`'s build script via
the `cc` crate and linked into each native binary automatically — no separate
build step required.

## What's implemented vs. deferred

**Implemented:** tiers 0–3 + dynamic fallback; i64/f64/string/bigint; lists,
indexing, `.length`; control flow (if/else-if/else, while, for-over-range,
loop, do-while, break/continue); logical &&/||, %, bitwise, shifts; lambdas,
match, templates, and member access are *not* yet implemented (they error
clearly rather than silently no-op).

**Deferred:** rational/decimal tier (exact `0.1 + 0.2`); Karatsuba bignum;
async/tasks/channels/atomics; the full builtin library (fs/net/db/regex/json);
BigInt literals (bignum currently arises only from overflow escalation);
compile-time constant folding.
