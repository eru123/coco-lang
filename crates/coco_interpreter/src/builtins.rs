use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

use coco_gc::{CoW, Gc};

use crate::error::{RuntimeError, Signal};
use crate::value::{AtomicInner, ChannelInner, Value};
use num_bigint::BigInt;
use num_traits::ToPrimitive;

// ============================================================================
// TCP connection registry
// ============================================================================

/// Resource type tracked by the TCP registry.
enum TcpResource {
    Listener(TcpListener),
    Stream(TcpStream),
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

fn with_tcp_stream<F: FnOnce(&mut TcpStream) -> Result<Value, Signal>>(
    handle: usize,
    f: F,
) -> Result<Value, Signal> {
    let mut reg = TCP_REGISTRY.lock().unwrap();
    match reg.get_mut(&handle) {
        Some(TcpResource::Stream(s)) => f(s),
        Some(_) => Err(Signal::Error(RuntimeError::new("TCP handle is not a stream"))),
        None => Err(Signal::Error(RuntimeError::new("invalid TCP handle"))),
    }
}

fn with_tcp_listener<F: FnOnce(&TcpListener) -> Result<Value, Signal>>(
    handle: usize,
    f: F,
) -> Result<Value, Signal> {
    let reg = TCP_REGISTRY.lock().unwrap();
    match reg.get(&handle) {
        Some(TcpResource::Listener(l)) => f(l),
        Some(_) => Err(Signal::Error(RuntimeError::new("TCP handle is not a listener"))),
        None => Err(Signal::Error(RuntimeError::new("invalid TCP handle"))),
    }
}

/// Execute a built-in function by name with the given arguments.
/// The `heap` parameter is used for builtins that need to allocate GC-managed values.
pub fn call_builtin(name: &str, args: &[Value], heap: &mut coco_gc::Heap) -> Result<Value, Signal> {
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

        // ---- Filesystem builtins ----
        "fs_readFile" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new("fs_readFile() expects 1 argument (path)")));
            }
            let path = match &args[0] { Value::String(s) => s.clone(), _ => return Err(Signal::Error(RuntimeError::new("fs_readFile() expects a string path"))) };
            match std::fs::read_to_string(&path) {
                Ok(content) => Ok(Value::String(content)),
                Err(e) => Err(Signal::Error(RuntimeError::new(format!("fs_readFile: {}", e)))),
            }
        }
        "fs_writeFile" => {
            if args.len() != 2 {
                return Err(Signal::Error(RuntimeError::new("fs_writeFile() expects 2 arguments (path, content)")));
            }
            let path = match &args[0] { Value::String(s) => s.clone(), _ => return Err(Signal::Error(RuntimeError::new("fs_writeFile() expects a string path"))) };
            let content = match &args[1] { Value::String(s) => s.clone(), _ => format!("{}", args[1]) };
            match std::fs::write(&path, &content) {
                Ok(_) => Ok(Value::Null),
                Err(e) => Err(Signal::Error(RuntimeError::new(format!("fs_writeFile: {}", e)))),
            }
        }
        "fs_exists" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new("fs_exists() expects 1 argument (path)")));
            }
            let path = match &args[0] { Value::String(s) => s.clone(), _ => return Err(Signal::Error(RuntimeError::new("fs_exists() expects a string path"))) };
            Ok(Value::Bool(std::path::Path::new(&path).exists()))
        }
        "fs_readDir" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new("fs_readDir() expects 1 argument (path)")));
            }
            let path = match &args[0] { Value::String(s) => s.clone(), _ => return Err(Signal::Error(RuntimeError::new("fs_readDir() expects a string path"))) };
            match std::fs::read_dir(&path) {
                Ok(entries) => {
                    let mut names = Vec::new();
                    for entry in entries.flatten() {
                        names.push(Value::String(entry.file_name().to_string_lossy().to_string()));
                    }
                    let cow = coco_gc::CoW::new(names);
                    let (id, ptr) = heap.allocate(cow);
                    Ok(Value::List(coco_gc::Gc::new(heap, id, ptr)))
                }
                Err(e) => Err(Signal::Error(RuntimeError::new(format!("fs_readDir: {}", e)))),
            }
        }
        "fs_mkdir" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new("fs_mkdir() expects 1 argument (path)")));
            }
            let path = match &args[0] { Value::String(s) => s.clone(), _ => return Err(Signal::Error(RuntimeError::new("fs_mkdir() expects a string path"))) };
            match std::fs::create_dir_all(&path) {
                Ok(_) => Ok(Value::Null),
                Err(e) => Err(Signal::Error(RuntimeError::new(format!("fs_mkdir: {}", e)))),
            }
        }
        "fs_remove" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new("fs_remove() expects 1 argument (path)")));
            }
            let path = match &args[0] { Value::String(s) => s.clone(), _ => return Err(Signal::Error(RuntimeError::new("fs_remove() expects a string path"))) };
            let p = std::path::Path::new(&path);
            let result = if p.is_dir() { std::fs::remove_dir_all(p) } else { std::fs::remove_file(p) };
            match result {
                Ok(_) => Ok(Value::Null),
                Err(e) => Err(Signal::Error(RuntimeError::new(format!("fs_remove: {}", e)))),
            }
        }
        "fs_stat" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new("fs_stat() expects 1 argument (path)")));
            }
            let path = match &args[0] { Value::String(s) => s.clone(), _ => return Err(Signal::Error(RuntimeError::new("fs_stat() expects a string path"))) };
            match std::fs::metadata(&path) {
                Ok(meta) => {
                    let mut map = HashMap::new();
                    map.insert("exists".to_string(), Value::Bool(true));
                    map.insert("isFile".to_string(), Value::Bool(meta.is_file()));
                    map.insert("isDir".to_string(), Value::Bool(meta.is_dir()));
                    map.insert("size".to_string(), Value::Int(BigInt::from(meta.len())));
                     { let cow = coco_gc::CoW::new(map); let (id, ptr) = heap.allocate(cow); Ok(Value::Map(coco_gc::Gc::new(heap, id, ptr))) }
                }
                Err(_) => {
                    let mut map = HashMap::new();
                    map.insert("exists".to_string(), Value::Bool(false));
                     { let cow = coco_gc::CoW::new(map); let (id, ptr) = heap.allocate(cow); Ok(Value::Map(coco_gc::Gc::new(heap, id, ptr))) }
                }
            }
        }

        // ---- TCP/HTTP builtins ----
        "tcp_listen" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new("tcp_listen() expects 1 argument (port)")));
            }
            let port = match &args[0] { Value::Int(n) => n.to_u16().unwrap_or(0), _ => return Err(Signal::Error(RuntimeError::new("tcp_listen() expects an integer port"))) };
            match TcpListener::bind(format!("0.0.0.0:{}", port)) {
                Ok(listener) => {
                    let handle = alloc_tcp_handle();
                    register_tcp(handle, TcpResource::Listener(listener));
                    Ok(Value::Int(BigInt::from(handle)))
                }
                Err(e) => Err(Signal::Error(RuntimeError::new(format!("tcp_listen: {}", e)))),
            }
        }
        "tcp_accept" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new("tcp_accept() expects 1 argument (server_handle)")));
            }
            let handle = match &args[0] { Value::Int(n) => n.to_usize().unwrap_or(0), _ => return Err(Signal::Error(RuntimeError::new("tcp_accept() expects an integer handle"))) };
            // Accept under the listener lock
            let (stream, addr) = {
                let reg = TCP_REGISTRY.lock().unwrap();
                match reg.get(&handle) {
                    Some(TcpResource::Listener(l)) => {
                        match l.accept() {
                            Ok((s, a)) => (s, a),
                            Err(e) => return Err(Signal::Error(RuntimeError::new(format!("tcp_accept: {}", e)))),
                        }
                    }
                    Some(_) => return Err(Signal::Error(RuntimeError::new("handle is not a TCP listener"))),
                    None => return Err(Signal::Error(RuntimeError::new("invalid TCP handle"))),
                }
            };
            let client_handle = alloc_tcp_handle();
            register_tcp(client_handle, TcpResource::Stream(stream));
            let mut map = HashMap::new();
            map.insert("handle".to_string(), Value::Int(BigInt::from(client_handle)));
            map.insert("address".to_string(), Value::String(addr.to_string()));
             { let cow = coco_gc::CoW::new(map); let (id, ptr) = heap.allocate(cow); Ok(Value::Map(coco_gc::Gc::new(heap, id, ptr))) }
        }
        "tcp_read" => {
            if args.len() != 2 {
                return Err(Signal::Error(RuntimeError::new("tcp_read() expects 2 arguments (handle, max_bytes)")));
            }
            let handle = match &args[0] { Value::Int(n) => n.to_usize().unwrap_or(0), _ => return Err(Signal::Error(RuntimeError::new("tcp_read() expects an integer handle"))) };
            let max_bytes = match &args[1] { Value::Int(n) => n.to_usize().unwrap_or(1024), _ => return Err(Signal::Error(RuntimeError::new("tcp_read() expects an integer max_bytes"))) };
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
                return Err(Signal::Error(RuntimeError::new("tcp_write() expects 2 arguments (handle, data)")));
            }
            let handle = match &args[0] { Value::Int(n) => n.to_usize().unwrap_or(0), _ => return Err(Signal::Error(RuntimeError::new("tcp_write() expects an integer handle"))) };
            let data = match &args[1] { Value::String(s) => s.clone(), _ => format!("{}", args[1]) };
            with_tcp_stream(handle, |s| {
                match s.write_all(data.as_bytes()) {
                    Ok(_) => Ok(Value::Null),
                    Err(e) => Err(Signal::Error(RuntimeError::new(format!("tcp_write: {}", e)))),
                }
            })
        }
        "tcp_close" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new("tcp_close() expects 1 argument (handle)")));
            }
            let handle = match &args[0] { Value::Int(n) => n.to_usize().unwrap_or(0), _ => return Err(Signal::Error(RuntimeError::new("tcp_close() expects an integer handle"))) };
            // Just remove from registry — the Drop impl handles closing.
            take_tcp(handle);
            Ok(Value::Null)
        }
        "tcp_connect" => {
            if args.len() != 2 {
                return Err(Signal::Error(RuntimeError::new("tcp_connect() expects 2 arguments (host, port)")));
            }
            let host = match &args[0] { Value::String(s) => s.clone(), _ => return Err(Signal::Error(RuntimeError::new("tcp_connect() expects a string host"))) };
            let port = match &args[1] { Value::Int(n) => n.to_u16().unwrap_or(0), _ => return Err(Signal::Error(RuntimeError::new("tcp_connect() expects an integer port"))) };
            match TcpStream::connect(format!("{}:{}", host, port)) {
                Ok(stream) => {
                    let handle = alloc_tcp_handle();
                    register_tcp(handle, TcpResource::Stream(stream));
                    Ok(Value::Int(BigInt::from(handle)))
                }
                Err(e) => Err(Signal::Error(RuntimeError::new(format!("tcp_connect: {}", e)))),
            }
        }

        // ---- JSON builtins ----
        "json_parse" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new("json_parse() expects 1 argument (string)")));
            }
            let json_str = match &args[0] { Value::String(s) => s.clone(), _ => return Err(Signal::Error(RuntimeError::new("json_parse() expects a string"))) };
            Ok(json_to_coco(&json_str, heap))
        }
        "json_stringify" => {
            if args.len() != 1 {
                return Err(Signal::Error(RuntimeError::new("json_stringify() expects 1 argument (value)")));
            }
            Ok(Value::String(coco_to_json_string(&args[0])))
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

// ============================================================================
// JSON conversion helpers
// ============================================================================

/// Parse a JSON string into a Coco Value using a simple recursive-descent parser.
fn json_to_coco(json: &str, heap: &mut coco_gc::Heap) -> Value {
    let trimmed = json.trim();
    if trimmed.is_empty() {
        return Value::Null;
    }
    let bytes = trimmed.as_bytes();
    match bytes[0] {
        b'{' => json_parse_object(trimmed, heap),
        b'[' => json_parse_array(trimmed, heap),
        b'"' => json_parse_string(trimmed),
        b't' | b'f' => json_parse_bool(trimmed),
        b'n' => Value::Null,
        _ => json_parse_number(trimmed),
    }
}

fn json_parse_object(s: &str, heap: &mut coco_gc::Heap) -> Value {
    let mut map = HashMap::new();
    let inner = &s[1..s.len() - 1];
    if !inner.trim().is_empty() {
        let pairs = json_split_top_level(inner, b',');
        for pair in pairs {
            let colon = json_find_outside_string(pair, b':');
            if let Some(pos) = colon {
                let key = json_to_value_string(&pair[..pos].trim().trim_matches('"'));
                let val = json_to_coco(&pair[pos + 1..], heap);
                map.insert(key, val);
            }
        }
    }
    let cow = coco_gc::CoW::new(map);
    let (id, ptr) = heap.allocate(cow);
    Value::Map(coco_gc::Gc::new(heap, id, ptr))
}

fn json_parse_array(s: &str, heap: &mut coco_gc::Heap) -> Value {
    let inner = &s[1..s.len() - 1];
    if inner.trim().is_empty() {
        let cow = coco_gc::CoW::new(Vec::new());
        let (id, ptr) = heap.allocate(cow);
        return Value::List(coco_gc::Gc::new(heap, id, ptr));
    }
    let parts = json_split_top_level(inner, b',');
    let mut items = Vec::with_capacity(parts.len());
    for part in parts {
        items.push(json_to_coco(part, heap));
    }
    let cow = coco_gc::CoW::new(items);
    let (id, ptr) = heap.allocate(cow);
    Value::List(coco_gc::Gc::new(heap, id, ptr))
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
                b'"' => { result.push('"'); i += 2; }
                b'\\' => { result.push('\\'); i += 2; }
                b'/' => { result.push('/'); i += 2; }
                b'n' => { result.push('\n'); i += 2; }
                b't' => { result.push('\t'); i += 2; }
                b'r' => { result.push('\r'); i += 2; }
                b'u' => {
                    // Unicode escape \uXXXX — parse hex
                    if i + 6 <= len {
                        let hex = std::str::from_utf8(&bytes[i+2..i+6]).unwrap_or("");
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
                _ => { result.push(bytes[i+1] as char); i += 2; }
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
        Value::Int(BigInt::from(n))
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
            if f.is_nan() { "null".to_string() }
            else if f.is_infinite() { "null".to_string() }
            else { f.to_string() }
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
                if i > 0 { out.push(','); }
                out.push_str(&coco_to_json_string(item));
            }
            out.push(']');
            out
        }
        Value::Map(map) => {
            let mut out = String::from("{");
            let mut first = true;
            for (k, v) in map.data.iter() {
                if !first { out.push(','); }
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
