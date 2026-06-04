//! GC heap.
//!
//! Owns all GC-allocated objects. Provides allocation, reference counting,
//! and mark-sweep collection. Objects are stored as raw pointers for fast
//! `Gc<T>` deref. All methods take `&self` for interpreter ergonomics.

use std::any::Any;
use std::cell::Cell;
use std::fmt;

use crate::gc::GcRef;

pub trait GcObj: Any + fmt::Debug {
    fn size(&self) -> usize;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

struct HeapEntry {
    /// Raw pointer to the heap-allocated object (owned by us).
    ptr: *mut (),
    /// Drop function for this type.
    drop_fn: unsafe fn(*mut ()),
    /// Reference count from outstanding `Gc<T>` handles.
    refcount: Cell<usize>,
    /// Object size in bytes.
    size: usize,
}

unsafe impl Send for HeapEntry {}

#[derive(Debug, Clone, Default)]
pub struct GcStats {
    pub allocations: usize,
    pub collections: usize,
    pub bytes_allocated: usize,
    pub bytes_freed: usize,
    pub alive_objects: usize,
}

pub struct Heap {
    entries: Vec<HeapEntry>,
    pub stats: GcStats,
    /// Debug mode: collect on every N allocations (0 = disabled).
    pub collect_interval: usize,
    allocs_since_collect: usize,
}

impl Heap {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            stats: GcStats::default(),
            collect_interval: 0,
            allocs_since_collect: 0,
        }
    }

    /// Allocate a value on the heap. Returns a GcRef and a raw pointer.
    pub fn allocate<T: GcObj + 'static>(&mut self, value: T) -> (GcRef, *const T) {
        let size = value.size();
        let ptr = Box::into_raw(Box::new(value)) as *mut ();
        let id = self.entries.len();
        self.entries.push(HeapEntry {
            ptr,
            drop_fn: drop_obj::<T>,
            refcount: Cell::new(1),
            size,
        });
        self.stats.allocations += 1;
        self.stats.bytes_allocated += size;
        self.stats.alive_objects += 1;

        self.allocs_since_collect += 1;
        if self.collect_interval > 0 && self.allocs_since_collect >= self.collect_interval {
            self.allocs_since_collect = 0;
            self.collect();
        }

        (GcRef(id), ptr as *const T)
    }

    pub fn inc_ref(&self, id: GcRef) {
        if let Some(entry) = self.entries.get(id.0) {
            let rc = entry.refcount.get();
            entry.refcount.set(rc + 1);
        }
    }

    pub fn dec_ref(&self, id: GcRef) -> bool {
        if let Some(entry) = self.entries.get(id.0) {
            let rc = entry.refcount.get();
            if rc > 0 {
                entry.refcount.set(rc - 1);
            }
            entry.refcount.get() == 0
        } else {
            false
        }
    }

    pub fn mark(&self, id: GcRef) {
        // For mark-sweep integration — not used in current refcount-only mode.
        let _ = id;
    }

    /// Get the refcount for an object (for CoW checks).
    pub fn refcount(&self, id: GcRef) -> usize {
        self.entries
            .get(id.0)
            .map(|e| e.refcount.get())
            .unwrap_or(0)
    }

    pub fn live_count(&self) -> usize {
        self.stats.alive_objects
    }

    pub fn collect(&mut self) {
        self.stats.collections += 1;
        let before = self.entries.len();
        let mut freed = 0;
        let mut freed_bytes = 0;

        self.entries.retain(|entry| {
            if entry.refcount.get() == 0 {
                freed += 1;
                freed_bytes += entry.size;
                // SAFETY: refcount 0 means no outstanding Gc<T> references.
                unsafe { (entry.drop_fn)(entry.ptr); }
                false
            } else {
                true
            }
        });

        self.stats.bytes_freed += freed_bytes;
        self.stats.alive_objects = self.entries.len();

        if freed > 0 {
            eprintln!(
                "GC: collected {} objects ({} bytes), {} remain (swept {})",
                freed, freed_bytes, self.entries.len(), before
            );
        }
    }
}

/// Type-erased drop function.
unsafe fn drop_obj<T>(ptr: *mut ()) {
    unsafe { drop(Box::from_raw(ptr as *mut T)); }
}

impl Drop for Heap {
    fn drop(&mut self) {
        // Free all remaining entries.
        for entry in &self.entries {
            unsafe { (entry.drop_fn)(entry.ptr); }
        }
    }
}

impl Default for Heap {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Heap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Heap")
            .field("entries", &self.entries.len())
            .field("stats", &self.stats)
            .finish()
    }
}
