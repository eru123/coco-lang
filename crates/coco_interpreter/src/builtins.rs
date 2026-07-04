use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

use crate::error::{RuntimeError, Signal};
use crate::value::{value_eq, AtomicInner, ChannelInner, Value};
use num_bigint::BigInt;
use num_traits::ToPrimitive;

/// Allocate a Coco list value backed by Arc<CoW<Vec<Value>>>.
fn arc_list(items: Vec<Value>) -> Value {
    Value::List(std::sync::Arc::new(coco_gc::CoW::new(items)))
}

/// Allocate a Coco map value backed by Arc<CoW<HashMap<String, Value>>>.
fn arc_map(map: HashMap<String, Value>) -> Value {
    Value::Map(std::sync::Arc::new(coco_gc::CoW::new(map)))
}

/// Wrap a BigInt result into `Int64` if it fits in i64, else keep as `Int`.
/// Mirrors `Vm::normalize_int` so builtins keep the i64 fast path sticky.
fn normalize_bigint(n: BigInt) -> Value {
    use num_traits::ToPrimitive;
    if let Some(i) = n.to_i64() {
        Value::Int64(i)
    } else {
        Value::Int(n)
    }
}

/// Compare two numeric `Value`s (ints of either representation, or floats)
/// for ordering. Returns `None` if either is non-numeric or types mismatch
/// in a way that can't be compared. Used by `min`/`max`.
fn cmp_values(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    use num_traits::ToPrimitive;
    match (a, b) {
        // Both ints: compare via i64 when both fit, else BigInt.
        (x, y) if x.is_int() && y.is_int() => match (x.as_i64(), y.as_i64()) {
            (Some(xv), Some(yv)) => Some(xv.cmp(&yv)),
            _ => {
                let xa = x.to_bigint()?;
                let yb = y.to_bigint()?;
                Some(xa.cmp(&yb))
            }
        },
        (Value::Float(x), Value::Float(y)) => x.partial_cmp(y),
        // Mixed int/float: compare as f64 (may lose precision for huge ints,
        // but matches the pre-existing min/max semantics).
        (x, Value::Float(y)) if x.is_int() => Some(x.to_bigint()?.to_f64()?.partial_cmp(y)?),
        (Value::Float(x), y) if y.is_int() => Some(x.partial_cmp(&y.to_bigint()?.to_f64()?)?),
        _ => None,
    }
}

// ============================================================================
// TCP connection registry
// ============================================================================

/// Resource type tracked by the TCP registry.
enum TcpResource {
    Listener(TcpListener),
    Stream(TcpStream),
    Udp(std::net::UdpSocket),
}

/// Global registry mapping handle IDs to TCP resources.
static TCP_REGISTRY: std::sync::LazyLock<Arc<Mutex<HashMap<usize, TcpResource>>>> =
    std::sync::LazyLock::new(|| Arc::new(Mutex::new(HashMap::new())));

static NEXT_TCP_HANDLE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);

fn alloc_tcp_handle() -> usize {
    NEXT_TCP_HANDLE.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
}

fn register_tcp(handle: usize, resource: TcpResource) {
    TCP_REGISTRY.lock().unwrap().insert(handle, resource);
}

fn take_tcp(handle: usize) {
    TCP_REGISTRY.lock().unwrap().remove(&handle);
}

pub(crate) fn with_tcp_stream<F: FnOnce(&mut TcpStream) -> Result<Value, Signal>>(
    handle: usize,
    f: F,
) -> Result<Value, Signal> {
    let mut reg = TCP_REGISTRY.lock().unwrap();
    match reg.get_mut(&handle) {
        Some(TcpResource::Stream(s)) => f(s),
        Some(_) => Err(Signal::Error(RuntimeError::new(
            "TCP handle is not a stream",
        ))),
        None => Err(Signal::Error(RuntimeError::new("invalid TCP handle"))),
    }
}

pub(crate) fn with_tcp_listener<F: FnOnce(&TcpListener) -> Result<Value, Signal>>(
    handle: usize,
    f: F,
) -> Result<Value, Signal> {
    let reg = TCP_REGISTRY.lock().unwrap();
    match reg.get(&handle) {
        Some(TcpResource::Listener(l)) => f(l),
        Some(_) => Err(Signal::Error(RuntimeError::new(
            "handle is not a TCP listener",
        ))),
        None => Err(Signal::Error(RuntimeError::new("invalid TCP handle"))),
    }
}

fn with_udp_socket<F: FnOnce(&std::net::UdpSocket) -> Result<Value, Signal>>(
    handle: usize,
    f: F,
) -> Result<Value, Signal> {
    let reg = TCP_REGISTRY.lock().unwrap();
    match reg.get(&handle) {
        Some(TcpResource::Udp(s)) => f(s),
        Some(_) => Err(Signal::Error(RuntimeError::new(
            "handle is not a UDP socket",
        ))),
        None => Err(Signal::Error(RuntimeError::new("invalid UDP handle"))),
    }
}

// ============================================================================
// Base64 encode / decode (no external dependencies)
// ============================================================================

const B64_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn b64_encode(data: &[u8]) -> String {
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(B64_CHARS[((triple >> 18) & 0x3F) as usize] as char);
        out.push(B64_CHARS[((triple >> 12) & 0x3F) as usize] as char);
        out.push(if chunk.len() > 1 {
            B64_CHARS[((triple >> 6) & 0x3F) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64_CHARS[(triple & 0x3F) as usize] as char
        } else {
            '='
        });
    }
    out
}

