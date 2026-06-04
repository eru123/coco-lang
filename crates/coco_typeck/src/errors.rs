//! Type checking error types.

use coco_span::Span;
use std::fmt;

/// Severity of a type checker diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// A type checking error or warning.
#[derive(Debug, Clone)]
pub struct TypeckError {
    /// Error code (e.g., "T001").
    pub code: &'static str,
    /// Human-readable message.
    pub message: String,
    /// Source span where the error occurred.
    pub span: Span,
    /// Severity level.
    pub severity: Severity,
}

impl TypeckError {
    pub fn new(code: &'static str, message: String, span: Span, severity: Severity) -> Self {
        Self {
            code,
            message,
            span,
            severity,
        }
    }

    /// T001: Type mismatch.
    pub fn type_mismatch(expected: &str, got: &str, span: Span) -> Self {
        Self::new(
            "T001",
            format!("type mismatch: expected `{}`, got `{}`", expected, got),
            span,
            Severity::Error,
        )
    }

    /// T002: Argument count mismatch.
    pub fn arg_count(expected: usize, got: usize, span: Span) -> Self {
        Self::new(
            "T002",
            format!(
                "argument count mismatch: expected {}, got {}",
                expected, got
            ),
            span,
            Severity::Error,
        )
    }

    /// T003: Undefined variable.
    pub fn undefined_var(name: &str, span: Span) -> Self {
        Self::new(
            "T003",
            format!("undefined variable `{}`", name),
            span,
            Severity::Error,
        )
    }

    /// T004: Null access.
    pub fn null_access(span: Span) -> Self {
        Self::new(
            "T004",
            "possible null access".to_string(),
            span,
            Severity::Error,
        )
    }

    /// T005: Missing return.
    pub fn missing_return(span: Span) -> Self {
        Self::new(
            "T005",
            "missing return value in function with non-void return type".to_string(),
            span,
            Severity::Error,
        )
    }

    /// T006: Incompatible operands.
    pub fn incompatible_operands(op: &str, left: &str, right: &str, span: Span) -> Self {
        Self::new(
            "T006",
            format!(
                "incompatible operands for `{}`: `{}` and `{}`",
                op, left, right
            ),
            span,
            Severity::Error,
        )
    }

    /// T007: Property not found.
    pub fn property_not_found(type_name: &str, prop: &str, span: Span) -> Self {
        Self::new(
            "T007",
            format!("property `{}` not found on type `{}`", prop, type_name),
            span,
            Severity::Error,
        )
    }
}

impl fmt::Display for TypeckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}
