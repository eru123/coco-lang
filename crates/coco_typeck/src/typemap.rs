//! A span-keyed map of inferred expression types, produced during type
//! checking and consumed by the native codegen to specialize arithmetic (and
//! other operations) on statically-known types.
//!
//! See `docs/adaptive-numeric-tower.md`: when both operands of a binary op
//! resolve to a concrete `Ty` (e.g. `Int`+`Int`), the codegen emits a native
//! fast-path op; when either is `Unknown`/`Mixed`, it falls back to a runtime
//! tag-dispatched call. This map is how the codegen learns the static types.

use std::collections::HashMap;

use coco_span::Span;

use crate::types::Ty;

/// Maps an AST node's `Span` to its inferred `Ty`. Spans are unique per node
/// within a single source file (the lexer is monotonic), so they serve as
/// stable node identities without adding IDs to the AST.
#[derive(Debug, Default, Clone)]
pub struct TypeMap {
    inner: HashMap<Span, Ty>,
}

impl TypeMap {
    /// Create an empty map.
    pub fn new() -> Self {
        Self { inner: HashMap::new() }
    }

    /// Record the inferred type for the node at `span`.
    pub fn insert(&mut self, span: Span, ty: Ty) {
        // Last-write-wins; collisions shouldn't happen for distinct nodes,
        // but zero-width or synthesized spans could clash. We keep the first
        // (most-outer) inference to prefer broader context.
        self.inner.entry(span).or_insert(ty);
    }

    /// Look up the inferred type for the node at `span`, if any.
    pub fn get(&self, span: Span) -> Option<&Ty> {
        self.inner.get(&span)
    }

    /// Number of recorded nodes.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Whether the map is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}
