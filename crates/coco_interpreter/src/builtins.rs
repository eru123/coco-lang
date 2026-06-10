use crate::error::{RuntimeError, Signal};
use crate::value::{AtomicInner, ChannelInner, Value};
use num_bigint::BigInt;
use num_traits::ToPrimitive;
use std::sync::{Arc, Mutex};

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
                Value::String(s) => Ok(Value::Int(BigInt::from(s.len()))),
                Value::List(l) => Ok(Value::Int(BigInt::from(l.data.len()))),
                Value::Map(m) => Ok(Value::Int(BigInt::from(m.data.len()))),
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
                Value::String(s) => match s.parse::<BigInt>() {
                    Ok(n) => Ok(Value::Int(n)),
                    Err(_) => Ok(Value::Null),
                },
                Value::Int(n) => Ok(Value::Int(n.clone())),
                Value::Float(f) => Ok(Value::Int(BigInt::from(*f as i64))),
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
                Value::Int(n) => {
                    use num_traits::ToPrimitive;
                    Ok(Value::Float(n.to_f64().unwrap_or(0.0)))
                }
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
                Value::Int(n) => Ok(Value::Int(if *n >= BigInt::from(0) { n.clone() } else { -n.clone() })),
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
                    (Value::Int(a), Value::Float(b)) => {
                        use num_traits::ToPrimitive;
                        if a.to_f64().unwrap_or(f64::INFINITY) > *b { best = arg; }
                    }
                    (Value::Float(a), Value::Int(b)) => {
                        use num_traits::ToPrimitive;
                        if *a > b.to_f64().unwrap_or(f64::NEG_INFINITY) { best = arg; }
                    }
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
                    (Value::Int(a), Value::Float(b)) => {
                        use num_traits::ToPrimitive;
                        if a.to_f64().unwrap_or(f64::NEG_INFINITY) < *b { best = arg; }
                    }
                    (Value::Float(a), Value::Int(b)) => {
                        use num_traits::ToPrimitive;
                        if *a < b.to_f64().unwrap_or(f64::INFINITY) { best = arg; }
                    }
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
                Value::Float(f) => Ok(Value::Int(BigInt::from(f.floor() as i64))),
                Value::Int(n) => Ok(Value::Int(n.clone())),
                _ => Err(Signal::Error(RuntimeError::new("floor() expects a number"))),
            }
        }
        "ceil" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new("ceil() expects 1 argument")));
            }
            match &args[0] {
                Value::Float(f) => Ok(Value::Int(BigInt::from(f.ceil() as i64))),
                Value::Int(n) => Ok(Value::Int(n.clone())),
                _ => Err(Signal::Error(RuntimeError::new("ceil() expects a number"))),
            }
        }
        "round" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new("round() expects 1 argument")));
            }
            match &args[0] {
                Value::Float(f) => Ok(Value::Int(BigInt::from(f.round() as i64))),
                Value::Int(n) => Ok(Value::Int(n.clone())),
                _ => Err(Signal::Error(RuntimeError::new("round() expects a number"))),
            }
        }
        "sqrt" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new("sqrt() expects 1 argument")));
            }
            match &args[0] {
                Value::Float(f) => Ok(Value::Float(f.sqrt())),
                Value::Int(n) => {
                    use num_traits::ToPrimitive;
                    Ok(Value::Float(n.to_f64().unwrap_or(0.0).sqrt()))
                }
                _ => Err(Signal::Error(RuntimeError::new("sqrt() expects a number"))),
            }
        }
        "pow" => {
            if args.len() != 2 {
                return Err(Signal::Error(RuntimeError::new("pow() expects 2 arguments")));
            }
            match (&args[0], &args[1]) {
                (Value::Int(a), Value::Int(b)) => {
                    use num_traits::ToPrimitive;
                    if let Some(exp) = b.to_u32() {
                        Ok(Value::Int(a.pow(exp)))
                    } else {
                        Err(Signal::Error(RuntimeError::new("exponent too large")))
                    }
                }
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a.powf(*b))),
                (Value::Int(a), Value::Float(b)) => {
                    use num_traits::ToPrimitive;
                    Ok(Value::Float(a.to_f64().unwrap_or(0.0).powf(*b)))
                }
                (Value::Float(a), Value::Int(b)) => {
                    use num_traits::ToPrimitive;
                    if let Some(exp) = b.to_i32() {
                        Ok(Value::Float(a.powi(exp)))
                    } else {
                        Err(Signal::Error(RuntimeError::new("exponent too large")))
                    }
                }
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
                        use num_traits::ToPrimitive;
                        let h = RandomState::new().build_hasher().finish();
                        let max_u64 = max.to_u64().unwrap_or(u64::MAX);
                        let rem = if max_u64 > 0 { h % max_u64 } else { 0 };
                        Ok(Value::Int(BigInt::from(rem)))
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
                Value::Channel(_) => "channel",
                Value::Atomic(_) => "atomic",
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

        // ---- Concurrency primitives ----
        "chan" => {
            let cap = if args.is_empty() {
                0 // unbuffered
            } else if args.len() == 1 {
                match &args[0] {
                    Value::Int(n) => n.to_usize().unwrap_or(0),
                    _ => {
                        return Err(Signal::Error(RuntimeError::new(
                            "chan() capacity must be an integer",
                        )))
                    }
                }
            } else {
                return Err(Signal::Error(RuntimeError::new(
                    "chan() expects 0 or 1 arguments",
                )));
            };
            Ok(Value::Channel(Arc::new(Mutex::new(ChannelInner::new(
                cap,
            )))))
        }
        "Atomic" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new(
                    "Atomic() expects 1 argument",
                )));
            }
            Ok(Value::Atomic(Arc::new(Mutex::new(AtomicInner::new(
                args[0].clone(),
            )))))
        }

        // Channel methods (called via member dispatch)
        "chan_send" => {
            if args.len() != 2 {
                return Err(Signal::Error(RuntimeError::new(
                    "channel.send() expects 1 argument (value)",
                )));
            }
            let ch = match &args[0] {
                Value::Channel(arc) => arc.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "send() called on non-channel",
                    )))
                }
            };
            let mut inner = ch.lock().map_err(|e| {
                Signal::Error(RuntimeError::new(format!("channel lock poisoned: {}", e)))
            })?;
            if inner.closed {
                return Err(Signal::Error(RuntimeError::new(
                    "cannot send on closed channel",
                )));
            }
            if inner.capacity > 0 && inner.queue.len() >= inner.capacity {
                return Err(Signal::Error(RuntimeError::new(
                    "channel is full (blocking send not yet supported)",
                )));
            }
            inner.queue.push_back(args[1].clone());
            Ok(Value::Null)
        }
        "chan_recv" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new(
                    "channel.recv() expects 0 arguments (caller is channel)",
                )));
            }
            let ch = match &args[0] {
                Value::Channel(arc) => arc.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "recv() called on non-channel",
                    )))
                }
            };
            let mut inner = ch.lock().map_err(|e| {
                Signal::Error(RuntimeError::new(format!("channel lock poisoned: {}", e)))
            })?;
            if inner.queue.is_empty() {
                if inner.closed {
                    return Ok(Value::Null);
                }
                return Err(Signal::Error(RuntimeError::new(
                    "channel is empty (blocking recv not yet supported)",
                )));
            }
            let val = inner.queue.pop_front().unwrap_or(Value::Null);
            Ok(val)
        }
        "chan_close" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new(
                    "channel.close() expects 0 arguments",
                )));
            }
            let ch = match &args[0] {
                Value::Channel(arc) => arc.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "close() called on non-channel",
                    )))
                }
            };
            let mut inner = ch.lock().map_err(|e| {
                Signal::Error(RuntimeError::new(format!("channel lock poisoned: {}", e)))
            })?;
            inner.closed = true;
            Ok(Value::Null)
        }

        // Atomic methods (called via member dispatch)
        "atomic_load" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new(
                    "atomic.load() expects 0 arguments",
                )));
            }
            let atm = match &args[0] {
                Value::Atomic(arc) => arc.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "load() called on non-atomic",
                    )))
                }
            };
            let inner = atm.lock().map_err(|e| {
                Signal::Error(RuntimeError::new(format!("atomic lock poisoned: {}", e)))
            })?;
            Ok(inner.value.clone())
        }
        "atomic_store" => {
            if args.len() != 2 {
                return Err(Signal::Error(RuntimeError::new(
                    "atomic.store() expects 1 argument (value)",
                )));
            }
            let atm = match &args[0] {
                Value::Atomic(arc) => arc.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "store() called on non-atomic",
                    )))
                }
            };
            let mut inner = atm.lock().map_err(|e| {
                Signal::Error(RuntimeError::new(format!("atomic lock poisoned: {}", e)))
            })?;
            inner.value = args[1].clone();
            Ok(Value::Null)
        }
        "atomic_add" => {
            if args.len() != 2 {
                return Err(Signal::Error(RuntimeError::new(
                    "atomic.add() expects 1 argument (value)",
                )));
            }
            let atm = match &args[0] {
                Value::Atomic(arc) => arc.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "add() called on non-atomic",
                    )))
                }
            };
            let mut inner = atm.lock().map_err(|e| {
                Signal::Error(RuntimeError::new(format!("atomic lock poisoned: {}", e)))
            })?;
            match (&inner.value, &args[1]) {
                (Value::Int(a), Value::Int(b)) => {
                    inner.value = Value::Int(a + b);
                }
                (Value::Float(a), Value::Float(b)) => {
                    inner.value = Value::Float(a + b);
                }
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "atomic.add() expects numeric values",
                    )))
                }
            }
            Ok(Value::Null)
        }
        "atomic_sub" => {
            if args.len() != 2 {
                return Err(Signal::Error(RuntimeError::new(
                    "atomic.sub() expects 1 argument (value)",
                )));
            }
            let atm = match &args[0] {
                Value::Atomic(arc) => arc.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "sub() called on non-atomic",
                    )))
                }
            };
            let mut inner = atm.lock().map_err(|e| {
                Signal::Error(RuntimeError::new(format!("atomic lock poisoned: {}", e)))
            })?;
            match (&inner.value, &args[1]) {
                (Value::Int(a), Value::Int(b)) => {
                    inner.value = Value::Int(a - b);
                }
                (Value::Float(a), Value::Float(b)) => {
                    inner.value = Value::Float(a - b);
                }
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "atomic.sub() expects numeric values",
                    )))
                }
            }
            Ok(Value::Null)
        }
        "atomic_cas" => {
            if args.len() != 3 {
                return Err(Signal::Error(RuntimeError::new(
                    "atomic.compareAndSwap() expects 2 arguments (old, new)",
                )));
            }
            let atm = match &args[0] {
                Value::Atomic(arc) => arc.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "compareAndSwap() called on non-atomic",
                    )))
                }
            };
            let mut inner = atm.lock().map_err(|e| {
                Signal::Error(RuntimeError::new(format!("atomic lock poisoned: {}", e)))
            })?;
            let swapped = values_eq(&inner.value, &args[1]);
            if swapped {
                inner.value = args[2].clone();
            }
            Ok(Value::Bool(swapped))
        }

        _ => Err(Signal::Error(RuntimeError::new(format!(
            "unknown builtin '{}'",
            name
        )))),
    }
}

/// Equality comparison for atomic CAS.
fn values_eq(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Int(a), Value::Int(b)) => a == b,
        (Value::Float(a), Value::Float(b)) => a == b,
        (Value::String(a), Value::String(b)) => a == b,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Null, Value::Null) => true,
        _ => false,
    }
}
