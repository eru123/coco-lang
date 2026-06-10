//! Tree-walking interpreter for the Coco programming language.
//!
//! Executes a Coco program by walking the AST produced by `coco_parser`.
//! Supports expressions, statements, functions, closures, and control flow.

pub mod builtins;
pub mod compiler;
pub mod env;
pub mod error;
pub mod eval_expr;
pub mod exec_item;
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
        _ => None,
    }
}

pub use error::RuntimeError;
pub use value::Value;

use std::collections::HashMap;
use std::path::PathBuf;

use coco_gc::{CoW, Gc, Heap};
use coco_parser::Parser;
use coco_syntax::Program;
use env::Environment;
use error::{ControlFlow, IResult, Signal};
use stack::CallStack;

/// The Coco tree-walking interpreter.
pub struct Interpreter {
    pub(crate) env: Environment,
    pub(crate) heap: Heap,
    pub debug: bool,
    pub(crate) call_stack: CallStack,
    pub(crate) source_file: Option<PathBuf>,
}

impl Interpreter {
    /// Create a new interpreter with built-in functions registered.
    pub fn new() -> Self {
        let mut env = Environment::new();
        let heap = Heap::new();

        // Register built-in functions
        env.define(
            "print".to_string(),
            Value::BuiltinFn("print".to_string()),
            false,
        );
        env.define(
            "len".to_string(),
            Value::BuiltinFn("len".to_string()),
            false,
        );
        env.define(
            "toString".to_string(),
            Value::BuiltinFn("toString".to_string()),
            false,
        );
        env.define(
            "parseInt".to_string(),
            Value::BuiltinFn("parseInt".to_string()),
            false,
        );
        env.define(
            "parseFloat".to_string(),
            Value::BuiltinFn("parseFloat".to_string()),
            false,
        );
        // Result constructors
        env.define(
            "Ok".to_string(),
            Value::BuiltinFn("Ok".to_string()),
            false,
        );
        env.define(
            "Err".to_string(),
            Value::BuiltinFn("Err".to_string()),
            false,
        );
        // Math
        env.define(
            "abs".to_string(),
            Value::BuiltinFn("abs".to_string()),
            false,
        );
        env.define(
            "min".to_string(),
            Value::BuiltinFn("min".to_string()),
            false,
        );
        env.define(
            "max".to_string(),
            Value::BuiltinFn("max".to_string()),
            false,
        );
        env.define(
            "floor".to_string(),
            Value::BuiltinFn("floor".to_string()),
            false,
        );
        env.define(
            "ceil".to_string(),
            Value::BuiltinFn("ceil".to_string()),
            false,
        );
        env.define(
            "round".to_string(),
            Value::BuiltinFn("round".to_string()),
            false,
        );
        env.define(
            "sqrt".to_string(),
            Value::BuiltinFn("sqrt".to_string()),
            false,
        );
        env.define(
            "pow".to_string(),
            Value::BuiltinFn("pow".to_string()),
            false,
        );
        env.define(
            "random".to_string(),
            Value::BuiltinFn("random".to_string()),
            false,
        );
        // Type checking
        env.define(
            "typeOf".to_string(),
            Value::BuiltinFn("typeOf".to_string()),
            false,
        );
        env.define(
            "isOk".to_string(),
            Value::BuiltinFn("isOk".to_string()),
            false,
        );
        env.define(
            "isErr".to_string(),
            Value::BuiltinFn("isErr".to_string()),
            false,
        );
        env.define(
            "unwrap".to_string(),
            Value::BuiltinFn("unwrap".to_string()),
            false,
        );

        Self {
            env,
            heap,
            debug: false,
            call_stack: CallStack::new(),
            source_file: None,
        }
    }

    /// Set debug mode. In debug mode, GC stats are printed on drop.
    pub fn set_debug(&mut self, debug: bool) {
        self.debug = debug;
        if debug {
            self.heap.collect_interval = 1000;
        }
    }

    /// Set the source file path for error reporting.
    pub fn set_source_file(&mut self, path: PathBuf) {
        self.source_file = Some(path);
    }

    /// Allocate a CoW list on the heap.
    pub(crate) fn alloc_list(&mut self, items: Vec<Value>) -> Value {
        let cow = CoW::new(items);
        let (id, ptr) = self.heap.allocate(cow);
        Value::List(Gc::new(&self.heap, id, ptr))
    }

    /// Allocate a CoW map on the heap.
    pub(crate) fn alloc_map(&mut self, items: HashMap<String, Value>) -> Value {
        let cow = CoW::new(items);
        let (id, ptr) = self.heap.allocate(cow);
        Value::Map(Gc::new(&self.heap, id, ptr))
    }

    /// Parse source code and execute it. Returns the value of the last expression.
    pub fn eval_source(&mut self, src: &str) -> Result<Value, RuntimeError> {
        let program = self.parse(src)?;
        self.exec_program(&program).map_err(|sig| match sig {
            Signal::Error(e) => e,
            Signal::Flow(ControlFlow::Return(val)) => {
                RuntimeError::new(format!("unexpected return outside function: {}", val))
            }
            Signal::Flow(_) => RuntimeError::new("unexpected control flow outside loop"),
        })
    }

    /// Parse source, register all top-level items, then call main().
    pub fn run_main(&mut self, src: &str) -> Result<Value, RuntimeError> {
        let program = self.parse(src)?;

        // First pass: register all items (especially functions)
        for item in &program.items {
            let result = self.exec_item(item);
            if let Err(Signal::Error(e)) = result {
                return Err(e
                    .with_stack(&self.call_stack)
                    .with_file(self.source_file.clone().unwrap_or_default()));
            }
        }

        // Look up and call main()
        let main_fn = self.env.get("main").cloned();
        match main_fn {
            Some(Value::Function(func)) => {
                self.call_function(&func, vec![]).map_err(|sig| match sig {
                    Signal::Error(e) => e
                        .with_stack(&self.call_stack)
                        .with_file(self.source_file.clone().unwrap_or_default()),
                    Signal::Flow(ControlFlow::Return(val)) => {
                        RuntimeError::new(format!("unexpected: {}", val))
                            .with_stack(&self.call_stack)
                            .with_file(self.source_file.clone().unwrap_or_default())
                    }
                    Signal::Flow(_) => RuntimeError::new("unexpected control flow in main")
                        .with_stack(&self.call_stack)
                        .with_file(self.source_file.clone().unwrap_or_default()),
                })
            }
            Some(_) => Err(RuntimeError::new("'main' is not a function")
                .with_stack(&self.call_stack)
                .with_file(self.source_file.clone().unwrap_or_default())),
            None => Err(RuntimeError::new("no 'main' function defined")
                .with_stack(&self.call_stack)
                .with_file(self.source_file.clone().unwrap_or_default())),
        }.map(|val| {
            // Clean up call stack on success
            self.call_stack = CallStack::new();
            val
        })
    }

    fn parse(&self, src: &str) -> Result<Program, RuntimeError> {
        let mut parser = Parser::new(src);
        let program = parser.parse_program();
        // We allow parse diagnostics (warning level) but still execute
        Ok(program)
    }

    fn exec_program(&mut self, program: &Program) -> IResult {
        let mut last = Value::Null;
        for item in &program.items {
            last = self.exec_item(item)?;
        }
        Ok(last)
    }
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}
