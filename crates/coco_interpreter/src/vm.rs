//! Stack-based bytecode VM — executes compiled Chunks.
//!
//! ## Architecture
//!
//! The VM maintains:
//! - A value stack (`Vec<Value>`) for operands and intermediate results
//! - A call frame stack for function calls
//! - A global store (`HashMap<String, Value>`) for top-level bindings
//! - A GC `Heap` for CoW collections
//! - A handler stack for try/catch
//!
//! Execution is a single `match`-based dispatch loop over bytecode opcodes.

use std::collections::HashMap;
// use std::sync::Arc;

use coco_gc::{CoW, GcRef, Heap};
use num_bigint::BigInt;

use crate::builtins::call_builtin;
use crate::ir::{read_i16, read_u16, Chunk, FnObj, *};
use crate::task::{TaskId, TaskScheduler};
use crate::value::Value;

// ============================================================================
// VM error
// ============================================================================

/// Error produced by the VM at runtime.
#[derive(Debug, Clone)]
pub struct VmError {
    pub message: String,
}

impl VmError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
        }
    }
}

impl std::fmt::Display for VmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "VmError: {}", self.message)
    }
}

/// VM result type.
pub type VmResult<T> = Result<T, VmError>;

// ============================================================================
// Call frame
// ============================================================================

/// A function invocation frame.
struct CallFrame {
    /// The function being executed.
    closure: Value,
    /// Instruction pointer — index into `chunk.code`.
    ip: usize,
    /// Base index in the VM's value stack where this frame's locals start.
    stack_offset: usize,
}

// ============================================================================
// VM
// ============================================================================

/// The Coco bytecode virtual machine.
pub struct Vm {
    /// Operand stack.
    stack: Vec<Value>,
    /// Call frames (innermost last).
    frames: Vec<CallFrame>,
    /// Global variable bindings.
    globals: HashMap<String, Value>,
    /// GC heap.
    heap: Heap,
    /// Async task scheduler for cooperative multitasking.
    scheduler: TaskScheduler,
    /// Exception handler stack: (handler_ip, stack_depth).
    handlers: Vec<(usize, usize)>,
    /// Debug mode.
    pub debug: bool,
    /// Set by async opcodes to signal task should yield to scheduler.
    yield_flag: bool,
    /// The currently executing task id (set by run_task).
    current_task: TaskId,
    /// Stack of `$` (this) bindings for OOP method calls.
    this_stack: Vec<Value>,
}

impl Vm {
    /// Create a new VM.
    pub fn new() -> Self {
        Self {
            stack: Vec::new(),
            frames: Vec::new(),
            globals: HashMap::new(),
            heap: Heap::new(),
            scheduler: TaskScheduler::new(),
            handlers: Vec::new(),
            debug: false,
            yield_flag: false,
            current_task: 0,
            this_stack: Vec::new(),
        }
    }

    /// Set debug mode.
    pub fn set_debug(&mut self, debug: bool) {
        self.debug = debug;
    }

    /// Register a built-in function as a global.
    pub fn register_builtin(&mut self, name: &str, builtin_name: &str) {
        self.globals.insert(
            name.to_string(),
            Value::BuiltinFn(builtin_name.to_string()),
        );
    }

    /// Replace this VM's globals (used to seed a worker VM for parallel runs).
    pub fn set_globals(&mut self, globals: std::collections::HashMap<String, Value>) {
        self.globals = globals;
    }

    /// Call a `FnObj` with the given args and run it to completion.
    /// Used by the parallel runtime to execute a `run` clause on a fresh VM.
    pub fn call_function(&mut self, fn_obj: crate::ir::FnObj, args: Vec<Value>) -> VmResult<Value> {
        if args.len() != fn_obj.arity {
            return Err(VmError::new(format!(
                "{}() expects {} arguments, got {}",
                fn_obj.name, fn_obj.arity, args.len()
            )));
        }
        // Build a stack: [FnObj, arg0, arg1, ...] and a frame, then run.
        let mut stack: Vec<Value> = Vec::with_capacity(args.len() + 1);
        stack.push(Value::FnObj(fn_obj.clone()));
        for a in args {
            stack.push(a);
        }
        self.stack = stack;
        self.frames.clear();
        self.frames.push(CallFrame {
            closure: Value::FnObj(fn_obj),
            ip: 0,
            // Locals (params) start after the closure at stack[0], so the
            // first param (slot 0) maps to stack[1]. This matches the calling
            // convention used by `call`/`async_call`.
            stack_offset: 1,
        });
        self.run_loop()
    }

    // ========================================================================
    // Public API
    // ========================================================================

    /// Execute a compiled Chunk as a top-level script and return the result.
    ///
    /// First runs the script chunk synchronously to register all globals
    /// (functions, variables). Then looks up `main()` and runs it through
    /// the async task scheduler. Async calls and awaits in main() are
    /// cooperatively scheduled.
    pub fn run(&mut self, chunk: &Chunk) -> Result<Value, VmError> {
        self.register_builtins();

        // Phase 1: run the script chunk synchronously to register globals.
        let _ = self._execute_chunk_sync(chunk)?;

        // Phase 2: look up main and run it through the scheduler.
        let main_fn = self.globals.get("main").cloned();
        match main_fn {
            Some(Value::FnObj(fn_obj)) => {
                // Spawn main as root task.
                let stack = vec![Value::FnObj(fn_obj.clone())];
                let root_id = self.scheduler.spawn(
                    Value::FnObj(fn_obj),
                    0,
                    1, // locals start after the closure
                    stack,
                );
                self.scheduler.set_root(root_id);

                // Scheduler loop.
                loop {
                    if let Some(result) = self.scheduler.root_result() {
                        return result;
                    }
                    if let Some(err) = self.scheduler.root_failed() {
                        return Err(VmError::new(err));
                    }
                    let task_id = match self.scheduler.dequeue() {
                        Some(id) => id,
                        None => {
                            return Err(VmError::new("scheduler deadlock: no ready tasks"));
                        }
                    };
                    self.run_task(task_id)?;
                }
            }
            Some(_) => Err(VmError::new("'main' is not a function")),
            None => {
                // No main function — return top of stack (script result).
                Ok(self.stack.pop().unwrap_or(Value::Null))
            }
        }
    }

    /// Run a single task until it yields or completes.
    fn run_task(&mut self, task_id: TaskId) -> Result<(), VmError> {
        // Restore task state.
        {
            let task = self
                .scheduler
                .get(task_id)
                .ok_or_else(|| VmError::new(format!("task {} not found", task_id)))?;
            self.frames.clear();
            self.frames.push(CallFrame {
                closure: task.frame.closure.clone(),
                ip: task.frame.ip,
                stack_offset: task.frame.stack_offset,
            });
            self.stack = task.stack.clone();
            self.yield_flag = false;
            self.current_task = task_id;

            // If this task was awaiting another, replace the TaskHandle on the
            // stack with the result value.
            if let Some(awaited_id) = task.awaited_task {
                if let Some(awaited) = self.scheduler.get(awaited_id) {
                    match &awaited.state {
                        crate::task::TaskState::Completed(val) => {
                            // Find and replace the TaskHandle with the result
                            if let Some(pos) = self.stack.iter().rposition(|v| {
                                matches!(v, Value::TaskHandle(id) if *id == awaited_id)
                            }) {
                                self.stack[pos] = val.clone();
                            }
                        }
                        crate::task::TaskState::Failed(err) => {
                            return Err(VmError::new(err.clone()));
                        }
                        _ => {}
                    }
                }
            }
        }

        // Run until frames exhausted or yield flag set.
        let result = self.run_loop();

        if self.yield_flag {
            // Task suspended — save state.
            let frame = self.frames.last().expect("frame on yield");
            self.scheduler.save_suspended_state(
                task_id,
                frame.closure.clone(),
                frame.ip,
                frame.stack_offset,
                self.stack.clone(),
            );
            return Ok(());
        }

        // Task completed — mark as complete.
        match result {
            Ok(val) => self.scheduler.complete(task_id, val),
            Err(e) => self.scheduler.fail(task_id, e.message.clone()),
        }
        Ok(())
    }

