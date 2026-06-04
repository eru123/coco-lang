//! `Trace` trait for GC-participating types.

use crate::gc::GcRef;
use crate::heap::Heap;

pub trait Trace {
    fn trace(&self, heap: &Heap);
}

impl Trace for i64 {
    fn trace(&self, _: &Heap) {}
}
impl Trace for f64 {
    fn trace(&self, _: &Heap) {}
}
impl Trace for bool {
    fn trace(&self, _: &Heap) {}
}
impl Trace for String {
    fn trace(&self, _: &Heap) {}
}

impl<T: Trace> Trace for Vec<T> {
    fn trace(&self, heap: &Heap) {
        for item in self {
            item.trace(heap);
        }
    }
}

impl<T: Trace> Trace for Option<T> {
    fn trace(&self, heap: &Heap) {
        if let Some(v) = self {
            v.trace(heap);
        }
    }
}

impl Trace for GcRef {
    fn trace(&self, heap: &Heap) {
        heap.mark(*self);
    }
}
