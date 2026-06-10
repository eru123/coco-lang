use crate::error::{RuntimeError, Signal};
use crate::value::Value;

/// Execute a built-in function by name with the given arguments.
pub fn call_builtin(name: &str, args: &[Value]) -> Result<Value, Signal> {
    match name {
        // ---- I/O ----
        "print" => {
            let parts: Vec<String> = args.iter().map(|a| format!("{}", a)).collect();
            println!("{}", parts.join(" "));
            Ok(Value::Null)
        }

        // ---- Type conversion ----
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

        // ---- Result constructors ----
        "Ok" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new("Ok() expects 1 argument")));
            }
            Ok(Value::Ok(Box::new(args[0].clone())))
        }
        "Err" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new("Err() expects 1 argument")));
            }
            Ok(Value::Err(Box::new(args[0].clone())))
        }

        // ---- Math ----
        "abs" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new("abs() expects 1 argument")));
            }
            match &args[0] {
                Value::Int(n) => Ok(Value::Int(n.abs())),
                Value::Float(f) => Ok(Value::Float(f.abs())),
                _ => Err(Signal::Error(RuntimeError::new("abs() expects a number"))),
            }
        }
        "min" => {
            if args.len() < 2 {
                return Err(Signal::Error(RuntimeError::new("min() expects at least 2 arguments")));
            }
            let mut best = &args[0];
            for arg in &args[1..] {
                match (best, arg) {
                    (Value::Int(a), Value::Int(b)) if a > b => best = arg,
                    (Value::Float(a), Value::Float(b)) if a > b => best = arg,
                    (Value::Int(a), Value::Float(b)) if *a as f64 > *b => best = arg,
                    (Value::Float(a), Value::Int(b)) if *a > *b as f64 => best = arg,
                    _ => {}
                }
            }
            Ok(best.clone())
        }
        "max" => {
            if args.len() < 2 {
                return Err(Signal::Error(RuntimeError::new("max() expects at least 2 arguments")));
            }
            let mut best = &args[0];
            for arg in &args[1..] {
                match (best, arg) {
                    (Value::Int(a), Value::Int(b)) if a < b => best = arg,
                    (Value::Float(a), Value::Float(b)) if a < b => best = arg,
                    (Value::Int(a), Value::Float(b)) if (*a as f64) < *b => best = arg,
                    (Value::Float(a), Value::Int(b)) if *a < *b as f64 => best = arg,
                    _ => {}
                }
            }
            Ok(best.clone())
        }
        "floor" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new("floor() expects 1 argument")));
            }
            match &args[0] {
                Value::Float(f) => Ok(Value::Int(f.floor() as i64)),
                Value::Int(n) => Ok(Value::Int(*n)),
                _ => Err(Signal::Error(RuntimeError::new("floor() expects a number"))),
            }
        }
        "ceil" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new("ceil() expects 1 argument")));
            }
            match &args[0] {
                Value::Float(f) => Ok(Value::Int(f.ceil() as i64)),
                Value::Int(n) => Ok(Value::Int(*n)),
                _ => Err(Signal::Error(RuntimeError::new("ceil() expects a number"))),
            }
        }
        "round" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new("round() expects 1 argument")));
            }
            match &args[0] {
                Value::Float(f) => Ok(Value::Int(f.round() as i64)),
                Value::Int(n) => Ok(Value::Int(*n)),
                _ => Err(Signal::Error(RuntimeError::new("round() expects a number"))),
            }
        }
        "sqrt" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new("sqrt() expects 1 argument")));
            }
            match &args[0] {
                Value::Float(f) => Ok(Value::Float(f.sqrt())),
                Value::Int(n) => Ok(Value::Float((*n as f64).sqrt())),
                _ => Err(Signal::Error(RuntimeError::new("sqrt() expects a number"))),
            }
        }
        "pow" => {
            if args.len() != 2 {
                return Err(Signal::Error(RuntimeError::new("pow() expects 2 arguments")));
            }
            match (&args[0], &args[1]) {
                (Value::Int(a), Value::Int(b)) => {
                    if *b >= 0 {
                        Ok(Value::Int(a.pow(*b as u32)))
                    } else {
                        Ok(Value::Float((*a as f64).powi(*b as i32)))
                    }
                }
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a.powf(*b))),
                (Value::Int(a), Value::Float(b)) => Ok(Value::Float((*a as f64).powf(*b))),
                (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a.powi(*b as i32))),
                _ => Err(Signal::Error(RuntimeError::new("pow() expects numbers"))),
            }
        }
        "random" => {
            if args.is_empty() {
                // random() returns float 0.0..1.0
                use std::collections::hash_map::RandomState;
                use std::hash::{BuildHasher, Hasher};
                let h = RandomState::new().build_hasher().finish();
                Ok(Value::Float((h as f64) / (u64::MAX as f64)))
            } else if args.len() == 1 {
                // random(max) returns int 0..max
                match &args[0] {
                    Value::Int(max) => {
                        use std::collections::hash_map::RandomState;
                        use std::hash::{BuildHasher, Hasher};
                        let h = RandomState::new().build_hasher().finish();
                        Ok(Value::Int((h % (*max as u64)) as i64))
                    }
                    _ => Err(Signal::Error(RuntimeError::new(
                        "random() with 1 argument expects an integer max",
                    ))),
                }
            } else {
                Err(Signal::Error(RuntimeError::new(
                    "random() expects 0 or 1 arguments",
                )))
            }
        }

        // ---- Type checking ----
        "typeOf" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new("typeOf() expects 1 argument")));
            }
            let type_name = match &args[0] {
                Value::Int(_) => "int",
                Value::Float(_) => "float",
                Value::String(_) => "string",
                Value::Bool(_) => "bool",
                Value::Null => "null",
                Value::List(_) => "list",
                Value::Map(_) => "map",
                Value::Function(_) => "function",
                Value::BuiltinFn(_) => "builtin",
                Value::FnObj(_) => "function",
                Value::TaskHandle(_) => "task",
                Value::Ok(_) => "result",
                Value::Err(_) => "result",
            };
            Ok(Value::String(type_name.to_string()))
        }
        "isOk" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new("isOk() expects 1 argument")));
            }
            Ok(Value::Bool(matches!(args[0], Value::Ok(_))))
        }
        "isErr" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new("isErr() expects 1 argument")));
            }
            Ok(Value::Bool(matches!(args[0], Value::Err(_))))
        }
        "unwrap" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new("unwrap() expects 1 argument")));
            }
            match &args[0] {
                Value::Ok(v) => Ok((**v).clone()),
                Value::Err(e) => Err(Signal::Error(RuntimeError::new(format!(
                    "unwrap() called on Err: {}",
                    e
                )))),
                _ => Err(Signal::Error(RuntimeError::new(
                    "unwrap() expects a Result (Ok or Err)",
                ))),
            }
        }

        _ => Err(Signal::Error(RuntimeError::new(format!(
            "unknown builtin '{}'",
            name
        )))),
    }
}
