//! Tree-walking interpreter for the Coco programming language.
//!
//! Executes a Coco program by walking the AST produced by `coco_parser`.
//! Supports expressions, statements, functions, closures, and control flow.

pub mod builtins;
pub mod env;
pub mod error;
pub mod eval_expr;
pub mod exec_item;
pub mod exec_stmt;
pub mod value;

pub use error::RuntimeError;
pub use value::Value;

use std::collections::HashMap;

use coco_gc::{CoW, Gc, Heap};
use coco_parser::Parser;
use coco_syntax::Program;
use env::Environment;
use error::{ControlFlow, IResult, Signal};

/// The Coco tree-walking interpreter.
pub struct Interpreter {
    pub(crate) env: Environment,
    pub(crate) heap: Heap,
    pub debug: bool,
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

        Self {
            env,
            heap,
            debug: false,
        }
    }

    /// Set debug mode. In debug mode, GC stats are printed on drop.
    pub fn set_debug(&mut self, debug: bool) {
        self.debug = debug;
        if debug {
            self.heap.collect_interval = 1000;
        }
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
                return Err(e);
            }
        }

        // Look up and call main()
        let main_fn = self.env.get("main").cloned();
        match main_fn {
            Some(Value::Function(func)) => {
                self.call_function(&func, vec![]).map_err(|sig| match sig {
                    Signal::Error(e) => e,
                    Signal::Flow(ControlFlow::Return(val)) => {
                        // This shouldn't happen since call_function catches returns
                        RuntimeError::new(format!("unexpected: {}", val))
                    }
                    Signal::Flow(_) => RuntimeError::new("unexpected control flow in main"),
                })
            }
            Some(_) => Err(RuntimeError::new("'main' is not a function")),
            None => Err(RuntimeError::new("no 'main' function defined")),
        }
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
