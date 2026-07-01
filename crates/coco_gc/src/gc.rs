//! `Gc<T>` — a GC-managed reference.
//!
//! Stores a raw pointer and an object id. Clone bumps the heap refcount.
//! Drop decrements. Deref gives direct access to the underlying T.

use std::fmt;
use std::ops::Deref;

use crate::heap::Heap;

/// Index into the heap. Used for refcount operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GcRef(pub usize);

/// A garbage-collected reference to a heap-allocated `T`.
///
/// `Clone` is cheap (just bumps a refcount).
/// `Drop` decrements the refcount.
/// `Deref` provides direct access (the pointer is stable until collection).
pub struct Gc<T: 'static> {
    /// Raw pointer to the heap-allocated T.
    ptr: *const T,
    /// Heap entry id for refcount management.
    id: GcRef,
    /// Heap reference for refcount operations.
    /// SAFETY: must outlive all Gc<T> instances.
    heap: *const Heap,
}

impl<T: 'static> Gc<T> {
    /// Allocate a value on the heap.
    pub fn new(heap: &Heap, id: GcRef, ptr: *const T) -> Self {
        Gc {
            ptr,
            id,
            heap: heap as *const Heap,
        }
    }

    pub fn id(&self) -> GcRef {
        self.id
    }

    /// Get a raw pointer to the object.
    pub fn as_ptr(&self) -> *const T {
        self.ptr
    }

    /// Get mutable access. Only valid when refcount == 1.
    ///
    /// # Safety
    ///
    /// Caller must ensure no other `Gc<T>` references to this object exist
    /// (refcount must be 1). Violation causes aliased mutable access.
    pub unsafe fn as_mut_ptr(&self) -> *mut T {
        self.ptr as *mut T
    }

    fn heap(&self) -> &Heap {
        unsafe { &*self.heap }
    }
}

impl<T: 'static> Clone for Gc<T> {
    fn clone(&self) -> Self {
        self.heap().inc_ref(self.id);
        Gc {
            ptr: self.ptr,
            id: self.id,
            heap: self.heap,
        }
    }
}

impl<T: 'static> Drop for Gc<T> {
    fn drop(&mut self) {
        self.heap().dec_ref(self.id);
    }
}

impl<T: 'static> Deref for Gc<T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.ptr }
    }
}

impl<T: fmt::Debug + 'static> fmt::Debug for Gc<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.deref(), f)
    }
}

impl<T: fmt::Display + 'static> fmt::Display for Gc<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self.deref(), f)
    }
}

// SAFETY: `Gc<T>` holds a raw pointer into a `Heap` and a `*const Heap`.
// The pointer is stable for the lifetime of the `Heap` (objects are not
// moved by the GC; collection only drops unreachable entries). The `Heap`
// must outlive all `Gc<T>` handles — an invariant the VM already enforces.
// We assert `Send`/`Sync` so that `Value` (which contains `Gc<CoW<...>>`)
// can be `Send + Sync`, enabling cross-thread value sharing in `parallel`
// blocks. This is sound as long as the `Heap` is not concurrently mutated
// without synchronization; the parallel-runtime path must ensure each
// thread either owns its `Heap` or accesses a shared one through a lock.
unsafe impl<T: 'static + Send> Send for Gc<T> {}
unsafe impl<T: 'static + Send + Sync> Sync for Gc<T> {}
