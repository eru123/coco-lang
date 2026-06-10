# Building Coco

## Prerequisites

- **Rust** 1.80+ (install via [rustup](https://rustup.rs))
- **LLVM 18** (optional, only for `--features native`)

## Quick Start

```bash
# Clone and build
git clone https://github.com/eru123/coco-lang
cd coco-lang
cargo build

# Run the tests
cargo test -p coco_interpreter -p coco_typeck -p coco_parser

# Run a Coco program
cargo run -- run examples/01-hello.co
```

## Feature Flags

| Feature | Description |
|---------|-------------|
| `native` | LLVM AOT compilation (`coco build --native`). Requires LLVM 18. |
| `tree-walker` | Deprecated tree-walking interpreter (not needed for development). |

## Native Compilation (LLVM)

For `coco build --native`:

```bash
# Ubuntu/Debian
sudo apt install llvm-18-dev

# macOS
brew install llvm@18

# Then build with native feature
cargo build --features native
```

If LLVM is not installed system-wide, the build script attempts to download
a prebuilt LLVM 18 tarball automatically.

## Running Tests

```bash
# All core tests
cargo test -p coco_lexer -p coco_parser -p coco_interpreter -p coco_typeck -p coco_safety

# CLI tests
cargo test -p coco_cli

# Run the co test suite
bash scripts/run-co-tests.sh
```

## Project Structure

| Crate | Purpose |
|-------|---------|
| `coco_span` | Source locations and SourceMap |
| `coco_diagnostics` | Ariadne-based error rendering |
| `coco_lexer` | Lexer / tokenizer |
| `coco_syntax` | AST definitions |
| `coco_parser` | Recursive-descent parser |
| `coco_typeck` | Type checker |
| `coco_safety` | Memory safety analysis |
| `coco_interpreter` | Bytecode VM (compiler + runtime) |
| `coco_formatter` | Code formatter |
| `coco_codegen` | LLVM native codegen (optional) |
| `coco_cli` | CLI tool (`coco` binary) |
