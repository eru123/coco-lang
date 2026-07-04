use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::sync::{Arc, Mutex};

use coco_gc::CoW;
use num_bigint::BigInt;
use num_traits::ToPrimitive;

use crate::ir::FnObj;

// ============================================================================
// Channel inner state
// ============================================================================

/// Internal state of a channel: a bounded buffer with close flag.
#[derive(Debug)]
pub struct ChannelInner {
    pub queue: VecDeque<Value>,
    pub capacity: usize,
    pub closed: bool,
}

impl ChannelInner {
    pub fn new(capacity: usize) -> Self {
        Self {
            queue: VecDeque::new(),
            capacity,
            closed: false,
        }
    }
}

// ============================================================================
// Atomic inner state
// ============================================================================

/// Internal state of an atomic cell.
#[derive(Debug)]
pub struct AtomicInner {
    pub value: Value,
}

impl AtomicInner {
    pub fn new(value: Value) -> Self {
        Self { value }
    }
}

// ============================================================================
// Runtime values
// ============================================================================

/// Runtime values in the Coco interpreter.
///
/// Integers use an adaptive representation: small values that fit in `i64`
/// are stored as `Int64(i64)` (no heap allocation), and only overflow
/// escalates to `Int(BigInt)`. The two variants are semantically identical —
/// `value_eq`, `typeof`, `is_truthy`, and arithmetic all treat `Int64(1)` and
/// `Int(BigInt::from(1))` as the same value. Use the `int_from_i64` /
/// `as_i64` / `to_bigint` helpers to move between representations.
///
/// List and Map are heap-allocated with copy-on-write semantics.
/// Primitive types are stack-allocated.
///
/// Note: `Value::Function` has been removed. All user-defined functions now
/// use `Value::FnObj` (compiled bytecode) executed by the VM.
#[derive(Clone)]
pub enum Value {
    /// A small integer that fits in `i64` — the fast path, no allocation.
    Int64(i64),
    /// A big integer that overflowed `i64` range.
    Int(BigInt),
    Float(f64),
    String(String),
    Bool(bool),
    Null,
    List(Arc<CoW<Vec<Value>>>),
    Map(Arc<CoW<HashMap<String, Value>>>),
    BuiltinFn(String),
    FnObj(FnObj),
    /// Handle to an async task managed by the scheduler.
    TaskHandle(usize),
    /// Result::Ok variant wrapping a success value.
    Ok(Box<Value>),
    /// Result::Err variant wrapping an error value.
    Err(Box<Value>),
    /// Channel — typed buffered communication. Thread-safe via Arc<Mutex<...>>.
    Channel(Arc<Mutex<ChannelInner>>),
    /// Atomic — thread-safe mutable cell. Thread-safe via Arc<Mutex<...>>.
    Atomic(Arc<Mutex<AtomicInner>>),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int64(n) => write!(f, "{}", n),
            Value::Int(n) => write!(f, "{}", n),
            Value::Float(n) => write!(f, "{}", n),
            Value::String(s) => write!(f, "{}", s),
            Value::Bool(b) => write!(f, "{}", b),
            Value::Null => write!(f, "null"),
            Value::List(items) => {
                write!(f, "[")?;
                for (i, item) in items.data.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", item)?;
                }
                write!(f, "]")
            }
            Value::Map(map) => {
                write!(f, "{{")?;
                for (i, (k, v)) in map.data.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "\"{}\": {}", k, v)?;
                }
                write!(f, "}}")
            }
            Value::BuiltinFn(name) => write!(f, "<builtin {}>", name),
            Value::FnObj(fo) => write!(f, "<fn {}>", fo.name),
            Value::TaskHandle(id) => write!(f, "<task {}>", id),
            Value::Ok(v) => write!(f, "Ok({})", v),
            Value::Err(v) => write!(f, "Err({})", v),
            Value::Channel(_) => write!(f, "<channel>"),
            Value::Atomic(inner) => {
                if let Ok(guard) = inner.lock() {
                    write!(f, "Atomic({})", guard.value)
                } else {
                    write!(f, "<atomic (poisoned)>")
                }
            }
        }
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int64(n) => write!(f, "Int({})", n),
            Value::Int(n) => write!(f, "Int({})", n),
            Value::Float(n) => write!(f, "Float({})", n),
            Value::String(s) => write!(f, "String({:?})", s),
            Value::Bool(b) => write!(f, "Bool({})", b),
            Value::Null => write!(f, "Null"),
            Value::List(items) => write!(f, "List({:?})", &items.data),
            Value::Map(map) => write!(f, "Map({:?})", &map.data),
            Value::BuiltinFn(name) => write!(f, "BuiltinFn({})", name),
            Value::FnObj(fo) => write!(f, "FnObj({})", fo.name),
            Value::TaskHandle(id) => write!(f, "TaskHandle({})", id),
            Value::Ok(v) => write!(f, "Ok({:?})", v),
            Value::Err(v) => write!(f, "Err({:?})", v),
            Value::Channel(_) => write!(f, "Channel"),
            Value::Atomic(inner) => {
                if let Ok(guard) = inner.lock() {
                    write!(f, "Atomic({:?})", guard.value)
                } else {
                    write!(f, "Atomic(poisoned)")
                }
            }
        }
    }
}

