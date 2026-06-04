//! Safety analysis diagnostics for the Coco programming language.
//!
//! Provides structured error and warning types for memory safety violations
//! detected at compile time.

use coco_span::Span;
use std::fmt;

/// Severity of a safety diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// A safety analysis diagnostic.
///
/// Codes:
/// - S001: Use of possibly uninitialized variable
/// - S002: Mutable variable captured across parallel/coro boundary
/// - S003: Unsafe block used
/// - S004: Collection mutated while being iterated
/// - S005: (reserved)
/// - S006: (reserved)
#[derive(Debug, Clone)]
pub struct SafetyError {
    pub code: &'static str,
    pub message: String,
    pub span: Span,
    pub severity: Severity,
}

impl SafetyError {
    pub fn new(code: &'static str, message: String, span: Span, severity: Severity) -> Self {
        Self {
            code,
            message,
            span,
            severity,
        }
    }

    /// S001: Variable may be used before initialization.
    pub fn uninitialized_var(name: &str, span: Span) -> Self {
        Self::new(
            "S001",
            format!("variable `{}` may be used before it is initialized", name),
            span,
            Severity::Error,
        )
    }

    /// S002: Mutable variable captured across parallel/coro boundary.
    pub fn mutable_capture(name: &str, context: &str, span: Span) -> Self {
        Self::new(
            "S002",
            format!(
                "mutable variable `{}` captured across `{}` boundary — data race risk",
                name, context
            ),
            span,
            Severity::Error,
        )
    }

    /// S003: Unsafe block used.
    pub fn unsafe_block_used(span: Span) -> Self {
        Self::new(
            "S003",
            "unsafe block used — memory safety guarantees do not apply inside this block"
                .to_string(),
            span,
            Severity::Warning,
        )
    }

    /// S004: Collection mutated during iteration.
    pub fn iterator_invalidation(name: &str, span: Span) -> Self {
        Self::new(
            "S004",
            format!(
                "collection `{}` is mutated while being iterated — this may cause runtime errors",
                name
            ),
            span,
            Severity::Warning,
        )
    }
}

impl fmt::Display for SafetyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}
