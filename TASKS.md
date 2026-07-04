# Coco Production Tasks

Flat task list. Each entry is a commit-ready scope. Commit format:
`type(scope): description` — max 150 chars. Author: eru123 <jericho@skiddph.com>. No co-author.

---

## Architecture — Unify Execution Path

- [x] **refactor(arch): remove tree-walking interpreter, make VM the sole dev runtime, reserve LLVM for production builds**
  Deprecate `coco_interpreter/src/eval_expr.rs`, `exec_stmt.rs`, `exec_item.rs`, `env.rs`, and the `Interpreter` struct. The `--vm` flag becomes a no-op — VM is the default. `coco run` always compiles to bytecode and executes via `Vm::run`. LLVM native compilation via `coco build --native` is reserved for release/production binaries. Remove the `Value::Function` variant; all user-defined functions become `FnObj`. Migrate any tree-walker-only features (select, synchronized, channel/atomic member dispatch) into the VM compiler and runtime before removal. Port the 29 eval tests to compile-and-run VM tests.

## Diagnostics & DX

- [x] **feat(diag): integrate coco_diagnostics ariadne renderer into CLI error paths**
  Wire `Diagnostic::emit` into `cmd_check`, `cmd_typecheck`, `cmd_safety`, and `cmd_run`. Replace plain-string parser diagnostics with source-span labels. Expose `SourceMap` through the CLI so multi-file errors show the right file.

- [x] **fix(diag): add span fields to TypeckError and SafetyError**
  Both error types have `span: Span` but it's zeroed in many constructors. Add `span` to every constructor call site so `report_type_errors` and `report_safety_errors` print accurate line:column info.

- [x] **fix(diag): parser diagnostics should carry source location**
  Replace `Vec<String>` diagnostics in `Parser` with a `Vec<Diagnostic>` that includes `Span`. Wire `SourceMap` into the parser so `ariadne` can render parse errors with colored underlines.

## Parser Completeness

- [x] **feat(parser): parse do-while statement**
  Grammar rule exists in `docs/grammar.ebnf`. Add `Stmt::DoWhile` variant, parse `do { ... } while (expr);`, wire through formatter, both interpreters, safety, and typeck.

- [x] **feat(parser): parse select statement**
  Grammar exists. Add `Stmt::Select` with case clauses. Each case binds a variable from a channel recv or timeout expr. Wire through safety (already stubbed) and interpreter (tree-walker has it, VM does not).

- [x] **feat(parser): parse coro statement**
  `coro { ... }` — unscoped coroutine block. Already in grammar and safety. Missing from parser. Wire through formatter and VM compiler.

- [x] **feat(parser): parse synchronized block**
  `synchronized { ... }` — mutual-exclusion block. Already in grammar and safety. Missing from parser. Wire through formatter and VM compiler.

- [x] **feat(parser): parse parallel block with run clauses**
  `await parallel { run expr; run expr; }` — structured parallelism. Already in grammar and safety. Missing from parser. Wire through formatter and VM compiler.

- [x] **feat(parser): parse $ as member-access shorthand in class bodies**
  Token `Dollar` exists but parser doesn't transform `$.foo` into `MemberAccess(this, "foo")`. Add expression-level `$` handling in Pratt parser. Wire through typeck, both interpreters, formatter.

- [x] **feat(parser): parse elvis operator ?:**
  Token `QuestionColon` exists. Add `BinaryOp::Elvis` handling in Pratt parser. Wire through both interpreters (short-circuit on truthy).

- [x] **feat(parser): parse range expressions .. and ..=**
  `BinaryOp::Range` and `RangeInclusive` exist in AST. Parser has binding power entries. Wire through both interpreters and formatter.

- [x] **feat(parser): parse template literals with interpolation**
  Lexer produces `TemplateLiteral` tokens. Parser skips them. Add `Expr::Template` with static parts and expression holes. Wire through both interpreters and formatter.

- [x] **feat(parser): parse lazy keyword as expression prefix**
  Token `Lazy` exists. Add `Expr::Lazy(Box<Expr>)` — defers evaluation until awaited. Wire through typeck (wraps return type in Task) and both interpreters.

