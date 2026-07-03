//! Coco runtime — bytecode VM and supporting infrastructure.
//!
//! The bytecode VM (`vm` module) is the sole execution model. Source is
//! parsed and compiled to a `Chunk`, then executed by the VM. Compiled
//! chunks can be serialized to a `.cb` (coco-build) artifact via
//! `serialize_chunk` and reloaded with `deserialize_chunk`, skipping the
//! parse/compile steps on subsequent runs.

pub mod builtins;
pub mod compiler;
#[cfg(feature = "db")]
pub mod db;
pub mod error;
#[cfg(feature = "async-io")]
pub mod io_loop;
pub mod ir;
pub mod parallel;
pub mod serialize;
pub mod stack;
pub mod task;
pub mod value;
pub mod verify;
pub mod vm;

// ============================================================================
// Embedded stdlib sources
// ============================================================================

/// Returns the source code for a stdlib module, or None if not found.
pub fn get_stdlib_source(module: &str) -> Option<&'static str> {
    match module {
        "std/fs" => Some(include_str!("stdlib/fs.co")),
        "std/json" => Some(include_str!("stdlib/json.co")),
        "std/http" => Some(include_str!("stdlib/http.co")),
        "std/string" => Some(include_str!("stdlib/string.co")),
        "std/process" => Some(include_str!("stdlib/process.co")),
        "std/time" => Some(include_str!("stdlib/time.co")),
        "std/encoding" => Some(include_str!("stdlib/encoding.co")),
        "std/path" => Some(include_str!("stdlib/path.co")),
        "std/math" => Some(include_str!("stdlib/math.co")),
        "std/io" => Some(include_str!("stdlib/io.co")),
        "std/regex" => Some(include_str!("stdlib/regex.co")),
        "std/net" => Some(include_str!("stdlib/net.co")),
        "std/collections" => Some(include_str!("stdlib/collections.co")),
        "std/url" => Some(include_str!("stdlib/url.co")),
        "std/testing" => Some(include_str!("stdlib/testing.co")),
        "std/crypto" => Some(include_str!("stdlib/crypto.co")),
        "std/log" => Some(include_str!("stdlib/log.co")),
        "std/random" => Some(include_str!("stdlib/random.co")),
        "std/csv" => Some(include_str!("stdlib/csv.co")),
        "std/cache" => Some(include_str!("stdlib/cache.co")),
        "std/context" => Some(include_str!("stdlib/context.co")),
        "std/xml" => Some(include_str!("stdlib/xml.co")),
        "std/yaml" => Some(include_str!("stdlib/yaml.co")),
        #[cfg(feature = "db")]
        "std/db" => Some(include_str!("stdlib/db.co")),
        _ => None,
    }
}

pub use error::RuntimeError;
pub use serialize::{deserialize_chunk, serialize_chunk};
pub use value::Value;
