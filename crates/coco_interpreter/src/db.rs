//! SQLite database builtins for `std/db`.
//!
//! Exposes `db_open`, `db_exec`, `db_query`, `db_close` as builtins. Open
//! connections are tracked in a global registry keyed by integer handle (the
//! same pattern as the TCP registry), so `.co` code passes around plain ints.
//! `db_query` builds proper `Value::List<Value::Map>` results via the heap.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use coco_gc::{CoW, Gc, Heap};
use num_traits::ToPrimitive;
use rusqlite::types::ValueRef;
use rusqlite::Connection;

use crate::error::{RuntimeError, Signal};
use crate::value::Value;

/// Global registry of open SQLite connections keyed by handle id.
static DB_REGISTRY: std::sync::LazyLock<Arc<Mutex<HashMap<usize, Connection>>>> =
    std::sync::LazyLock::new(|| Arc::new(Mutex::new(HashMap::new())));

static NEXT_DB_HANDLE: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(1);

fn alloc_db_handle() -> usize {
    NEXT_DB_HANDLE.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
}

/// Allocate a `Value::Map` on the heap from a HashMap.
fn alloc_map(heap: &mut Heap, map: HashMap<String, Value>) -> Value {
    let cow = CoW::new(map);
    let (id, ptr) = heap.allocate(cow);
    Value::Map(Gc::new(heap, id, ptr))
}

/// Allocate a `Value::List` on the heap from a Vec.
fn alloc_list(heap: &mut Heap, items: Vec<Value>) -> Value {
    let cow = CoW::new(items);
    let (id, ptr) = heap.allocate(cow);
    Value::List(Gc::new(heap, id, ptr))
}

/// Open (or create) a SQLite database file. Returns a handle int.
/// `:memory:` opens an in-memory database.
pub fn db_open(args: &[Value]) -> Result<Value, Signal> {
    if args.len() != 1 {
        return Err(Signal::Error(RuntimeError::new(
            "db_open(path) expects 1 argument",
        )));
    }
    let path = match &args[0] {
        Value::String(s) => s.clone(),
        _ => return Err(Signal::Error(RuntimeError::new("db_open expects a string path"))),
    };
    let conn = Connection::open(&path).map_err(|e| {
        Signal::Error(RuntimeError::new(format!("db_open failed: {}", e)))
    })?;
    let handle = alloc_db_handle();
    DB_REGISTRY.lock().unwrap().insert(handle, conn);
    Ok(Value::Int(num_bigint::BigInt::from(handle)))
}

/// Execute a statement that returns no rows (INSERT/UPDATE/DELETE/DDL).
/// Returns the number of rows affected.
pub fn db_exec(args: &[Value]) -> Result<Value, Signal> {
    if args.len() < 2 || args.len() > 3 {
        return Err(Signal::Error(RuntimeError::new(
            "db_exec(handle, sql, [params]) expects 2 or 3 arguments",
        )));
    }
    let handle = match &args[0] {
        Value::Int(n) => n.to_usize().unwrap_or(0),
        _ => return Err(Signal::Error(RuntimeError::new("db_exec: handle must be int"))),
    };
    let sql = match &args[1] {
        Value::String(s) => s.clone(),
        _ => return Err(Signal::Error(RuntimeError::new("db_exec: sql must be string"))),
    };
    let params = params_from_value(args.get(2));

    let mut reg = DB_REGISTRY.lock().unwrap();
    let conn = reg
        .get_mut(&handle)
        .ok_or_else(|| Signal::Error(RuntimeError::new("invalid db handle")))?;
    let affected = conn
        .execute(&sql, rusqlite::params_from_iter(params.iter()))
        .map_err(|e| Signal::Error(RuntimeError::new(format!("db_exec failed: {}", e))))?;
    Ok(Value::Int(num_bigint::BigInt::from(affected)))
}