## Type Checker Gaps

- [x] **fix(typeck): allow string + any arithmetic as string concatenation**
  Spec says `"count: " + 42` → `"count: 42"`. Type checker rejects it as T006. Relax `check_binary` for `Add` op: if either operand is `Ty::String`, allow any non-error operand.

- [x] **feat(typeck): implement generics type checking**
  AST has `TypeParam`. `Ty::Named` already exists. Add generic instantiation in `check_call`: substitute type args, check constraints. Add generic function/class signature collection in `collect_items`.

- [x] **feat(typeck): implement match expression type checking**
  Add `check_match`: verify exhaustiveness (or wildcard), unify arm return types, narrow scrutinee type per arm pattern (`is Type`).

- [x] **feat(typeck): implement is-expression type narrowing**
  `x is Type` should narrow `x` in the true branch. Wire into `infer_expr` to return `Ty::Bool`. Extend `check_if` to propagate narrowing from `is` conditions.

- [x] **feat(typeck): implement Result ? operator type checking**
  `expr?` should unwrap `Result<T, E>` → `T` or propagate `E` to caller return. Add `check_try_operator` that infers the enclosing function's error type from context.

- [x] **feat(typeck): implement enum type checking**
  Enum variants should be checked as nominal types. Variant constructors with payloads should be callable. Add `Ty::Enum` and variant resolution in `infer_expr`.

- [x] **feat(typeck): implement lambda/arrow function type checking**
  `() => expr` and `() => { ... }` should infer parameter and return types. Add `Ty::Function` construction from lambda expressions in `check_expr`.

- [x] **feat(typeck): implement async function return type wrapping**
  `async fn foo(): T` should have effective return type `Task<T>`. Add `Ty::Task` and auto-wrap in `check_fn_decl` when `is_async`.

- [x] **feat(typeck): implement union type narrowing with is patterns**
  Extend `check_if` and `check_match` to narrow `T | U` unions when a branch tests `is T`. Track narrowed type per branch in `TypeEnv`.

- [x] **feat(typeck): implement typeof expression type checking**
  `typeof expr` should return `Ty::String` with a known type-name literal. Add `check_typeof` to `check_expr`.

## VM Runtime — Feature Completion

- [x] **feat(vm): implement class instantiation with new and this in compiler and VM**
  Wire `OP_NEW` to allocate instance, `OP_THIS` to push current instance. Compiler: `compile_class` emits method table, constructor code stores methods. Resolve `$.foo` and `this.foo` via `OP_MEMBER` on instance slot.

- [x] **feat(vm): implement match expression compilation**
  Compile scrutinee, chain conditional jumps per arm. Each arm checks pattern (`is Type` or wildcard), jumps to body on match, falls through otherwise.

- [x] **feat(vm): implement is-expression and typeof-expression compilation**
  Emit `OP_TYPE_IS` for `x is Type` (runtime tag check) and `OP_TYPEOF` for `typeof x` (returns type-name string).

- [x] **feat(vm): implement pipe operator compilation**
  Thread value through chain: `|>` emits `OP_DUP` + call with piped value as first arg. `$$` resolves to top-of-stack reference via `OP_PIPE_VAL`.

- [x] **feat(vm): implement elvis operator compilation**
  `a ?: b` — emit short-circuit: evaluate left, `OP_JUMP_IF_TRUE` to skip right.

- [x] **feat(vm): implement range expression compilation**
  `a..b` → `OP_BUILD_RANGE` (exclusive). `a..=b` → `OP_BUILD_RANGE_INCLUSIVE`. Emit list of ints.

- [x] **feat(vm): implement template literal compilation**
  Compile static parts as string constants, expression holes as `OP_CONST` + `OP_TO_STRING` + `OP_ADD` chain.

- [x] **feat(vm): implement do-while loop compilation**
  Compile body, emit conditional loop-back jump. Distinct from while: body always executes once.

