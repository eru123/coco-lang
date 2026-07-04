//! Type environment with scoped bindings and function signatures.

use std::cell::RefCell;
use std::collections::HashMap;

use coco_span::Span;

use crate::typemap::TypeMap;
use crate::types::Ty;

/// A function signature used during type checking.
#[derive(Debug, Clone)]
pub struct FnSig {
    pub params: Vec<Ty>,
    pub ret: Ty,
    /// Whether this function has any type annotations at all.
    pub is_typed: bool,
    /// Generic type parameter names (e.g., ["T", "U"] for `fn foo<T, U>(...)`).
    pub type_params: Vec<String>,
}

/// Kinds of named type shapes Coco can type-check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeKind {
    Class,
    Interface,
    Trait,
}

/// Properties, methods, and constructor signature for a named type.
#[derive(Debug, Clone)]
pub struct TypeShape {
    pub name: String,
    pub kind: TypeKind,
    pub properties: HashMap<String, Ty>,
    pub methods: HashMap<String, FnSig>,
    pub constructor: Option<FnSig>,
}

impl TypeShape {
    pub fn new(name: String, kind: TypeKind) -> Self {
        Self {
            name,
            kind,
            properties: HashMap::new(),
            methods: HashMap::new(),
            constructor: None,
        }
    }

    pub fn member_type(&self, name: &str) -> Option<Ty> {
        if let Some(ty) = self.properties.get(name) {
            return Some(ty.clone());
        }
        self.methods.get(name).map(|sig| Ty::Function {
            params: sig.params.clone(),
            ret: Box::new(sig.ret.clone()),
        })
    }
}

/// A scoped type environment.
#[derive(Debug)]
pub struct TypeEnv {
    /// Stack of scopes; each scope maps variable names to types.
    scopes: Vec<HashMap<String, Ty>>,
    /// Registered function signatures (by name).
    functions: HashMap<String, FnSig>,
    /// Registered nominal type shapes (classes, interfaces, traits).
    shapes: HashMap<String, TypeShape>,
    /// Current `this`/`$` type stack while checking class or trait bodies.
    self_stack: Vec<Ty>,
    /// Inferred types keyed by AST node span, recorded during inference so the
    /// bytecode compiler can specialize arithmetic on statically-known types.
    /// Interior-mutable so `infer_expr` (which takes `&TypeEnv`) can record
    /// into it.
    types: RefCell<TypeMap>,
}

impl TypeEnv {
    pub fn new() -> Self {
        let mut env = Self {
            scopes: vec![HashMap::new()],
            functions: HashMap::new(),
            shapes: HashMap::new(),
            self_stack: Vec::new(),
            types: RefCell::new(TypeMap::new()),
        };
        env.register_builtins();
        env
    }

    /// Record the inferred type for the node at `span`. Called from
    /// `infer_expr` for every expression, building the type map the bytecode
    /// compiler uses for arithmetic specialization.
    pub fn record_type(&self, span: Span, ty: Ty) {
        self.types.borrow_mut().insert(span, ty);
    }

    /// Take the recorded type map out of the environment (replacing it with an
    /// empty one). Used by `check()` to return the map in `TypeckResult`.
    pub fn take_types(&self) -> TypeMap {
        std::mem::take(&mut *self.types.borrow_mut())
    }

    fn register_builtins(&mut self) {
        // print(...) -> void — accepts any number of mixed args
        self.functions.insert(
            "print".to_string(),
            FnSig {
                params: vec![Ty::Mixed],
                ret: Ty::Void,
                is_typed: true,
                type_params: vec![],
            },
        );

        // len(x: mixed) -> int
        self.functions.insert(
            "len".to_string(),
            FnSig {
                params: vec![Ty::Mixed],
                ret: Ty::Int,
                is_typed: true,
                type_params: vec![],
            },
        );

        // toString(x: mixed) -> string
        self.functions.insert(
            "toString".to_string(),
            FnSig {
                params: vec![Ty::Mixed],
                ret: Ty::String,
                is_typed: true,
                type_params: vec![],
            },
        );

        // parseInt(x: string) -> int
        self.functions.insert(
            "parseInt".to_string(),
            FnSig {
                params: vec![Ty::String],
                ret: Ty::Int,
                is_typed: true,
                type_params: vec![],
            },
        );

        // parseFloat(x: string) -> float
        self.functions.insert(
            "parseFloat".to_string(),
            FnSig {
                params: vec![Ty::String],
                ret: Ty::Float,
                is_typed: true,
                type_params: vec![],
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

    /// Assign an existing variable if it is already in scope, otherwise define it locally.
    pub fn assign(&mut self, name: &str, ty: Ty) {
        for scope in self.scopes.iter_mut().rev() {
            if scope.contains_key(name) {
                scope.insert(name.to_string(), ty);
                return;
            }
        }
        self.define(name.to_string(), ty);
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

    /// Register a named type shape.
    pub fn register_shape(&mut self, shape: TypeShape) {
        self.shapes.insert(shape.name.clone(), shape);
    }

    /// Look up a named type shape.
    pub fn lookup_shape(&self, name: &str) -> Option<&TypeShape> {
        self.shapes.get(name)
    }

    /// Register an enum type.
    pub fn register_enum(&mut self, name: String, variants: Vec<String>) {
        let ty = crate::types::Ty::Enum(name.clone(), variants);
        self.define(name, ty);
    }

    /// Look up an enum type by name.
    pub fn lookup_enum(&self, name: &str) -> Option<crate::types::Ty> {
        self.lookup(name).cloned()
    }

    /// Push a current `this`/`$` type.
    pub fn push_self(&mut self, ty: Ty) {
        self.self_stack.push(ty);
    }

    /// Pop the current `this`/`$` type.
    pub fn pop_self(&mut self) {
        self.self_stack.pop();
    }

    /// Return the current `this`/`$` type, if any.
    pub fn current_self(&self) -> Option<&Ty> {
        self.self_stack.last()
    }
}

impl Default for TypeEnv {
    fn default() -> Self {
        Self::new()
    }
}