    /// Execute a chunk directly (for internal / legacy use).
    fn _execute_chunk_sync(&mut self, chunk: &Chunk) -> Result<Value, VmError> {
        let script_fn = Value::FnObj(FnObj {
            name: "<script>".to_string(),
            arity: 0,
            chunk: chunk.clone(),
            is_async: false,
        });
        self.push(script_fn.clone());
        self.frames.push(CallFrame {
            closure: script_fn,
            ip: 0,
            stack_offset: 0,
        });
        self.run_loop()
    }

    // ========================================================================
    // Main execution loop
    // ========================================================================

    fn run_loop(&mut self) -> Result<Value, VmError> {
        loop {
            // If we have no frames, we're done.
            if self.frames.is_empty() {
                let val = self.stack.pop().unwrap_or(Value::Null);
                return Ok(val);
            }

            // Read the next opcode (IP is at the opcode, not advanced).
            let op = {
                let frame = self.frames.last().expect("no frame");
                let chunk = self.chunk_of(&frame.closure).expect("no chunk");
                if frame.ip >= chunk.code.len() {
                    return Err(VmError::new("instruction pointer out of bounds"));
                }
                chunk.code[frame.ip]
            };

            // Debug trace.
            if self.debug {
                let frame = self.frames.last().unwrap();
                let chunk = self.chunk_of(&frame.closure).unwrap();
                let mut d = String::new();
                crate::ir::disassemble_instruction(chunk, frame.ip, &mut d);
                eprint!("[vm] {}    stack=[", d.trim());
                for (i, v) in self.stack.iter().enumerate() {
                    if i > 0 {
                        eprint!(", ");
                    }
                    eprint!("{}", v);
                }
                eprintln!("]");
            }

            // Dispatch (each arm is responsible for advancing IP).
            self.dispatch(op)?;
            // Yield check: async ops set yield_flag to suspend the task.
            if self.yield_flag {
                return Ok(Value::Null);
            }
        }
    }