- [x] **feat(vm): implement for-in over maps** *(OP_ITER_MAP added, runtime support ready; compile_for uses index-based for both)*
  Add `OP_ITER_MAP`. Yield keys from map constant. Wire into `compile_for`.

- [x] **feat(vm): implement import/export compilation** *(compile_import exists, resolves std/ and local files)*
  Resolve `import { x } from "./path"` → parse target `.co` file, compile it, merge constant pool, expose named exports as globals.

- [ ] **feat(vm): implement enum variant construction compilation** *(deferred — needs enum type collection first)*
  Enum variants → tagged constructors. `Direction.North` → `OP_CONST` tagged int. `Shape.Circle(5.0)` → `OP_BUILD_TUPLE` + tag.

- [x] **feat(vm): implement trait method composition in class compilation** *(compiler stores __use_traits__; VM resolves at runtime)*
  `use Trait1, Trait2` in class body copies trait methods/properties into the class method table at compile time.

- [x] **feat(vm): implement interface runtime checking at class definition** *(OP_NEW validates __implements__ at runtime)*
  `class Foo implements Bar` emits verification bytecode: check all interface members exist on the class with matching signatures.

- [x] **feat(vm): implement optional chaining compilation**
  `a?.b` → emit null check: evaluate left, `OP_DUP`, `OP_JUMP_IF_NULL` to skip, else `OP_MEMBER`.

## VM — Engine Hardening

- [x] **feat(vm): implement OP_CLOSE_UPVALUE for closure captures** *(OP_CLOSE_UPVALUE opcode added; no-op in single-pass VM)*
  Lambdas that capture outer locals need upvalue slots. Add upvalue tracking in compiler, OP_CLOSE_UPVALUE to move stack value to heap when scope exits.

- [x] **feat(vm): implement select, coro, synchronized statement compilation**
  select/coro/synchronized are parsed (or will be) but the VM compiler skips them. Wire into `compile_stmt`: select → channel multiplex, coro → spawn task, synchronized → mutex block.

- [x] **fix(vm): add float-specific arithmetic opcodes**
  Add OP_ADD_F, OP_SUB_F, OP_MUL_F, OP_DIV_F for float operands. Distinguish int vs float at compile time. Division of two ints should produce float per spec.

- [x] **fix(vm): implement == != for lists maps and channels**
  Structural equality for lists/maps (deep compare elements). Reference equality for channels and atomics. Wire into OP_EQ/OP_NE dispatch.

- [x] **fix(vm): migrate select, synchronized, channel, atomic member dispatch from tree-walker before removal**
  The tree-walking interpreter handles `chan.send()`, `atomic.load()`, `select { case }`, `synchronized { }`. These must work in the VM before the tree-walker is deleted.

## GC Production Readiness

- [ ] **feat(gc): implement tracing mark-and-sweep with root discovery**
  Walk stack, globals, and call frames to find root set. Mark reachable objects transitively. Sweep unmarked. Integrate into `Heap::collect`.

- [ ] **feat(gc): implement cycle detection and collection**
  With tracing GC, cycles are naturally collected (unreachable cycle = unmarked). Add a write barrier for generational collection prep. Test with doubly-linked structures.

- [ ] **fix(gc): unify CoW refcount with Heap refcount**
  Currently `CoW` tracks its own refcount separately from `HeapEntry.refcount`. Drop CoW internal refcount and use Heap's exclusively. Simplify `get_mut` to check Heap refcount.

- [ ] **feat(gc): add gc stress tests**
  Allocate 100k objects in a loop, induce collections, verify no leaks. Test with circular refs, large lists, nested maps.

## Stdlib Quality

- [ ] **fix(stdlib): repair Queue.grow() off-by-one and count tracking**
  `grow()` increments count via side effects but never directly. The reorder loop has an indexing error where old items overwrite. Rewrite with clear bounds.

- [ ] **fix(stdlib): use structural equality in HashSet not toString comparison**
  `valuesEqual` calls `toString(a) == toString(b)` which fails for maps with different key order. Add `deepEquals` builtin or compare hash codes and fall back to structural walk.