fn b64_decode(s: &str) -> Result<Vec<u8>, String> {
    let s = s.trim_end_matches('=');
    let mut bytes = Vec::new();
    let mut buffer: u32 = 0;
    let mut bits = 0;
    for c in s.chars() {
        if c.is_whitespace() {
            continue;
        }
        let val = match c {
            'A'..='Z' => c as u32 - 'A' as u32,
            'a'..='z' => c as u32 - 'a' as u32 + 26,
            '0'..='9' => c as u32 - '0' as u32 + 52,
            '+' => 62,
            '/' => 63,
            _ => return Err(format!("invalid base64 character: {}", c)),
        };
        buffer = (buffer << 6) | val;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            bytes.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    Ok(bytes)
}

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
                Value::String(s) => Ok(Value::int_from_i64(s.len() as i64)),
                Value::List(l) => Ok(Value::int_from_i64(l.data.len() as i64)),
                Value::Map(m) => Ok(Value::int_from_i64(m.data.len() as i64)),
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
        "deepEquals" => {
            // Structural equality: deep for lists/maps, type-strict for
            // primitives, reference-based for channels/atomics. Used by the
            // HashSet (and any code needing value equality) instead of
            // toString comparison, which is order-dependent and conflates types.
            if args.len() != 2 {
                return Err(Signal::Error(RuntimeError::new(
                    "deepEquals(a, b) expects 2 arguments",
                )));
            }
            Ok(Value::Bool(value_eq(&args[0], &args[1])))
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
                Value::Int64(n) => Ok(Value::Int64(*n)),
                Value::Int(n) => Ok(Value::Int(n.clone())),
                Value::Float(f) => Ok(Value::int_from_i64(*f as i64)),
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
                Value::Int64(n) => Ok(Value::Float(*n as f64)),
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
                Value::Int64(n) => {
                    // i64::MIN abs overflows; escalate to BigInt for that one case.
                    use num_traits::Signed;
                    match n.checked_abs() {
                        Some(a) => Ok(Value::Int64(a)),
                        None => Ok(normalize_bigint((-BigInt::from(*n)).abs())),
                    }
                }
                Value::Int(n) => {
                    use num_traits::Signed;
                    Ok(normalize_bigint(n.abs()))
                }
                Value::Float(f) => Ok(Value::Float(f.abs())),
                _ => Err(Signal::Error(RuntimeError::new("abs() expects a number"))),
            }
        }
        "min" => {
            if args.len() < 2 {
                return Err(Signal::Error(RuntimeError::new(
                    "min() expects at least 2 arguments",
                )));
            }
            let mut best = &args[0];
            for arg in &args[1..] {
                if let Some(ord) = cmp_values(best, arg) {
                    if ord == std::cmp::Ordering::Greater {
                        best = arg;
                    }
                }
            }
            Ok(best.clone())
        }
        "max" => {
            if args.len() < 2 {
                return Err(Signal::Error(RuntimeError::new(
                    "max() expects at least 2 arguments",
                )));
            }
            let mut best = &args[0];
            for arg in &args[1..] {
                if let Some(ord) = cmp_values(best, arg) {
                    if ord == std::cmp::Ordering::Less {
                        best = arg;
                    }
                }
            }
            Ok(best.clone())
        }
        "floor" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new(
                    "floor() expects 1 argument",
                )));
            }
            match &args[0] {
                Value::Float(f) => Ok(Value::int_from_i64(f.floor() as i64)),
                Value::Int64(n) => Ok(Value::Int64(*n)),
                Value::Int(n) => Ok(Value::Int(n.clone())),
                _ => Err(Signal::Error(RuntimeError::new("floor() expects a number"))),
            }
        }
        "ceil" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new(
                    "ceil() expects 1 argument",
                )));
            }
            match &args[0] {
                Value::Float(f) => Ok(Value::int_from_i64(f.ceil() as i64)),
                Value::Int64(n) => Ok(Value::Int64(*n)),
                Value::Int(n) => Ok(Value::Int(n.clone())),
                _ => Err(Signal::Error(RuntimeError::new("ceil() expects a number"))),
            }
        }
        "round" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new(
                    "round() expects 1 argument",
                )));
            }
            match &args[0] {
                Value::Float(f) => Ok(Value::int_from_i64(f.round() as i64)),
                Value::Int64(n) => Ok(Value::Int64(*n)),
                Value::Int(n) => Ok(Value::Int(n.clone())),
                _ => Err(Signal::Error(RuntimeError::new("round() expects a number"))),
            }
        }
        "sqrt" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new(
                    "sqrt() expects 1 argument",
                )));
            }
            match &args[0] {
                Value::Float(f) => Ok(Value::Float(f.sqrt())),
                Value::Int64(n) => Ok(Value::Float((*n as f64).sqrt())),
                Value::Int(n) => {
                    use num_traits::ToPrimitive;
                    Ok(Value::Float(n.to_f64().unwrap_or(0.0).sqrt()))
                }
                _ => Err(Signal::Error(RuntimeError::new("sqrt() expects a number"))),
            }
        }
        "pow" => {
            if args.len() != 2 {
                return Err(Signal::Error(RuntimeError::new(
                    "pow() expects 2 arguments",
                )));
            }
            match (&args[0], &args[1]) {
                (a, b) if a.is_int() && b.is_int() => {
                    use num_traits::ToPrimitive;
                    let exp = b.to_bigint().unwrap();
                    if let Some(exp_u32) = exp.to_u32() {
                        let base = a.to_bigint().unwrap();
                        // normalize back to Int64 if it fits
                        let r = base.pow(exp_u32);
                        Ok(if let Some(i) = r.to_i64() {
                            Value::Int64(i)
                        } else {
                            Value::Int(r)
                        })
                    } else {
                        Err(Signal::Error(RuntimeError::new("exponent too large")))
                    }
                }
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a.powf(*b))),
                (a, Value::Float(b)) if a.is_int() => {
                    use num_traits::ToPrimitive;
                    Ok(Value::Float(
                        a.to_bigint().unwrap().to_f64().unwrap_or(0.0).powf(*b),
                    ))
                }
                (Value::Float(a), b) if b.is_int() => {
                    use num_traits::ToPrimitive;
                    if let Some(exp) = b.to_bigint().unwrap().to_i32() {
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
                    Value::Int64(max) => {
                        use std::collections::hash_map::RandomState;
                        use std::hash::{BuildHasher, Hasher};
                        let h = RandomState::new().build_hasher().finish();
                        let max_u64 = (*max as u64).max(1);
                        let rem = if max_u64 > 0 { h % max_u64 } else { 0 };
                        Ok(Value::int_from_i64(rem as i64))
                    }
                    Value::Int(max) => {
                        use num_traits::ToPrimitive;
                        use std::collections::hash_map::RandomState;
                        use std::hash::{BuildHasher, Hasher};
                        let h = RandomState::new().build_hasher().finish();
                        let max_u64 = max.to_u64().unwrap_or(u64::MAX);
                        let rem = if max_u64 > 0 { h % max_u64 } else { 0 };
                        Ok(Value::int_from_i64(rem as i64))
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
                return Err(Signal::Error(RuntimeError::new(
                    "typeOf() expects 1 argument",
                )));
            }
            let type_name = match &args[0] {
                Value::Int64(_) | Value::Int(_) => "int",
                Value::Float(_) => "float",
                Value::String(_) => "string",
                Value::Bool(_) => "bool",
                Value::Null => "null",
                Value::List(_) => "list",
                Value::Map(_) => "map",
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
                return Err(Signal::Error(RuntimeError::new(
                    "isOk() expects 1 argument",
                )));
            }
            Ok(Value::Bool(matches!(args[0], Value::Ok(_))))
        }
        "isErr" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new(
                    "isErr() expects 1 argument",
                )));
            }
            Ok(Value::Bool(matches!(args[0], Value::Err(_))))
        }
        "unwrap" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new(
                    "unwrap() expects 1 argument",
                )));
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

        // ---- Database builtins (std/db, backed by SQLite) ----
        #[cfg(feature = "db")]
        "db_open" => crate::db::db_open(args),
        #[cfg(feature = "db")]
        "db_exec" => crate::db::db_exec(args),
        #[cfg(feature = "db")]
        "db_query" => crate::db::db_query(args),
        #[cfg(feature = "db")]
        "db_close" => crate::db::db_close(args),

        // ---- Async I/O event loop (mio-backed fd readiness) ----
        #[cfg(feature = "async-io")]
        "io_wait" => crate::io_loop::io_wait(args),

        // ---- Filesystem builtins ----
        "fs_readFile" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new(
                    "fs_readFile() expects 1 argument (path)",
                )));
            }
            let path = match &args[0] {
                Value::String(s) => s.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "fs_readFile() expects a string path",
                    )))
                }
            };
            match std::fs::read_to_string(&path) {
                Ok(content) => Ok(Value::String(content)),
                Err(e) => Err(Signal::Error(RuntimeError::new(format!(
                    "fs_readFile: {}",
                    e
                )))),
            }
        }
        "fs_writeFile" => {
            if args.len() != 2 {
                return Err(Signal::Error(RuntimeError::new(
                    "fs_writeFile() expects 2 arguments (path, content)",
                )));
            }
            let path = match &args[0] {
                Value::String(s) => s.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "fs_writeFile() expects a string path",
                    )))
                }
            };
            let content = match &args[1] {
                Value::String(s) => s.clone(),
                _ => format!("{}", args[1]),
            };
            match std::fs::write(&path, &content) {
                Ok(_) => Ok(Value::Null),
                Err(e) => Err(Signal::Error(RuntimeError::new(format!(
                    "fs_writeFile: {}",
                    e
                )))),
            }
        }
        "fs_exists" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new(
                    "fs_exists() expects 1 argument (path)",
                )));
            }
            let path = match &args[0] {
                Value::String(s) => s.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "fs_exists() expects a string path",
                    )))
                }
            };
            Ok(Value::Bool(std::path::Path::new(&path).exists()))
        }
        "fs_readDir" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new(
                    "fs_readDir() expects 1 argument (path)",
                )));
            }
            let path = match &args[0] {
                Value::String(s) => s.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "fs_readDir() expects a string path",
                    )))
                }
            };
            match std::fs::read_dir(&path) {
                Ok(entries) => {
                    let mut names = Vec::new();
                    for entry in entries.flatten() {
                        names.push(Value::String(
                            entry.file_name().to_string_lossy().to_string(),
                        ));
                    }
                    Ok(arc_list(names))
                }
                Err(e) => Err(Signal::Error(RuntimeError::new(format!(
                    "fs_readDir: {}",
                    e
                )))),
            }
        }
        "fs_mkdir" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new(
                    "fs_mkdir() expects 1 argument (path)",
                )));
            }
            let path = match &args[0] {
                Value::String(s) => s.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "fs_mkdir() expects a string path",
                    )))
                }
            };
            match std::fs::create_dir_all(&path) {
                Ok(_) => Ok(Value::Null),
                Err(e) => Err(Signal::Error(RuntimeError::new(format!("fs_mkdir: {}", e)))),
            }
        }
        "fs_remove" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new(
                    "fs_remove() expects 1 argument (path)",
                )));
            }
            let path = match &args[0] {
                Value::String(s) => s.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "fs_remove() expects a string path",
                    )))
                }
            };
            let p = std::path::Path::new(&path);
            let result = if p.is_dir() {
                std::fs::remove_dir_all(p)
            } else {
                std::fs::remove_file(p)
            };
            match result {
                Ok(_) => Ok(Value::Null),
                Err(e) => Err(Signal::Error(RuntimeError::new(format!(
                    "fs_remove: {}",
                    e
                )))),
            }
        }
        "fs_stat" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new(
                    "fs_stat() expects 1 argument (path)",
                )));
            }
            let path = match &args[0] {
                Value::String(s) => s.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "fs_stat() expects a string path",
                    )))
                }
            };
            match std::fs::metadata(&path) {
                Ok(meta) => {
                    let mut map = HashMap::new();
                    map.insert("exists".to_string(), Value::Bool(true));
                    map.insert("isFile".to_string(), Value::Bool(meta.is_file()));
                    map.insert("isDir".to_string(), Value::Bool(meta.is_dir()));
                    map.insert("size".to_string(), Value::int_from_i64(meta.len() as i64));
                    {
                        Ok(arc_map(map))
                    }
                }
                Err(_) => {
                    let mut map = HashMap::new();
                    map.insert("exists".to_string(), Value::Bool(false));
                    {
                        Ok(arc_map(map))
                    }
                }
            }
        }

        // ---- TCP/HTTP builtins ----
        "tcp_listen" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new(
                    "tcp_listen() expects 1 argument (port)",
                )));
            }
            let port = match &args[0] {
                Value::Int(n) => n.to_u16().unwrap_or(0),
                Value::Int64(n) => (*n as u16),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "tcp_listen() expects an integer port",
                    )))
                }
            };
            match TcpListener::bind(format!("0.0.0.0:{}", port)) {
                Ok(listener) => {
                    let handle = alloc_tcp_handle();
                    register_tcp(handle, TcpResource::Listener(listener));
                    Ok(Value::int_from_i64(handle as i64))
                }
                Err(e) => Err(Signal::Error(RuntimeError::new(format!(
                    "tcp_listen: {}",
                    e
                )))),
            }
        }
        "tcp_accept" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new(
                    "tcp_accept() expects 1 argument (server_handle)",
                )));
            }
            let handle = match &args[0] {
                Value::Int(n) => n.to_usize().unwrap_or(0),
                Value::Int64(n) => (*n as usize),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "tcp_accept() expects an integer handle",
                    )))
                }
            };
            // Accept under the listener lock
            let (stream, addr) = {
                let reg = TCP_REGISTRY.lock().unwrap();
                match reg.get(&handle) {
                    Some(TcpResource::Listener(l)) => match l.accept() {
                        Ok((s, a)) => (s, a),
                        Err(e) => {
                            return Err(Signal::Error(RuntimeError::new(format!(
                                "tcp_accept: {}",
                                e
                            ))))
                        }
                    },
                    Some(_) => {
                        return Err(Signal::Error(RuntimeError::new(
                            "handle is not a TCP listener",
                        )))
                    }
                    None => return Err(Signal::Error(RuntimeError::new("invalid TCP handle"))),
                }
            };
            let client_handle = alloc_tcp_handle();
            register_tcp(client_handle, TcpResource::Stream(stream));
            let mut map = HashMap::new();
            map.insert(
                "handle".to_string(),
                Value::int_from_i64(client_handle as i64),
            );
            map.insert("address".to_string(), Value::String(addr.to_string()));
            {
                Ok(arc_map(map))
            }
        }
        "tcp_read" => {
            if args.len() != 2 {
                return Err(Signal::Error(RuntimeError::new(
                    "tcp_read() expects 2 arguments (handle, max_bytes)",
                )));
            }
            let handle = match &args[0] {
                Value::Int(n) => n.to_usize().unwrap_or(0),
                Value::Int64(n) => (*n as usize),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "tcp_read() expects an integer handle",
                    )))
                }
            };
            let max_bytes = match &args[1] {
                Value::Int(n) => n.to_usize().unwrap_or(1024),
                Value::Int64(n) => (*n as usize),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "tcp_read() expects an integer max_bytes",
                    )))
                }
            };
            with_tcp_stream(handle, |s| {
                let mut buf = vec![0u8; max_bytes];
                match s.read(&mut buf) {
                    Ok(0) => Ok(Value::Null), // EOF
                    Ok(n) => {
                        buf.truncate(n);
                        Ok(Value::String(String::from_utf8_lossy(&buf).to_string()))
                    }
                    Err(e) => Err(Signal::Error(RuntimeError::new(format!("tcp_read: {}", e)))),
                }
            })
        }
        "tcp_write" => {
            if args.len() != 2 {
                return Err(Signal::Error(RuntimeError::new(
                    "tcp_write() expects 2 arguments (handle, data)",
                )));
            }
            let handle = match &args[0] {
                Value::Int(n) => n.to_usize().unwrap_or(0),
                Value::Int64(n) => (*n as usize),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "tcp_write() expects an integer handle",
                    )))
                }
            };
            let data = match &args[1] {
                Value::String(s) => s.clone(),
                _ => format!("{}", args[1]),
            };
            with_tcp_stream(handle, |s| match s.write_all(data.as_bytes()) {
                Ok(_) => Ok(Value::Null),
                Err(e) => Err(Signal::Error(RuntimeError::new(format!(
                    "tcp_write: {}",
                    e
                )))),
            })
        }
        "tcp_close" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new(
                    "tcp_close() expects 1 argument (handle)",
                )));
            }
            let handle = match &args[0] {
                Value::Int(n) => n.to_usize().unwrap_or(0),
                Value::Int64(n) => (*n as usize),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "tcp_close() expects an integer handle",
                    )))
                }
            };
            // Just remove from registry — the Drop impl handles closing.
            take_tcp(handle);
            Ok(Value::Null)
        }
        "tcp_connect" => {
            if args.len() != 2 {
                return Err(Signal::Error(RuntimeError::new(
                    "tcp_connect() expects 2 arguments (host, port)",
                )));
            }
            let host = match &args[0] {
                Value::String(s) => s.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "tcp_connect() expects a string host",
                    )))
                }
            };
            let port = match &args[1] {
                Value::Int(n) => n.to_u16().unwrap_or(0),
                Value::Int64(n) => (*n as u16),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "tcp_connect() expects an integer port",
                    )))
                }
            };
            match TcpStream::connect(format!("{}:{}", host, port)) {
                Ok(stream) => {
                    let handle = alloc_tcp_handle();
                    register_tcp(handle, TcpResource::Stream(stream));
                    Ok(Value::int_from_i64(handle as i64))
                }
                Err(e) => Err(Signal::Error(RuntimeError::new(format!(
                    "tcp_connect: {}",
                    e
                )))),
            }
        }

        // ---- JSON builtins ----
        "json_parse" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new(
                    "json_parse() expects 1 argument (string)",
                )));
            }
            let json_str = match &args[0] {
                Value::String(s) => s.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "json_parse() expects a string",
                    )))
                }
            };
            Ok(json_to_coco(&json_str))
        }
        "json_stringify" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new(
                    "json_stringify() expects 1 argument (value)",
                )));
            }
            Ok(Value::String(coco_to_json_string(&args[0])))
        }

        // ---- String operations ----
        "str_split" => {
            if args.len() != 2 {
                return Err(Signal::Error(RuntimeError::new(
                    "str_split(str, delim) expects 2 args",
                )));
            }
            let s = match &args[0] {
                Value::String(s) => s.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "str_split: arg 1 must be string",
                    )))
                }
            };
            let delim = match &args[1] {
                Value::String(d) => d.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "str_split: arg 2 must be string",
                    )))
                }
            };
            let parts: Vec<Value> = s
                .split(&delim)
                .map(|p| Value::String(p.to_string()))
                .collect();
            Ok(arc_list(parts))
        }
        "str_replace" => {
            if args.len() != 3 {
                return Err(Signal::Error(RuntimeError::new(
                    "str_replace(str, from, to) expects 3 args",
                )));
            }
            let s = match &args[0] {
                Value::String(s) => s.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "str_replace: arg 1 must be string",
                    )))
                }
            };
            let from = match &args[1] {
                Value::String(f) => f.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "str_replace: arg 2 must be string",
                    )))
                }
            };
            let to = match &args[2] {
                Value::String(t) => t.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "str_replace: arg 3 must be string",
                    )))
                }
            };
            Ok(Value::String(s.replace(&from, &to)))
        }
        "str_trim" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new(
                    "str_trim(str) expects 1 arg",
                )));
            }
            match &args[0] {
                Value::String(s) => Ok(Value::String(s.trim().to_string())),
                _ => Err(Signal::Error(RuntimeError::new(
                    "str_trim expects a string",
                ))),
            }
        }
        "str_substring" => {
            if args.len() != 3 {
                return Err(Signal::Error(RuntimeError::new(
                    "str_substring(str, start, end) expects 3 args",
                )));
            }
            let s = match &args[0] {
                Value::String(s) => s.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "str_substring: arg 1 must be string",
                    )))
                }
            };
            let start = match &args[1] {
                Value::Int(n) => n.to_usize().unwrap_or(0),
                Value::Int64(n) => (*n as usize),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "str_substring: arg 2 must be int",
                    )))
                }
            };
            let end = match &args[2] {
                Value::Int(n) => n.to_usize().unwrap_or(s.len()),
                Value::Int64(n) => (*n as usize),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "str_substring: arg 3 must be int",
                    )))
                }
            };
            let chars: Vec<char> = s.chars().collect();
            let end = end.min(chars.len());
            if start > end {
                return Ok(Value::String(String::new()));
            }
            Ok(Value::String(chars[start..end].iter().collect()))
        }
        "str_indexOf" => {
            if args.len() != 2 {
                return Err(Signal::Error(RuntimeError::new(
                    "str_indexOf(str, search) expects 2 args",
                )));
            }
            let s = match &args[0] {
                Value::String(s) => s.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "str_indexOf: arg 1 must be string",
                    )))
                }
            };
            let search = match &args[1] {
                Value::String(f) => f.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "str_indexOf: arg 2 must be string",
                    )))
                }
            };
            match s.find(&search) {
                Some(i) => Ok(Value::int_from_i64(i as i64)),
                None => Ok(Value::int_from_i64(-1 as i64)),
            }
        }
        "str_toUpper" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new(
                    "str_toUpper(str) expects 1 arg",
                )));
            }
            match &args[0] {
                Value::String(s) => Ok(Value::String(s.to_uppercase())),
                _ => Err(Signal::Error(RuntimeError::new(
                    "str_toUpper expects a string",
                ))),
            }
        }
        "str_toLower" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new(
                    "str_toLower(str) expects 1 arg",
                )));
            }
            match &args[0] {
                Value::String(s) => Ok(Value::String(s.to_lowercase())),
                _ => Err(Signal::Error(RuntimeError::new(
                    "str_toLower expects a string",
                ))),
            }
        }
        "str_startsWith" => {
            if args.len() != 2 {
                return Err(Signal::Error(RuntimeError::new(
                    "str_startsWith(str, prefix) expects 2 args",
                )));
            }
            let s = match &args[0] {
                Value::String(s) => s.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "str_startsWith: arg 1 must be string",
                    )))
                }
            };
            let prefix = match &args[1] {
                Value::String(p) => p.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "str_startsWith: arg 2 must be string",
                    )))
                }
            };
            Ok(Value::Bool(s.starts_with(&prefix)))
        }
        "str_endsWith" => {
            if args.len() != 2 {
                return Err(Signal::Error(RuntimeError::new(
                    "str_endsWith(str, suffix) expects 2 args",
                )));
            }
            let s = match &args[0] {
                Value::String(s) => s.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "str_endsWith: arg 1 must be string",
                    )))
                }
            };
            let suffix = match &args[1] {
                Value::String(p) => p.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "str_endsWith: arg 2 must be string",
                    )))
                }
            };
            Ok(Value::Bool(s.ends_with(&suffix)))
        }

        // ---- Process / CLI ----
        "process_args" => {
            let args: Vec<Value> = std::env::args().map(|a| Value::String(a)).collect();
            Ok(arc_list(args))
        }
        "process_env" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new(
                    "process_env(key) expects 1 arg",
                )));
            }
            let key = match &args[0] {
                Value::String(k) => k.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "process_env expects a string key",
                    )))
                }
            };
            match std::env::var(&key) {
                Ok(v) => Ok(Value::String(v)),
                Err(_) => Ok(Value::Null),
            }
        }
        "process_exit" => {
            let code = if args.is_empty() {
                0
            } else {
                match &args[0] {
                    Value::Int(n) => n.to_i32().unwrap_or(0),
                    Value::Int64(n) => (*n as i32),
                    _ => 0,
                }
            };
            std::process::exit(code);
        }
        "process_cwd" => match std::env::current_dir() {
            Ok(p) => Ok(Value::String(p.to_string_lossy().to_string())),
            Err(e) => Err(Signal::Error(RuntimeError::new(format!("cwd: {}", e)))),
        },

        // ---- Time ----
        "time_now" => match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            Ok(d) => Ok(Value::int_from_i64(d.as_millis() as i64 as i64)),
            Err(_) => Ok(Value::int_from_i64(0 as i64)),
        },
        "time_sleep" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new(
                    "time_sleep(ms) expects 1 arg",
                )));
            }
            let ms = match &args[0] {
                Value::Int(n) => n.to_u64().unwrap_or(0),
                Value::Int64(n) => (*n as u64),
                Value::Float(f) => (*f * 1000.0) as u64,
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "time_sleep expects a number (milliseconds)",
                    )))
                }
            };
            std::thread::sleep(std::time::Duration::from_millis(ms));
            Ok(Value::Null)
        }

        // ---- Type casts ----
        "int" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new("int(x) expects 1 arg")));
            }
            match &args[0] {
                Value::Int64(n) => Ok(Value::Int64(*n)),
                Value::Int(n) => Ok(Value::Int(n.clone())),
                Value::Float(f) => Ok(Value::int_from_i64(*f as i64)),
                Value::String(s) => match s.parse::<BigInt>() {
                    Ok(n) => Ok(Value::Int(n)),
                    Err(_) => Err(Signal::Error(RuntimeError::new(format!(
                        "cannot convert '{}' to int",
                        s
                    )))),
                },
                Value::Bool(b) => Ok(Value::int_from_i64(if *b { 1 } else { 0 })),
                _ => Err(Signal::Error(RuntimeError::new(
                    "int() cannot convert this value",
                ))),
            }
        }
        "float" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new("float(x) expects 1 arg")));
            }
            match &args[0] {
                Value::Float(f) => Ok(Value::Float(*f)),
                Value::Int64(n) => Ok(Value::Float(*n as f64)),
                Value::Int(n) => {
                    use num_traits::ToPrimitive;
                    Ok(Value::Float(n.to_f64().unwrap_or(0.0)))
                }
                Value::String(s) => match s.parse::<f64>() {
                    Ok(f) => Ok(Value::Float(f)),
                    Err(_) => Err(Signal::Error(RuntimeError::new(format!(
                        "cannot convert '{}' to float",
                        s
                    )))),
                },
                Value::Bool(b) => Ok(Value::Float(if *b { 1.0 } else { 0.0 })),
                _ => Err(Signal::Error(RuntimeError::new(
                    "float() cannot convert this value",
                ))),
            }
        }
        "bool" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new("bool(x) expects 1 arg")));
            }
            Ok(Value::Bool(args[0].is_truthy()))
        }

        // ---- Error constructor ----
        "error" => {
            let msg = if args.is_empty() {
                "error".to_string()
            } else {
                format!("{}", args[0])
            };
            let mut map = HashMap::new();
            map.insert("message".to_string(), Value::String(msg));
            map.insert("__error__".to_string(), Value::Bool(true));
            Ok(arc_map(map))
        }

        // ---- Encoding ----
        "base64_encode" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new(
                    "base64_encode(str) expects 1 arg",
                )));
            }
            let s = match &args[0] {
                Value::String(s) => s.clone(),
                _ => format!("{}", args[0]),
            };
            Ok(Value::String(b64_encode(s.as_bytes())))
        }
        "base64_decode" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new(
                    "base64_decode(str) expects 1 arg",
                )));
            }
            let s = match &args[0] {
                Value::String(s) => s.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "base64_decode expects a string",
                    )))
                }
            };
            match b64_decode(&s) {
                Ok(v) => Ok(Value::String(String::from_utf8_lossy(&v).to_string())),
                Err(e) => Err(Signal::Error(RuntimeError::new(e))),
            }
        }
        "hex_encode" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new(
                    "hex_encode(str) expects 1 arg",
                )));
            }
            let s = match &args[0] {
                Value::String(s) => s.clone(),
                _ => format!("{}", args[0]),
            };
            Ok(Value::String(
                s.as_bytes()
                    .iter()
                    .map(|b| format!("{:02x}", b))
                    .collect::<String>(),
            ))
        }
        "hex_decode" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new(
                    "hex_decode(str) expects 1 arg",
                )));
            }
            let s = match &args[0] {
                Value::String(s) => s.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "hex_decode expects a string",
                    )))
                }
            };
            if s.len() % 2 != 0 {
                return Err(Signal::Error(RuntimeError::new(
                    "hex_decode: odd length string",
                )));
            }
            let bytes: Result<Vec<u8>, _> = (0..s.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
                .collect();
            match bytes {
                Ok(v) => Ok(Value::String(String::from_utf8_lossy(&v).to_string())),
                Err(e) => Err(Signal::Error(RuntimeError::new(format!(
                    "hex_decode: {}",
                    e
                )))),
            }
        }

        // ---- Extended socket ops ----
        "tcp_readLine" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new(
                    "tcp_readLine(handle) expects 1 arg",
                )));
            }
            let handle = match &args[0] {
                Value::Int(n) => n.to_usize().unwrap_or(0),
                Value::Int64(n) => (*n as usize),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "tcp_readLine expects an int handle",
                    )))
                }
            };
            with_tcp_stream(handle, |s| {
                let mut buf = Vec::new();
                let mut byte = [0u8; 1];
                loop {
                    match s.read(&mut byte) {
                        Ok(0) => break,
                        Ok(_) => {
                            buf.push(byte[0]);
                            if byte[0] == b'\n' {
                                break;
                            }
                        }
                        Err(e) => {
                            return Err(Signal::Error(RuntimeError::new(format!(
                                "tcp_readLine: {}",
                                e
                            ))))
                        }
                    }
                }
                Ok(Value::String(
                    String::from_utf8_lossy(&buf)
                        .trim_end_matches(|c| c == '\r' || c == '\n')
                        .to_string(),
                ))
            })
        }
        "tcp_setTimeout" => {
            if args.len() != 2 {
                return Err(Signal::Error(RuntimeError::new(
                    "tcp_setTimeout(handle, ms) expects 2 args",
                )));
            }
            let handle = match &args[0] {
                Value::Int(n) => n.to_usize().unwrap_or(0),
                Value::Int64(n) => (*n as usize),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "tcp_setTimeout expects an int handle",
                    )))
                }
            };
            let ms = match &args[1] {
                Value::Int(n) => n.to_u64().unwrap_or(0),
                Value::Int64(n) => (*n as u64),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "tcp_setTimeout expects int milliseconds",
                    )))
                }
            };
            with_tcp_stream(handle, |s| {
                s.set_read_timeout(Some(std::time::Duration::from_millis(ms)))
                    .map_err(|e| {
                        Signal::Error(RuntimeError::new(format!("tcp_setTimeout: {}", e)))
                    })?;
                Ok(Value::Null)
            })
        }

        // ---- UDP socket ops ----
        "udp_bind" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new(
                    "udp_bind(port) expects 1 arg",
                )));
            }
            let port = match &args[0] {
                Value::Int(n) => n.to_u16().unwrap_or(0),
                Value::Int64(n) => (*n as u16),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "udp_bind expects an int port",
                    )))
                }
            };
            match std::net::UdpSocket::bind(format!("0.0.0.0:{}", port)) {
                Ok(socket) => {
                    let h = alloc_tcp_handle();
                    register_tcp(h, TcpResource::Udp(socket));
                    Ok(Value::int_from_i64(h as i64))
                }
                Err(e) => Err(Signal::Error(RuntimeError::new(format!("udp_bind: {}", e)))),
            }
        }
        "udp_send" => {
            if args.len() != 4 {
                return Err(Signal::Error(RuntimeError::new(
                    "udp_send(handle, host, port, data) expects 4 args",
                )));
            }
            let handle = match &args[0] {
                Value::Int(n) => n.to_usize().unwrap_or(0),
                Value::Int64(n) => (*n as usize),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "udp_send expects an int handle",
                    )))
                }
            };
            let host = match &args[1] {
                Value::String(s) => s.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "udp_send expects a string host",
                    )))
                }
            };
            let port = match &args[2] {
                Value::Int(n) => n.to_u16().unwrap_or(0),
                Value::Int64(n) => (*n as u16),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "udp_send expects an int port",
                    )))
                }
            };
            let data = match &args[3] {
                Value::String(s) => s.clone(),
                _ => format!("{}", args[3]),
            };
            with_udp_socket(handle, |s| {
                s.send_to(data.as_bytes(), format!("{}:{}", host, port))
                    .map_err(|e| Signal::Error(RuntimeError::new(format!("udp_send: {}", e))))?;
                Ok(Value::Null)
            })
        }
        "udp_recv" => {
            if args.len() != 2 {
                return Err(Signal::Error(RuntimeError::new(
                    "udp_recv(handle, max_bytes) expects 2 args",
                )));
            }
            let handle = match &args[0] {
                Value::Int(n) => n.to_usize().unwrap_or(0),
                Value::Int64(n) => (*n as usize),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "udp_recv expects an int handle",
                    )))
                }
            };
            let max = match &args[1] {
                Value::Int(n) => n.to_usize().unwrap_or(1024),
                Value::Int64(n) => (*n as usize),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "udp_recv expects int max_bytes",
                    )))
                }
            };
            with_udp_socket(handle, |s| {
                let mut buf = vec![0u8; max];
                match s.recv_from(&mut buf) {
                    Ok((n, addr)) => {
                        let mut map = HashMap::new();
                        map.insert(
                            "data".to_string(),
                            Value::String(String::from_utf8_lossy(&buf[..n]).to_string()),
                        );
                        map.insert("address".to_string(), Value::String(addr.to_string()));
                        Ok(arc_map(map))
                    }
                    Err(e) => Err(Signal::Error(RuntimeError::new(format!("udp_recv: {}", e)))),
                }
            })
        }
        "udp_close" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new(
                    "udp_close(handle) expects 1 arg",
                )));
            }
            let handle = match &args[0] {
                Value::Int(n) => n.to_usize().unwrap_or(0),
                Value::Int64(n) => (*n as usize),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "udp_close expects an int handle",
                    )))
                }
            };
            take_tcp(handle);
            Ok(Value::Null)
        }

        // ---- Regex ----
        "regex_match" => {
            if args.len() != 2 {
                return Err(Signal::Error(RuntimeError::new(
                    "regex_match(pattern, str) expects 2 args",
                )));
            }
            let pat = match &args[0] {
                Value::String(s) => s.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "regex_match expects a string pattern",
                    )))
                }
            };
            let s = match &args[1] {
                Value::String(s) => s.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "regex_match expects a string to search",
                    )))
                }
            };
            match regex::Regex::new(&pat) {
                Ok(re) => Ok(Value::Bool(re.is_match(&s))),
                Err(e) => Err(Signal::Error(RuntimeError::new(format!("regex: {}", e)))),
            }
        }
        "regex_replace" => {
            if args.len() != 3 {
                return Err(Signal::Error(RuntimeError::new(
                    "regex_replace(pattern, replacement, str) expects 3 args",
                )));
            }
            let pat = match &args[0] {
                Value::String(s) => s.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "regex_replace expects a string pattern",
                    )))
                }
            };
            let repl = match &args[1] {
                Value::String(s) => s.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "regex_replace expects a string replacement",
                    )))
                }
            };
            let s = match &args[2] {
                Value::String(s) => s.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "regex_replace expects a string",
                    )))
                }
            };
            match regex::Regex::new(&pat) {
                Ok(re) => Ok(Value::String(re.replace_all(&s, &repl[..]).to_string())),
                Err(e) => Err(Signal::Error(RuntimeError::new(format!("regex: {}", e)))),
            }
        }

        // ---- List mutation ----
        "list_push" => {
            if args.len() != 2 {
                return Err(Signal::Error(RuntimeError::new(
                    "list_push(list, value) expects 2 args",
                )));
            }
            let list = match &args[0] {
                Value::List(l) => l,
                _ => return Err(Signal::Error(RuntimeError::new("list_push expects a list"))),
            };
            let mut items: Vec<Value> = list.data.iter().cloned().collect();
            items.push(args[1].clone());
            Ok(arc_list(items))
        }
        "list_pop" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new(
                    "list_pop(list) expects 1 arg",
                )));
            }
            let list = match &args[0] {
                Value::List(l) => l,
                _ => return Err(Signal::Error(RuntimeError::new("list_pop expects a list"))),
            };
            if list.data.is_empty() {
                return Ok(Value::Null);
            }
            let mut items: Vec<Value> = list.data.iter().cloned().collect();
            let popped = items.pop().unwrap_or(Value::Null);
            // Return a tuple-like map with the popped value and the new list
            let mut map = HashMap::new();
            map.insert("value".to_string(), popped);
            map.insert("list".to_string(), arc_list(items));
            Ok(arc_map(map))
        }
        "list_insert" => {
            if args.len() != 3 {
                return Err(Signal::Error(RuntimeError::new(
                    "list_insert(list, index, value) expects 3 args",
                )));
            }
            let list = match &args[0] {
                Value::List(l) => l,
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "list_insert expects a list",
                    )))
                }
            };
            let idx = match &args[1] {
                Value::Int(n) => n.to_usize().unwrap_or(0),
                Value::Int64(n) => (*n as usize),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "list_insert expects an int index",
                    )))
                }
            };
            let mut items: Vec<Value> = list.data.iter().cloned().collect();
            let idx = idx.min(items.len());
            items.insert(idx, args[2].clone());
            Ok(arc_list(items))
        }
        "list_remove" => {
            if args.len() != 2 {
                return Err(Signal::Error(RuntimeError::new(
                    "list_remove(list, index) expects 2 args",
                )));
            }
            let list = match &args[0] {
                Value::List(l) => l,
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "list_remove expects a list",
                    )))
                }
            };
            let idx = match &args[1] {
                Value::Int(n) => n.to_usize().unwrap_or(0),
                Value::Int64(n) => (*n as usize),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "list_remove expects an int index",
                    )))
                }
            };
            if idx >= list.data.len() {
                return Ok(args[0].clone());
            }
            let mut items: Vec<Value> = list.data.iter().cloned().collect();
            items.remove(idx);
            Ok(arc_list(items))
        }
        "list_join" => {
            if args.len() != 2 {
                return Err(Signal::Error(RuntimeError::new(
                    "list_join(list, sep) expects 2 args",
                )));
            }
            let list = match &args[0] {
                Value::List(l) => l,
                _ => return Err(Signal::Error(RuntimeError::new("list_join expects a list"))),
            };
            let sep = match &args[1] {
                Value::String(s) => s.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "list_join expects a string separator",
                    )))
                }
            };
            let parts: Vec<String> = list.data.iter().map(|v| format!("{}", v)).collect();
            Ok(Value::String(parts.join(&sep)))
        }

        // ---- Map mutation ----
        "map_set" => {
            if args.len() != 3 {
                return Err(Signal::Error(RuntimeError::new(
                    "map_set(map, key, value) expects 3 args",
                )));
            }
            let map = match &args[0] {
                Value::Map(m) => m,
                _ => return Err(Signal::Error(RuntimeError::new("map_set expects a map"))),
            };
            let key = match &args[1] {
                Value::String(s) => s.clone(),
                _ => format!("{}", args[1]),
            };
            let mut data: HashMap<String, Value> = map
                .data
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            data.insert(key, args[2].clone());
            Ok(arc_map(data))
        }
        "map_get" => {
            if args.len() < 2 || args.len() > 3 {
                return Err(Signal::Error(RuntimeError::new(
                    "map_get(map, key, default?) expects 2-3 args",
                )));
            }
            let map = match &args[0] {
                Value::Map(m) => m,
                _ => return Err(Signal::Error(RuntimeError::new("map_get expects a map"))),
            };
            let key = match &args[1] {
                Value::String(s) => s.clone(),
                _ => format!("{}", args[1]),
            };
            match map.data.get(&key) {
                Some(v) => Ok(v.clone()),
                None => Ok(args.get(2).cloned().unwrap_or(Value::Null)),
            }
        }
        "map_has" => {
            if args.len() != 2 {
                return Err(Signal::Error(RuntimeError::new(
                    "map_has(map, key) expects 2 args",
                )));
            }
            let map = match &args[0] {
                Value::Map(m) => m,
                _ => return Err(Signal::Error(RuntimeError::new("map_has expects a map"))),
            };
            let key = match &args[1] {
                Value::String(s) => s.clone(),
                _ => format!("{}", args[1]),
            };
            Ok(Value::Bool(map.data.contains_key(&key)))
        }
        "map_delete" => {
            if args.len() != 2 {
                return Err(Signal::Error(RuntimeError::new(
                    "map_delete(map, key) expects 2 args",
                )));
            }
            let map = match &args[0] {
                Value::Map(m) => m,
                _ => return Err(Signal::Error(RuntimeError::new("map_delete expects a map"))),
            };
            let key = match &args[1] {
                Value::String(s) => s.clone(),
                _ => format!("{}", args[1]),
            };
            let mut data: HashMap<String, Value> = map
                .data
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            data.remove(&key);
            Ok(arc_map(data))
        }
        "map_keys" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new(
                    "map_keys(map) expects 1 arg",
                )));
            }
            let map = match &args[0] {
                Value::Map(m) => m,
                _ => return Err(Signal::Error(RuntimeError::new("map_keys expects a map"))),
            };
            let keys: Vec<Value> = map.data.keys().map(|k| Value::String(k.clone())).collect();
            Ok(arc_list(keys))
        }
        "map_values" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new(
                    "map_values(map) expects 1 arg",
                )));
            }
            let map = match &args[0] {
                Value::Map(m) => m,
                _ => return Err(Signal::Error(RuntimeError::new("map_values expects a map"))),
            };
            let vals: Vec<Value> = map.data.values().cloned().collect();
            Ok(arc_list(vals))
        }

        // ---- More utilities ----
        "str_contains" => {
            if args.len() != 2 {
                return Err(Signal::Error(RuntimeError::new(
                    "str_contains(str, search) expects 2 args",
                )));
            }
            let s = match &args[0] {
                Value::String(s) => s.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "str_contains expects a string",
                    )))
                }
            };
            let search = match &args[1] {
                Value::String(f) => f.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "str_contains: arg 2 must be string",
                    )))
                }
            };
            Ok(Value::Bool(s.contains(&search)))
        }
        "str_charAt" => {
            if args.len() != 2 {
                return Err(Signal::Error(RuntimeError::new(
                    "str_charAt(str, index) expects 2 args",
                )));
            }
            let s = match &args[0] {
                Value::String(s) => s.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "str_charAt expects a string",
                    )))
                }
            };
            let idx = match &args[1] {
                Value::Int(n) => n.to_usize().unwrap_or(0),
                Value::Int64(n) => (*n as usize),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "str_charAt expects an int index",
                    )))
                }
            };
            let chars: Vec<char> = s.chars().collect();
            Ok(if idx < chars.len() {
                Value::String(chars[idx].to_string())
            } else {
                Value::Null
            })
        }
        "str_repeat" => {
            if args.len() != 2 {
                return Err(Signal::Error(RuntimeError::new(
                    "str_repeat(str, count) expects 2 args",
                )));
            }
            let s = match &args[0] {
                Value::String(s) => s.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "str_repeat expects a string",
                    )))
                }
            };
            let n = match &args[1] {
                Value::Int(n) => n.to_usize().unwrap_or(1),
                Value::Int64(n) => (*n as usize),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "str_repeat expects an int count",
                    )))
                }
            };
            Ok(Value::String(s.repeat(n)))
        }
        "assert" => {
            if args.len() < 1 || args.len() > 2 {
                return Err(Signal::Error(RuntimeError::new(
                    "assert(condition, message?) expects 1-2 args",
                )));
            }
            if !args[0].is_truthy() {
                let msg = args
                    .get(1)
                    .map(|v| format!("{}", v))
                    .unwrap_or_else(|| "assertion failed".to_string());
                return Err(Signal::Error(RuntimeError::new(msg)));
            }
            Ok(Value::Null)
        }
        "typeIs" => {
            if args.len() != 2 {
                return Err(Signal::Error(RuntimeError::new(
                    "typeIs(value, typeName) expects 2 args",
                )));
            }
            let type_name = match &args[1] {
                Value::String(s) => s.clone(),
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "typeIs expects a string type name",
                    )))
                }
            };
            let matches = match (&args[0], type_name.as_str()) {
                (Value::Int64(_), "int")
                | (Value::Int(_), "int")
                | (Value::Float(_), "float")
                | (Value::String(_), "string")
                | (Value::Bool(_), "bool")
                | (Value::Null, "null")
                | (Value::List(_), "list")
                | (Value::Map(_), "map")
                | (Value::FnObj(_), "function")
                | (Value::BuiltinFn(_), "builtin")
                | (Value::TaskHandle(_), "task")
                | (Value::Ok(_), "result")
                | (Value::Err(_), "result")
                | (Value::Channel(_), "channel")
                | (Value::Atomic(_), "atomic") => true,
                _ => false,
            };
            Ok(Value::Bool(matches))
        }
        "range" => {
            if args.len() < 1 || args.len() > 3 {
                return Err(Signal::Error(RuntimeError::new(
                    "range(start, end, step?) expects 1-3 args",
                )));
            }
            // Accept both Int64 (i64 fast path) and Int (BigInt); the compiler
            // emits int literals as Int64, so the old Int-only match rejected
            // `range(0, 5)`.
            let to_i64 = |v: &Value| -> Result<i64, Signal> {
                v.as_i64()
                    .or_else(|| v.to_bigint().and_then(|b| b.to_i64()))
                    .ok_or_else(|| Signal::Error(RuntimeError::new("range expects int arguments")))
            };
            let start = to_i64(&args[0])?;
            let end = if args.len() >= 2 {
                to_i64(&args[1])?
            } else {
                let _e = start;
                0
            };
            let start = if args.len() == 1 { 0 } else { start };
            let step = if args.len() >= 3 {
                to_i64(&args[2])?
            } else {
                1
            };
            let mut items: Vec<Value> = Vec::new();
            let mut i = start;
            if step > 0 {
                while i < end {
                    items.push(Value::int_from_i64(i as i64));
                    i += step;
                }
            } else {
                while i > end {
                    items.push(Value::int_from_i64(i as i64));
                    i += step;
                }
            }
            Ok(arc_list(items))
        }

        // ---- Hashing ----
        "hash" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new(
                    "hash(value) expects 1 arg",
                )));
            }
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            match &args[0] {
                Value::Int64(n) => n.hash(&mut h),
                Value::Int(n) => n.hash(&mut h),
                Value::Float(f) => f.to_bits().hash(&mut h),
                Value::String(s) => s.hash(&mut h),
                Value::Bool(b) => b.hash(&mut h),
                Value::Null => 0u8.hash(&mut h),
                _ => format!("{:?}", args[0]).hash(&mut h),
            }
            Ok(Value::int_from_i64(h.finish() as i64))
        }

        // ---- SHA256 hashing ----
        "sha256" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new(
                    "sha256(str) expects 1 arg",
                )));
            }
            let s = match &args[0] {
                Value::String(s) => s.clone(),
                _ => format!("{}", args[0]),
            };
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(s.as_bytes());
            Ok(Value::String(format!("{:x}", hasher.finalize())))
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
            Ok(Value::Channel(Arc::new(Mutex::new(ChannelInner::new(cap)))))
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
                (a, b) if a.is_int() && b.is_int() => {
                    let ba = a.to_bigint().unwrap();
                    let bb = b.to_bigint().unwrap();
                    let r = ba + bb;
                    inner.value = normalize_bigint(r);
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
                (a, b) if a.is_int() && b.is_int() => {
                    let ba = a.to_bigint().unwrap();
                    let bb = b.to_bigint().unwrap();
                    let r = ba - bb;
                    inner.value = normalize_bigint(r);
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

// ============================================================================
// JSON conversion helpers
// ============================================================================

/// Parse a JSON string into a Coco Value using a simple recursive-descent parser.
fn json_to_coco(json: &str) -> Value {
    let trimmed = json.trim();
    if trimmed.is_empty() {
        return Value::Null;
    }
    let bytes = trimmed.as_bytes();
    match bytes[0] {
        b'{' => json_parse_object(trimmed),
        b'[' => json_parse_array(trimmed),
        b'"' => json_parse_string(trimmed),
        b't' | b'f' => json_parse_bool(trimmed),
        b'n' => Value::Null,
        _ => json_parse_number(trimmed),
    }
}

fn json_parse_object(s: &str) -> Value {
    let mut map = HashMap::new();
    let inner = &s[1..s.len() - 1];
    if !inner.trim().is_empty() {
        let pairs = json_split_top_level(inner, b',');
        for pair in pairs {
            let colon = json_find_outside_string(pair, b':');
            if let Some(pos) = colon {
                let key = json_to_value_string(&pair[..pos].trim().trim_matches('"'));
                let val = json_to_coco(&pair[pos + 1..]);
                map.insert(key, val);
            }
        }
    }
    arc_map(map)
}

fn json_parse_array(s: &str) -> Value {
    let inner = &s[1..s.len() - 1];
    if inner.trim().is_empty() {
        return arc_list(Vec::new());
    }
    let parts = json_split_top_level(inner, b',');
    let mut items = Vec::with_capacity(parts.len());
    for part in parts {
        items.push(json_to_coco(part));
    }
    arc_list(items)
}

fn json_parse_string(s: &str) -> Value {
    Value::String(json_to_value_string(s))
}

fn json_to_value_string(s: &str) -> String {
    let mut result = String::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    let len = bytes.len();
    while i < len {
        if bytes[i] == b'\\' && i + 1 < len {
            match bytes[i + 1] {
                b'"' => {
                    result.push('"');
                    i += 2;
                }
                b'\\' => {
                    result.push('\\');
                    i += 2;
                }
                b'/' => {
                    result.push('/');
                    i += 2;
                }
                b'n' => {
                    result.push('\n');
                    i += 2;
                }
                b't' => {
                    result.push('\t');
                    i += 2;
                }
                b'r' => {
                    result.push('\r');
                    i += 2;
                }
                b'u' => {
                    // Unicode escape \uXXXX — parse hex
                    if i + 6 <= len {
                        let hex = std::str::from_utf8(&bytes[i + 2..i + 6]).unwrap_or("");
                        if let Ok(code) = u32::from_str_radix(hex, 16) {
                            if let Some(c) = char::from_u32(code) {
                                result.push(c);
                            }
                        }
                        i += 6;
                    } else {
                        i += 1;
                    }
                    continue;
                }
                _ => {
                    result.push(bytes[i + 1] as char);
                    i += 2;
                }
            }
        } else if bytes[i] != b'"' {
            result.push(bytes[i] as char);
            i += 1;
        } else {
            i += 1;
        }
    }
    result
}

fn json_parse_bool(s: &str) -> Value {
    Value::Bool(s.trim() == "true")
}

fn json_parse_number(s: &str) -> Value {
    let trimmed = s.trim();
    if let Ok(n) = trimmed.parse::<i64>() {
        Value::int_from_i64(n as i64)
    } else if let Ok(f) = trimmed.parse::<f64>() {
        Value::Float(f)
    } else {
        Value::Null
    }
}

/// Split a string by delimiter, but only at the top level (respecting nested brackets and strings).
fn json_split_top_level(s: &str, delim: u8) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0;
    let mut in_string = false;
    let mut start = 0;
    let bytes = s.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if in_string {
            if b == b'"' && (i == 0 || bytes[i - 1] != b'\\') {
                in_string = false;
            }
        } else {
            match b {
                b'"' => in_string = true,
                b'{' | b'[' => depth += 1,
                b'}' | b']' => depth -= 1,
                _ if b == delim && depth == 0 => {
                    parts.push(&s[start..i]);
                    start = i + 1;
                }
                _ => {}
            }
        }
    }
    parts.push(&s[start..]);
    parts
}

