# Coco Standard Library (`std/`)

> High-level organization of the built-in standard library. Detailed API docs are deferred — this is the module structure spec.

---

## Design principles

- **Batteries included.** Common backend tasks should not require third-party packages.
- **Consistent naming.** Modules use short, descriptive names (not `std/filesystem`, just `std/fs`).
- **Tree-shakeable.** Importing one function does not pull in the whole module (compiler-level when native compilation lands).
- **Web-first ergonomics.** HTTP, JSON, crypto are first-class.

---

## Module index

### Core runtime

| Module | Purpose | Key exports |
|---|---|---|
| `std/context` | Cancellation, deadlines, values | `Context`, `withCancel`, `withTimeout`, `withDeadline` |
| `std/process` | CLI args, exit, signals, env | `args`, `exit`, `signal`, `env`, `cwd`, `pid` |
| `std/time` | Time, timers, sleep | `sleep`, `now`, `since`, `until`, `Timer`, `Ticker` |
| `std/async` | Async primitives | `parallel`, `select`, `chan`, `Atomic`, `Mutex`, `lazy` |

### Data

| Module | Purpose | Key exports |
|---|---|---|
| `std/json` | JSON parsing and serialization | `parse`, `stringify`, `JsonValue` |
| `std/crypto` | Hashing, encryption, random | `sha256`, `md5`, `randomBytes`, `randomInt`, `hash`, `hmac` |
| `std/encoding` | Base64, hex, url encoding | `base64Encode`, `base64Decode`, `hexEncode`, `hexDecode`, `urlEncode`, `urlDecode` |

### I/O

| Module | Purpose | Key exports |
|---|---|---|
| `std/fs` | File system operations | `readFile`, `writeFile`, `exists`, `mkdir`, `readDir`, `remove`, `stat`, `watch` |
| `std/path` | Path manipulation | `Path`, `join`, `dirname`, `basename`, `extname`, `resolve`, `relative`, `isAbsolute` |
| `std/io` | Streams and buffers | `stdin`, `stdout`, `stderr`, `Readable`, `Writable`, `Buffer` |

### Networking

| Module | Purpose | Key exports |
|---|---|---|
| `std/http` | HTTP server and client | `Server`, `Request`, `Response`, `Middleware`, `Router`, `listen`, `fetch`, `get`, `post` |
| `std/net` | TCP, UDP, TLS | `listen`, `dial`, `Conn`, `TlsConfig`, `resolve` |
| `std/url` | URL parsing | `URL`, `parse`, `format` |

### Data stores

| Module | Purpose | Key exports |
|---|---|---|
| `std/db` | Database abstraction | `query`, `queryOne`, `insert`, `update`, `delete`, `transaction`, `Pool` |
| `std/cache` | In-memory cache | `Cache`, `TTLCache`, `get`, `set`, `delete`, `clear` |

### Data structures

| Module | Purpose | Key exports |
|---|---|---|
| `std/collections` | Advanced data structures | `Queue`, `Stack`, `Set`, `OrderedMap`, `PriorityQueue`, `LRU` |

### Format and parsing

| Module | Purpose | Key exports |
|---|---|---|
| `std/regex` | Regular expressions | `Regex`, `compile`, `match`, `replace`, `split` |
| `std/csv` | CSV parsing | `parse`, `stringify`, `CsvOptions` |
| `std/yaml` | YAML parsing | `parse`, `stringify` |
| `std/xml` | XML parsing | `parse`, `stringify` |

### Math and science

| Module | Purpose | Key exports |
|---|---|---|
| `std/math` | Math constants and functions | `abs`, `ceil`, `floor`, `round`, `max`, `min`, `pow`, `sqrt`, `random` |
| `std/random` | Random number generation | `int`, `float`, `string`, `shuffle`, `pick`, `uuid` |

### Testing (dev dependency)

| Module | Purpose | Key exports |
|---|---|---|
| `std/test` | Test runner and assertions | `test`, `describe`, `it`, `expect`, `assert`, `mock`, `bench` |

---

## Module granularity guidelines

- **One purpose per module.** If a module name needs a conjunction, split it.
- **Max ~20 exports per module.** More than that suggests the module should be split.
- **No deep nesting.** Modules are one level deep (`std/http`, not `std/net/http`). Sub-packages are allowed for large domains.
- **Stable first.** Once a module ships in a stable release, its public API is SemVer-locked.

## What's deferred

These modules are planned but not needed for v1:

| Module | Purpose | When |
|---|---|---|
| `std/os` | OS-level operations (signals, users, limits) | v1+ |
| `std/compress` | Gzip, zlib, brotli | v1+ |
| `std/image` | Image processing | v2 |
| `std/ffi` | FFI loading and type mapping | Phase 8 |
| `std/cli` | CLI framework (flags, prompts, progress) | v1+ |
| `std/template` | HTML/text templating | v1+ |
| `std/smtp` | Email sending | v1+ |