/// Run a SELECT and return rows as a `Value::List` of `Value::Map`
/// (column name -> value). Needs the heap to allocate the result objects.
pub fn db_query(args: &[Value], heap: &mut Heap) -> Result<Value, Signal> {
    if args.len() < 2 || args.len() > 3 {
        return Err(Signal::Error(RuntimeError::new(
            "db_query(handle, sql, [params]) expects 2 or 3 arguments",
        )));
    }
    let handle = match &args[0] {
        Value::Int(n) => n.to_usize().unwrap_or(0),
        _ => return Err(Signal::Error(RuntimeError::new("db_query: handle must be int"))),
    };
    let sql = match &args[1] {
        Value::String(s) => s.clone(),
        _ => return Err(Signal::Error(RuntimeError::new("db_query: sql must be string"))),
    };
    let params = params_from_value(args.get(2));

    let reg = DB_REGISTRY.lock().unwrap();
    let conn = reg
        .get(&handle)
        .ok_or_else(|| Signal::Error(RuntimeError::new("invalid db handle")))?;
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| Signal::Error(RuntimeError::new(format!("db_query prepare: {}", e))))?;
    let col_count = stmt.column_count();
    let col_names: Vec<String> = (0..col_count)
        .map(|i| stmt.column_name(i).unwrap_or_default().to_string())
        .collect();

    // We must collect all rows before allocating (the borrow on `stmt` and
    // `conn` is released by query_map iteration, but we can't borrow `heap`
    // while holding the DB_REGISTRY lock's connection ref — collect first).
    let mut collected: Vec<HashMap<String, Value>> = Vec::new();
    let rows = stmt
        .query_map(rusqlite::params_from_iter(params.iter()), |row| {
            let mut map: HashMap<String, Value> = HashMap::new();
            for i in 0..col_count {
                let val = match row.get_ref(i) {
                    Ok(ValueRef::Null) => Value::Null,
                    Ok(ValueRef::Integer(n)) => Value::Int(num_bigint::BigInt::from(n)),
                    Ok(ValueRef::Real(f)) => Value::Float(f),
                    Ok(ValueRef::Text(bytes)) => {
                        Value::String(String::from_utf8_lossy(bytes).to_string())
                    }
                    Ok(ValueRef::Blob(bytes)) => {
                        Value::String(format!("<blob {} bytes>", bytes.len()))
                    }
                    Err(_) => Value::Null,
                };
                map.insert(col_names[i].clone(), val);
            }
            Ok(map)
        })
        .map_err(|e| Signal::Error(RuntimeError::new(format!("db_query: {}", e))))?;
    for row in rows {
        let map = row
            .map_err(|e| Signal::Error(RuntimeError::new(format!("db_query row: {}", e))))?;
        collected.push(map);
    }
    drop(stmt);
    drop(reg);

    // Now allocate the result list of maps on the heap.
    let mut row_values: Vec<Value> = Vec::with_capacity(collected.len());
    for map in collected {
        row_values.push(alloc_map(heap, map));
    }
    Ok(alloc_list(heap, row_values))
}

/// Close a database connection.
pub fn db_close(args: &[Value]) -> Result<Value, Signal> {
    if args.len() != 1 {
        return Err(Signal::Error(RuntimeError::new(
            "db_close(handle) expects 1 argument",
        )));
    }
    let handle = match &args[0] {
        Value::Int(n) => n.to_usize().unwrap_or(0),
        _ => return Err(Signal::Error(RuntimeError::new("db_close: handle must be int"))),
    };
    let mut reg = DB_REGISTRY.lock().unwrap();
    reg.remove(&handle);
    Ok(Value::Null)
}

// --- helpers ---

fn params_from_value(arg: Option<&Value>) -> Vec<rusqlite::types::Value> {
    let mut out = Vec::new();
    if let Some(Value::List(list)) = arg {
        for v in &list.data {
            out.push(value_to_sql(v));
        }
    }
    out
}

fn value_to_sql(v: &Value) -> rusqlite::types::Value {
    use num_traits::ToPrimitive;
    match v {
        Value::Int(n) => rusqlite::types::Value::Integer(n.to_i64().unwrap_or(0)),
        Value::Float(f) => rusqlite::types::Value::Real(*f),
        Value::String(s) => rusqlite::types::Value::Text(s.clone()),
        Value::Bool(b) => rusqlite::types::Value::Integer(if *b { 1 } else { 0 }),
        Value::Null => rusqlite::types::Value::Null,
        _ => rusqlite::types::Value::Text(format!("{}", v)),
    }
}
