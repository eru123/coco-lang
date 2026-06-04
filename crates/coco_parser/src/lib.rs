//! Parser for the Coco programming language.
//!
//! Converts a token stream into an AST using recursive descent
//! for declarations and statements, and Pratt parsing for expressions.
//! Includes error recovery: after a syntax error, synchronizes to
//! the next known sync point and continues parsing.

pub mod parser;
pub mod expr;

pub use parser::Parser;
