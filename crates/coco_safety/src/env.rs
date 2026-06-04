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
}

impl Default for SafetyEnv {
    fn default() -> Self {
        Self::new()
    }
}