    /// Dispatch a single opcode.
    fn dispatch(&mut self, op: u8) -> VmResult<()> {
        // After processing, non-jump instructions advance IP by this amount.
        // Jump/call/return instructions set IP directly and skip the advance.
        let step = 1 + operand_bytes(op).unwrap_or(0);

        match op {
            // ---- Constants ----
            OP_CONST => {
                let idx = self.read_u16_operand() as usize;
                let val = self.constants()[idx].clone();
                self.push(val);
            }
            OP_NULL => self.push(Value::Null),
            OP_TRUE => self.push(Value::Bool(true)),
            OP_FALSE => self.push(Value::Bool(false)),

            // ---- Async operations ----
            OP_ASYNC_CALL => {
                let arg_count = self.read_u8_operand() as usize;
                self.step_ip(step);
                let task_id = self.async_call(arg_count)?;
                self.push(Value::TaskHandle(task_id));
                return Ok(());
            }
            OP_AWAIT => {
                let handle = self.pop();
                if let Value::TaskHandle(target_id) = handle {
                    let done = self.scheduler.get(target_id).map_or(false, |t| {
                        matches!(
                            t.state,
                            crate::task::TaskState::Completed(_)
                                | crate::task::TaskState::Failed(_)
                        )
                    });
                    if done {
                        let val = self.scheduler.get(target_id).and_then(|t| match &t.state {
                            crate::task::TaskState::Completed(v) => Some(v.clone()),
                            crate::task::TaskState::Failed(err) => {
                                return Some(Value::String(err.clone()));
                            }
                            _ => None,
                        }).unwrap_or(Value::Null);
                        self.push(val);
                        self.step_ip(step);
                        return Ok(());
                    }
                    // Push the handle back — we'll replace it with the result on resume.
                    self.push(Value::TaskHandle(target_id));
                    self.scheduler.suspend_awaiting(self.current_task, target_id);
                    self.step_ip(step);
                    self.yield_flag = true;
                    return Ok(());
                }
                return Err(VmError::new("await requires a task handle"));
            }
            OP_PARALLEL_RUN => {
                // Pop N TaskHandles, extract their (FnObj, args) from the
                // scheduler, run them concurrently on OS threads, and push the
                // last result. Falls back to serial await if a handle isn't a
                // pending task (e.g. already completed or a non-task value).
                let n = self.read_u8_operand() as usize;
                self.step_ip(step);
                let mut handles: Vec<Value> = Vec::with_capacity(n);
                for _ in 0..n {
                    handles.push(self.pop());
                }
                handles.reverse();
                let mut runs: Vec<crate::parallel::ParallelRun> = Vec::with_capacity(n);
                for h in &handles {
                    if let Value::TaskHandle(id) = h {
                        if let Some(task) = self.scheduler.take_task(*id) {
                            if let Value::FnObj(fn_obj) = task.frame.closure.clone() {
                                // stack[0] is the FnObj; stack[1..] are args.
                                let args = task.stack.iter().skip(1).cloned().collect();
                                runs.push(crate::parallel::ParallelRun {
                                    callee: fn_obj,
                                    args,
                                });
                                continue;
                            }
                        }
                    }
                    // Fallback: not a runnable task — leave it to be awaited
                    // serially by pushing a null result slot.
                    runs.push(crate::parallel::ParallelRun {
                        callee: crate::ir::FnObj {
                            name: "<noop>".to_string(),
                            arity: 0,
                            chunk: crate::ir::Chunk::new(),
                            is_async: false,
                        },
                        args: vec![],
                    });
                }
                let result = crate::parallel::parallel_join(runs, &self.globals)?;
                self.push(result);
                return Ok(());
            }
            OP_LAZY_CALL => {
                let arg_count = self.read_u8_operand() as usize;
                self.step_ip(step);
                let task_id = self.async_call(arg_count)?;
                self.push(Value::TaskHandle(task_id));
                return Ok(());
            }

            // ---- Variables ----
            OP_LOAD_LOCAL => {
                let slot = self.read_u16_operand() as usize;
                let frame = self.current_frame()?;
                let idx = frame.stack_offset + slot;
                if idx >= self.stack.len() {
                    return Err(VmError::new(format!("local {} out of bounds", slot)));
                }
                let val = self.stack[idx].clone();
                self.push(val);
            }
            OP_STORE_LOCAL => {
                let slot = self.read_u16_operand() as usize;
                let frame = self.current_frame()?;
                let idx = frame.stack_offset + slot;
                let val = self.pop();
                while idx >= self.stack.len() {
                    self.stack.push(Value::Null);
                }
                self.stack[idx] = val;
            }
            OP_LOAD_GLOBAL => {
                let idx = self.read_u16_operand() as usize;
                let name = self.string_constant(idx)?;
                let val = self.globals.get(&name).cloned().unwrap_or(Value::Null);
                self.push(val);
            }
            OP_STORE_GLOBAL => {
                let idx = self.read_u16_operand() as usize;
                let name = self.string_constant(idx)?;
                let val = self.pop();
                self.globals.insert(name, val);
            }
            OP_DEFINE_GLOBAL => {
                let idx = self.read_u16_operand() as usize;
                let name = self.string_constant(idx)?;
                let val = self.pop();
                self.globals.insert(name, val);
            }

            // ---- Arithmetic ----
            OP_ADD => self.binop(|a, b| Self::vm_add(a, b))?,
            OP_SUB => self.binop(|a, b| Self::vm_sub(a, b))?,
            OP_MUL => self.binop(|a, b| Self::vm_mul(a, b))?,
            OP_DIV => self.binop(|a, b| Self::vm_div(a, b))?,
            OP_MOD => self.binop(|a, b| Self::vm_mod(a, b))?,
            OP_POW => self.binop(|a, b| Self::vm_pow(a, b))?,

            // ---- Comparison ----
            OP_EQ => self.binop(|a, b| Ok(Value::Bool(Self::vm_eq(&a, &b))))?,
            OP_NE => self.binop(|a, b| Ok(Value::Bool(!Self::vm_eq(&a, &b))))?,
            OP_LT => self.binop(|a, b| Self::vm_cmp(a, b, |o| o == std::cmp::Ordering::Less))?,
            OP_GT => self.binop(|a, b| Self::vm_cmp(a, b, |o| o == std::cmp::Ordering::Greater))?,
            OP_LE => self.binop(|a, b| Self::vm_cmp(a, b, |o| o != std::cmp::Ordering::Greater))?,
            OP_GE => self.binop(|a, b| Self::vm_cmp(a, b, |o| o != std::cmp::Ordering::Less))?,

            // ---- Bitwise ----
            OP_BIT_AND => self.binop(|a, b| Self::vm_bitop(a, b, |x, y| x & y))?,
            OP_BIT_OR => self.binop(|a, b| Self::vm_bitop(a, b, |x, y| x | y))?,
            OP_BIT_XOR => self.binop(|a, b| Self::vm_bitop(a, b, |x, y| x ^ y))?,
            OP_SHL => self.binop(|a, b| {
                use num_traits::ToPrimitive;
                Self::vm_bitop(a, b, |x, y| {
                    let shift = y.to_usize().unwrap_or(0);
                    x << shift
                })
            })?,
            OP_SHR => self.binop(|a, b| {
                use num_traits::ToPrimitive;
                Self::vm_bitop(a, b, |x, y| {
                    let shift = y.to_usize().unwrap_or(0);
                    x >> shift
                })
            })?,

            // ---- Unary ----
            OP_NEG => {
                let val = self.pop();
                self.push(Self::vm_neg(val)?);
            }
            OP_NOT => {
                let val = self.pop();
                self.push(Value::Bool(!val.is_truthy()));
            }
            OP_BIT_NOT => {
                let val = self.pop();
                match val {
                    Value::Int(n) => self.push(Value::Int(!n)),
                    _ => return Err(VmError::new("bitwise NOT requires integer")),
                }
            }

            // ---- Control flow ----
            OP_JUMP => {
                let offset = self.read_i16_operand();
                self.do_jump(offset);
                return Ok(());
            }
            OP_JUMP_IF_FALSE => {
                let offset = self.read_i16_operand();
                let cond = self.pop();
                if !cond.is_truthy() {
                    self.do_jump(offset);
                    return Ok(());
                }
                self.step_ip(step);
                return Ok(());
            }
            OP_JUMP_IF_TRUE => {
                let offset = self.read_i16_operand();
                let cond = self.pop();
                if cond.is_truthy() {
                    self.do_jump(offset);
                    return Ok(());
                }
                self.step_ip(step);
                return Ok(());
            }
            OP_POP_JUMP_IF_FALSE => {
                let offset = self.read_i16_operand();
                let cond = self.peek();
                if !cond.is_truthy() {
                    let _ = self.pop();
                    self.do_jump(offset);
                    return Ok(());
                }
                self.step_ip(step);
                return Ok(());
            }
            OP_LOOP => {
                let offset = self.read_i16_operand();
                // LOOP offset = end_of_loop - target (positive). Target = ip + 3 - offset.
                self.do_loop_back(offset);
                return Ok(());
            }

            // ---- Functions ----
            OP_CALL => {
                let arg_count = self.read_u8_operand() as usize;
                self.step_ip(step);
                self.call(arg_count)?;
                return Ok(());
            }
            OP_RETURN => {
                self.do_return()?;
                return Ok(());
            }
            OP_MAKE_CLOSURE => {
                let idx = self.read_u16_operand() as usize;
                let fn_val = self.constants()[idx].clone();
                self.push(fn_val);
            }

            // ---- Collections ----
            OP_BUILD_LIST => {
                let count = self.read_u16_operand() as usize;
                let start = self.stack.len() - count;
                let items: Vec<Value> = self.stack.drain(start..).collect();
                let val = self.alloc_list(items);
                self.push(val);
            }
            OP_BUILD_MAP => {
                let pair_count = self.read_u16_operand() as usize;
                let start = self.stack.len() - pair_count * 2;
                let mut map = HashMap::new();
                let drained: Vec<Value> = self.stack.drain(start..).collect();
                for chunk in drained.chunks(2) {
                    if let (Value::String(key), val) = (&chunk[0], &chunk[1]) {
                        map.insert(key.clone(), val.clone());
                    }
                }
                let val = self.alloc_map(map);
                self.push(val);
            }
            OP_INDEX => {
                let _index = self.pop();
                let collection = self.pop();
                self.push(Self::vm_index(collection, _index)?);
            }
            OP_STORE_INDEX => {
                let _value = self.pop();
                let _ = self.pop(); // index
                let _ = self.pop(); // collection
                self.push(_value);
            }
            OP_MEMBER => {
                let idx = self.read_u16_operand() as usize;
                let prop = self.string_constant(idx)?;
                let obj = self.pop();
                self.push(self.vm_member(obj, &prop)?);
            }
            OP_STORE_MEMBER => {
                let idx = self.read_u16_operand() as usize;
                let _prop = self.string_constant(idx)?;
                let _value = self.pop();
                let _obj = self.pop();
                self.push(_value);
            }

            // ---- Stack ----
            OP_POP => {
                let _ = self.pop();
            }
            OP_DUP => {
                let val = self.peek();
                self.push(val);
            }

            // ---- Exceptions ----
            OP_THROW => {
                let err_val = self.pop();
                if let Some(&(handler_ip, target_depth)) = self.handlers.last() {
                    while self.stack.len() > target_depth {
                        self.stack.pop();
                    }
                    self.handlers.pop();
                    self.stack.push(err_val);
                    if let Some(frame) = self.frames.last_mut() {
                        frame.ip = handler_ip;
                    }
                    return Ok(());
                } else {
                    return Err(VmError::new(format!("unhandled exception: {}", err_val)));
                }
            }
            OP_TRY_BEGIN => {
                let handler_offset = self.read_u16_operand() as i16;
                let frame = self.current_frame()?;
                let handler_ip = ((frame.ip as i32) + 3 + (handler_offset as i32)) as usize;
                self.handlers.push((handler_ip, self.stack.len()));
            }
            OP_TRY_END => {
                self.handlers.pop();
            }
            OP_CATCH => {} // Error value already on stack from THROW.

            // ---- Error propagation (? operator) ----
            OP_TRY => {
                let val = self.pop();
                match val {
                    Value::Ok(v) => self.push(*v),
                    Value::Err(e) => {
                        // Propagate the error as an exception.
                        self.push(Value::Err(e));
                        // Use throw mechanism.
                        if let Some(&(handler_ip, target_depth)) = self.handlers.last() {
                            while self.stack.len() > target_depth {
                                self.stack.pop();
                            }
                            self.handlers.pop();
                            if let Some(frame) = self.frames.last_mut() {
                                frame.ip = handler_ip;
                            }
                            return Ok(());
                        } else {
                            return Err(VmError::new(format!(
                                "unhandled error propagation: {:?}",
                                self.stack.last().unwrap()
                            )));
                        }
                    }
                    other => {
                        return Err(VmError::new(format!(
                            "? operator requires Result type, got {}",
                            other
                        )));
                    }
                }
            }

            // ---- OOP ----
            OP_THIS => {
                let this_val = self.this_stack.last().cloned().unwrap_or(Value::Null);
                self.push(this_val);
            }
            OP_METHOD_CALL => {
                let name_idx = self.read_u16_operand() as usize;
                // arg_count is at ip+3 (after op + u16)
                let frame = self.frames.last().expect("no frame");
                let chunk = self.chunk_of(&frame.closure).expect("no chunk");
                let arg_count = chunk.code[frame.ip + 3] as usize;
                let method_name = self.string_constant(name_idx)?;
                let mut args = Vec::with_capacity(arg_count);
                for _ in 0..arg_count {
                    args.push(self.pop());
                }
                args.reverse();
                let obj = self.pop();
                let method = self.vm_member(obj.clone(), &method_name)?;
                self.this_stack.push(obj);
                self.call_value(method, args)?;
                self.this_stack.pop();
                return Ok(());
            }
            OP_SUPER_METHOD => {
                let name_idx = self.read_u16_operand() as usize;
                let frame = self.frames.last().expect("no frame");
                let chunk = self.chunk_of(&frame.closure).expect("no chunk");
                let arg_count = chunk.code[frame.ip + 3] as usize;
                let method_name = self.string_constant(name_idx)?;
                let mut args = Vec::with_capacity(arg_count);
                for _ in 0..arg_count {
                    args.push(self.pop());
                }
                args.reverse();
                let current_this = self.this_stack.last().cloned().unwrap_or(Value::Null);
                let class_name = match &current_this {
                    Value::Map(m) => m.data.get("__class__")
                        .and_then(|v| match v { Value::String(s) => Some(s.clone()), _ => None }),
                    _ => None,
                };
                let parent_method = match class_name {
                    Some(name) => {
                        let class_val = self.globals.get(&name).cloned().unwrap_or(Value::Null);
                        match &class_val {
                            Value::Map(class_map) => {
                                if let Some(parent_val) = class_map.data.get("__parent__") {
                                    let mut current = Some(parent_val.clone());
                                    let mut found = None;
                                    while let Some(Value::Map(ref cmap)) = current {
                                        if let Some(m) = cmap.data.get(&method_name) {
                                            found = Some(m.clone());
                                            break;
                                        }
                                        current = cmap.data.get("__parent__").cloned();
                                    }
                                    found
                                } else {
                                    None
                                }
                            }
                            _ => None,
                        }
                    }
                    None => None,
                };
                match parent_method {
                    Some(method) => self.call_value(method, args)?,
                    None => return Err(VmError::new(format!(
                        "no method '{}' found in super chain", method_name
                    ))),
                }
                return Ok(());
            }
            OP_NEW => {
                let arg_count = self.read_u8_operand() as usize;
                let mut args = Vec::with_capacity(arg_count);
                for _ in 0..arg_count {
                    args.push(self.pop());
                }
                args.reverse();
                let class_val = self.pop();
                let class_map = match &class_val {
                    Value::Map(m) => &m.data,
                    _ => return Err(VmError::new("'new' requires a class value")),
                };
                let class_name = class_map.get("__class__")
                    .and_then(|v| match v { Value::String(s) => Some(s.clone()), _ => None })
                    .unwrap_or_default();

                // Runtime implements validation
                if let Some(Value::String(iface_names)) = class_map.get("__implements__") {
                    let defined: Vec<&str> = class_map.keys()
                        .filter(|k| !k.starts_with("__"))
                        .map(|k| k.as_str())
                        .collect();
                    for iface_name in iface_names.split(',') {
                        let iface_name = iface_name.trim();
                        if let Some(iface_val) = self.globals.get(iface_name) {
                            if let Value::Map(iface_map) = iface_val {
                                for (method_name, _) in &iface_map.data {
                                    if !method_name.starts_with("__")
                                        && !defined.contains(&method_name.as_str())
                                    {
                                        return Err(VmError::new(format!(
                                            "class '{}' does not implement interface method '{}' from '{}'",
                                            class_name, method_name, iface_name
                                        )));
                                    }
                                }
                            }
                        } else {
                            return Err(VmError::new(format!(
                                "interface '{}' not found (implemented by class '{}')",
                                iface_name, class_name
                            )));
                        }
                    }
                }

                // Create instance
                let mut instance_map = HashMap::new();
                instance_map.insert("__class__".to_string(), Value::String(class_name.clone()));
                for (key, val) in class_map.iter() {
                    if key.starts_with("__prop_") {
                        let prop_name = key.strip_prefix("__prop_").unwrap_or(key);
                        instance_map.insert(prop_name.to_string(), val.clone());
                    }
                }
                let instance = self.alloc_map(instance_map);
                if let Some(ctor_val) = class_map.get("__constructor__") {
                    self.this_stack.push(instance.clone());
                    self.call_value(ctor_val.clone(), args)?;
                    let final_instance = self.this_stack.pop().unwrap_or(instance);
                    self.push(final_instance);
                } else {
                    self.push(instance);
                }
                return Ok(());
            }

            // ---- Concurrency ----

            OP_SELECT_TRY_RECV => {
                let offset = self.read_i16_operand();
                let channel_val = self.pop();
                match &channel_val {
                    Value::Channel(arc) => {
                        let mut inner = arc.lock().map_err(|_| {
                            VmError::new("channel lock poisoned")
                        })?;
                        if !inner.queue.is_empty() && !inner.closed {
                            let val = inner.queue.pop_front().unwrap_or(Value::Null);
                            self.push(val);
                            // Has data — fall through to body
                        } else {
                            // No data — jump to next case
                            self.do_jump(offset);
                            return Ok(());
                        }
                    }
                    _ => {
                        return Err(VmError::new(
                            "select case expression must evaluate to a channel",
                        ));
                    }
                }
            }
            OP_CHANNEL_SEND => {
                let value = self.pop();
                let channel_val = self.pop();
                match &channel_val {
                    Value::Channel(arc) => {
                        let mut inner = arc.lock().map_err(|_| {
                            VmError::new("channel lock poisoned")
                        })?;
                        if inner.closed {
                            return Err(VmError::new("send on closed channel"));
                        }
                        if inner.queue.len() < inner.capacity {
                            inner.queue.push_back(value);
                            self.push(Value::Null); // send returns null
                        } else {
                            return Err(VmError::new("channel full (select/try only; use async for blocking)"));
                        }
                    }
                    _ => return Err(VmError::new("send requires a channel")),
                }
            }
            OP_CHANNEL_RECV => {
                let channel_val = self.pop();
                match &channel_val {
                    Value::Channel(arc) => {
                        let mut inner = arc.lock().map_err(|_| {
                            VmError::new("channel lock poisoned")
                        })?;
                        if let Some(val) = inner.queue.pop_front() {
                            self.push(val);
                        } else {
                            return Err(VmError::new("recv on empty channel (use select/try or async)"));
                        }
                    }
                    _ => return Err(VmError::new("recv requires a channel")),
                }
            }
            OP_ATOMIC_LOAD => {
                let atomic_val = self.pop();
                match &atomic_val {
                    Value::Atomic(inner) => {
                        let guard = inner.lock().map_err(|_| {
                            VmError::new("atomic lock poisoned")
                        })?;
                        self.push(guard.value.clone());
                    }
                    _ => return Err(VmError::new("load requires an atomic value")),
                }
            }
            OP_ATOMIC_STORE => {
                let new_value = self.pop();
                let atomic_val = self.pop();
                match &atomic_val {
                    Value::Atomic(inner) => {
                        let mut guard = inner.lock().map_err(|_| {
                            VmError::new("atomic lock poisoned")
                        })?;
                        guard.value = new_value;
                        self.push(Value::Null);
                    }
                    _ => return Err(VmError::new("store requires an atomic value")),
                }
            }

            // ---- Float arithmetic ----
            OP_ADD_F => self.binop(|a, b| Self::vm_add_f(a, b))?,
            OP_SUB_F => self.binop(|a, b| Self::vm_sub_f(a, b))?,
            OP_MUL_F => self.binop(|a, b| Self::vm_mul_f(a, b))?,
            OP_DIV_F => self.binop(|a, b| Self::vm_div_f(a, b))?,

            // ---- Type introspection ----
            OP_TYPE_IS => {
                let idx = self.read_u16_operand() as usize;
                let type_name = self.string_constant(idx)?;
                let val = self.pop();
                let result = Self::vm_type_is(&val, &type_name);
                self.push(Value::Bool(result));
            }
            OP_TYPEOF => {
                let val = self.pop();
                let type_str = Self::vm_typeof(&val);
                self.push(Value::String(type_str));
            }

            // ---- Pipe ----
            OP_PIPE_VAL => {
                // $$ is the value most recently passed through a pipe.
                // For now, it's the top-of-stack value (kept by the pipe compiler).
                let val = self.peek();
                self.push(val);
            }

            // ---- Map iteration ----
            OP_ITER_MAP => {
                // Pop map and index, push next key (or null if done)
                let idx_val = self.pop();
                let map_val = self.pop();
                match (&map_val, &idx_val) {
                    (Value::Map(map), Value::Int(i)) => {
                        use num_traits::ToPrimitive;
                        let idx = i.to_usize().unwrap_or(usize::MAX);
                        let keys: Vec<&String> = map.data.keys().collect();
                        if idx < keys.len() {
                            self.push(Value::String(keys[idx].clone()));
                            self.push(Value::Int(BigInt::from(idx + 1)));
                            self.push(map_val);
                            self.push(Value::Bool(true));
                        } else {
                            self.push(Value::Null);
                            self.push(Value::Bool(false));
                        }
                    }
                    _ => return Err(VmError::new("ITER_MAP requires a map and int index")),
                }
            }

            // ---- Closures ----
            OP_CLOSE_UPVALUE => {
                // Move the top-of-stack value to the heap (upvalue capture).
                // Currently a no-op since all values are already heap-allocated or Copy.
                // Future: move the value to an upvalue slot that outlives the stack frame.
            }

            _ => {
                return Err(VmError::new(format!(
                    "unknown opcode: {} ({})",
                    op,
                    opcode_name(op)
                )))
            }
        }

        // Normal instructions advance IP.
        self.step_ip(step);
        Ok(())
    }

