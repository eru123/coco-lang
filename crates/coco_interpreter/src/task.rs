//! Async task model and cooperative scheduler.
//!
//! The scheduler manages async tasks in the VM. Each async function call
//! creates a task with its own call frame and saved stack. `await` suspends
//! the current task and registers it to be woken when the awaited task
//! completes.
//!
//! ## States
//!
//! ```text
//! Pending → Running → Completed / Failed
//!              ↑         |
//!              └─ Await ─┘ (re-enters running when awaited task finishes)
//! ```

use std::collections::{HashMap, VecDeque};

use crate::value::Value;
use crate::vm::VmError;
#[cfg(test)]
use num_bigint::BigInt;

/// Unique task identifier.
pub type TaskId = usize;

/// Execution state of an async task.
#[derive(Debug, Clone)]
pub enum TaskState {
    /// Created but not yet started (e.g., `lazy` calls).
    Pending,
    /// Currently executing or ready to execute.
    Running,
    /// Finished successfully with a value.
    Completed(Value),
    /// Terminated with an error.
    Failed(String),
}

/// A saved execution context for a suspended async task.
#[derive(Debug, Clone)]
pub struct TaskFrame {
    /// The closure (function object) being executed.
    pub closure: Value,
    /// Instruction pointer into the chunk's code.
    pub ip: usize,
    /// Base index in the VM's value stack for this frame's locals.
    pub stack_offset: usize,
}

/// An async task managed by the scheduler.
#[derive(Debug, Clone)]
pub struct Task {
    /// Unique task id.
    pub id: TaskId,
    /// Current state.
    pub state: TaskState,
    /// The saved call frame for resumption.
    pub frame: TaskFrame,
    /// Saved operand stack at suspension point.
    pub stack: Vec<Value>,
    /// IDs of tasks that are awaiting this one.
    pub awaiters: Vec<TaskId>,
    /// The task this task is currently awaiting (if suspended).
    pub awaited_task: Option<TaskId>,
}

/// Cooperative async task scheduler.
///
/// The scheduler is embedded in the VM. It manages all async tasks and
/// runs them cooperatively — a task yields only at `await` points.
pub struct TaskScheduler {
    /// All tasks, keyed by id.
    tasks: HashMap<TaskId, Task>,
    /// Queue of task ids that are ready to run.
    ready_queue: VecDeque<TaskId>,
    /// Next task id to assign.
    next_id: TaskId,
    /// The root (top-level) task id — the scheduler stops when this completes.
    root_id: Option<TaskId>,
}

