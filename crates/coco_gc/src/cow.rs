//! Copy-on-Write wrapper for GC-managed data.
//!
//! `CoW<T>` wraps a heap-allocated `T`. Sharing is governed solely by the
//! `Heap`'s refcount (bumped by `Gc<T>::clone`, decremented by `Gc<T>`'s
//! `Drop`) — `CoW` itself no longer tracks a separate refcount. Callers that
//! need copy-on-write semantics should check `Heap::refcount(id)` before
//! mutating shared data.
//!
//! Historical note: an earlier design gave `CoW` its own `Cell<usize>`
//! refcount and a `get_mut` that copied when shared. That count was never
//! kept in sync with the `Heap`'s count and `get_mut` was never called, so
//! the two diverged. The refcount is now unified on the `Heap` exclusively.

use std::any::Any;
use std::fmt;

use crate::heap::GcObj;

/// A CoW wrapper stored inside a `Gc<T>`. The actual data lives on the heap.
pub struct CoW<T: Clone + 'static> {
    pub data: T,
}

impl<T: Clone + 'static> CoW<T> {
    pub fn new(data: T) -> Self {
        Self { data }
    }

    /// Get mutable access to the inner data.
    ///
    /// This does **not** perform copy-on-write automatically. The caller is
    /// responsible for checking `Heap::refcount(id)` first if shared mutation
    /// must be avoided, since `CoW` no longer holds its own refcount.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.data
    }
}

impl<T: Clone + fmt::Debug + 'static> fmt::Debug for CoW<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.data, f)
    }
}

impl<T: Clone + fmt::Display + 'static> fmt::Display for CoW<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.data, f)
    }
}

// GcObj impl for CoW<T> — works for any Clone + Debug + 'static T.
impl<T: Clone + fmt::Debug + 'static> GcObj for CoW<T> {
    fn size(&self) -> usize {
        std::mem::size_of::<Self>()
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
