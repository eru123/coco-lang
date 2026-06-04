//! Copy-on-Write wrapper for GC-managed data.
//!
//! CoW<T> wraps a heap-allocated T and provides shared-read, copy-on-write-mutate semantics.
//! Clone bumps a refcount; mutation triggers a copy if the data is shared.

use std::cell::Cell;
use std::fmt;

use crate::heap::GcObj;

/// A CoW wrapper stored inside a Gc<T>. The actual data lives on the heap.
/// Clone creates a new reference. Mutation via `get_mut()` copies if shared.
pub struct CoW<T: GcObj + Clone + 'static> {
    /// The actual data.
    pub data: T,
    /// How many Gc references point to this data.
    refcount: Cell<usize>,
}

impl<T: GcObj + Clone + 'static> CoW<T> {
    pub fn new(data: T) -> Self {
        Self {
            data,
            refcount: Cell::new(1),
        }
    }

    pub fn refcount(&self) -> usize {
        self.refcount.get()
    }

    pub fn inc_ref(&self) {
        let rc = self.refcount.get();
        self.refcount.set(rc + 1);
    }

    /// Decrement refcount. Returns true if it reached zero.
    pub fn dec_ref(&self) -> bool {
        let rc = self.refcount.get();
        if rc > 0 {
            self.refcount.set(rc - 1);
        }
        self.refcount.get() == 0
    }

    /// Get a mutable reference. If the refcount is > 1, clones first.
    pub fn get_mut(&mut self) -> &mut T {
        if self.refcount.get() > 1 {
            self.data = self.data.clone();
            self.refcount.set(1);
        }
        &mut self.data
    }
}

impl<T: GcObj + Clone + fmt::Debug + 'static> fmt::Debug for CoW<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.data, f)
    }
}

impl<T: GcObj + Clone + fmt::Display + 'static> fmt::Display for CoW<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.data, f)
    }
}

// Blanket GcObj impl for CoW
impl<T: GcObj + Clone + 'static> GcObj for CoW<T> {
    fn size(&self) -> usize {
        self.data.size()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
