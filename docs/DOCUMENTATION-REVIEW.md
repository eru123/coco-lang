# Coco Documentation Review — 2026-06-04

## Status: ✅ Documentation synchronized and optimized

---

## Updated Files

### 1. Grammar (EBNF)
**File:** `docs/grammar.ebnf`
- ✅ Added pipe operators (`|>`, `->`, `<|`, `<-`) to expression hierarchy
- ✅ Added `$$` pipe placeholder to primary expressions
- ✅ Updated version to 1.1 with changelog comment
- ✅ Added note about pipe direction mixing being a semantic error
- ✅ Updated reserved words to include special symbols documentation

### 2. Language Reference
**File:** `docs/language-reference.md`
- ✅ Comprehensive pipe operator section with `$$` placeholder
- ✅ Examples showing both `$$` and lambda syntax
- ✅ Clear rules about valid/invalid operator mixing
- ✅ When to use pipes vs method chaining guidance
- ✅ Operator table updated with pipe operators

### 3. Architecture Decision Records
**File:** `docs/decisions/016-pipe-operator.md`
- ✅ Complete ADR with context, decision, rationale
- ✅ `$$` placeholder design and scope restriction
- ✅ Implementation notes for parser, type checker, codegen
- ✅ Error message examples
- ✅ Alternatives considered with rationale

### 4. Examples
**File:** `examples/21-pipe-operator.co`
- ✅ 10 comprehensive examples demonstrating all features
- ✅ Shows `$$` usage with functions and methods
- ✅ Demonstrates mixing `$$` with lambdas
- ✅ Right-to-left and left-to-right examples
- ✅ Integration with async/await and Result propagation
- ✅ Invalid examples (commented out) with error explanations

### 5. Main README
**File:** `README.md`
- ✅ Updated main example to show pipe operators with `$$`
- ✅ Demonstrates real-world usage in HTTP server

### 6. Editor Support
**File:** `editors/vscode/syntaxes/coco.tmLanguage.json`
- ✅ Syntax highlighting for all four pipe operators
- ✅ Syntax highlighting for `$$` placeholder

---

## Redundant/Outdated Files Analysis

### Keep (Active Documentation)

1. **`docs/charter.md`** — Core language identity (referenced by CLAUDE.md)
2. **`docs/language-reference.md`** — Primary syntax guide (30-min read, up to date)
3. **`docs/type-system.md`** — Type system deep dive (complementary to language-reference)
4. **`docs/concurrency.md`** — Concurrency model specification
5. **`docs/safety-promise.md`** — Memory safety guarantees
6. **`docs/grammar.ebnf`** — Formal grammar (now includes pipe operators)
7. **`docs/decisions/*.md`** — 16 ADRs documenting design decisions (all current)

### Consider Removing (Redundant)

1. **`docs/PIPE-OPERATOR-SUMMARY.md`** — Duplicates ADR 016 content
   - **Recommendation:** Remove. ADR 016 is comprehensive and canonical.
   - All info in summary is in either ADR 016 or language-reference.md

2. **`docs/AUDIT-2026-06-04.md`** — Historical audit findings
   - **Recommendation:** Keep for historical reference (shows issues were addressed)
   - Consider archiving to `docs/archive/` or `docs/audits/`

3. **`docs/AUDIT-ACTIONS-2026-06-04.md`** — Audit remediation log
   - **Recommendation:** Keep for historical reference
   - Consider archiving alongside audit document

4. **`docs/superpowers/`** — Agentic workflow metadata
   - **Recommendation:** Keep if using Claude Code workflows, otherwise archive
   - These are workflow instructions, not language spec

### Consolidation Opportunities

**None identified.** The core documentation is well-structured:
- Charter = identity
- Language-reference.md = syntax overview
- Type-system.md = type system deep dive
- Concurrency.md = concurrency deep dive  
- Safety-promise.md = safety deep dive
- ADRs = design decisions with rationale
- Grammar = formal specification

Each serves a distinct purpose without significant overlap.

---

## Missing Documentation

1. ~~**Module Resolution** — How imports resolve to files~~ → `docs/module-resolution.md` (2026-06-04)

2. ~~**Standard Library Outline** — What's in `std/`?~~ → `docs/stdlib.md` (2026-06-04)

3. ~~**FFI Specification** — How unsafe {} and ffi.load() work~~ → `docs/ffi.md` (2026-06-04)

4. ~~**Build System** — Package manifest, dependencies, compilation~~ → `docs/build-system.md` (2026-06-04)

All four previously missing specs are now documented.

---

## Consistency Check

### Syntax Consistency ✅

| Feature | EBNF | language-reference.md | Examples | Status |
|---------|------|----------------------|----------|--------|
| `function`, `fn`, `f` keywords | ✅ | ✅ | ✅ | Consistent |
| Pipe operators `\|>`, `->`, `<\|`, `<-` | ✅ | ✅ | ✅ | Consistent |
| `$$` placeholder | ✅ | ✅ | ✅ | Consistent |
| `$` for `this` | ✅ | ✅ | ✅ | Consistent |
| Single quotes for chars only | ✅ | ✅ | ✅ | Consistent |
| `new Atomic<T>(value)` | ✅ | ✅ | ✅ | Consistent |
| Result propagation `?` | ✅ | ✅ | ✅ | Consistent |
| Match expressions | ✅ | ✅ | ✅ | Consistent |

### Documentation Cross-References ✅

- CLAUDE.md references language-reference.md ✅
- README examples match language-reference.md ✅
- ADRs cross-reference each other appropriately ✅
- Examples demonstrate features from language-reference.md ✅

---

## Recommendations

### Immediate Actions

1. **Remove redundant summary:**
   ```bash
   git rm docs/PIPE-OPERATOR-SUMMARY.md
   ```
   All content is in ADR 016 and language-reference.md.

2. **Archive audit documents:**
   ```bash
   mkdir -p docs/archive/audits
   git mv docs/AUDIT-2026-06-04.md docs/archive/audits/
   git mv docs/AUDIT-ACTIONS-2026-06-04.md docs/archive/audits/
   ```
   Keep for history but move out of main docs/ directory.

### Phase 5 Completion Tasks

1. ~~**Write module resolution spec**~~ → `docs/module-resolution.md`
2. ~~**Write stdlib organization spec**~~ → `docs/stdlib.md`
3. ~~**Write FFI spec**~~ → `docs/ffi.md`
4. ~~**Write build system spec**~~ → `docs/build-system.md`
5. ~~**Update CLAUDE.md**~~ (noted in docs resolution)

### Long-Term (Phase 6+)

1. Create `docs/stdlib/` directory with detailed API reference
2. Implement module resolution in the compiler
3. Implement stdlib modules as they ship

---

## Summary

✅ **Grammar updated** — Pipe operators and `$$` in EBNF  
✅ **Documentation synchronized** — All docs reflect current design  
✅ **Examples comprehensive** — 21 example files demonstrate features  
✅ **No critical inconsistencies** — Syntax matches across EBNF, docs, examples  

**Cleanup needed:** Remove PIPE-OPERATOR-SUMMARY.md (redundant), archive audit files

**Missing specs:** Module resolution, stdlib outline (not blocking Phase 0-1 completion)
