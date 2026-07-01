//! Coco runtime — bytecode VM and supporting infrastructure.
//!
//! The bytecode VM (`vm` module) is the sole dev runtime.
//! LLVM native compilation is reserved for production builds via `coco build --native`.
//!
//! The tree-walking interpreter modules (`eval_expr`, `exec_stmt`, `exec_item`, `env`)
//! are deprecated and gated behind `#[cfg(feature = "tree-walker")]`.

pub mod builtins;
pub mod compiler;
pub mod db;
pub mod parallel;
#[cfg(feature = "tree-walker")]
#[deprecated(since = "0.2.0", note = "Tree-walking interpreter is deprecated; use the VM")]
pub mod env;
pub mod error;
#[cfg(feature = "tree-walker")]
#[deprecated(since = "0.2.0", note = "Tree-walking interpreter is deprecated; use the VM")]
pub mod eval_expr;
#[cfg(feature = "tree-walker")]
#[deprecated(since = "0.2.0", note = "Tree-walking interpreter is deprecated; use the VM")]
pub mod exec_item;
#[cfg(feature = "tree-walker")]
#[deprecated(since = "0.2.0", note = "Tree-walking interpreter is deprecated; use the VM")]
pub mod exec_stmt;
pub mod ir;
pub mod stack;
pub mod task;
pub mod value;
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
        "std/db" => Some(include_str!("stdlib/db.co")),
        _ => None,
    }
}

pub use error::RuntimeError;
pub use value::Value;

// ============================================================================
// Tree-walking interpreter (deprecated, gated behind "tree-walker" feature)
// ============================================================================

#[cfg(feature = "tree-walker")]
mod tree_walker_interp {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use coco_gc::{CoW, Gc, Heap};
    use coco_parser::Parser;
    use coco_syntax::Program;
    use crate::env::Environment;
    use crate::error::{ControlFlow, IResult, RuntimeError, Signal};
    use crate::stack::CallStack;
    use crate::value::Value;

    /// The Coco tree-walking interpreter (deprecated).
    #[deprecated(since = "0.2.0", note = "Use the bytecode VM instead")]
    pub struct Interpreter {
        pub(crate) env: Environment,
        pub(crate) heap: Heap,
        pub debug: bool,
        pub(crate) call_stack: CallStack,
        pub(crate) source_file: Option<PathBuf>,
    }

    impl Interpreter {
        pub fn new() -> Self {
            let mut env = Environment::new();
            let heap = Heap::new();
            env.define("print".to_string(), Value::BuiltinFn("print".to_string()), false);
            env.define("len".to_string(), Value::BuiltinFn("len".to_string()), false);
            env.define("toString".to_string(), Value::BuiltinFn("toString".to_string()), false);
            env.define("parseInt".to_string(), Value::BuiltinFn("parseInt".to_string()), false);
            env.define("parseFloat".to_string(), Value::BuiltinFn("parseFloat".to_string()), false);
            env.define("Ok".to_string(), Value::BuiltinFn("Ok".to_string()), false);
            env.define("Err".to_string(), Value::BuiltinFn("Err".to_string()), false);
            env.define("abs".to_string(), Value::BuiltinFn("abs".to_string()), false);
            env.define("min".to_string(), Value::BuiltinFn("min".to_string()), false);
            env.define("max".to_string(), Value::BuiltinFn("max".to_string()), false);
            env.define("floor".to_string(), Value::BuiltinFn("floor".to_string()), false);
            env.define("ceil".to_string(), Value::BuiltinFn("ceil".to_string()), false);
            env.define("round".to_string(), Value::BuiltinFn("round".to_string()), false);
            env.define("sqrt".to_string(), Value::BuiltinFn("sqrt".to_string()), false);
            env.define("pow".to_string(), Value::BuiltinFn("pow".to_string()), false);
            env.define("random".to_string(), Value::BuiltinFn("random".to_string()), false);
            env.define("typeOf".to_string(), Value::BuiltinFn("typeOf".to_string()), false);
            env.define("isOk".to_string(), Value::BuiltinFn("isOk".to_string()), false);
            env.define("isErr".to_string(), Value::BuiltinFn("isErr".to_string()), false);
            env.define("unwrap".to_string(), Value::BuiltinFn("unwrap".to_string()), false);
            Self { env, heap, debug: false, call_stack: CallStack::new(), source_file: None }
        }

        pub fn set_debug(&mut self, debug: bool) { self.debug = debug; if debug { self.heap.collect_interval = 1000; } }
        pub fn set_source_file(&mut self, path: PathBuf) { self.source_file = Some(path); }

        pub(crate) fn alloc_list(&mut self, items: Vec<Value>) -> Value {
            let cow = CoW::new(items);
            let (id, ptr) = self.heap.allocate(cow);
            Value::List(Gc::new(&self.heap, id, ptr))
        }
        pub(crate) fn alloc_map(&mut self, items: HashMap<String, Value>) -> Value {
            let cow = CoW::new(items);
            let (id, ptr) = self.heap.allocate(cow);
            Value::Map(Gc::new(&self.heap, id, ptr))
        }

        pub fn eval_source(&mut self, src: &str) -> Result<Value, RuntimeError> {
            let program = self.parse(src)?;
            self.exec_program(&program).map_err(|sig| match sig {
                Signal::Error(e) => e,
                Signal::Flow(ControlFlow::Return(val)) => RuntimeError::new(format!("unexpected return outside function: {}", val)),
                Signal::Flow(_) => RuntimeError::new("unexpected control flow outside loop"),
            })
        }

        pub fn run_main(&mut self, src: &str) -> Result<Value, RuntimeError> {
            let program = self.parse(src)?;
            for item in &program.items {
                let result = self.exec_item(item);
                if let Err(Signal::Error(e)) = result {
                    return Err(e.with_stack(&self.call_stack).with_file(self.source_file.clone().unwrap_or_default()));
                }
            }
            let main_fn = self.env.get("main").cloned();
            match main_fn {
                Some(v) => Err(RuntimeError::new(format!("'main' is not a callable FnObj: {}", v))
                    .with_stack(&self.call_stack)
                    .with_file(self.source_file.clone().unwrap_or_default())),
                None => Err(RuntimeError::new("no 'main' function defined")
                    .with_stack(&self.call_stack)
                    .with_file(self.source_file.clone().unwrap_or_default())),
            }
        }

        fn parse(&self, src: &str) -> Result<Program, RuntimeError> {
            let mut parser = Parser::new(src);
            let program = parser.parse_program();
            Ok(program)
        }

        fn exec_program(&mut self, program: &Program) -> IResult {
            let mut last = Value::Null;
            for item in &program.items { last = self.exec_item(item)?; }
            Ok(last)
        }
    }

    impl Default for Interpreter {
        fn default() -> Self { Self::new() }
    }
}

#[cfg(feature = "tree-walker")]
#[allow(deprecated)]
pub use tree_walker_interp::Interpreter;
