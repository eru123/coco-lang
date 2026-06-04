//! Lexer (tokenizer) for the Coco programming language.
//!
//! Converts source text into a stream of tokens, preserving
//! span information for error reporting and source reconstruction.

pub mod lexer;
pub mod token;

pub use lexer::Lexer;
pub use token::{Token, TokenKind};
