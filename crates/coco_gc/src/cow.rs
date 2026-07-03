//! Copy-on-Write wrapper used as the payload of `Arc<CoW<T>>` values.
//!
//! `CoW<T>` is a plain `{ data: T }` wrapper. Copy-on-write and lifetime are
//! governed entirely by the enclosing `std::sync::Arc` (refcounting), so `CoW`
//! itself carries no refcount or GC state. The VM's `Value::List` and
//! `Value::Map` store `Arc<CoW<Vec<Value>>>` / `Arc<CoW<HashMap<...>>>`; a
//! clone of the `Value` bumps the `Arc`'s refcount, and mutation goes through
//! `Arc::make_mut` which copies only when the refcount is > 1.

use std::fmt;

/// A transparent wrapper around `T`, used inside `Arc<CoW<T>>`.
///
/// The `data` field is public because every access site reads or writes it
/// directly (e.g. `list.data.push(...)`).
pub struct CoW<T: Clone + 'static> {
    pub data: T,
}

impl<T: Clone + 'static> CoW<T> {
    pub fn new(data: T) -> Self {
        Self { data }
    }

    /// Get mutable access to the inner data.
    ///
    /// Callers that need copy-on-write semantics should go through the
    /// enclosing `Arc` (`Arc::make_mut`), not this method.
    pub fn get_mut(&mut self) -> &mut T {
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
