use crate::error::{RuntimeError, Signal};
use crate::value::Value;

/// Execute a built-in function by name with the given arguments.
pub fn call_builtin(name: &str, args: &[Value]) -> Result<Value, Signal> {
    match name {
        "print" => {
            let parts: Vec<String> = args.iter().map(|a| format!("{}", a)).collect();
            println!("{}", parts.join(" "));
            Ok(Value::Null)
        }
        "len" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new("len() expects 1 argument")));
            }
            match &args[0] {
                Value::String(s) => Ok(Value::Int(s.len() as i64)),
                Value::List(l) => Ok(Value::Int(l.data.len() as i64)),
                Value::Map(m) => Ok(Value::Int(m.data.len() as i64)),
                _ => Err(Signal::Error(RuntimeError::new(
                    "len() expects a string, list, or map",
                ))),
            }
        }
        "toString" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new(
                    "toString() expects 1 argument",
                )));
            }
            Ok(Value::String(format!("{}", args[0])))
        }
        "parseInt" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new(
                    "parseInt() expects 1 argument",
                )));
            }
            match &args[0] {
                Value::String(s) => match s.parse::<i64>() {
                    Ok(n) => Ok(Value::Int(n)),
                    Err(_) => Ok(Value::Null),
                },
                Value::Int(n) => Ok(Value::Int(*n)),
                Value::Float(f) => Ok(Value::Int(*f as i64)),
                _ => Ok(Value::Null),
            }
        }
        "parseFloat" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new(
                    "parseFloat() expects 1 argument",
                )));
            }
            match &args[0] {
                Value::String(s) => match s.parse::<f64>() {
                    Ok(f) => Ok(Value::Float(f)),
                    Err(_) => Ok(Value::Null),
                },
                Value::Int(n) => Ok(Value::Float(*n as f64)),
                Value::Float(f) => Ok(Value::Float(*f)),
                _ => Ok(Value::Null),
            }
        }
        _ => Err(Signal::Error(RuntimeError::new(format!(
            "unknown builtin '{}'",
            name
        )))),
    }
}