    // ========================================================================
    // Async call
    // ========================================================================

    /// Perform an async call: pop args + callee, spawn a new task, return task id.
    fn async_call(&mut self, arg_count: usize) -> Result<TaskId, VmError> {
        let mut args = Vec::with_capacity(arg_count);
        for _ in 0..arg_count {
            args.push(self.pop());
        }
        args.reverse();
        let callee = self.pop();

        match callee {
            Value::FnObj(fn_obj) => {
                let _chunk = fn_obj.chunk.clone();
                let arity = fn_obj.arity;
                if arg_count != arity {
                    return Err(VmError::new(format!(
                        "expected {} arguments but got {}",
                        arity, arg_count
                    )));
                }
                let mut new_stack: Vec<Value> = Vec::new();
                new_stack.push(Value::FnObj(fn_obj.clone()));
                for arg in args {
                    new_stack.push(arg);
                }
                let task_id = self.scheduler.spawn(
                    Value::FnObj(fn_obj),
                    0,
                    1, // locals start after closure
                    new_stack,
                );
                Ok(task_id)
            }
            Value::BuiltinFn(name) => {
                let result = call_builtin(&name, &args, &mut self.heap)
                    .map_err(|e| VmError::new(format!("builtin error: {:?}", e)))?;
                let task_id = self.scheduler.spawn(Value::Null, 0, 0, vec![result.clone()]);
                self.scheduler.complete(task_id, result);
                Ok(task_id)
            }
            _ => Err(VmError::new("can only call functions and builtins")),
        }
    }

