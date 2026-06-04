# Build System

> How Coco projects are structured, configured, and compiled.

---

## Project structure

A minimal Coco project:

```
my-project/
├── coco.toml          # project manifest (required)
├── src/
│   ├── main.co        # entry point
│   └── ...            # other source files
├── tests/             # test files (optional)
│   └── ...
└── vendor/            # dependencies (managed by package manager)
    └── ...
```

`coco.toml` must exist at the project root. It declares metadata, dependencies, and compiler settings.

## `coco.toml` — the project manifest

### `[package]`

```toml
[package]
name = "my-api"
version = "1.0.0"
description = "A fast HTTP API"
authors = ["Jane Doe <jane@example.com>"]
license = "MIT"
edition = "1.0"
```

| Field | Required | Description |
|---|---|---|
| `name` | Yes | Package name (lowercase, hyphens allowed) |
| `version` | Yes | SemVer string |
| `description` | No | Short description |
| `authors` | No | List of `"Name <email>"` |
| `license` | No | SPDX identifier |
| `edition` | No | Coco language edition (default: latest) |

### `[dependencies]`

```toml
[dependencies]
coco-http = "1.2.0"                   # exact version
coco-json = ">=1.0, <2.0"            # version range
coco-cache = { version = "0.5", optional = true }
my-local = { path = "../my-local-lib" }  # local path dependency
```

Version constraint syntax follows SemVer ranges (`^`, `~`, `>=`, `<`, `=`).

### `[dev-dependencies]`

Dependencies only needed for testing:

```toml
[dev-dependencies]
coco-test-utils = "1.0"
coco-mock = "0.3"
```

### `[build]`

```toml
[build]
target = "linux-x86_64"              # cross-compilation target (native if omitted)
output = "bin/my-api"                # custom output path (default: target/<name>)
optimize = true                      # enable release optimizations (default: false)
debug = false                        # include debug symbols (default: true in dev)
strip = true                         # strip symbols from binary (default: false in release)
```

| Field | Default | Description |
|---|---|---|
| `target` | host triple | Cross-compilation target (`linux-x86_64`, `macos-arm64`, etc.) |
| `output` | `target/<name>` | Custom binary output path |
| `optimize` | `false` | Enable optimization passes |
| `debug` | `true` (dev) | Include debug symbols |
| `strip` | `false` | Strip debug symbols |

### `[safety]`

```toml
[safety]
mode = "application"                     # application | library | systems
allow_unsafe_dependencies = false        # block deps with unsafe code

[safety.allow]
ffi = "sqlite3"                          # allowlist FFI libraries
deps = ["coco-ffi-png"]                  # allowlist unsafe packages
```

| Field | Default | Description |
|---|---|---|
| `mode` | `"application"` | Safety enforcement level |
| `allow_unsafe_dependencies` | `false` | Allow dependencies that use `unsafe` |
| `[safety.allow].ffi` | `""` | Semicolon-separated allowed FFI library names |
| `[safety.allow].deps` | `[]` | Explicitly allowlisted unsafe package names |

### `[scripts]` (optional)

```toml
[scripts]
build = "echo Building..."          # pre-build hook
test = "coco test --coverage"       # custom test command
lint = "coco check --strict"        # lint hook
```

Scripts run via `coco run-script <name>`.

## Compilation modes

### Interpreted (current — Phase 3-6)

Source is parsed and executed directly by the tree-walking interpreter:

```bash
coco run src/main.co           # interpret a file
coco run --no-check src/main.co  # skip type + safety checks
```

### Bytecode VM (Phase 7)

```bash
coco build                      # compile to bytecode
coco run                        # run bytecode (faster than interpreted)
```

### Native compilation (Phase 11)

```bash
coco build                      # compile to native binary
coco build --release            # optimized native binary
./target/my-api                 # run the binary directly
```

## CLI reference

| Command | Phase | Description |
|---|---|---|
| `coco lex <file>` | 2 | Tokenize and print tokens |
| `coco parse <file>` | 2 | Parse and print AST |
| `coco fmt <file>` | 2 | Format source code |
| `coco fmt -w <file>` | 2 | Format and write in-place |
| `coco check <file>` | 2+ | Parse + type check + safety analysis |
| `coco typecheck <file>` | 4 | Type check only |
| `coco safety <file>` | 5 | Safety analysis only |
| `coco run <file>` | 3 | Interpret and execute |
| `coco run --no-check` | 3 | Execute without type/safety checks |
| `coco build` | 7+ | Compile (bytecode or native) |
| `coco test` | 7+ | Run test suite |
| `coco init <name>` | 10 | Scaffold a new project |
| `coco add <package>` | 10 | Add a dependency |
| `coco install` | 10 | Install dependencies |

## Build pipeline

The compiler pipeline for a single file:

```
Source (.co)
  → Lexer          (coco_lexer)     → Token stream
  → Parser         (coco_parser)    → AST
  → Type checker   (coco_typeck)    → Typed AST + diagnostics
  → Safety analyzer (coco_safety)   → Safety diagnostics
  → Interpreter/VM (coco_interpreter / future VM) → Execution
  → (future) Codegen                → Native binary
```

For multi-file projects, module resolution runs between parsing and type checking.

## Entry points

- `src/main.co` is the default entry point for executables
- Library projects (no `main.co`) compile to a shared object or archive
- Entry point can be overridden with `[build].entry` in `coco.toml`

## Dependencies resolution

Dependencies are resolved from:

1. `vendor/` directory (vendored dependencies, highest priority)
2. Package registry (future — Phase 10)
3. Git repositories (`coco add <git-url>`, future)
4. Local paths (`path = "..."` in `coco.toml`)

## Lockfile

`coco.lock` records exact dependency versions for reproducible builds. It is generated by `coco install` and should be committed to version control.

## What's deferred

| Feature | When |
|---|---|
| Package registry (`coco add`, `coco publish`) | Phase 10 |
| Workspaces (multi-crate projects) | Phase 10 |
| Build scripts and code generation | Phase 11 |
| Cross-compilation support | Phase 11 |
| Incremental compilation | Phase 11 |
| LTO (link-time optimization) | Phase 11 |
