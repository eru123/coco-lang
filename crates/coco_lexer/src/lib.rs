//! Lexer (tokenizer) for the Coco programming language.
//!
//! Converts source text into a stream of tokens, preserving
//! span information for error reporting and source reconstruction.

pub mod token;
pub mod lexer;

pub use token::{Token, TokenKind};
pub use lexer::Lexer;
