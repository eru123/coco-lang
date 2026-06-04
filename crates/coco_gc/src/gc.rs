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
    /// SAFETY: caller must ensure no other Gc<T> references exist.
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