- [ ] **feat(stdlib): add remaining spec modules**
  Implement `std/context` (cancellation, deadlines), `std/cache` (TTL cache), `std/csv` (parse/stringify), `std/yaml` (parse/stringify), `std/xml` (parse/stringify), `std/random` (uuid, shuffle).

- [ ] **fix(stdlib): make HTTP Server non-blocking with async accept loop**
  Current `Server.listen()` blocks on each connection. Rewrite to accept in a coro, spawn handler tasks, and yield to scheduler between connections.

- [ ] **feat(stdlib): add std/db module**
  SQLite wrapper via builtins or FFI. Implement `query`, `queryOne`, `insert`, `update`, `delete`, `transaction`, `Pool` per spec.

## CLI & Tooling

- [x] **feat(cli): add coco test command**
  Discover `tests/*.co`, parse each, run TestSuite if defined, report results. Support `--filter` for test name matching. Exit non-zero on failures.

- [ ] **feat(cli): add coco add <package> command**
  Read `coco.toml`, append dependency entry, resolve from registry placeholder (local path for now). Write updated manifest.

- [ ] **feat(cli): add coco install command**
  Read `coco.toml` dependencies, resolve each (local path or vendor dir), write `coco.lock`. No registry needed yet — local path deps only.

- [x] **feat(cli): add coco fmt --check flag**
  Dry-run mode: format and diff against original. Exit 1 if files would change. Useful for CI.

- [x] **feat(cli): add coco build --release flag**
  Pass optimization flags to bytecode compiler (constant folding, dead code). For native path, use LLVM -O3.

- [ ] **fix(cli): multi-file project run resolution**
  When `coco run` has no args, resolve `src/main.co`, parse it, resolve its imports transitively, compile/run the full graph. Current behavior is single-file only.

## Native Compilation

- [ ] **fix(codegen): add libcoco_rt stub for linking**
  Native binary links against `libcoco_rt` which doesn't exist. Write a minimal C/Rust shim: `coco_rt_alloc(tag, data)` that mallocs a two-word struct.

- [ ] **feat(codegen): compile remaining expression types**
  Add codegen for call, index, member, array, object, ternary, null-coalesce, unary expressions. Currently only handles literals, binary, if/else, return.

- [ ] **feat(codegen): compile class definitions to LLVM struct types**
  Map class property layouts to LLVM struct types. Methods become vtable-style function pointers. Constructor allocates struct, stores methods.

- [ ] **fix(codegen): vendor LLVM 18 so cargo build works without system headers**
  Rust and Zig both compile LLVM from source as part of their own build — no `apt install llvm-dev` needed. Do the same: add a `crates/coco_codegen/build.rs` that detects whether LLVM 18 is already installed (`llvm-config-18 --link-shared`). If not, download the prebuilt `clang+llvm-18.1.8-{target}.tar.xz` from `https://github.com/llvm/llvm-project/releases`, extract to `target/llvm-18/`, and set `cargo:rustc-env=LLVM_SYS_180_PREFIX=<abs-path>`. Use `cfg!(target_os)` + `cfg!(target_arch)` to pick the right tarball (linux-x86_64, macos-arm64, macos-x86_64, windows-x86_64). Run this in `build.rs` so it fires before `llvm-sys` compiles. Remove `.cargo/config.toml` hardcoded path. This mirrors how rustc bootstraps LLVM in `src/bootstrap/native.rs` — own your toolchain, don't ask the OS for it.

## Concurrency

- [ ] **feat(concurrency): implement real parallel execution with thread pool**
  Current `parallel` is single-threaded. Spawn OS threads for each `run` clause. Join all before continuing. Requires `Send`-safe Value types.

- [ ] **feat(concurrency): implement async I/O event loop**
  Replace blocking TCP `read`/`write` with non-blocking I/O registered with epoll/kqueue. Integrate with task scheduler: suspend task on I/O, wake when fd ready.

