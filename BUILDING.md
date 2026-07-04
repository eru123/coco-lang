# BUILDING.md

## Toolchain

- Rust stable
- Required tools: `cargo`, `rustfmt`, `clippy`
- No system LLVM or native backend is required. The AOT/LLVM backend and
  `--binary` flavor were removed. Use the bytecode VM and CLI tools only.

## Treat `target/` as a build cache only

`target/` is retained for incremental builds. Packaging, CI artifacts,
installers, and release bundles should not ship this directory.

The reproducible deliverable is a release binary, or a `.cb` artifact emitted
by `coco build` alongside source for `coco run` workflows.

## Workspace layout

- crates use `path = "..."` dependencies only; avoid absolute paths
- bytecode artifacts are emitted under the current working directory when
  using `coco build`
- The portable build artifact is the `.cb` file produced by
  `coco_interpreter::serialize_chunk`

## Build

```bash
cargo build
```

## Run

```bash
cargo run -- hello.co
```

## Tests

```bash
cargo test
```

Interpreter tests:

```bash
cargo test -p coco_interpreter
```

CLI tests:

```bash
cargo test -p coco_cli
```

## Formatting

```bash
cargo fmt
```

## Lint

```bash
cargo clippy
```

## CLI reference

- `coco lex FILE.co`
- `coco parse FILE.co`
- `coco fmt FILE.co`
- `coco fmt -w FILE.co`
- `coco check FILE.co`
- `coco typecheck FILE.co`
- `coco safety FILE.co`
- `coco run FILE.co`
- `coco run --no-check FILE.co`
- `coco build FILE.co`
- `coco build --disasm FILE.co`
- `coco test`
- `coco init NAME`

## Packaging notes

- build with `cargo build --release`
- install the release binary into `$PREFIX/bin`
- do not rely on `target/` as a stable bundled or redistributable component
- do not ship `.cb` files from `target/`; serialize artifacts to a user data
  or app path if needed
- no native host toolchain is needed beyond Rust
