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
    /// Fat pointer to the heap-allocated trait object (owns the data + vtable).
    /// Carries the vtable so the GC can downcast via `as_any` for tracing.
    obj: *mut dyn GcObj,
    /// Drop function for this type.
    drop_fn: unsafe fn(*mut ()),
    /// Reference count from outstanding `Gc<T>` handles.
    refcount: Cell<usize>,
    /// Mark bit for tracing collection. Set during the mark phase, cleared
    /// before each collection cycle.
    marked: Cell<bool>,
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
    /// Number of objects reclaimed by tracing collection (cycles included).
    pub traced_freed: usize,
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
        let boxed: Box<dyn GcObj> = Box::new(value);
        let obj = Box::into_raw(boxed);
        // The typed data pointer is the same address; coerce for Gc<T> deref.
        let ptr = obj as *mut () as *const T;
        let id = self.entries.len();
        self.entries.push(HeapEntry {
            obj,
            drop_fn: drop_obj::<T>,
            refcount: Cell::new(1),
            marked: Cell::new(false),
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

        (GcRef(id), ptr)
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
        // Mark the object reachable. Used by the tracing collector's mark
        // phase; a no-op effect only when no tracing collection is run.
        if let Some(entry) = self.entries.get(id.0) {
            entry.marked.set(true);
        }
    }

    /// Returns whether `id` was marked during the last tracing mark phase.
    pub fn is_marked(&self, id: GcRef) -> bool {
        self.entries
            .get(id.0)
            .map(|e| e.marked.get())
            .unwrap_or(false)
    }

    /// Borrow the `GcObj` stored at `id` as `&dyn Any` for downcasting.
    ///
    /// Used by tracers that need to inspect an object's children (e.g. the
    /// interpreter downcasting `CoW<Vec<Value>>` to extract inner `GcRef`s).
    pub fn obj_as_any(&self, id: GcRef) -> Option<&dyn Any> {
        // SAFETY: the object is alive for as long as &self borrows the heap,
        // and no mutation occurs during the borrow. `obj` is a fat pointer
        // carrying the correct vtable.
        self.entries.get(id.0).map(|entry| unsafe { (&*entry.obj).as_any() })
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
                unsafe {
                    (entry.drop_fn)(entry.obj as *mut ());
                }
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
                freed,
                freed_bytes,
                self.entries.len(),
                before
            );
        }
    }

    /// Tracing mark-and-sweep collection with root discovery.
    ///
    /// `roots` are the directly-reachable `GcRef`s (VM stack + globals). The
    /// `tracer` closure receives the downcast `&dyn Any` for an object and
    /// returns the `GcRef`s of objects its data references (e.g. the `Value`s
    /// inside a `List`/`Map`). The interpreter supplies this because it owns
    /// the `Value` type; `coco_gc` is agnostic to the concrete value
    /// representation.
    ///
    /// This collects unreachable cycles that pure refcounting cannot, since a
    /// mutually-referential cluster with no external root is never marked and
    /// is therefore swept despite each member having refcount > 0.
    ///
    /// Note: this frees objects whose refcount is non-zero but which are
    /// unreachable. Outstanding `Gc<T>` handles to such objects become
    /// dangling; callers must ensure roots accurately reflect all live
    /// references before invoking this.
    pub fn collect_tracing<F>(&mut self, roots: &[GcRef], tracer: F)
    where
        F: Fn(&dyn Any) -> Vec<GcRef>,
    {
        self.stats.collections += 1;

        // Mark phase (shared borrow): clear marks, then BFS from roots.
        self.mark_roots(roots, &tracer);

        // Sweep phase (exclusive borrow): free unmarked entries. These are
        // unreachable, including unreachable cycles (each member retains a
        // refcount > 0 but was never reached from a root).
        let before = self.entries.len();
        let mut freed = 0;
        let mut freed_bytes = 0;

        self.entries.retain(|entry| {
            if entry.marked.get() {
                true
            } else {
                freed += 1;
                freed_bytes += entry.size;
                // SAFETY: the object is unreachable (no path from any root),
                // so no live `Gc<T>` will deref it again.
                unsafe {
                    (entry.drop_fn)(entry.obj as *mut ());
                }
                false
            }
        });

        self.stats.bytes_freed += freed_bytes;
        self.stats.traced_freed += freed;
        self.stats.alive_objects = self.entries.len();

        if freed > 0 {
            eprintln!(
                "GC: tracing collected {} objects ({} bytes), {} remain (swept {})",
                freed,
                freed_bytes,
                self.entries.len(),
                before
            );
        }
    }

    /// Mark phase: clear all marks, then BFS from `roots`, calling `tracer`
    /// on each reached object's `&dyn Any` to discover child `GcRef`s.
    /// Marks are stored in `Cell`s so this only needs `&self`.
    fn mark_roots<F>(&self, roots: &[GcRef], tracer: &F)
    where
        F: Fn(&dyn Any) -> Vec<GcRef>,
    {
        for entry in &self.entries {
            entry.marked.set(false);
        }

        let mut worklist: Vec<GcRef> = roots.to_vec();
        while let Some(id) = worklist.pop() {
            let entry = match self.entries.get(id.0) {
                Some(e) => e,
                None => continue,
            };
            if entry.marked.get() {
                continue; // Already marked — avoid re-traversing cycles.
            }
            entry.marked.set(true);
            // Downcast the object and let the tracer discover children.
            // SAFETY: object is alive for the duration of this shared borrow.
            let any: &dyn Any = unsafe { (&*entry.obj).as_any() };
            for child in tracer(any) {
                worklist.push(child);
            }
        }
    }
}

/// Type-erased drop function.
unsafe fn drop_obj<T>(ptr: *mut ()) {
    unsafe {
        drop(Box::from_raw(ptr as *mut T));
    }
}

impl Drop for Heap {
    fn drop(&mut self) {
        // Free all remaining entries.
        for entry in &self.entries {
            unsafe {
                (entry.drop_fn)(entry.obj as *mut ());
            }
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
