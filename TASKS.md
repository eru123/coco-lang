# Coco Production Tasks

Flat task list. Each entry is a commit-ready scope. Commit format:
`type(scope): description` — max 150 chars. Author: eru123 <jericho@skiddph.com>. No co-author.

| Status | ID | Scope |
|--------|----|-------|
| `[x]` | 1 | Near-term completion |
| `[ ]` | 2 | Packaging & distribution |
| `[ ]` | 3 | Language + tooling polish |
| `[ ]` | 4 | Runtime stability |
| `[ ]` | 5 | Project graph |
| `[ ]` | 6 | Performance measurement |
| `[ ]` | 7 | Docs/docs sync |
| `[ ]` | 8 | Packaging |
| `[ ]` | 9 | Release gate |

## Near-term

- `[x]` Type system: basic inference, unions, recursion, safety constraints
- `[x]` Error runtime: Result propagation, exception handling, decorrelated handlers
- `[x]` Safety analysis: escape/capture analysis, race prevention
- `[x]` Concurrency model: async/await, channels, blocking I/O, select
- `[x]` Parser/syntax: classes, interfaces, traits, generics, super
- `[x]` Stdlib: regex/JSON, DB, IO, process/time, TCP, channels

## Top-level docs

- `[x]` **Top-level docs** — update README/BUILDING/CLAUDE/CONTRIBUTING/TASKS to reflect VM-only execution, no LLVM backend.

| Status | ID | Doc | Notes |
|--------|----|-----|-------|
| `[x]` | 1 | README.md | Current architecture, CLI, build/test paths |
| `[x]` | 2 | BUILDING.md | Toolchain and build workflow |
| `[x]` | 3 | CLAUDE.md | Agent guidance and conventions |
| `[x]` | 4 | CONTRIBUTING.md | Commit format, PR workflow |
| `[ ]` | 5 | TASKS.md | This file; keep task groupings current |

## Packaging

- `[ ]` **Package manager release checklist**
- `[ ]` TarCZ2 bundle for Linux releases
- `[ ]` macOS release workflow
- `[ ]` Windows packaging test
- `[ ]` Landing page content

## Native execution path research

- `[ ]` **Native execution platform research** — if a future translation target is needed, design and document an explicit platform/backend model; do not enable an LLVM path without a tracked decision.
- `[ ]` Define versioning + changelog workflow

## Docs audit

- `[ ]` **Docs audit** — verify README, BUILDING, docs, and code comments describe the VM-only runtime and current stdlib surface.
- `[~] AOT/native backend removal complete; no stale references remain in code comments or docs.

## Release gates

- `[ ]` **Release gate** — test/typecheck/check/ci/bins all green and documented before any release tag.
