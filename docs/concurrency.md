# Coco Concurrency Model

> This document defines how Coco handles async operations, parallelism, and shared state.

## Implementation Status

| Feature | Parser | Type Check | Safety | VM | Notes |
|---------|--------|------------|--------|----|-------|
| `async fn` / `await` | ✅ | ✅ | ✅ | ✅ | Task scheduler in VM |
| `lazy` | ✅ | ✅ stub | — | ✅ | Compiles to async lambda |
| `parallel { run }` | ✅ | ✅ | ✅ | ✅ | Sequential in single-threaded VM |
| `coro { }` | ✅ | — | ✅ | ✅ | Fire-and-forget task |
| `select { case }` | ✅ | — | — | ✅ | OP_SELECT_TRY_RECV in VM |
| `chan<T>` | ✅ | — | — | ✅ | Arc<Mutex<ChannelInner>> |
| `Atomic<T>` | ✅ | — | — | ✅ | Arc<Mutex<AtomicInner>> |
| `synchronized { }` | ✅ | — | — | ✅ | Scoped block (single-threaded) |
| Real multi-threading | — | — | — | ⬜ | Planned |
| Async I/O event loop | — | — | — | ⬜ | Planned |

---

## Overview

Coco provides safe real concurrency with multi-core execution. The developer-facing model is simple; the compiler and runtime enforce safety automatically.

Key primitives:
- `async fn` — asynchronous function
- `await` — collect async result
- `lazy` — defer async execution
- `parallel { run ... }` — structured multi-task execution
- `coro { ... }` — spawn coroutine
- `select { case ... }` — multiplex channels/events
- `chan<T>(capacity)` — typed channel
- `new Atomic<T>(value)` — atomic operations
- `synchronized { ... }` — mutual exclusion block

---

## Async Functions

```coco
async fn fetchUser(id: int): Result<User, HttpError> {
    const response = await http.get(`/users/${id}`)?;
    return Ok(User.fromJson(response.body));
}
```

- `async fn` declares a function that may suspend
- Calling an async function **starts execution immediately** (eager)
- `await` collects the result and suspends until available
- `lazy asyncFn()` defers execution until awaited

```coco
const p = fetchUser(1);          // starts NOW
const task = lazy fetchUser(2);  // cold, does nothing
const user = await task;         // NOW starts and completes
```

---

## Structured Parallelism

```coco
const [user, posts, comments] = await parallel {
    run getUser(id);
    run getPosts(id);
    run getComments(id);
};
```

Rules:
- Child tasks are scoped to the `parallel` block
- Parent waits for all children to complete
- Cancellation propagates to children on error
- Errors from children propagate to parent
- Captured state is checked for safety at compile time

---

## Coroutines

```coco
// Structured (preferred):
await parallel {
    run { processItems(); }
}

// Unscoped (allowed with scrutiny):
coro { backgroundCleanup(); }
```

Unscoped `coro`:
- Allowed but generates compiler warnings
- Subject to lifetime analysis
- Leak detection in debug builds
- Should be used sparingly for fire-and-forget background work

---

## Channels

```coco
const jobs = chan<Job>(100);
const results = chan<JobResult>(100);

coro {
    for job in jobs {
        results.send(await processJob(job));
    }
}

jobs.send(new Job("task-1"));
const result = results.recv();
```

Channel operations:
- `chan<T>(capacity)` — create buffered channel
- `chan<T>()` — create unbuffered (rendezvous) channel
- `.send(value)` — send value (blocks if full)
- `.recv()` — receive value (blocks if empty)
- `.close()` — close channel
- Iterating a channel consumes until closed

---

## Select

```coco
select {
    case msg = inbox.recv():
        handle(msg);
    case _ = timeout(5000):
        print("timed out");
}
```

`select` multiplexes multiple channel operations. First ready case wins.

---

## Atomics

```coco
const counter = new Atomic<int>(0);

await parallel {
    run { counter.add(1); }
    run { counter.add(1); }
}

print(counter.load()); // 2
```

Atomic operations: `.load()`, `.store(v)`, `.add(v)`, `.sub(v)`, `.compareAndSwap(old, new)`

---

## Synchronized Blocks

```coco
const cache = synchronizedMap<string, User>();

await parallel {
    run { cache.set("a", userA); }
    run { cache.set("b", userB); }
}
```

`synchronized { }` provides mutual exclusion for a block of code. The runtime manages the lock.

---

## Race Prevention Rules

**Compile-time rejection:** Any mutable capture of a local variable across `parallel` or `coro` boundaries is a compile error.

Rejected:
```coco
let total = 0;
await parallel {
    run { total += 1; } // ERROR: mutable capture across parallel boundary
}
```

Accepted alternatives:
```coco
// Atomics:
const total = new Atomic<int>(0);
await parallel {
    run { total.add(expensiveA()); }
    run { total.add(expensiveB()); }
}

// Channels:
const results = chan<int>();
await parallel {
    run { results.send(expensiveA()); }
    run { results.send(expensiveB()); }
}
const total = results.recv() + results.recv();

// Collect from parallel:
const [a, b] = await parallel {
    run expensiveA();
    run expensiveB();
};
const total = a + b;
```

**Immutable sharing is always safe:** Reading `const` values from parallel tasks is allowed.

```coco
const config = loadConfig();
await parallel {
    run { useConfig(config); }  // OK: config is immutable
    run { useConfig(config); }  // OK: reading shared immutable data
}
```

---

## Cancellation

```coco
async fn worker(ctx: Context, jobs: Receiver<Job>): Result<void, Error> {
    loop {
        select {
            case job = jobs.recv():
                await process(job)?;
            case _ = ctx.cancelled():
                return Ok();
        }
    }
}
```

Cancellation primitives:
- `Context` — carries deadline, cancellation signal, and values
- `ctx.cancelled()` — channel that closes on cancellation
- `ctx.withTimeout(ms)` — derived context with deadline
- `ctx.withCancel()` — derived context with manual cancel function
