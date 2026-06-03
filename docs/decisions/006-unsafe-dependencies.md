# ADR-006: Unsafe Dependencies Blocked by Default

**Status:** Accepted
**Date:** 2026-06-03

## Context

In application safety mode, should packages that use `unsafe` internally be allowed as dependencies?

## Decision

Blocked by default in `application` mode. Must be explicitly allowlisted in `coco.toml`.

```toml
[safety]
mode = "application"
allow_unsafe_dependencies = false

[safety.allow]
coco-ffi-png = "audited"
```

## Rationale

- Forces awareness of which dependencies use unsafe code
- Application developers should not unknowingly depend on unsafe code
- Allowlisting creates an audit trail
- Library authors are incentivized to minimize unsafe usage

## Consequences

- Some packages will be blocked until allowlisted
- Package metadata must declare unsafe usage
- First-time setup may require allowlisting common packages
- `coco build` clearly reports which packages are blocked and why
