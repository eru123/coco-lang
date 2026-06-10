//! Scoped safety environment for tracking variable bindings.
//!
//! Tracks mutability and initialization state for each variable,
//! using a stack of scopes (mirroring `coco_typeck::env::TypeEnv`).

use coco_span::Span;
use std::collections::HashMap;

/// State of a variable binding in the safety environment.
#[derive(Debug, Clone)]
pub struct Binding {
    /// Whether this is a `let` (mutable) or `const` (immutable).
    pub is_mutable: bool,
    /// Whether the binding has been assigned/initialized.
    pub initialized: bool,
    /// Source span of the declaration.
    pub span: Span,
}

/// Scoped environment for safety analysis.
///
/// Tracks variable bindings across nested scopes (blocks, function bodies).
/// Each scope is a `HashMap<String, Binding>`.
#[derive(Debug)]
pub struct SafetyEnv {
    /// Stack of scopes; each scope maps variable names to their bindings.
    scopes: Vec<HashMap<String, Binding>>,
}

impl SafetyEnv {
    /// Create a new empty safety environment with one global scope.
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
        }
    }

    /// Push a new scope.
    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    /// Pop the current scope.
    pub fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    /// Scope depth.
    pub fn depth(&self) -> usize {
        self.scopes.len()
    }

    /// Define a new binding in the current scope.
    pub fn define(&mut self, name: String, is_mutable: bool, initialized: bool, span: Span) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(
                name,
                Binding {
                    is_mutable,
                    initialized,
                    span,
                },
            );
        }
    }

    /// Mark a variable as initialized.
    /// Searches from innermost to outermost scope.
    pub fn mark_initialized(&mut self, name: &str) {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(binding) = scope.get_mut(name) {
                binding.initialized = true;
                return;
            }
        }
    }

    /// Look up a variable by name, searching from innermost to outermost scope.
    pub fn lookup(&self, name: &str) -> Option<&Binding> {
        for scope in self.scopes.iter().rev() {
            if let Some(binding) = scope.get(name) {
                return Some(binding);
            }
        }
        None
    }

    /// Check if a variable is defined in any scope.
    pub fn exists(&self, name: &str) -> bool {
        self.lookup(name).is_some()
    }

    /// Take a snapshot of all bindings across all scopes.
    /// Returns a clone of the entire scope stack for later restoration.
    pub fn snapshot(&self) -> Vec<HashMap<String, Binding>> {
        self.scopes.clone()
    }

    /// Restore the environment to a previously captured snapshot.
    pub fn restore(&mut self, snapshot: &[HashMap<String, Binding>]) {
        self.scopes = snapshot.to_vec();
    }

    /// Merge two environments by intersection: after the merge,
    /// a variable is initialized only if it was initialized in BOTH
    /// the `then_branch` and `else_branch` final states.
    ///
    /// Variables that were defined in only one branch are kept but
    /// marked as not initialized (conservative).
    pub fn merge_initialized(
        &mut self,
        then_snapshot: &[HashMap<String, Binding>],
        else_snapshot: &[HashMap<String, Binding>],
    ) {
        // Build the set of initialized variable names in each branch.
        let then_init: std::collections::HashSet<String> = initialized_names(then_snapshot);
        let else_init: std::collections::HashSet<String> = initialized_names(else_snapshot);

        // Intersection: only vars initialized in BOTH branches stay initialized.
        let intersection: std::collections::HashSet<String> =
            then_init.intersection(&else_init).cloned().collect();

        // Mark all current bindings: initialized only if in intersection.
        for scope in self.scopes.iter_mut() {
            for (name, binding) in scope.iter_mut() {
                if !intersection.contains(name) {
                    binding.initialized = false;
                }
            }
        }
    }
}

/// Collect all variable names that are initialized across all scopes.
fn initialized_names(scopes: &[HashMap<String, Binding>]) -> std::collections::HashSet<String> {
    let mut names = std::collections::HashSet::new();
    for scope in scopes {
        for (name, binding) in scope {
            if binding.initialized {
                names.insert(name.clone());
            }
        }
    }
    names
}

impl Default for SafetyEnv {
    fn default() -> Self {
        Self::new()
    }
}
