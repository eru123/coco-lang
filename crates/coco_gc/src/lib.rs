//! Tracing garbage collector for the Coco runtime.
//!
//! Implements a simple mark-sweep collector:
//! - `Heap` manages all allocated GC objects.
//! - `Gc<T>` is a reference-counted smart pointer that participates in GC.
//! - `Trace` trait allows the GC to walk object graphs.
//!
//! Architecture:
//! - Objects are stored in a flat `Vec<Box<dyn GcObj>>` indexed by `ObjId`.
//! - `Gc<T>` holds an `ObjId` plus a clone count (strong references).
//! - Mark phase: walk roots, mark all reachable objects.
//! - Sweep phase: deallocate unmarked objects, compact remaining.

pub mod cow;
pub mod gc;
pub mod heap;
pub mod trace;

pub use cow::CoW;
pub use gc::Gc;
pub use heap::{GcObj, Heap};
pub use trace::Trace;
