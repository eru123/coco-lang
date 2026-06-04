//! Automatic memory safety analyzer for the Coco programming language.
//!
//! Performs compile-time safety analysis on parsed Coco programs:
//! - **Definite assignment**: every variable is initialized before use.
//! - **Capture analysis**: mutable variables captured in `parallel`/`coro` blocks
//!   are rejected as data-race risks.
//! - **Unsafe block reporting**: all `unsafe { }` blocks are audited.
//! - **Iterator invalidation**: mutations of collections during iteration are detected.
//!
//! Usage:
//! ```ignore
//! use coco_parser::Parser;
//! use coco_safety::analyze;
//!
//! let mut parser = Parser::new("fn main(): int { const x = 1; return x; }");
//! let program = parser.parse_program();
//! let result = analyze(&program);
//! assert!(!result.has_errors());
//! ```

pub mod collect;
pub mod diagnostics;
pub mod env;

pub use diagnostics::{SafetyError, Severity};

use coco_syntax::Program;

/// The result of safety analysis on a program.
#[derive(Debug)]
pub struct SafetyResult {
    /// Safety errors found.
    pub errors: Vec<SafetyError>,
    /// Safety warnings found.
    pub warnings: Vec<SafetyError>,
}

impl SafetyResult {
    /// Returns true if there are any errors.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Returns true when safety analysis produced no errors.
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }

    /// Returns true if there are any warnings.
    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }

    /// Number of safety errors.
    pub fn error_count(&self) -> usize {
        self.errors.len()
    }

    /// Number of safety warnings.
    pub fn warning_count(&self) -> usize {
        self.warnings.len()
    }

    /// Total number of diagnostics.
    pub fn diagnostic_count(&self) -> usize {
        self.error_count() + self.warning_count()
    }
}

/// Run the safety analyzer on a parsed program.
///
/// Two-pass approach (mirrors `coco_typeck::check`):
/// 1. Collect: gather variable bindings and mutability info.
/// 2. Check: run all safety analysis passes.
///
/// Returns a `SafetyResult` with any errors or warnings found.
pub fn analyze(program: &Program) -> SafetyResult {
    let mut env = env::SafetyEnv::new();
    // Pass 1: collect bindings
    collect::collect_bindings(&program.items, &mut env);

    // Pass 2: safety checks (wired in subsequent tasks)

    SafetyResult {
        errors: Vec::new(),
        warnings: Vec::new(),
    }
}
