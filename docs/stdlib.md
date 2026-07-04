# Coco Standard Library

This document describes stdlib surface area available in the current runtime and builtins.

## Core runtime

- context values, cancellation, timeouts
- process and environment access
- timers and time primitives
- async task primitives

## Data

- JSON parsing/serialization
- PCRE-style regular expressions
- hashing and random primitives
- base64/hex/url encoders

## I/O

- filesystem operations
- path utilities
- stdio streams/buffers

## Networking

- HTTP client/server surface
- TCP/UDP/TLS primitives
- URL parsing

## Data stores

- database abstraction surface
- in-memory caching primitives

## Collections

- list, map, queue, set, ordered map, priority queue, LRU concepts where applicable

## Formatting

- CSV, YAML, XML parsing/serialization surface where implemented

## Math/science

- math constants and functions
- random number generation utilities

## Testing

- test runner and assertion helpers

## Notes

- Some modules are registered as builtins rather than importable packages. How to load them depends on the future module system.
- This document describes intended module shape, not final stabilized public API.