    // ========================================================================
    // Function call / return
    // ========================================================================

    fn call(&mut self, arg_count: usize) -> VmResult<()> {
        // Function is at stack[stack.len() - 1 - arg_count]
        let fn_idx = self.stack.len() - 1 - arg_count;
        let callee = self.stack[fn_idx].clone();

        match callee {
            Value::FnObj(fn_obj) => {
                if fn_obj.is_async {
                    // Async call: pop args and callee, spawn task, push TaskHandle.
                    let mut args = Vec::with_capacity(arg_count);
                    for _ in 0..arg_count {
                        args.push(self.pop());
                    }
                    args.reverse();
                    let _callee = self.pop(); // pop the FnObj itself

                    let mut new_stack = vec![Value::FnObj(fn_obj.clone())];
                    for arg in args {
                        new_stack.push(arg);
                    }
                    let task_id = self.scheduler.spawn(
                        Value::FnObj(fn_obj),
                        0,
                        1,
                        new_stack,
                    );
                    self.push(Value::TaskHandle(task_id));
                    return Ok(());
                }

                let arity = fn_obj.arity;
                if arg_count != arity {
                    return Err(VmError::new(format!(
                        "{}() expects {} arguments, got {}",
                        fn_obj.name, arity, arg_count
                    )));
                }

                // Pop the args and function; the args become locals.
                let fn_val = self.stack.remove(fn_idx);
                // Start of locals is right where the args were.
                let stack_offset = fn_idx;

                self.frames.push(CallFrame {
                    closure: fn_val,
                    ip: 0,
                    stack_offset,
                });
            }
            Value::BuiltinFn(name) => {
                // Collect args
                let args: Vec<Value> = self.stack.drain(fn_idx + 1..).collect();
                self.stack.pop(); // pop the BuiltinFn itself
                let result = call_builtin(&name, &args, &mut self.heap)
                    .map_err(|s| VmError::new(format!("builtin error: {:?}", s)))?;
                self.push(result);
            }
            _ => {
                return Err(VmError::new(format!(
                    "{} is not callable",
                    callee
                )));
            }
        }

        Ok(())
    }

    /// Call a function value directly (not from the stack).
    /// Used by OP_METHOD_CALL, OP_SUPER_METHOD, and OP_NEW.
    fn call_value(&mut self, callee: Value, args: Vec<Value>) -> VmResult<()> {
        let arg_count = args.len();
        match callee {
            Value::FnObj(fn_obj) => {
                if fn_obj.is_async {
                    // Async method call: spawn a task instead of pushing a call frame.
                    // The `this` value is on this_stack (pushed by the caller).
                    let mut new_stack = vec![Value::FnObj(fn_obj.clone())];
                    // Include `this` as the first argument (slot 1).
                    if let Some(this_val) = self.this_stack.last() {
                        new_stack.push(this_val.clone());
                    } else {
                        new_stack.push(Value::Null);
                    }
                    for arg in args {
                        new_stack.push(arg);
                    }
                    let task_id = self.scheduler.spawn(
                        Value::FnObj(fn_obj),
                        0,
                        2, // locals start after closure (0) + this (1)
                        new_stack,
                    );
                    self.push(Value::TaskHandle(task_id));
                } else {
                    let arity = fn_obj.arity;
                    if arg_count != arity {
                        return Err(VmError::new(format!(
                            "{}() expects {} arguments, got {}",
                            fn_obj.name, arity, arg_count
                        )));
                    }
                    // Push the function and args onto the stack as locals
                    let stack_offset = self.stack.len();
                    self.stack.push(Value::FnObj(fn_obj));
                    for arg in args {
                        self.stack.push(arg);
                    }
                    self.frames.push(CallFrame {
                        closure: self.stack[stack_offset].clone(),
                        ip: 0,
                        stack_offset,
                    });
                }
            }
            Value::BuiltinFn(name) => {
                let result = call_builtin(&name, &args, &mut self.heap)
                    .map_err(|s| VmError::new(format!("builtin error: {:?}", s)))?;
                self.push(result);
            }
            _ => {
                return Err(VmError::new(format!(
                    "{} is not callable",
                    callee
                )));
            }
        }
        Ok(())
    }

    fn do_return(&mut self) -> VmResult<()> {
        let return_val = self.pop();
        let frame = self.frames.pop().ok_or_else(|| VmError::new("return without frame"))?;

        // Pop all locals belonging to this frame.
        let new_len = frame.stack_offset;
        self.stack.truncate(new_len);

        // Push the return value.
        self.push(return_val);
        Ok(())
    }

    // ========================================================================
    // Stack helpers
    // ========================================================================

    fn push(&mut self, val: Value) {
        self.stack.push(val);
    }

    fn pop(&mut self) -> Value {
        self.stack.pop().unwrap_or(Value::Null)
    }

    fn peek(&self) -> Value {
        self.stack.last().cloned().unwrap_or(Value::Null)
    }

    // ========================================================================
    // Frame helpers
    // ========================================================================

    fn current_frame(&self) -> VmResult<&CallFrame> {
        self.frames
            .last()
            .ok_or_else(|| VmError::new("no active frame"))
    }