fn json_find_outside_string(s: &str, target: u8) -> Option<usize> {
    let mut in_string = false;
    let bytes = s.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if in_string {
            if b == b'"' && (i == 0 || bytes[i - 1] != b'\\') {
                in_string = false;
            }
        } else {
            match b {
                b'"' => in_string = true,
                _ if b == target => return Some(i),
                _ => {}
            }
        }
    }
    None
}

/// Convert a Coco Value to a JSON-formatted string manually.
fn coco_to_json_string(val: &Value) -> String {
    match val {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Int(n) => n.to_string(),
        Value::Float(f) => {
            if f.is_nan() {
                "null".to_string()
            } else if f.is_infinite() {
                "null".to_string()
            } else {
                f.to_string()
            }
        }
        Value::String(s) => {
            let mut out = String::from("\"");
            for c in s.chars() {
                match c {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    _ => out.push(c),
                }
            }
            out.push('"');
            out
        }
        Value::List(list) => {
            let mut out = String::from("[");
            for (i, item) in list.data.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&coco_to_json_string(item));
            }
            out.push(']');
            out
        }
        Value::Map(map) => {
            let mut out = String::from("{");
            let mut first = true;
            for (k, v) in map.data.iter() {
                if !first {
                    out.push(',');
                }
                first = false;
                // JSON-escape key
                out.push('"');
                for c in k.chars() {
                    match c {
                        '"' => out.push_str("\\\""),
                        '\\' => out.push_str("\\\\"),
                        '\n' => out.push_str("\\n"),
                        _ => out.push(c),
                    }
                }
                out.push('"');
                out.push(':');
                out.push_str(&coco_to_json_string(v));
            }
            out.push('}');
            out
        }
        _ => format!("\"{}\"", val), // fallback for non-JSON types
    }
}
