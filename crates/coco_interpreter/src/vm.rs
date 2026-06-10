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

use coco_gc::{CoW, Gc, Heap};

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
                    self.scheduler.suspend_awaiting(self.current_task, target_id);
                    self.step_ip(step);
                    self.yield_flag = true;
                    return Ok(());
                }
                return Err(VmError::new("await requires a task handle"));
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
            OP_SHL => self.binop(|a, b| Self::vm_bitop(a, b, |x, y| x << y))?,
            OP_SHR => self.binop(|a, b| Self::vm_bitop(a, b, |x, y| x >> y))?,

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
                self.push(Self::vm_member(obj, &prop)?);
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
                let result = call_builtin(&name, &args)
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
                let result = call_builtin(&name, &args)
                    .map_err(|s| VmError::new(format!("builtin error: {:?}", s)))?;
                self.push(result);
            }
            Value::Function(func) => {
                // Tree-walking function (interop): not supported in pure VM mode.
                return Err(VmError::new(format!(
                    "cannot call tree-walking function '{}' in VM mode",
                    func.name
                )));
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
            "parseInt".to_string(),
            Value::BuiltinFn("parseInt".to_string()),
        );
        self.globals.insert(
            "parseFloat".to_string(),
            Value::BuiltinFn("parseFloat".to_string()),
        );
    }

    // ========================================================================
    // GC helpers
    // ========================================================================

    fn alloc_list(&mut self, items: Vec<Value>) -> Value {
        let cow = CoW::new(items);
        let (id, ptr) = self.heap.allocate(cow);
        Value::List(Gc::new(&self.heap, id, ptr))
    }

    fn alloc_map(&mut self, items: HashMap<String, Value>) -> Value {
        let cow = CoW::new(items);
        let (id, ptr) = self.heap.allocate(cow);
        Value::Map(Gc::new(&self.heap, id, ptr))
    }

    // ========================================================================
    // Arithmetic implementations (static so they can be used in closures)
    // ========================================================================

    fn vm_add(a: Value, b: Value) -> VmResult<Value> {
        match (a, b) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a + b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
            (Value::Int(a), Value::Float(b)) => Ok(Value::Float(a as f64 + b)),
            (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a + b as f64)),
            (Value::String(a), Value::String(b)) => Ok(Value::String(format!("{}{}", a, b))),
            _ => Err(VmError::new("invalid operands for +")),
        }
    }

    fn vm_sub(a: Value, b: Value) -> VmResult<Value> {
        match (a, b) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a - b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a - b)),
            (Value::Int(a), Value::Float(b)) => Ok(Value::Float(a as f64 - b)),
            (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a - b as f64)),
            _ => Err(VmError::new("invalid operands for -")),
        }
    }

    fn vm_mul(a: Value, b: Value) -> VmResult<Value> {
        match (a, b) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a * b)),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a * b)),
            (Value::Int(a), Value::Float(b)) => Ok(Value::Float(a as f64 * b)),
            (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a * b as f64)),
            _ => Err(VmError::new("invalid operands for *")),
        }
    }

    fn vm_div(a: Value, b: Value) -> VmResult<Value> {
        match (a, b) {
            (Value::Int(a), Value::Int(b)) => {
                if b == 0 {
                    return Err(VmError::new("division by zero"));
                }
                Ok(Value::Int(a / b))
            }
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a / b)),
            (Value::Int(a), Value::Float(b)) => Ok(Value::Float(a as f64 / b)),
            (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a / b as f64)),
            _ => Err(VmError::new("invalid operands for /")),
        }
    }

    fn vm_mod(a: Value, b: Value) -> VmResult<Value> {
        match (a, b) {
            (Value::Int(a), Value::Int(b)) => {
                if b == 0 {
                    return Err(VmError::new("modulo by zero"));
                }
                Ok(Value::Int(a % b))
            }
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a % b)),
            (Value::Int(a), Value::Float(b)) => Ok(Value::Float(a as f64 % b)),
            (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a % b as f64)),
            _ => Err(VmError::new("invalid operands for %")),
        }
    }

    fn vm_pow(a: Value, b: Value) -> VmResult<Value> {
        match (a, b) {
            (Value::Int(a), Value::Int(b)) => {
                if b >= 0 {
                    Ok(Value::Int(a.pow(b as u32)))
                } else {
                    Ok(Value::Float((a as f64).powi(b as i32)))
                }
            }
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a.powf(b))),
            (Value::Int(a), Value::Float(b)) => Ok(Value::Float((a as f64).powf(b))),
            (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a.powi(b as i32))),
            _ => Err(VmError::new("invalid operands for **")),
        }
    }

    fn vm_eq(a: &Value, b: &Value) -> bool {
        match (a, b) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Null, Value::Null) => true,
            (Value::TaskHandle(a), Value::TaskHandle(b)) => a == b,
            _ => false,
        }
    }

    fn vm_neg(val: Value) -> VmResult<Value> {
        match val {
            Value::Int(n) => Ok(Value::Int(-n)),
            Value::Float(f) => Ok(Value::Float(-f)),
            _ => Err(VmError::new("cannot negate non-number")),
        }
    }

    fn vm_cmp(a: Value, b: Value, pred: impl Fn(std::cmp::Ordering) -> bool) -> VmResult<Value> {
        let ord = match (&a, &b) {
            (Value::Int(a), Value::Int(b)) => a.cmp(b),
            (Value::Float(a), Value::Float(b)) => {
                a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
            }
            (Value::Int(a), Value::Float(b)) => (*a as f64)
                .partial_cmp(b)
                .unwrap_or(std::cmp::Ordering::Equal),
            (Value::Float(a), Value::Int(b)) => a
                .partial_cmp(&(*b as f64))
                .unwrap_or(std::cmp::Ordering::Equal),
            (Value::String(a), Value::String(b)) => a.cmp(b),
            _ => return Err(VmError::new("cannot compare these values")),
        };
        Ok(Value::Bool(pred(ord)))
    }

    fn vm_bitop(a: Value, b: Value, op: fn(i64, i64) -> i64) -> VmResult<Value> {
        match (a, b) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(op(a, b))),
            _ => Err(VmError::new("bitwise operations require integers")),
        }
    }

    fn vm_index(collection: Value, index: Value) -> VmResult<Value> {
        match (&collection, &index) {
            (Value::List(list), Value::Int(i)) => {
                let idx = if *i < 0 {
                    (list.data.len() as i64 + *i) as usize
                } else {
                    *i as usize
                };
                list.data
                    .get(idx)
                    .cloned()
                    .ok_or_else(|| VmError::new("index out of bounds"))
            }
            (Value::Map(map), Value::String(key)) => {
                Ok(map.data.get(key).cloned().unwrap_or(Value::Null))
            }
            _ => Err(VmError::new("invalid index operation")),
        }
    }

    fn vm_member(obj: Value, prop: &str) -> VmResult<Value> {
        match &obj {
            Value::List(list) if prop == "length" => Ok(Value::Int(list.data.len() as i64)),
            Value::String(s) if prop == "length" => Ok(Value::Int(s.len() as i64)),
            Value::Map(map) => Ok(map.data.get(prop).cloned().unwrap_or(Value::Null)),
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
        assert!(matches!(result, Value::Int(42)));
    }

    #[test]
    fn test_vm_addition() {
        let result = run_src("fn main() { return 1 + 2; }").unwrap();
        assert!(matches!(result, Value::Int(3)));
    }

    #[test]
    fn test_vm_variables() {
        let result =
            run_src("fn main() { let x = 10; let y = 20; return x + y; }").unwrap();
        assert!(matches!(result, Value::Int(30)));
    }

    #[test]
    fn test_vm_if_true() {
        let result = run_src(
            "fn main() { let x = 0; if true { x = 1; } return x; }",
        )
        .unwrap();
        assert!(matches!(result, Value::Int(1)));
    }

    #[test]
    fn test_vm_if_false() {
        let result = run_src(
            "fn main() { let x = 0; if false { x = 1; } return x; }",
        )
        .unwrap();
        assert!(matches!(result, Value::Int(0)));
    }

    #[test]
    fn test_vm_while_loop() {
        let result = run_src(
            "fn main() { let x = 0; while x < 5 { x += 1; } return x; }",
        )
        .unwrap();
        assert!(matches!(result, Value::Int(5)));
    }

    #[test]
    fn test_vm_function_call() {
        let result = run_src(
            "fn add(a, b) { return a + b; } fn main() { return add(2, 3); }",
        )
        .unwrap();
        assert!(matches!(result, Value::Int(5)));
    }

    #[test]
    fn test_vm_recursion() {
        let result = run_src(
            "fn fib(n) { if n <= 1 { return n; } return fib(n - 1) + fib(n - 2); } fn main() { return fib(10); }",
        )
        .unwrap();
        assert!(matches!(result, Value::Int(55)));
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
        assert!(matches!(result, Value::Int(1)));
    }

    #[test]
    fn test_vm_null_coalesce() {
        let result =
            run_src("fn main() { let x = null ?? 42; return x; }").unwrap();
        assert!(matches!(result, Value::Int(42)));
    }

    #[test]
    fn test_vm_list_literal() {
        let result = run_src("fn main() { let a = [1, 2, 3]; return a.length; }").unwrap();
        assert!(matches!(result, Value::Int(3)));
    }
}