impl TaskScheduler {
    /// Create a new empty scheduler.
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
            ready_queue: VecDeque::new(),
            next_id: 0,
            root_id: None,
        }
    }

    /// Allocate a new task id.
    fn alloc_id(&mut self) -> TaskId {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Create a new task and enqueue it as ready.
    ///
    /// Returns the task id for use as an async handle (promise).
    pub fn spawn(
        &mut self,
        closure: Value,
        ip: usize,
        stack_offset: usize,
        stack: Vec<Value>,
    ) -> TaskId {
        let id = self.alloc_id();
        let task = Task {
            id,
            state: TaskState::Running,
            frame: TaskFrame {
                closure,
                ip,
                stack_offset,
            },
            stack,
            awaiters: Vec::new(),
            awaited_task: None,
        };
        self.tasks.insert(id, task);
        self.ready_queue.push_back(id);
        id
    }

    /// Mark the root task. The scheduler loop stops when the root completes.
    pub fn set_root(&mut self, id: TaskId) {
        self.root_id = Some(id);
    }

    /// Get a mutable reference to a task.
    pub fn get_mut(&mut self, id: TaskId) -> Option<&mut Task> {
        self.tasks.get_mut(&id)
    }

    /// Get an immutable reference to a task.
    pub fn get(&self, id: TaskId) -> Option<&Task> {
        self.tasks.get(&id)
    }

    /// Iterate over all tasks (used by the tracing GC for root discovery).
    pub fn tasks(&self) -> impl Iterator<Item = &Task> {
        self.tasks.values()
    }

    /// Dequeue the next ready task, or None if the scheduler is done.
    pub fn dequeue(&mut self) -> Option<TaskId> {
        self.ready_queue.pop_front()
    }

    /// Mark a task as complete and wake its awaiters.
    pub fn complete(&mut self, id: TaskId, value: Value) {
        if let Some(task) = self.tasks.get_mut(&id) {
            task.state = TaskState::Completed(value.clone());
            // Wake all awaiters.
            let awaiters: Vec<TaskId> = task.awaiters.drain(..).collect();
            for awaiter_id in awaiters {
                if let Some(awaiter) = self.tasks.get_mut(&awaiter_id) {
                    awaiter.state = TaskState::Running;
                }
                self.ready_queue.push_back(awaiter_id);
            }
        }
    }

    /// Mark a task as failed and wake its awaiters with the error.
    pub fn fail(&mut self, id: TaskId, error: String) {
        if let Some(task) = self.tasks.get_mut(&id) {
            task.state = TaskState::Failed(error.clone());
            let awaiters: Vec<TaskId> = task.awaiters.drain(..).collect();
            for awaiter_id in awaiters {
                if let Some(awaiter) = self.tasks.get_mut(&awaiter_id) {
                    awaiter.state = TaskState::Running;
                }
                self.ready_queue.push_back(awaiter_id);
            }
        }
    }

    /// Suspend the current task waiting on another task.
    ///
    /// `current_id` is the task that is calling `await`.
    /// `target_id` is the task being awaited.
    pub fn suspend_awaiting(&mut self, current_id: TaskId, target_id: TaskId) {
        if let Some(current) = self.tasks.get_mut(&current_id) {
            current.state = TaskState::Pending;
            current.awaited_task = Some(target_id);
        }
        if let Some(target) = self.tasks.get_mut(&target_id) {
            // If target is already complete, immediately re-enqueue current.
            match &target.state {
                TaskState::Completed(_) | TaskState::Failed(_) => {
                    if let Some(current) = self.tasks.get_mut(&current_id) {
                        current.state = TaskState::Running;
                    }
                    self.ready_queue.push_back(current_id);
                    return;
                }
                _ => {}
            }
            target.awaiters.push(current_id);
        } else {
            // Target doesn't exist — treat as immediate failure.
            if let Some(current) = self.tasks.get_mut(&current_id) {
                current.state = TaskState::Failed(format!("awaited task {} not found", target_id));
            }
            self.ready_queue.push_back(current_id);
        }
    }

    /// Save the suspended task's state (frame, stack).
    pub fn save_suspended_state(
        &mut self,
        id: TaskId,
        closure: Value,
        ip: usize,
        stack_offset: usize,
        stack: Vec<Value>,
    ) {
        if let Some(task) = self.tasks.get_mut(&id) {
            task.frame = TaskFrame {
                closure,
                ip,
                stack_offset,
            };
            task.stack = stack;
        }
    }

    /// Check whether the root task has completed or failed.
    /// Returns Some(value) if completed, Some(Err) if failed, None if still running.
    pub fn root_result(&self) -> Option<Result<Value, VmError>> {
        let root_id = self.root_id?;
        let task = self.tasks.get(&root_id)?;
        match &task.state {
            TaskState::Completed(val) => Some(Ok(val.clone())),
            TaskState::Failed(err) => Some(Err(VmError::new(err.clone()))),
            _ => None,
        }
    }

    /// Check whether the root task has failed.
    pub fn root_failed(&self) -> Option<String> {
        let root_id = self.root_id?;
        let task = self.tasks.get(&root_id)?;
        match &task.state {
            TaskState::Failed(err) => Some(err.clone()),
            _ => None,
        }
    }

    /// How many tasks are still pending/running?
    pub fn active_count(&self) -> usize {
        self.tasks
            .values()
            .filter(|t| matches!(t.state, TaskState::Pending | TaskState::Running))
            .count()
    }

    /// Clear a task and its resources after it's no longer needed.
    pub fn remove(&mut self, id: TaskId) {
        self.tasks.remove(&id);
    }
}

impl Default for TaskScheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Value;

    #[test]
    fn test_spawn_and_complete() {
        let mut sched = TaskScheduler::new();
        let id = sched.spawn(Value::Null, 0, 0, vec![]);
        sched.set_root(id);

        // Task should be enqueued.
        assert_eq!(sched.dequeue(), Some(id));
        assert_eq!(sched.active_count(), 1);

        // Complete it.
        sched.complete(id, Value::Int(BigInt::from(42)));
        assert!(matches!(sched.root_result(), Some(Ok(Value::Int(n))) if n == BigInt::from(42)));
    }

    #[test]
    fn test_await_wakes_on_completion() {
        let mut sched = TaskScheduler::new();

        // Spawn a child task.
        let child_id = sched.spawn(Value::Null, 0, 0, vec![]);
        // Spawn the root task that will await the child.
        let root_id = sched.spawn(Value::Null, 0, 0, vec![]);
        sched.set_root(root_id);

        // Both tasks are initially enqueued — dequeue both first.
        assert_eq!(sched.dequeue(), Some(child_id));
        assert_eq!(sched.dequeue(), Some(root_id));

        // Now suspend root awaiting child.
        sched.suspend_awaiting(root_id, child_id);

        // Root should no longer be in ready queue.
        assert_eq!(sched.dequeue(), None);
        assert_eq!(sched.active_count(), 2); // both pending

        // Complete child → should wake root.
        sched.complete(child_id, Value::Int(BigInt::from(7)));
        assert_eq!(sched.dequeue(), Some(root_id));
        sched.complete(root_id, Value::Int(BigInt::from(7)));
        assert!(matches!(sched.root_result(), Some(Ok(Value::Int(n))) if n == BigInt::from(7)));
    }

    #[test]
    fn test_task_failure_propagates() {
        let mut sched = TaskScheduler::new();
        let root_id = sched.spawn(Value::Null, 0, 0, vec![]);
        sched.set_root(root_id);
        sched.fail(root_id, "boom".to_string());
        assert_eq!(sched.root_failed(), Some("boom".to_string()));
    }
}
