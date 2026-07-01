use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::sync::{Arc, Mutex};

use coco_gc::{CoW, Gc, GcRef};
use num_bigint::BigInt;

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
/// List and Map are heap-allocated with copy-on-write semantics.
/// Primitive types are stack-allocated.
///
/// Note: `Value::Function` has been removed. All user-defined functions now
/// use `Value::FnObj` (compiled bytecode) executed by the VM.
#[derive(Clone)]
pub enum Value {
    Int(BigInt),
    Float(f64),
    String(String),
    Bool(bool),
    Null,
    List(Gc<CoW<Vec<Value>>>),
    Map(Gc<CoW<HashMap<String, Value>>>),
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
            Value::Int(n) => !n.iter_u32_digits().all(|d| d == 0) && *n != BigInt::from(0),
            Value::Float(f) => *f != 0.0,
            Value::String(s) => !s.is_empty(),
            Value::List(l) => !l.data.is_empty(),
            Value::Map(m) => !m.data.is_empty(),
            Value::BuiltinFn(_) | Value::FnObj(_) | Value::TaskHandle(_) => true,
            Value::Ok(_) => true,
            Value::Channel(_) => true,
            Value::Atomic(_) => true,
            Value::Err(v) => match v.as_ref() {
                Value::Null => false,
                Value::Bool(b) => *b,
                Value::Int(n) => !n.iter_u32_digits().all(|d| d == 0) && *n != BigInt::from(0),
                Value::String(s) => !s.is_empty(),
                _ => true,
            },
        }
    }

    /// Returns the heap `GcRef` if this value is a GC-managed heap object
    /// (currently `List` and `Map`), else `None`. Used by the tracing GC to
    /// discover roots and to walk the object graph.
    pub fn gc_ref(&self) -> Option<GcRef> {
        match self {
            Value::List(g) => Some(g.id()),
            Value::Map(g) => Some(g.id()),
            _ => None,
        }
    }

}
