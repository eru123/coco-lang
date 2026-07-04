# VM Audit: async, stdlib, and silent-no-op sweep

A sweep of the bytecode VM (`coco_interpreter`) for stubs, silent no-ops, and
gaps — the same class of bug that previously caused `native_while` to spin
forever. This documents what was found, what was fixed, and what is deferred.

## Fixed this pass

### `OP_STORE_INDEX` / `OP_STORE_MEMBER` — were silent no-ops
`a[i] = x` and `obj.field = x` silently did nothing: the opcodes popped the
value, index, and collection, then pushed the value back without mutating.
**Fixed:** implemented real mutation via `Arc::make_mut` (copy-on-write), with
`OP_STORE_INDEX_LOCAL` / `OP_STORE_MEMBER_LOCAL` write-back opcodes for local
targets so the mutation is visible to the binding (the plain opcodes only
mutate the stack copy, which CoW hides when the Arc is shared). Added `OP_SWAP`
for the compound-assign stack reordering.

### `OP_PARALLEL_RUN` `<noop>` fallback — was silent degradation
When a handle passed to `parallel { run ... }` wasn't a runnable task
(already-completed, a non-task value, or a task whose closure vanished), the VM
silently substituted an empty `<noop>` function. **Fixed:** now a hard error
with a clear message — silent degradation masks bugs.

### Bitops — were always BigInt
`vm_bitop` and `OP_BIT_NOT` always converted to `BigInt` even for `Int64`
operands. **Fixed:** i64 fast path with BigInt fallback (bitops on small ints
no longer allocate).

## Intentional no-ops (by design, documented)

### `OP_CLOSE_UPVALUE` — no-op
Closures in this VM do not capture escaping locals (free variables resolve as
globals, and all values are `Arc`/`Copy`). So closing an upvalue is a no-op.
This is a deliberate architectural choice, not a bug. Real upvalue capture
would be a larger refactor (tracked as deferred).

## Deferred (feature work, not silent bugs — they error clearly)

### Channel blocking — errors instead of blocks
`chan_send` on a full channel and `chan_recv` on an empty channel currently
*error* rather than suspending the task cooperatively. The scheduler supports
`suspend_awaiting`, so full blocking semantics are achievable; this is feature
work. Until then, the non-blocking contract is documented in the error
messages.

### `io_wait` task suspension
`io_loop.rs` provides mio-backed fd-readiness, but does not suspend the Coco
task — it reports readiness without parking the VM. Wiring it to the
scheduler's suspend mechanism is a larger refactor (deferred).

### Rational/decimal exact tier
`0.1 + 0.2` is inexact (IEEE f64) like most languages. An exact rational/
decimal tier (Scheme-style) is the highest-value deferred tower item — needs a
new `Value` variant and full arithmetic.

### Karatsuba bignum; BigInt literals
Bignum arithmetic is schoolbook (correct, not fast). AST integer literals are
`i64` (large literals truncate); bignum arises only from overflow escalation.

## Stdlib — all real, no stubs

The full builtin library is implemented (not stubbed): `print`/`len`/`toString`
/`range`, list/map ops (copy-returning), string ops, JSON, regex, base64/hex,
fs, tcp, process, time, SQLite (`db`, feature-gated), `io_wait` (feature-gated
`async-io`). Unknown builtins error loudly (`unknown builtin '{}'`).
