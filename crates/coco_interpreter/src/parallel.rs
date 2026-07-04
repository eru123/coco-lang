//! Real parallel execution for `parallel { run expr; ... }` blocks.
//!
//! Each `run` clause is a `FnObj` (plus args) executed on its own OS thread
//! via `std::thread::scope`. Each worker builds a fresh `Vm` seeded with the
//! parent's globals (shared read-only — `Value` is `Send + Sync`), runs the
//! function via `Vm::call_function`, and returns the result. The block joins
//! all workers before continuing and yields the last run's result (matching
//! the existing sequential semantics).

use std::collections::HashMap;
use std::thread;

use crate::ir::FnObj;
use crate::value::Value;
use crate::vm::{Vm, VmError};

/// A unit of parallel work: a callable and its arguments.
pub struct ParallelRun {
    pub callee: FnObj,
    pub args: Vec<Value>,
}

/// Execute `runs` concurrently on OS threads, joining before return.
///
/// `parent_globals` is shared read-only across workers so they can resolve
/// free names. Each worker gets its own stack.
///
/// Returns the result of the *last* run (matching the existing `parallel`
/// semantics where the final expression's value is the block's value), or
/// `Null` if there are no runs. If any worker errors, the first error wins.
pub fn parallel_join(
    runs: Vec<ParallelRun>,
    parent_globals: &HashMap<String, Value>,
) -> Result<Value, VmError> {
    if runs.is_empty() {
        return Ok(Value::Null);
    }

    let results: Vec<Result<Value, String>> = thread::scope(|s| {
        let handles: Vec<_> = runs
            .into_iter()
            .map(|run| s.spawn(move || execute_run(run, parent_globals)))
            .collect();

        handles
            .into_iter()
            .map(|h| {
                h.join()
                    .unwrap_or(Err("worker thread panicked".to_string()))
            })
            .collect()
    });

    let mut last_ok = Value::Null;
    for r in results {
        match r {
            Ok(v) => last_ok = v,
            Err(e) => return Err(VmError::new(format!("parallel run failed: {}", e))),
        }
    }
    Ok(last_ok)
}

/// Run a single `ParallelRun` on a fresh VM seeded with the parent globals.
fn execute_run(run: ParallelRun, parent_globals: &HashMap<String, Value>) -> Result<Value, String> {
    let mut vm = Vm::new();
    vm.set_globals(parent_globals.clone());
    vm.call_function(run.callee, run.args)
        .map_err(|e| e.message)
}
