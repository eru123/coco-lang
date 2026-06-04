//! Type checker for the Coco programming language.
//!
//! Implements gradual type checking:
//! - Annotated code (with type annotations) is fully checked.
//! - Unannotated code produces no type errors (treated as `mixed`).
//!
//! Usage:
//! ```ignore
//! use coco_parser::Parser;
//! use coco_typeck::check;
//!
//! let mut parser = Parser::new("fn add(a: int, b: int): int { return a + b; }");
//! let program = parser.parse_program();
//! let result = check(&program);
//! assert!(!result.has_errors());
//! ```

pub mod check_expr;
pub mod check_item;
pub mod check_stmt;
pub mod convert;
pub mod env;
pub mod errors;
pub mod infer;
pub mod types;
pub mod unify;

use coco_syntax::Program;
use errors::TypeckError;

/// The result of type checking a program.
#[derive(Debug)]
pub struct TypeckResult {
    /// Type errors found.
    pub errors: Vec<TypeckError>,
    /// Type warnings found.
    pub warnings: Vec<TypeckError>,
}

impl TypeckResult {
    /// Returns true if there are any errors.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Returns true when type checking produced no errors.
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }

    /// Returns true if there are any warnings.
    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }

    /// Number of type errors.
    pub fn error_count(&self) -> usize {
        self.errors.len()
    }

    /// Number of type warnings.
    pub fn warning_count(&self) -> usize {
        self.warnings.len()
    }

    /// Total number of errors and warnings.
    pub fn diagnostic_count(&self) -> usize {
        self.error_count() + self.warning_count()
    }

    /// First type error, if any.
    pub fn primary_error(&self) -> Option<&TypeckError> {
        self.errors.first()
    }
}

/// Run the type checker on a parsed program.
///
/// Two-pass approach:
/// 1. Collect: gather function signatures and top-level bindings.
/// 2. Check: validate function bodies and expressions.
pub fn check(program: &Program) -> TypeckResult {
    let mut env = env::TypeEnv::new();
    let mut all_errors: Vec<TypeckError> = Vec::new();

    // Pass 1: collect signatures
    check_item::collect_items(&program.items, &mut env);

    // Pass 2: check bodies
    check_item::check_items(&program.items, &mut env, &mut all_errors);

    // Separate errors from warnings
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    for e in all_errors {
        match e.severity {
            errors::Severity::Error => errors.push(e),
            errors::Severity::Warning => warnings.push(e),
        }
    }

    TypeckResult { errors, warnings }
}
