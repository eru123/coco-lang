//! `libcoco_rt` — minimal C-ABI runtime stub for native (LLVM) Coco builds.
//!
//! The LLVM code generator declares `coco_rt_alloc` as an external function
//! and emits calls to it when boxing heap values (e.g. string allocation).
//! This crate provides that symbol so the native binary links cleanly.
//!
//! `coco_rt_alloc(tag, data)` allocates a two-word struct `{ i64 tag, i64
//! data }` on the heap and returns a pointer to it. The struct matches the
//! `Value` layout the codegen uses (see `crates/coco_codegen/src/lib.rs`).
//!
//! Built as a `staticlib` so `cc obj.o -o binary -lcoco_rt` (or the static
//! archive path) resolves the symbol.

use std::alloc::{alloc, Layout};

/// The Coco runtime value as emitted by the codegen: `{ i64 tag, i64 data }`.
/// tag 0=int, 1=float, 2=string(ptr), 3=bool, 4=null.
#[repr(C)]
pub struct CocoValue {
    pub tag: i64,
    pub data: i64,
}

/// Allocate a `CocoValue` on the heap and return a pointer to it.
///
/// # Safety
/// This is a C-ABI entry point called from LLVM-emitted code. The returned
/// pointer is heap-allocated and never freed (a production runtime would
/// integrate with the GC; this stub is sufficient for linking and basic
/// native execution).
#[no_mangle]
pub extern "C" fn coco_rt_alloc(tag: i64, data: i64) -> *mut CocoValue {
    // SAFETY: Layout for CocoValue is valid (two i64s, 8-byte aligned).
    let layout = Layout::new::<CocoValue>();
    let ptr = unsafe { alloc(layout) } as *mut CocoValue;
    if ptr.is_null() {
        // Allocation failure: abort rather than dereference null.
        std::process::abort();
    }
    unsafe {
        (*ptr).tag = tag;
        (*ptr).data = data;
    }
    ptr
}

/// Free a `CocoValue` previously allocated by `coco_rt_alloc`.
///
/// # Safety
/// `ptr` must point to a `CocoValue` allocated by `coco_rt_alloc`.
#[no_mangle]
pub extern "C" fn coco_rt_free(ptr: *mut CocoValue) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: caller guarantees ptr came from coco_rt_alloc (Layout::new).
    unsafe {
        std::alloc::dealloc(ptr as *mut u8, Layout::new::<CocoValue>());
    }
}
