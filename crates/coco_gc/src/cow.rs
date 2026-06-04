//! Copy-on-Write wrapper for GC-managed data.
//!
//! CoW<T> wraps a heap-allocated T and provides shared-read, copy-on-write-mutate semantics.
//! Clone bumps a refcount; mutation triggers a copy if the data is shared.

use std::any::Any;
use std::cell::Cell;
use std::fmt;

use crate::heap::GcObj;

/// A CoW wrapper stored inside a Gc<T>. The actual data lives on the heap.
/// Clone creates a new reference. Mutation via `get_mut()` copies if shared.
pub struct CoW<T: Clone + 'static> {
    pub data: T,
    refcount: Cell<usize>,
}

impl<T: Clone + 'static> CoW<T> {
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

    pub fn dec_ref(&self) -> bool {
        let rc = self.refcount.get();
        if rc > 0 {
            self.refcount.set(rc - 1);
        }
        self.refcount.get() == 0
    }

    pub fn get_mut(&mut self) -> &mut T {
        if self.refcount.get() > 1 {
            self.data = self.data.clone();
            self.refcount.set(1);
        }
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