    fn chunk_of<'a>(&self, val: &'a Value) -> Option<&'a Chunk> {
        match val {
            Value::FnObj(fo) => Some(&fo.chunk),
            _ => None,
        }
    }

    fn constants(&self) -> &[Value] {
        let frame = self.frames.last().expect("no frame");
        match &frame.closure {
            Value::FnObj(fo) => &fo.chunk.constants,
            _ => &[],
        }
    }

    /// Advance IP by `n` bytes. Used for normal (non-jump) instructions.
    fn step_ip(&mut self, n: usize) {
        if let Some(frame) = self.frames.last_mut() {
            frame.ip += n;
        }
    }

    /// Jump to target: target = ip + 3 + offset (where offset is the raw i16 from bytecode).
    fn do_jump(&mut self, offset: i16) {
        if let Some(frame) = self.frames.last_mut() {
            frame.ip = ((frame.ip as i32) + 3 + (offset as i32)) as usize;
        }
    }

    /// Loop back: target = ip + 3 - offset (offset is positive backward distance).
    fn do_loop_back(&mut self, offset: i16) {
        if let Some(frame) = self.frames.last_mut() {
            frame.ip = ((frame.ip as i32) + 3 - (offset as i32)) as usize;
        }
    }

    // ========================================================================
    // Operand readers
    // ========================================================================

    fn read_u16_operand(&self) -> u16 {
        let frame = self.frames.last().expect("no frame");
        let chunk = self.chunk_of(&frame.closure).expect("no chunk");
        read_u16(&chunk.code[frame.ip + 1..frame.ip + 3])
    }

    fn read_i16_operand(&self) -> i16 {
        let frame = self.frames.last().expect("no frame");
        let chunk = self.chunk_of(&frame.closure).expect("no chunk");
        read_i16(&chunk.code[frame.ip + 1..frame.ip + 3])
    }

    fn read_u8_operand(&self) -> u8 {
        let frame = self.frames.last().expect("no frame");
        let chunk = self.chunk_of(&frame.closure).expect("no chunk");
        chunk.code[frame.ip + 1]
    }

    fn string_constant(&self, idx: usize) -> VmResult<String> {
        let constants = self.constants();
        match constants.get(idx) {
            Some(Value::String(s)) => Ok(s.clone()),
            _ => Err(VmError::new(format!(
                "constant {} is not a string",
                idx
            ))),
        }
    }

    // ========================================================================
    // Builtins
    // ========================================================================

    fn register_builtins(&mut self) {
        self.globals.insert(
            "print".to_string(),
            Value::BuiltinFn("print".to_string()),
        );
        self.globals.insert(
            "len".to_string(),
            Value::BuiltinFn("len".to_string()),
        );
        self.globals.insert(
            "toString".to_string(),
            Value::BuiltinFn("toString".to_string()),
        );
        self.globals.insert(
            "deepEquals".to_string(),
            Value::BuiltinFn("deepEquals".to_string()),
        );
        self.globals.insert(
            "parseInt".to_string(),
            Value::BuiltinFn("parseInt".to_string()),
        );
        self.globals.insert(
            "parseFloat".to_string(),
            Value::BuiltinFn("parseFloat".to_string()),
        );
        // Result constructors
        self.globals.insert(
            "Ok".to_string(),
            Value::BuiltinFn("Ok".to_string()),
        );
        self.globals.insert(
            "Err".to_string(),
            Value::BuiltinFn("Err".to_string()),
        );
        // Math
        self.globals.insert(
            "abs".to_string(),
            Value::BuiltinFn("abs".to_string()),
        );
        self.globals.insert(
            "min".to_string(),
            Value::BuiltinFn("min".to_string()),
        );
        self.globals.insert(
            "max".to_string(),
            Value::BuiltinFn("max".to_string()),
        );
        self.globals.insert(
            "floor".to_string(),
            Value::BuiltinFn("floor".to_string()),
        );
        self.globals.insert(
            "ceil".to_string(),
            Value::BuiltinFn("ceil".to_string()),
        );
        self.globals.insert(
            "round".to_string(),
            Value::BuiltinFn("round".to_string()),
        );
        self.globals.insert(
            "sqrt".to_string(),
            Value::BuiltinFn("sqrt".to_string()),
        );
        self.globals.insert(
            "pow".to_string(),
            Value::BuiltinFn("pow".to_string()),
        );
        self.globals.insert(
            "random".to_string(),
            Value::BuiltinFn("random".to_string()),
        );
        // Type checking
        self.globals.insert(
            "typeOf".to_string(),
            Value::BuiltinFn("typeOf".to_string()),
        );
        self.globals.insert(
            "isOk".to_string(),
            Value::BuiltinFn("isOk".to_string()),
        );
        self.globals.insert(
            "isErr".to_string(),
            Value::BuiltinFn("isErr".to_string()),
        );
        self.globals.insert(
            "unwrap".to_string(),
            Value::BuiltinFn("unwrap".to_string()),
        );
        // Database builtins (std/db).
        for name in ["db_open", "db_exec", "db_query", "db_close"] {
            self.globals
                .insert(name.to_string(), Value::BuiltinFn(name.to_string()));
        }
        // Time builtins (used by std/time and parallel-block timing).
        for name in ["time_now", "time_sleep"] {
            self.globals
                .insert(name.to_string(), Value::BuiltinFn(name.to_string()));
        }
        // Async I/O event loop primitive (mio-backed fd readiness).
        self.globals
            .insert("io_wait".to_string(), Value::BuiltinFn("io_wait".to_string()));
        // TCP builtins (used by std/net and the HTTP server).
        for name in ["tcp_listen", "tcp_accept", "tcp_read", "tcp_write", "tcp_close", "tcp_connect"] {
            self.globals
                .insert(name.to_string(), Value::BuiltinFn(name.to_string()));
        }
        // Register every remaining builtin as a global so stdlib modules (and
        // user code) can call them directly by name. Each is handled by
        // `call_builtin`; this exposes the name so bare calls resolve.
        for name in [
            "abs", "assert", "atomic_add", "atomic_cas", "atomic_load", "atomic_store",
            "atomic_sub", "bool", "ceil", "chan", "chan_close", "chan_recv", "chan_send",
            "error", "float", "floor", "fs_exists", "fs_mkdir", "fs_remove", "fs_stat",
            "hash", "hex_decode", "hex_encode", "int", "json_parse", "json_stringify",
            "list_insert", "list_join", "list_pop", "list_push", "list_remove",
            "map_delete", "map_get", "map_has", "map_keys", "map_set", "map_values",
            "max", "min", "pow", "process_args", "process_cwd", "process_env", "process_exit",
            "range", "regex_match", "regex_replace", "round", "sha256", "sqrt",
            "str_charAt", "str_contains", "str_endsWith", "str_indexOf", "str_repeat",
            "str_replace", "str_split", "str_startsWith", "str_substring", "str_toLower",
            "str_toUpper", "str_trim", "udp_bind", "udp_close", "udp_recv", "udp_send",
        ] {
            self.globals
                .insert(name.to_string(), Value::BuiltinFn(name.to_string()));
        }
    }

    // ========================================================================
    // GC helpers
    // ========================================================================

    fn alloc_list(&mut self, items: Vec<Value>) -> Value {
        // Arc-backed: the value owns its data and outlives the VM/Heap.
        Value::List(std::sync::Arc::new(CoW::new(items)))
    }

    fn alloc_map(&mut self, items: HashMap<String, Value>) -> Value {
        Value::Map(std::sync::Arc::new(CoW::new(items)))
    }

    /// Run a tracing GC cycle.
    ///
    /// Roots are the operand stack, globals, the `$` (this) stack, and the
    /// scheduler's task stacks. The tracer downcasts each heap object to its
    /// `CoW<Vec<Value>>` / `CoW<HashMap<String, Value>>` form and collects the
    /// `GcRef`s of any nested heap objects, so cycles unreachable from the
    /// roots are reclaimed.
    pub fn gc_collect(&mut self) {
        // Gather roots: every GcRef reachable from live Values.
        let mut roots: Vec<GcRef> = Vec::new();
        for v in &self.stack {
            if let Some(id) = v.gc_ref() {
                roots.push(id);
            }
        }
        for v in self.globals.values() {
            if let Some(id) = v.gc_ref() {
                roots.push(id);
            }
        }
        for v in &self.this_stack {
            if let Some(id) = v.gc_ref() {
                roots.push(id);
            }
        }
        // Task stacks hold live Values awaiting resumption.
        for task in self.scheduler.tasks() {
            for v in &task.stack {
                if let Some(id) = v.gc_ref() {
                    roots.push(id);
                }
            }
        }

        // Tracer: given an object's &dyn Any, return child GcRefs.
        // Lists hold Vec<Value>; Maps hold HashMap<String, Value>.
        self.heap.collect_tracing(&roots, |any| {
            let mut children = Vec::new();
            if let Some(list) = any.downcast_ref::<CoW<Vec<Value>>>() {
                for v in &list.data {
                    if let Some(id) = v.gc_ref() {
                        children.push(id);
                    }
                }
            } else if let Some(map) = any.downcast_ref::<CoW<HashMap<String, Value>>>() {
                for v in map.data.values() {
                    if let Some(id) = v.gc_ref() {
                        children.push(id);
                    }
                }
            }
            children
        });
    }

    // ========================================================================
    // Arithmetic implementations (static so they can be used in closures)
    // ========================================================================

    /// Convert a BigInt to f64, returning an error if overflow.
    fn int_to_f64(n: &BigInt) -> VmResult<f64> {
        use num_traits::ToPrimitive;
        n.to_f64()
            .ok_or_else(|| VmError::new("integer too large to convert to float"))
    }

    fn vm_add(a: Value, b: Value) -> VmResult<Value> {
        match (a, b) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a + b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
            (Value::Int(a), Value::Float(b)) => Ok(Value::Float(Self::int_to_f64(&a)? + b)),
            (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a + Self::int_to_f64(&b)?)),
            (Value::String(a), Value::String(b)) => Ok(Value::String(format!("{}{}", a, b))),
            _ => Err(VmError::new("invalid operands for +")),
        }
    }

    fn vm_sub(a: Value, b: Value) -> VmResult<Value> {
        match (a, b) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a - b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a - b)),
            (Value::Int(a), Value::Float(b)) => Ok(Value::Float(Self::int_to_f64(&a)? - b)),
            (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a - Self::int_to_f64(&b)?)),
            _ => Err(VmError::new("invalid operands for -")),
        }
    }

    fn vm_mul(a: Value, b: Value) -> VmResult<Value> {
        match (a, b) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a * b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a * b)),
            (Value::Int(a), Value::Float(b)) => Ok(Value::Float(Self::int_to_f64(&a)? * b)),
            (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a * Self::int_to_f64(&b)?)),
            _ => Err(VmError::new("invalid operands for *")),
        }
    }

    fn vm_div(a: Value, b: Value) -> VmResult<Value> {
        match (a, b) {
            (Value::Int(a), Value::Int(b)) => {
                if b == BigInt::from(0) {
                    return Err(VmError::new("division by zero"));
                }
                Ok(Value::Int(a / b))
            }
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a / b)),
            (Value::Int(a), Value::Float(b)) => Ok(Value::Float(Self::int_to_f64(&a)? / b)),
            (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a / Self::int_to_f64(&b)?)),
            _ => Err(VmError::new("invalid operands for /")),
        }
    }

    fn vm_mod(a: Value, b: Value) -> VmResult<Value> {
        match (a, b) {
            (Value::Int(a), Value::Int(b)) => {
                if b == BigInt::from(0) {
                    return Err(VmError::new("modulo by zero"));
                }
                Ok(Value::Int(a % b))
            }
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a % b)),
            (Value::Int(a), Value::Float(b)) => Ok(Value::Float(Self::int_to_f64(&a)? % b)),
            (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a % Self::int_to_f64(&b)?)),
            _ => Err(VmError::new("invalid operands for %")),
        }
    }

    fn vm_pow(a: Value, b: Value) -> VmResult<Value> {
        use num_traits::ToPrimitive;
        match (a, b) {
            (Value::Int(a), Value::Int(b)) => {
                if let Some(exp) = b.to_u32() {
                    Ok(Value::Int(a.pow(exp)))
                } else if let Some(exp) = b.to_i32() {
                    // Negative exponent → float result
                    Ok(Value::Float(Self::int_to_f64(&a)?.powi(exp)))
                } else {
                    Err(VmError::new("exponent too large"))
                }
            }
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a.powf(b))),
            (Value::Int(a), Value::Float(b)) => Ok(Value::Float(Self::int_to_f64(&a)?.powf(b))),
            (Value::Float(a), Value::Int(b)) => {
                if let Some(exp) = b.to_i32() {
                    Ok(Value::Float(a.powi(exp)))
                } else {
                    Err(VmError::new("exponent too large"))
                }
            }
            _ => Err(VmError::new("invalid operands for **")),
        }
    }

    fn vm_eq(a: &Value, b: &Value) -> bool {
        // Delegate to the shared structural-equality function so the
        // `deepEquals` builtin and `==` operator stay in sync.
        crate::value::value_eq(a, b)
    }

    fn vm_neg(val: Value) -> VmResult<Value> {
        match val {
            Value::Int(n) => Ok(Value::Int(-n)),
            Value::Float(f) => Ok(Value::Float(-f)),
            _ => Err(VmError::new("cannot negate non-number")),
        }
    }

    fn vm_add_f(a: Value, b: Value) -> VmResult<Value> {
        match (a, b) {
            (Value::Float(x), Value::Float(y)) => Ok(Value::Float(x + y)),
            (Value::Float(x), Value::Int(y)) => Ok(Value::Float(x + Self::int_to_f64(&y)?)),
            (Value::Int(x), Value::Float(y)) => Ok(Value::Float(Self::int_to_f64(&x)? + y)),
            _ => Err(VmError::new("ADD_F requires float operands")),
        }
    }
    fn vm_sub_f(a: Value, b: Value) -> VmResult<Value> {
        match (a, b) {
            (Value::Float(x), Value::Float(y)) => Ok(Value::Float(x - y)),
            (Value::Float(x), Value::Int(y)) => Ok(Value::Float(x - Self::int_to_f64(&y)?)),
            (Value::Int(x), Value::Float(y)) => Ok(Value::Float(Self::int_to_f64(&x)? - y)),
            _ => Err(VmError::new("SUB_F requires float operands")),
        }
    }
    fn vm_mul_f(a: Value, b: Value) -> VmResult<Value> {
        match (a, b) {
            (Value::Float(x), Value::Float(y)) => Ok(Value::Float(x * y)),
            (Value::Float(x), Value::Int(y)) => Ok(Value::Float(x * Self::int_to_f64(&y)?)),
            (Value::Int(x), Value::Float(y)) => Ok(Value::Float(Self::int_to_f64(&x)? * y)),
            _ => Err(VmError::new("MUL_F requires float operands")),
        }
    }
    fn vm_div_f(a: Value, b: Value) -> VmResult<Value> {
        match (a, b) {
            (Value::Float(x), Value::Float(y)) => Ok(Value::Float(x / y)),
            (Value::Float(x), Value::Int(y)) => Ok(Value::Float(x / Self::int_to_f64(&y)?)),
            (Value::Int(x), Value::Float(y)) => Ok(Value::Float(Self::int_to_f64(&x)? / y)),
            _ => Err(VmError::new("DIV_F requires float operands")),
        }
    }

    fn vm_type_is(val: &Value, type_name: &str) -> bool {
        match type_name {
            "int" => matches!(val, Value::Int(_)),
            "float" => matches!(val, Value::Float(_)),
            "string" => matches!(val, Value::String(_)),
            "bool" => matches!(val, Value::Bool(_)),
            "null" => matches!(val, Value::Null),
            "list" => matches!(val, Value::List(_)),
            "map" => matches!(val, Value::Map(_)),
            "function" => matches!(val, Value::FnObj(_) | Value::BuiltinFn(_)),
            "channel" => matches!(val, Value::Channel(_)),
            "atomic" => matches!(val, Value::Atomic(_)),
            "task" => matches!(val, Value::TaskHandle(_)),
            _ => false,
        }
    }

    fn vm_typeof(val: &Value) -> String {
        match val {
            Value::Int(_) => "int",
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
        }.to_string()
    }

    fn vm_cmp(a: Value, b: Value, pred: impl Fn(std::cmp::Ordering) -> bool) -> VmResult<Value> {
        let ord = match (&a, &b) {
            (Value::Int(a), Value::Int(b)) => a.cmp(b),
            (Value::Float(a), Value::Float(b)) => {
                a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
            }
            (Value::Int(a), Value::Float(b)) => Self::int_to_f64(a)?
                .partial_cmp(b)
                .unwrap_or(std::cmp::Ordering::Equal),
            (Value::Float(a), Value::Int(b)) => a
                .partial_cmp(&Self::int_to_f64(b)?)
                .unwrap_or(std::cmp::Ordering::Equal),
            (Value::String(a), Value::String(b)) => a.cmp(b),
            _ => return Err(VmError::new("cannot compare these values")),
        };
        Ok(Value::Bool(pred(ord)))
    }

    fn vm_bitop<F>(a: Value, b: Value, op: F) -> VmResult<Value>
    where
        F: FnOnce(BigInt, BigInt) -> BigInt,
    {
        match (a, b) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(op(a, b))),
            // Bitwise on bools (per grammar: "bitwise on bools"): treat as 0/1
            // and return a Bool so word-form operators like `xor` stay logical.
            (Value::Bool(a), Value::Bool(b)) => {
                let result = op(BigInt::from(a as u8), BigInt::from(b as u8));
                Ok(Value::Bool(result != BigInt::from(0)))
            }
            _ => Err(VmError::new("bitwise operations require integers")),
        }
    }

    fn vm_index(collection: Value, index: Value) -> VmResult<Value> {
        use num_traits::ToPrimitive;
        match (&collection, &index) {
            (Value::List(list), Value::Int(i)) => {
                let idx = if *i < BigInt::from(0) {
                    let len = list.data.len() as i64;
                    let offset = i.to_i64().unwrap_or(0);
                    (len + offset).max(0) as usize
                } else {
                    i.to_usize().unwrap_or(usize::MAX)
                };
                list.data
                    .get(idx)
                    .cloned()
                    .ok_or_else(|| VmError::new("index out of bounds"))
            }
            (Value::Map(map), Value::String(key)) => {
                Ok(map.data.get(key).cloned().unwrap_or(Value::Null))
            }
            // Map indexed by int: yields the key at that position. This lets the
            // index-based `for k in map` loop reuse the same machinery as lists,
            // producing map keys in insertion order.
            (Value::Map(map), Value::Int(i)) => {
                let idx = i.to_usize().unwrap_or(usize::MAX);
                map.data
                    .keys()
                    .nth(idx)
                    .map(|k| Value::String(k.clone()))
                    .ok_or_else(|| VmError::new("index out of bounds"))
            }
            _ => Err(VmError::new("invalid index operation")),
        }
    }

    fn vm_member(&self, obj: Value, prop: &str) -> VmResult<Value> {
        match &obj {
            Value::List(list) if prop == "length" => Ok(Value::Int(BigInt::from(list.data.len()))),
            Value::String(s) if prop == "length" => Ok(Value::Int(BigInt::from(s.len()))),
            Value::Map(map) => {
                // `length` builtin: number of entries. Falls back to a user's
                // own "length" key only if present, so we check the data first.
                if prop == "length" && !map.data.contains_key("length") {
                    return Ok(Value::Int(BigInt::from(map.data.len())));
                }
                // Direct property access
                if let Some(val) = map.data.get(prop) {
                    return Ok(val.clone());
                }
                // OOP prototype chain lookup via __class__
                if let Some(Value::String(class_name)) = map.data.get("__class__") {
                    if let Some(class_val) = self.globals.get(class_name) {
                        if let Value::Map(class_map) = class_val {
                            // Check current class methods
                            if let Some(method) = class_map.data.get(prop) {
                                return Ok(method.clone());
                            }
                            // Resolve parent class via __parent_name__ or __parent__
                            let parent = class_map.data.get("__parent__").cloned()
                                .or_else(|| {
                                    class_map.data.get("__parent_name__")
                                        .and_then(|v| match v {
                                            Value::String(name) => self.globals.get(name).cloned(),
                                            _ => None,
                                        })
                                });
                            let mut current = parent;
                            while let Some(Value::Map(ref parent_map)) = current {
                                if let Some(method) = parent_map.data.get(prop) {
                                    return Ok(method.clone());
                                }
                                // Resolve next parent
                                current = parent_map.data.get("__parent__").cloned()
                                    .or_else(|| {
                                        parent_map.data.get("__parent_name__")
                                            .and_then(|v| match v {
                                                Value::String(name) => self.globals.get(name).cloned(),
                                                _ => None,
                                            })
                                    });
                            }
                            // Trait mixin resolution via __use_traits__
                            if let Some(Value::String(trait_names)) = class_map.data.get("__use_traits__") {
                                for trait_name in trait_names.split(',') {
                                    let trait_name = trait_name.trim();
                                    if let Some(trait_val) = self.globals.get(trait_name) {
                                        if let Value::Map(trait_map) = trait_val {
                                            if let Some(method) = trait_map.data.get(prop) {
                                                return Ok(method.clone());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(Value::Null)
            }
            _ => Err(VmError::new(format!(
                "cannot access property '{}' on value",
                prop
            ))),
        }
    }

    // ========================================================================
    // Generic binop
    // ========================================================================

    fn binop<F>(&mut self, op: F) -> VmResult<()>
    where
        F: FnOnce(Value, Value) -> VmResult<Value>,
    {
        let b = self.pop();
        let a = self.pop();
        let result = op(a, b)?;
        self.push(result);
        Ok(())
    }
}

impl Default for Vm {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::Compiler;
    use coco_parser::Parser;

    fn run_src(src: &str) -> Result<Value, VmError> {
        let mut parser = Parser::new(src);
        let program = parser.parse_program();
        let mut compiler = Compiler::new();
        let chunk = compiler.compile_script(&program).unwrap();
        let mut vm = Vm::new();
        vm.run(&chunk)
    }

    #[test]
    fn test_vm_int_literal() {
        let result = run_src("fn main() { return 42; }").unwrap();
        assert!(matches!(&result, Value::Int(n) if *n == BigInt::from(42)));
    }

    #[test]
    fn test_vm_addition() {
        let result = run_src("fn main() { return 1 + 2; }").unwrap();
        assert!(matches!(&result, Value::Int(n) if *n == BigInt::from(3)));
    }

    #[test]
    fn test_vm_variables() {
        let result =
            run_src("fn main() { let x = 10; let y = 20; return x + y; }").unwrap();
        assert!(matches!(&result, Value::Int(n) if *n == BigInt::from(30)));
    }

    #[test]
    fn test_vm_if_true() {
        let result = run_src(
            "fn main() { let x = 0; if true { x = 1; } return x; }",
        )
        .unwrap();
        assert!(matches!(&result, Value::Int(n) if *n == BigInt::from(1)));
    }

    #[test]
    fn test_vm_if_false() {
        let result = run_src(
            "fn main() { let x = 0; if false { x = 1; } return x; }",
        )
        .unwrap();
        assert!(matches!(&result, Value::Int(n) if *n == BigInt::from(0)));
    }

    #[test]
    fn test_vm_while_loop() {
        let result = run_src(
            "fn main() { let x = 0; while x < 5 { x += 1; } return x; }",
        )
        .unwrap();
        assert!(matches!(&result, Value::Int(n) if *n == BigInt::from(5)));
    }

    #[test]
    fn test_vm_function_call() {
        let result = run_src(
            "fn add(a, b) { return a + b; } fn main() { return add(2, 3); }",
        )
        .unwrap();
        assert!(matches!(&result, Value::Int(n) if *n == BigInt::from(5)));
    }

    #[test]
    fn test_vm_recursion() {
        let result = run_src(
            "fn fib(n) { if n <= 1 { return n; } return fib(n - 1) + fib(n - 2); } fn main() { return fib(10); }",
        )
        .unwrap();
        assert!(matches!(&result, Value::Int(n) if *n == BigInt::from(55)));
    }

    #[test]
    fn test_vm_string_concat() {
        let result =
            run_src("fn main() { return \"hello\" + \" world\"; }").unwrap();
        match result {
            Value::String(s) => assert_eq!(s, "hello world"),
            _ => panic!("expected string"),
        }
    }

    #[test]
    fn test_vm_ternary() {
        let result =
            run_src("fn main() { let x = true ? 1 : 2; return x; }").unwrap();
        assert!(matches!(&result, Value::Int(n) if *n == BigInt::from(1)));
    }

    #[test]
    fn test_vm_null_coalesce() {
        let result =
            run_src("fn main() { let x = null ?? 42; return x; }").unwrap();
        assert!(matches!(&result, Value::Int(n) if *n == BigInt::from(42)));
    }

    #[test]
    fn test_vm_list_literal() {
        let result = run_src("fn main() { let a = [1, 2, 3]; return a.length; }").unwrap();
        assert!(matches!(&result, Value::Int(n) if *n == BigInt::from(3)));
    }
}