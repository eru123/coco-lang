---
name: "source-command-commit"
description: "Generate terse Conventional Commits messages from staged changes. Use when user says \"commit\", \"write commit message\", or invokes /commit."
---

# source-command-commit

Use this skill when the user asks to run the migrated source command `commit`.

## Command Template

Write commit messages terse and exact. Conventional Commits format. No fluff. Why over what.

## Rules

**Subject line:**
- `<type>(<scope>): <imperative summary>` — `<scope>` optional
- Types: `feat`, `fix`, `refactor`, `perf`, `docs`, `test`, `chore`, `build`, `ci`, `style`, `revert`
- Imperative mood: "add", "fix", "remove" — not "added", "adds", "adding"
- ≤50 chars when possible, hard cap 72
- No trailing period
- Match project convention for capitalization after the colon (lowercase)

**Body (only if needed):**
- Skip entirely when subject is self-explanatory
- Add body only for: non-obvious *why*, breaking changes, migration notes, linked issues
- Wrap at 72 chars
- Bullets `-` not `*`
- Reference issues/PRs at end: `Closes #42`, `Refs #17`

**What NEVER goes in:**
- "This commit does X", "I", "we", "now", "currently" — the diff says what
- "As requested by..." — use Co-authored-by trailer
- "Generated with Codex" or any AI attribution
- Emoji
- Restating the file name when scope already says it

## Execution

1. Run `git status` and `git diff --cached` (or `git diff` if nothing staged) to see what changed
2. Analyze the semantic meaning of changes
3. Output commit message as a fenced code block ready to paste
4. Do NOT run `git commit`, stage files, or amend — only generate the message

## Auto-Clarity

Always include body for: breaking changes, security fixes, data migrations, reverts. Never compress these into subject-only.

## Scope conventions for this project

- `lexer` — coco_lexer crate
- `parser` — coco_parser crate
- `fmt` — coco_formatter crate
- `ast` — coco_syntax crate
- `cli` — coco_cli crate
- `span` — coco_span crate
- `diag` — coco_diagnostics crate
- No scope for cross-cutting changes
