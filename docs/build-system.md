# Build System and Project Artifacts

This document describes how a Coco project is packaged, built, and executed in the current codebase.

## Project layout

A standard Coco project:

```text
my-project/
  coco.toml          # project manifest
  src/
    main.co          # entry point
    ...
  tests/             # optional integration tests
    ...
  vendor/            # vendored dependencies
    ...
  target/            # build cache only; do not redistribute
```

`coco.toml` is used for project metadata and dependency declarations.

## Manifest

```toml
[package]
name = "my-api"
version = "1.0.0"
description = "A fast HTTP API"
authors = ["Jane Doe <jane@example.com>"]
license = "MIT"
edition = "1.0"
```

## Dependencies

```toml
[dependencies]
coco-http = "1.2.0"
coco-json = ">=1.0, <2.0"
coco-cache = { version = "0.5", optional = true }
my-local = { path = "../my-local-lib" }
```

Version constraints follow SemVer ranges (`^`, `~`, `>=`, `<`, `=`).

## Dev dependencies

```toml
[dev-dependencies]
coco-test-utils = "1.0"
coco-mock = "0.3"
```

## Build targets

Bytecode is the primary deliverable:

```bash
coco build            # writes a .cb artifact
```

Run bytecode:

```bash
coco run              # executes the VM path
coco run --no-check   # skips checks
```

Disassembly:

```bash
coco build --disasm
```

## Architecture note

`coco build` emits a `.cb` bytecode artifact through the VM compiler/serialization module. There is no native AOT backend. Treat `target/` as a build cache, not a redistributable artifact.

## Build pipeline

```text
Source (.co)
  -> Lexer (coco_lexer)
  -> Parser (coco_parser)
  -> Type checker (coco_typeck) -> optional typed diagnostics
  -> Safety analyzer (coco_safety) -> optional diagnostics
  -> Compiler (coco_interpreter) -> Chunk
  -> VM execution -> runtime behavior
```

For multi-file projects, module resolution runs between parsing and type checking.

## Entry points

- `src/main.co` is the default entry point for executables
- Entry point may be overridden by configuration or CLI invocation

## Deferred / removed

- No package registry integration yet
- No registry-based dependency resolution yet
- No `coco add`, `coco publish`, or install workflow yet
- Build scripts and code-generation hooks are not implemented
