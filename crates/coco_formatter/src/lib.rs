//! Source code formatter for the Coco programming language.
//!
//! Pretty-prints AST nodes back into formatted Coco source code.
//! Style: 4-space indent, ~100-char max line width, trailing commas
//! on multiline constructs, idempotent (format twice = same output).

pub mod formatter;

pub use formatter::Formatter;
