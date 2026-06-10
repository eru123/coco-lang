use std::collections::HashMap;

use crate::value::Value;

/// A single scope frame in the environment.
#[derive(Debug, Clone)]
struct Scope {
    bindings: HashMap<String, (Value, bool)>, // (value, is_mutable)
}

impl Scope {
    fn new() -> Self {
        Self {
            bindings: HashMap::new(),
        }
    }
}

/// Lexically scoped environment with a stack of scope frames.
#[derive(Debug, Clone)]
pub struct Environment {
    scopes: Vec<Scope>,
}

impl Default for Environment {
    fn default() -> Self {
        Self::new()
    }
}

impl Environment {
    pub fn new() -> Self {
        Self {
            scopes: vec![Scope::new()],
        }
    }

    /// Push a new scope frame.
    pub fn push_scope(&mut self) {
        self.scopes.push(Scope::new());
    }

    /// Pop the top scope frame.
    pub fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    /// Define a new variable in the current (topmost) scope.
    pub fn define(&mut self, name: String, value: Value, mutable: bool) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.bindings.insert(name, (value, mutable));
        }
    }

    /// Look up a variable by name, searching from innermost to outermost scope.
    pub fn get(&self, name: &str) -> Option<&Value> {
        for scope in self.scopes.iter().rev() {
            if let Some((val, _)) = scope.bindings.get(name) {
                return Some(val);
            }
        }
        None
    }

    /// Look up a variable only in the current (topmost) scope.
    /// Used for extracting exports from a module scope.
    pub fn get_current_scope(&self, name: &str) -> Option<Value> {
        self.scopes.last()
            .and_then(|scope| scope.bindings.get(name))
            .map(|(val, _)| val.clone())
    }

    /// Set an existing variable's value. Returns false if not found or not mutable.
    pub fn set(&mut self, name: &str, value: Value) -> Result<(), String> {
        for scope in self.scopes.iter_mut().rev() {
            if let Some((val, mutable)) = scope.bindings.get_mut(name) {
                if !*mutable {
                    return Err(format!("cannot reassign const '{}'", name));
                }
                *val = value;
                return Ok(());
            }
        }
        Err(format!("undefined variable '{}'", name))
    }
}
