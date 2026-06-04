//! Type environment with scoped bindings and function signatures.

use std::collections::HashMap;

use crate::types::Ty;

/// A function signature used during type checking.
#[derive(Debug, Clone)]
pub struct FnSig {
    pub params: Vec<Ty>,
    pub ret: Ty,
    /// Whether this function has any type annotations at all.
    pub is_typed: bool,
}

/// A scoped type environment.
#[derive(Debug)]
pub struct TypeEnv {
    /// Stack of scopes; each scope maps variable names to types.
    scopes: Vec<HashMap<String, Ty>>,
    /// Registered function signatures (by name).
    functions: HashMap<String, FnSig>,
}

impl TypeEnv {
    pub fn new() -> Self {
        let mut env = Self {
            scopes: vec![HashMap::new()],
            functions: HashMap::new(),
        };
        env.register_builtins();
        env
    }

    fn register_builtins(&mut self) {
        // print(...) -> void — accepts any number of mixed args
        self.functions.insert(
            "print".to_string(),
            FnSig {
                params: vec![Ty::Mixed],
                ret: Ty::Void,
                is_typed: true,
            },
        );

        // len(x: mixed) -> int
        self.functions.insert(
            "len".to_string(),
            FnSig {
                params: vec![Ty::Mixed],
                ret: Ty::Int,
                is_typed: true,
            },
        );

        // toString(x: mixed) -> string
        self.functions.insert(
            "toString".to_string(),
            FnSig {
                params: vec![Ty::Mixed],
                ret: Ty::String,
                is_typed: true,
            },
        );

        // parseInt(x: string) -> int
        self.functions.insert(
            "parseInt".to_string(),
            FnSig {
                params: vec![Ty::String],
                ret: Ty::Int,
                is_typed: true,
            },
        );

        // parseFloat(x: string) -> float
        self.functions.insert(
            "parseFloat".to_string(),
            FnSig {
                params: vec![Ty::String],
                ret: Ty::Float,
                is_typed: true,
            },
        );
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

    /// Define a variable in the current scope.
    pub fn define(&mut self, name: String, ty: Ty) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, ty);
        }
    }

    /// Look up a variable by name, searching from innermost to outermost scope.
    pub fn lookup(&self, name: &str) -> Option<&Ty> {
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Some(ty);
            }
        }
        None
    }

    /// Register a function signature.
    pub fn register_fn(&mut self, name: String, sig: FnSig) {
        self.functions.insert(name, sig);
    }

    /// Look up a function signature by name.
    pub fn lookup_fn(&self, name: &str) -> Option<&FnSig> {
        self.functions.get(name)
    }
}

impl Default for TypeEnv {
    fn default() -> Self {
        Self::new()
    }
}
