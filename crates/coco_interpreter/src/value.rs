use std::collections::HashMap;
use std::fmt;

use coco_syntax::{Block, Param};

/// Runtime values in the Coco interpreter.
#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Null,
    List(Vec<Value>),
    Map(HashMap<String, Value>),
    Function(Function),
    BuiltinFn(String),
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
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", item)?;
                }
                write!(f, "]")
            }
            Value::Map(map) => {
                write!(f, "{{")?;
                for (i, (k, v)) in map.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "\"{}\": {}", k, v)?;
                }
                write!(f, "}}")
            }
            Value::Function(func) => write!(f, "<fn {}>", func.name),
            Value::BuiltinFn(name) => write!(f, "<builtin {}>", name),
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
            Value::List(l) => !l.is_empty(),
            Value::Map(m) => !m.is_empty(),
            Value::Function(_) | Value::BuiltinFn(_) => true,
        }
    }

    /// Extract param names from AST params for function storage.
    pub fn extract_param_names(params: &[Param]) -> Vec<String> {
        params.iter().map(|p| p.name.name.clone()).collect()
    }
}