impl Value {
    /// Check if the value is truthy.
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Null => false,
            Value::Int64(n) => *n != 0,
            Value::Int(n) => *n != BigInt::from(0),
            Value::Float(f) => *f != 0.0,
            Value::String(s) => !s.is_empty(),
            Value::List(l) => !l.data.is_empty(),
            Value::Map(m) => !m.data.is_empty(),
            Value::BuiltinFn(_) | Value::FnObj(_) | Value::TaskHandle(_) => true,
            Value::Ok(_) => true,
            Value::Channel(_) => true,
            Value::Atomic(_) => true,
            Value::Err(v) => v.as_ref().is_truthy(),
        }
    }

    /// Construct an integer value from an `i64`, using the fast `Int64`
    /// variant (no allocation). Use this for all literal/constant integer
    /// construction; escalation to `Int(BigInt)` happens automatically in
    /// arithmetic on overflow.
    pub fn int_from_i64(n: i64) -> Value {
        Value::Int64(n)
    }

    /// If this is an integer that fits in `i64`, return it. Returns `None`
    /// for `Int(BigInt)` values that exceed `i64` range, and for non-integers.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int64(n) => Some(*n),
            Value::Int(n) => {
                use num_traits::ToPrimitive;
                n.to_i64()
            }
            _ => None,
        }
    }

    /// If this is an integer (either representation), return it as an owned
    /// `BigInt`. Returns `None` for non-integers.
    pub fn to_bigint(&self) -> Option<BigInt> {
        match self {
            Value::Int64(n) => Some(BigInt::from(*n)),
            Value::Int(n) => Some(n.clone()),
            _ => None,
        }
    }

    /// Whether this value is an integer of either representation.
    pub fn is_int(&self) -> bool {
        matches!(self, Value::Int64(_) | Value::Int(_))
    }

    /// Wrap a BigInt back into `Int64` if it fits, else keep it as `Int`.
    pub fn from_bigint(n: BigInt) -> Value {
        if let Some(i) = n.to_i64() {
            Value::Int64(i)
        } else {
            Value::Int(n)
        }
    }

}

/// Structural equality for `Value`s, exposed as the `deepEquals` builtin.
///
/// - Primitives compared by value, type-strict (`1 != "1"`, `1 != 1.0`).
/// - Lists/Maps compared structurally (deep, order-independent for maps).
/// - Channels/Atomics compared by reference identity.
///
/// This is shared between `Vm::vm_eq` and the `deepEquals` builtin so the
/// HashSet and other stdlib structures can compare values without resorting
/// to `toString` (which is order-dependent for maps and conflates types).
pub fn value_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        // Integers: Int64 and Int are the same value type. Compare via i64
        // when both fit (fast, no allocation), else fall back to BigInt.
        (Value::Int64(a), Value::Int64(b)) => a == b,
        (Value::Int64(a), Value::Int(b)) | (Value::Int(b), Value::Int64(a)) => {
            // The BigInt side must fit in i64 and equal the Int64 side.
            use num_traits::ToPrimitive;
            b.to_i64().map_or(false, |bi| bi == *a)
        }
        (Value::Int(a), Value::Int(b)) => a == b,
        (Value::Float(a), Value::Float(b)) => a == b,
        (Value::String(a), Value::String(b)) => a == b,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Null, Value::Null) => true,
        (Value::TaskHandle(a), Value::TaskHandle(b)) => a == b,
        (Value::Ok(a), Value::Ok(b)) => value_eq(a, b),
        (Value::Err(a), Value::Err(b)) => value_eq(a, b),
        (Value::List(a), Value::List(b)) => {
            if a.data.len() != b.data.len() {
                return false;
            }
            a.data.iter().zip(b.data.iter()).all(|(x, y)| value_eq(x, y))
        }
        (Value::Map(a), Value::Map(b)) => {
            if a.data.len() != b.data.len() {
                return false;
            }
            a.data
                .iter()
                .all(|(k, v)| b.data.get(k).map(|bv| value_eq(v, bv)).unwrap_or(false))
        }
        (Value::Channel(a), Value::Channel(b)) => Arc::ptr_eq(a, b),
        (Value::Atomic(a), Value::Atomic(b)) => Arc::ptr_eq(a, b),
        (Value::BuiltinFn(a), Value::BuiltinFn(b)) => a == b,
        (Value::FnObj(a), Value::FnObj(b)) => a.name == b.name && a.arity == b.arity,
        _ => false,
    }
}

#[cfg(test)]
mod send_sync_assert {
    use super::Value;
    fn _assert_send<T: Send>() {}
    fn _assert_sync<T: Sync>() {}
    fn _check() {
        _assert_send::<Value>();
        _assert_sync::<Value>();
    }
}
