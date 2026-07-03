//! Copy-on-Write wrapper for the Coco runtime.
//!
//! This crate provides `CoW<T>`, the payload type used inside the VM's
//! `Arc<CoW<T>>`-backed collections (`Value::List`, `Value::Map`). Memory
//! management for those values is plain `Arc` refcounting — there is no
//! tracing garbage collector. (An earlier version of this crate shipped a
//! mark-sweep `Heap`/`Gc`/`Trace` collector, but the VM never allocated into
//! it; it has been removed.)

pub mod cow;

pub use cow::CoW;
