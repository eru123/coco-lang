use std::collections::HashMap;
use std::fmt;

use coco_gc::{CoW, Gc};
use coco_syntax::{Block, Param};

use crate::ir::FnObj;

/// Runtime values in the Coco interpreter.
///
/// List and Map are heap-allocated with copy-on-write semantics.
/// Primitive types are stack-allocated.
#[derive(Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Null,
    List(Gc<CoW<Vec<Value>>>),
    Map(Gc<CoW<HashMap<String, Value>>>),
    Function(Function),
    BuiltinFn(String),
    FnObj(FnObj),
    /// Handle to an async task managed by the scheduler.
    TaskHandle(usize),
}

/// A user-defined function captured at runtime.
#[derive(Debug, Clone)]
pub struct Function {
    pub name: String,
    pub params: Vec<String>,
    pub body: Block,
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
            Value::Function(func) => write!(f, "<fn {}>", func.name),
            Value::BuiltinFn(name) => write!(f, "<builtin {}>", name),
            Value::FnObj(fo) => write!(f, "<fn {}>", fo.name),
            Value::TaskHandle(id) => write!(f, "<task {}>", id),
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
            Value::Function(func) => write!(f, "Function({})", func.name),
            Value::BuiltinFn(name) => write!(f, "BuiltinFn({})", name),
            Value::FnObj(fo) => write!(f, "FnObj({})", fo.name),
            Value::TaskHandle(id) => write!(f, "TaskHandle({})", id),
        }
    }
}

impl Value {
    /// Check if the value is truthy.
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Null => false,
            Value::Int(n) => *n != 0,
            Value::Float(f) => *f != 0.0,
            Value::String(s) => !s.is_empty(),
            Value::List(l) => !l.data.is_empty(),
            Value::Map(m) => !m.data.is_empty(),
            Value::Function(_) | Value::BuiltinFn(_) | Value::FnObj(_) | Value::TaskHandle(_) => true,
        }
    }

    /// Extract param names from AST params for function storage.
    pub fn extract_param_names(params: &[Param]) -> Vec<String> {
        params.iter().map(|p| p.name.name.clone()).collect()
    }
}