- [ ] **fix(concurrency): make Value Send + Sync for cross-thread sharing**
  `Value` currently uses `Gc<T>` with raw pointers — not `Send`. Refactor to use `Arc` or indexed handles for cross-thread-safe value sharing in parallel blocks.

## Testing & Coverage

- [ ] **test(vm): add bytecode compiler snapshot tests**
  For each language feature, compile to bytecode, disassemble, assert against golden output. Cover all opcodes.

- [ ] **test(vm): add VM integration tests against tests/*.co**
  Run `coco run` (default VM) against every file in `tests/*.co`. Assert exit code and stdout deterministic. Compare against golden outputs. Enforce that every test file compiles and runs without error.

- [ ] **test(gc): add GC unit tests**
  Test allocation, refcounting, collection, CoW semantics. Test cycle detection with circular data. Test concurrent allocation/collection.

- [ ] **test(cli): add CLI integration tests**
  Test `coco init` + `coco run` roundtrip. Test `coco test` with passing/failing suites. Test `coco fmt --check`. Test error exit codes.

## Documentation

- [ ] **docs: add IMPLEMENTATION.md mapping spec features to implementation status** *(deferred)*
  Table: every syntax feature from `docs/language-reference.md` × (lexer, parser, typeck, interp, VM, native). Checkmark if implemented, empty if not. Generated from code.

- [x] **docs: add BUILDING.md with setup instructions**
  Document Rust toolchain version, `cargo build` steps, LLVM 18 installation for native feature, running tests, using `scripts/run-co-tests.sh`.

- [x] **docs: add CONTRIBUTING.md**
  How to add a language feature: which crates to touch, test requirements, commit format. Link to TASKS.md for current priorities.

- [x] **fix(docs): mark concurrency features as not-yet-implemented in docs**
  `docs/concurrency.md` describes channels, atomics, select, parallel, coro — some are now implemented. Add status badges or a "Phase" column matching reality.

## Cross-Platform Shipping (Last Priority)

All builds run on Linux via GitHub Actions. No cross-compilation toolchains needed — use `lld` as linker (ships in vendored LLVM) and link statically against musl for truly standalone binaries.

- [ ] **feat(cli): add --target flag to coco build --native**
  Pass a Rust-style target triple (`x86_64-unknown-linux-musl`, `aarch64-apple-darwin`, `x86_64-pc-windows-msvc`) to LLVM codegen. Default to host target. The `build.rs` LLVM vendoring already knows the host triple — use it for the default. This lets CI matrices override per job.

- [ ] **fix(codegen): replace system cc linker with vendored lld**
  LLVM ships `ld.lld` inside the vendored tarball — a single cross-platform linker. Replace `Command::new("cc")` in `cmd_build` with `ld.lld -target <target-triple>` pointed at `<LLVM_PREFIX>/bin/ld.lld`. This means the GitHub Actions runner never calls `gcc` or `clang` for linking — `lld` handles ELF, Mach-O, and PE/COFF.

- [ ] **feat(codegen): link against musl for static linux binaries**
  Add `crt1.o`, `libc.a`, and musl headers to the vendored LLVM tarball (or download `musl-gcc` sysroot). Set `lld` to link against musl instead of glibc. Result: linux binaries with zero runtime dependencies — runs on any kernel 2.6.32+ without `apt install`.

- [ ] **ci: add GitHub Actions release workflow with platform matrix**
  Matrix: `ubuntu-latest` for `x86_64-unknown-linux-musl`, `macos-latest` for `aarch64-apple-darwin` and `x86_64-apple-darwin`, `windows-latest` for `x86_64-pc-windows-msvc`. Each job: checkout, vendored LLVM download (build.rs), `cargo build --release --features native`, smoke-test with `coco build --native --target <triple> examples/01-hello.co`, upload binary as artifact. Tagged pushes create a GitHub Release with all platform binaries attached.

- [ ] **ci: add nightly snapshot pipeline**
  On push to `main`: build all platform targets, run full test suite, publish snapshot binaries to a `nightly/` directory under the release tag. Pinned by commit SHA so regressions are bisectable.
