# Coco Phase 2: Lexer, Parser, Formatter — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement lexer, parser, and formatter in Rust. Produce a working CLI tool (`coco lex`, `coco parse`, `coco fmt`) that works on all 21 example programs.

**Architecture:** Rust workspace with 7 crates. Recursive descent parser with Pratt parsing for expressions. Lossless AST preserving all source info. Beautiful error messages with ariadne.

**Tech Stack:** Rust, ariadne, unicode-xid, clap

---

## Project Structure

```
coco-lang/
├── Cargo.toml                 # Workspace manifest
├── crates/
│   ├── coco_span/            # Source location tracking
│   ├── coco_diagnostics/     # Error reporting
│   ├── coco_lexer/           # Tokenization
│   ├── coco_syntax/          # AST definitions
│   ├── coco_parser/          # Parser implementation
│   ├── coco_formatter/       # Pretty-printer
│   └── coco_cli/             # CLI tool
└── examples/                  # 21 .co test files
```

---

## Task 1: Workspace Setup

**Files:**
- Create: `Cargo.toml` (workspace root)
- Create: `.gitignore`
- Create: `rust-toolchain.toml`

- [ ] **Step 1: Create workspace Cargo.toml**

```toml
[workspace]
members = [
    "crates/coco_span",
    "crates/coco_diagnostics",
    "crates/coco_lexer",
    "crates/coco_syntax",
    "crates/coco_parser",
    "crates/coco_formatter",
    "crates/coco_cli",
]
resolver = "2"

[workspace.package]
version = "0.1.0"
edition = "2021"
authors = ["Coco Language Team"]
license = "MIT"
repository = "https://github.com/your-org/coco-lang"

[workspace.dependencies]
# Internal crates
coco_span = { path = "crates/coco_span" }
coco_diagnostics = { path = "crates/coco_diagnostics" }
coco_lexer = { path = "crates/coco_lexer" }
coco_syntax = { path = "crates/coco_syntax" }
coco_parser = { path = "crates/coco_parser" }
coco_formatter = { path = "crates/coco_formatter" }

# External dependencies
ariadne = "0.4"
unicode-xid = "0.2"
clap = { version = "4.5", features = ["derive"] }

[profile.release]
opt-level = 3
lto = true
codegen-units = 1
```

- [ ] **Step 2: Create .gitignore**

```gitignore
/target/
**/*.rs.bk
*.pdb
Cargo.lock

# IDE
.vscode/
.idea/
*.swp
*.swo
*~
.DS_Store
```

- [ ] **Step 3: Create rust-toolchain.toml**

```toml
[toolchain]
channel = "stable"
components = ["rustfmt", "clippy"]
```

- [ ] **Step 4: Create crates/ directory**

```bash
mkdir -p crates
```

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml .gitignore rust-toolchain.toml crates/
git commit -m "feat(phase-2): setup Rust workspace structure"
```

---

## Task 2: coco_span — Source Location Tracking

**Files:**
- Create: `crates/coco_span/Cargo.toml`
- Create: `crates/coco_span/src/lib.rs`
- Create: `crates/coco_span/tests/span_tests.rs`

- [ ] **Step 1: Create crates/coco_span/Cargo.toml**

```toml
[package]
name = "coco_span"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[dependencies]
# No external dependencies
```

- [ ] **Step 2: Create crates/coco_span/src/lib.rs**

```rust
//! Source location tracking for the Coco compiler.
//!
//! This crate provides types for tracking locations in source files:
//! - `Span`: byte range in a source file
//! - `Location`: line and column position
//! - `SourceFile`: file content and metadata
//! - `SourceMap`: registry of all loaded files

use std::path::PathBuf;

/// A byte range in a source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub fn len(&self) -> usize {
        self.end - start
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    pub fn merge(&self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

/// A line and column position in a source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Location {
    pub line: usize,    // 1-based
    pub column: usize,  // 1-based
}

impl Location {
    pub fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }
}

/// A unique identifier for a source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileId(pub usize);

/// A source file with its content.
#[derive(Debug, Clone)]
pub struct SourceFile {
    pub id: FileId,
    pub path: PathBuf,
    pub content: String,
    line_starts: Vec<usize>,
}

impl SourceFile {
    pub fn new(id: FileId, path: PathBuf, content: String) -> Self {
        let line_starts = Self::compute_line_starts(&content);
        Self {
            id,
            path,
            content,
            line_starts,
        }
    }

    fn compute_line_starts(content: &str) -> Vec<usize> {
        let mut starts = vec![0];
        for (i, ch) in content.char_indices() {
            if ch == '\n' {
                starts.push(i + 1);
            }
        }
        starts
    }

    pub fn get_location(&self, offset: usize) -> Location {
        let line = match self.line_starts.binary_search(&offset) {
            Ok(line) => line,
            Err(line) => line - 1,
        };
        let line_start = self.line_starts[line];
        let column = offset - line_start;
        Location::new(line + 1, column + 1)
    }

    pub fn get_line(&self, line: usize) -> Option<&str> {
        if line == 0 || line > self.line_starts.len() {
            return None;
        }
        let start = self.line_starts[line - 1];
        let end = self.line_starts.get(line).copied().unwrap_or(self.content.len());
        Some(&self.content[start..end])
    }

    pub fn get_span_text(&self, span: Span) -> &str {
        &self.content[span.start..span.end]
    }
}

/// Registry of all loaded source files.
#[derive(Debug, Default)]
pub struct SourceMap {
    files: Vec<SourceFile>,
}

impl SourceMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_file(&mut self, path: PathBuf, content: String) -> FileId {
        let id = FileId(self.files.len());
        let file = SourceFile::new(id, path, content);
        self.files.push(file);
        id
    }

    pub fn get_file(&self, id: FileId) -> Option<&SourceFile> {
        self.files.get(id.0)
    }

    pub fn get_location(&self, file: FileId, offset: usize) -> Option<Location> {
        self.get_file(file).map(|f| f.get_location(offset))
    }

    pub fn get_span_text(&self, file: FileId, span: Span) -> Option<&str> {
        self.get_file(file).map(|f| f.get_span_text(span))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span_merge() {
        let s1 = Span::new(5, 10);
        let s2 = Span::new(8, 15);
        assert_eq!(s1.merge(s2), Span::new(5, 15));
    }

    #[test]
    fn test_location() {
        let content = "hello\nworld\nfoo";
        let file = SourceFile::new(FileId(0), PathBuf::from("test.co"), content.to_string());
        
        assert_eq!(file.get_location(0), Location::new(1, 1));   // 'h'
        assert_eq!(file.get_location(5), Location::new(1, 6));   // '\n'
        assert_eq!(file.get_location(6), Location::new(2, 1));   // 'w'
        assert_eq!(file.get_location(12), Location::new(3, 1));  // 'f'
    }

    #[test]
    fn test_get_line() {
        let content = "line 1\nline 2\nline 3";
        let file = SourceFile::new(FileId(0), PathBuf::from("test.co"), content.to_string());
        
        assert_eq!(file.get_line(1), Some("line 1\n"));
        assert_eq!(file.get_line(2), Some("line 2\n"));
        assert_eq!(file.get_line(3), Some("line 3"));
        assert_eq!(file.get_line(0), None);
        assert_eq!(file.get_line(4), None);
    }
}
```

- [ ] **Step 3: Create crates/coco_span/tests/span_tests.rs**

```rust
use coco_span::*;
use std::path::PathBuf;

#[test]
fn test_source_map() {
    let mut map = SourceMap::new();
    
    let id1 = map.add_file(PathBuf::from("a.co"), "hello\nworld".to_string());
    let id2 = map.add_file(PathBuf::from("b.co"), "foo\nbar".to_string());
    
    assert_eq!(id1, FileId(0));
    assert_eq!(id2, FileId(1));
    
    let loc = map.get_location(id1, 6).unwrap();
    assert_eq!(loc, Location::new(2, 1));
    
    let text = map.get_span_text(id2, Span::new(0, 3)).unwrap();
    assert_eq!(text, "foo");
}
```

- [ ] **Step 4: Test**

```bash
cd crates/coco_span && cargo test
```

- [ ] **Step 5: Commit**

```bash
git add crates/coco_span/
git commit -m "feat(span): implement source location tracking"
```

---

## Task 3: coco_diagnostics — Error Reporting

**Files:**
- Create: `crates/coco_diagnostics/Cargo.toml`
- Create: `crates/coco_diagnostics/src/lib.rs`
- Create: `crates/coco_diagnostics/tests/diagnostic_tests.rs`

- [ ] **Step 1: Create crates/coco_diagnostics/Cargo.toml**

```toml
[package]
name = "coco_diagnostics"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[dependencies]
coco_span = { workspace = true }
ariadne = { workspace = true }
```

- [ ] **Step 2: Create crates/coco_diagnostics/src/lib.rs**

```rust
//! Beautiful error reporting for the Coco compiler.
//!
//! Uses `ariadne` to emit colored, annotated diagnostics with source context.

use coco_span::{FileId, SourceFile, SourceMap, Span};
use ariadne::{Color, Label, Report, ReportKind, Source};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticLevel {
    Error,
    Warning,
    Note,
}

#[derive(Debug, Clone)]
pub struct DiagnosticLabel {
    pub span: Span,
    pub message: String,
    pub is_primary: bool,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub level: DiagnosticLevel,
    pub message: String,
    pub file: FileId,
    pub labels: Vec<DiagnosticLabel>,
    pub notes: Vec<String>,
}

impl Diagnostic {
    pub fn error(file: FileId, message: impl Into<String>) -> Self {
        Self {
            level: DiagnosticLevel::Error,
            message: message.into(),
            file,
            labels: Vec::new(),
            notes: Vec::new(),
        }
    }

    pub fn warning(file: FileId, message: impl Into<String>) -> Self {
        Self {
            level: DiagnosticLevel::Warning,
            message: message.into(),
            file,
            labels: Vec::new(),
            notes: Vec::new(),
        }
    }

    pub fn with_label(mut self, span: Span, message: impl Into<String>, is_primary: bool) -> Self {
        self.labels.push(DiagnosticLabel {
            span,
            message: message.into(),
            is_primary,
        });
        self
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    pub fn emit(&self, source_map: &SourceMap) {
        let source_file = source_map.get_file(self.file).expect("file not found");
        
        let kind = match self.level {
            DiagnosticLevel::Error => ReportKind::Error,
            DiagnosticLevel::Warning => ReportKind::Warning,
            DiagnosticLevel::Note => ReportKind::Advice,
        };

        let mut report = Report::build(kind, source_file.path.display().to_string(), 0)
            .with_message(&self.message);

        for label in &self.labels {
            let color = if label.is_primary { Color::Red } else { Color::Blue };
            let ariadne_label = Label::new((source_file.path.display().to_string(), label.span.start..label.span.end))
                .with_message(&label.message)
                .with_color(color);
            report = report.with_label(ariadne_label);
        }

        for note in &self.notes {
            report = report.with_note(note);
        }

        let source = Source::from(&source_file.content);
        report.finish().eprint((source_file.path.display().to_string(), source)).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_diagnostic_creation() {
        let diag = Diagnostic::error(FileId(0), "unexpected token")
            .with_label(Span::new(10, 15), "here", true)
            .with_note("expected ';'");

        assert_eq!(diag.level, DiagnosticLevel::Error);
        assert_eq!(diag.message, "unexpected token");
        assert_eq!(diag.labels.len(), 1);
        assert_eq!(diag.notes.len(), 1);
    }
}
```

- [ ] **Step 3: Create crates/coco_diagnostics/tests/diagnostic_tests.rs**

```rust
use coco_diagnostics::*;
use coco_span::*;
use std::path::PathBuf;

#[test]
fn test_emit_diagnostic() {
    let mut source_map = SourceMap::new();
    let file_id = source_map.add_file(
        PathBuf::from("test.co"),
        "let x = 10\nlet y = 20;".to_string(),
    );

    let diag = Diagnostic::error(file_id, "expected ';' after variable declaration")
        .with_label(Span::new(10, 10), "expected ';' here", true)
        .with_note("add a semicolon to complete the statement");

    // This would print to stderr in real usage
    // For tests, we just verify the diagnostic was created correctly
    assert_eq!(diag.level, DiagnosticLevel::Error);
    assert_eq!(diag.labels.len(), 1);
    assert_eq!(diag.notes.len(), 1);
}
```

- [ ] **Step 4: Test**

```bash
cd crates/coco_diagnostics && cargo test
```

- [ ] **Step 5: Commit**

```bash
git add crates/coco_diagnostics/
git commit -m "feat(diagnostics): implement error reporting with ariadne"
```

---

## Task 4: coco_lexer — Tokenization (Part 1: Foundation)

**Files:**
- Create: `crates/coco_lexer/Cargo.toml`
- Create: `crates/coco_lexer/src/lib.rs`
- Create: `crates/coco_lexer/src/token.rs`
- Create: `crates/coco_lexer/src/lexer.rs`

- [ ] **Step 1: Create crates/coco_lexer/Cargo.toml**

```toml
[package]
name = "coco_lexer"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[dependencies]
coco_span = { workspace = true }
unicode-xid = { workspace = true }
```

- [ ] **Step 2: Create crates/coco_lexer/src/lib.rs**

```rust
//! Lexical analysis for the Coco programming language.
//!
//! Converts source text into a stream of tokens.

pub mod token;
pub mod lexer;

pub use token::{Token, TokenKind};
pub use lexer::Lexer;
```

- [ ] **Step 3: Create crates/coco_lexer/src/token.rs**

```rust
use coco_span::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
    pub text: String,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span, text: String) -> Self {
        Self { kind, span, text }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    // Keywords
    Async, Await, Break, Case, Catch, Class, Const, Continue, Coro,
    Do, Else, Enum, Export, Extends, False, Finally, Fn, For,
    If, Implements, Import, In, Interface, Is, Lazy, Let, Loop,
    Match, New, Null, Of, Parallel, Private, Protected, Public,
    Readonly, Return, Run, Select, Static, Synchronized, This,
    Throw, Trait, True, Try, Type, Typeof, Unsafe, Use, Void, While,
    
    // Contextual keywords
    Ok, Err, Result,
    
    // Identifiers and literals
    Ident,
    IntLiteral,
    FloatLiteral,
    StringLiteral,
    CharLiteral,
    TemplateLiteral,
    
    // Operators
    Plus,           // +
    Minus,          // -
    Star,           // *
    Slash,          // /
    Percent,        // %
    StarStar,       // **
    
    Eq,             // ==
    Ne,             // !=
    Lt,             // <
    Gt,             // >
    Le,             // <=
    Ge,             // >=
    Spaceship,      // <=>
    
    And,            // &&
    Or,             // ||
    Not,            // !
    
    BitAnd,         // &
    BitOr,          // |
    BitXor,         // ^
    BitNot,         // ~
    Shl,            // <<
    Shr,            // >>
    
    Question,       // ?
    QuestionDot,    // ?.
    QuestionQuestion, // ??
    QuestionColon,  // ?:
    
    PipeRight,      // |>
    PipeLeft,       // <|
    DollarDollar,   // $$
    
    Assign,         // =
    PlusEq,         // +=
    MinusEq,        // -=
    StarEq,         // *=
    SlashEq,        // /=
    PercentEq,      // %=
    StarStarEq,     // **=
    ShlEq,          // <<=
    ShrEq,          // >>=
    BitAndEq,       // &=
    BitOrEq,        // |=
    BitXorEq,       // ^=
    
    PlusPlus,       // ++
    MinusMinus,     // --
    
    // Delimiters
    LBrace,         // {
    RBrace,         // }
    LParen,         // (
    RParen,         // )
    LBracket,       // [
    RBracket,       // ]
    Comma,          // ,
    Semi,           // ;
    Colon,          // :
    Dot,            // .
    Arrow,          // =>
    
    // Trivia
    Whitespace,
    LineComment,
    BlockComment,
    
    // Special
    Eof,
    Error,
}

impl TokenKind {
    pub fn from_keyword(text: &str) -> Option<Self> {
        Some(match text {
            "async" => Self::Async,
            "await" => Self::Await,
            "break" => Self::Break,
            "case" => Self::Case,
            "catch" => Self::Catch,
            "class" => Self::Class,
            "const" => Self::Const,
            "continue" => Self::Continue,
            "coro" => Self::Coro,
            "do" => Self::Do,
            "else" => Self::Else,
            "enum" => Self::Enum,
            "export" => Self::Export,
            "extends" => Self::Extends,
            "false" => Self::False,
            "finally" => Self::Finally,
            "fn" => Self::Fn,
            "for" => Self::For,
            "if" => Self::If,
            "implements" => Self::Implements,
            "import" => Self::Import,
            "in" => Self::In,
            "interface" => Self::Interface,
            "is" => Self::Is,
            "lazy" => Self::Lazy,
            "let" => Self::Let,
            "loop" => Self::Loop,
            "match" => Self::Match,
            "new" => Self::New,
            "null" => Self::Null,
            "of" => Self::Of,
            "parallel" => Self::Parallel,
            "private" => Self::Private,
            "protected" => Self::Protected,
            "public" => Self::Public,
            "readonly" => Self::Readonly,
            "return" => Self::Return,
            "run" => Self::Run,
            "select" => Self::Select,
            "static" => Self::Static,
            "synchronized" => Self::Synchronized,
            "this" => Self::This,
            "throw" => Self::Throw,
            "trait" => Self::Trait,
            "true" => Self::True,
            "try" => Self::Try,
            "type" => Self::Type,
            "typeof" => Self::Typeof,
            "unsafe" => Self::Unsafe,
            "use" => Self::Use,
            "void" => Self::Void,
            "while" => Self::While,
            "Ok" => Self::Ok,
            "Err" => Self::Err,
            "Result" => Self::Result,
            _ => return None,
        })
    }

    pub fn is_trivia(&self) -> bool {
        matches!(self, Self::Whitespace | Self::LineComment | Self::BlockComment)
    }
}
```

- [ ] **Step 4: Commit foundation**

```bash
git add crates/coco_lexer/
git commit -m "feat(lexer): add token definitions"
```

---

## Task 5: coco_lexer — Tokenization (Part 2: Lexer Implementation)

- [ ] **Step 1: Create crates/coco_lexer/src/lexer.rs**

```rust
use coco_span::Span;
use unicode_xid::UnicodeXID;
use crate::token::{Token, TokenKind};

pub struct Lexer<'a> {
    source: &'a str,
    cursor: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        Self { source, cursor: 0 }
    }

    pub fn next_token(&mut self) -> Token {
        self.skip_whitespace_and_comments();
        
        if self.is_eof() {
            return Token::new(TokenKind::Eof, Span::new(self.cursor, self.cursor), String::new());
        }

        let start = self.cursor;
        let ch = self.current_char();

        let kind = match ch {
            ch if is_id_start(ch) => self.lex_ident_or_keyword(),
            ch if ch.is_ascii_digit() => self.lex_number(),
            '"' => self.lex_string(),
            '\'' => self.lex_char(),
            '`' => self.lex_template_literal(),
            
            '+' => self.lex_plus(),
            '-' => self.lex_minus(),
            '*' => self.lex_star(),
            '/' => self.lex_slash(),
            '%' => self.lex_percent(),
            
            '=' => self.lex_eq(),
            '!' => self.lex_not(),
            '<' => self.lex_lt(),
            '>' => self.lex_gt(),
            
            '&' => self.lex_and(),
            '|' => self.lex_or(),
            '^' => self.lex_xor(),
            '~' => { self.advance(); TokenKind::BitNot },
            
            '?' => self.lex_question(),
            '$' => self.lex_dollar(),
            
            '{' => { self.advance(); TokenKind::LBrace },
            '}' => { self.advance(); TokenKind::RBrace },
            '(' => { self.advance(); TokenKind::LParen },
            ')' => { self.advance(); TokenKind::RParen },
            '[' => { self.advance(); TokenKind::LBracket },
            ']' => { self.advance(); TokenKind::RBracket },
            ',' => { self.advance(); TokenKind::Comma },
            ';' => { self.advance(); TokenKind::Semi },
            ':' => { self.advance(); TokenKind::Colon },
            '.' => { self.advance(); TokenKind::Dot },
            
            _ => {
                self.advance();
                TokenKind::Error
            }
        };

        let end = self.cursor;
        let text = self.source[start..end].to_string();
        Token::new(kind, Span::new(start, end), text)
    }

    fn is_eof(&self) -> bool {
        self.cursor >= self.source.len()
    }

    fn current_char(&self) -> char {
        self.source[self.cursor..].chars().next().unwrap_or('\0')
    }

    fn peek_char(&self, offset: usize) -> char {
        self.source[self.cursor + offset..].chars().next().unwrap_or('\0')
    }

    fn advance(&mut self) {
        if let Some(ch) = self.source[self.cursor..].chars().next() {
            self.cursor += ch.len_utf8();
        }
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            match self.current_char() {
                ' ' | '\t' | '\r' | '\n' => self.advance(),
                '/' if self.peek_char(1) == '/' => self.skip_line_comment(),
                '/' if self.peek_char(1) == '*' => self.skip_block_comment(),
                _ => break,
            }
        }
    }

    fn skip_line_comment(&mut self) {
        while !self.is_eof() && self.current_char() != '\n' {
            self.advance();
        }
    }

    fn skip_block_comment(&mut self) {
        self.advance(); // /
        self.advance(); // *
        while !self.is_eof() {
            if self.current_char() == '*' && self.peek_char(1) == '/' {
                self.advance(); // *
                self.advance(); // /
                break;
            }
            self.advance();
        }
    }

    fn lex_ident_or_keyword(&mut self) -> TokenKind {
        let start = self.cursor;
        while is_id_continue(self.current_char()) {
            self.advance();
        }
        let text = &self.source[start..self.cursor];
        TokenKind::from_keyword(text).unwrap_or(TokenKind::Ident)
    }

    fn lex_number(&mut self) -> TokenKind {
        // Check for hex, binary, octal
        if self.current_char() == '0' {
            match self.peek_char(1) {
                'x' | 'X' => return self.lex_hex(),
                'b' | 'B' => return self.lex_binary(),
                'o' | 'O' => return self.lex_octal(),
                _ => {}
            }
        }

        // Decimal number
        while self.current_char().is_ascii_digit() || self.current_char() == '_' {
            self.advance();
        }

        // Check for float
        if self.current_char() == '.' && self.peek_char(1).is_ascii_digit() {
            self.advance(); // .
            while self.current_char().is_ascii_digit() || self.current_char() == '_' {
                self.advance();
            }
            return TokenKind::FloatLiteral;
        }

        // Check for exponent
        if matches!(self.current_char(), 'e' | 'E') {
            self.advance();
            if matches!(self.current_char(), '+' | '-') {
                self.advance();
            }
            while self.current_char().is_ascii_digit() || self.current_char() == '_' {
                self.advance();
            }
            return TokenKind::FloatLiteral;
        }

        TokenKind::IntLiteral
    }

    fn lex_hex(&mut self) -> TokenKind {
        self.advance(); // 0
        self.advance(); // x
        while self.current_char().is_ascii_hexdigit() || self.current_char() == '_' {
            self.advance();
        }
        TokenKind::IntLiteral
    }

    fn lex_binary(&mut self) -> TokenKind {
        self.advance(); // 0
        self.advance(); // b
        while matches!(self.current_char(), '0' | '1' | '_') {
            self.advance();
        }
        TokenKind::IntLiteral
    }

    fn lex_octal(&mut self) -> TokenKind {
        self.advance(); // 0
        self.advance(); // o
        while self.current_char().is_digit(8) || self.current_char() == '_' {
            self.advance();
        }
        TokenKind::IntLiteral
    }

    fn lex_string(&mut self) -> TokenKind {
        self.advance(); // opening "
        while !self.is_eof() && self.current_char() != '"' {
            if self.current_char() == '\\' {
                self.advance();
                self.advance(); // skip escaped char
            } else {
                self.advance();
            }
        }
        if self.current_char() == '"' {
            self.advance(); // closing "
        }
        TokenKind::StringLiteral
    }

    fn lex_char(&mut self) -> TokenKind {
        self.advance(); // opening '
        if self.current_char() == '\\' {
            self.advance();
        }
        if !self.is_eof() {
            self.advance(); // char
        }
        if self.current_char() == '\'' {
            self.advance(); // closing '
        }
        TokenKind::CharLiteral
    }

    fn lex_template_literal(&mut self) -> TokenKind {
        self.advance(); // opening `
        while !self.is_eof() && self.current_char() != '`' {
            if self.current_char() == '\\' {
                self.advance();
                self.advance();
            } else if self.current_char() == '$' && self.peek_char(1) == '{' {
                // Handle template expression
                self.advance(); // $
                self.advance(); // {
                let mut depth = 1;
                while !self.is_eof() && depth > 0 {
                    match self.current_char() {
                        '{' => depth += 1,
                        '}' => depth -= 1,
                        _ => {}
                    }
                    self.advance();
                }
            } else {
                self.advance();
            }
        }
        if self.current_char() == '`' {
            self.advance(); // closing `
        }
        TokenKind::TemplateLiteral
    }

    fn lex_plus(&mut self) -> TokenKind {
        self.advance();
        match self.current_char() {
            '=' => { self.advance(); TokenKind::PlusEq },
            '+' => { self.advance(); TokenKind::PlusPlus },
            _ => TokenKind::Plus,
        }
    }

    fn lex_minus(&mut self) -> TokenKind {
        self.advance();
        match self.current_char() {
            '=' => { self.advance(); TokenKind::MinusEq },
            '-' => { self.advance(); TokenKind::MinusMinus },
            _ => TokenKind::Minus,
        }
    }

    fn lex_star(&mut self) -> TokenKind {
        self.advance();
        match self.current_char() {
            '*' => {
                self.advance();
                if self.current_char() == '=' {
                    self.advance();
                    TokenKind::StarStarEq
                } else {
                    TokenKind::StarStar
                }
            }
            '=' => { self.advance(); TokenKind::StarEq },
            _ => TokenKind::Star,
        }
    }

    fn lex_slash(&mut self) -> TokenKind {
        self.advance();
        if self.current_char() == '=' {
            self.advance();
            TokenKind::SlashEq
        } else {
            TokenKind::Slash
        }
    }

    fn lex_percent(&mut self) -> TokenKind {
        self.advance();
        if self.current_char() == '=' {
            self.advance();
            TokenKind::PercentEq
        } else {
            TokenKind::Percent
        }
    }

    fn lex_eq(&mut self) -> TokenKind {
        self.advance();
        match self.current_char() {
            '=' => { self.advance(); TokenKind::Eq },
            '>' => { self.advance(); TokenKind::Arrow },
            _ => TokenKind::Assign,
        }
    }

    fn lex_not(&mut self) -> TokenKind {
        self.advance();
        if self.current_char() == '=' {
            self.advance();
            TokenKind::Ne
        } else {
            TokenKind::Not
        }
    }

    fn lex_lt(&mut self) -> TokenKind {
        self.advance();
        match self.current_char() {
            '=' => {
                self.advance();
                if self.current_char() == '>' {
                    self.advance();
                    TokenKind::Spaceship
                } else {
                    TokenKind::Le
                }
            }
            '<' => {
                self.advance();
                if self.current_char() == '=' {
                    self.advance();
                    TokenKind::ShlEq
                } else {
                    TokenKind::Shl
                }
            }
            '|' => { self.advance(); TokenKind::PipeLeft },
            _ => TokenKind::Lt,
        }
    }

    fn lex_gt(&mut self) -> TokenKind {
        self.advance();
        match self.current_char() {
            '=' => { self.advance(); TokenKind::Ge },
            '>' => {
                self.advance();
                if self.current_char() == '=' {
                    self.advance();
                    TokenKind::ShrEq
                } else {
                    TokenKind::Shr
                }
            }
            _ => TokenKind::Gt,
        }
    }

    fn lex_and(&mut self) -> TokenKind {
        self.advance();
        match self.current_char() {
            '&' => { self.advance(); TokenKind::And },
            '=' => { self.advance(); TokenKind::BitAndEq },
            _ => TokenKind::BitAnd,
        }
    }

    fn lex_or(&mut self) -> TokenKind {
        self.advance();
        match self.current_char() {
            '|' => { self.advance(); TokenKind::Or },
            '=' => { self.advance(); TokenKind::BitOrEq },
            '>' => { self.advance(); TokenKind::PipeRight },
            _ => TokenKind::BitOr,
        }
    }

    fn lex_xor(&mut self) -> TokenKind {
        self.advance();
        if self.current_char() == '=' {
            self.advance();
            TokenKind::BitXorEq
        } else {
            TokenKind::BitXor
        }
    }

    fn lex_question(&mut self) -> TokenKind {
        self.advance();
        match self.current_char() {
            '.' => { self.advance(); TokenKind::QuestionDot },
            '?' => { self.advance(); TokenKind::QuestionQuestion },
            ':' => { self.advance(); TokenKind::QuestionColon },
            _ => TokenKind::Question,
        }
    }

    fn lex_dollar(&mut self) -> TokenKind {
        self.advance();
        if self.current_char() == '$' {
            self.advance();
            TokenKind::DollarDollar
        } else {
            TokenKind::Error
        }
    }
}

fn is_id_start(ch: char) -> bool {
    ch == '_' || UnicodeXID::is_xid_start(ch)
}

fn is_id_continue(ch: char) -> bool {
    UnicodeXID::is_xid_continue(ch)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(source: &str) -> Vec<TokenKind> {
        let mut lexer = Lexer::new(source);
        let mut tokens = Vec::new();
        loop {
            let token = lexer.next_token();
            if token.kind == TokenKind::Eof {
                break;
            }
            tokens.push(token.kind);
        }
        tokens
    }

    #[test]
    fn test_keywords() {
        let tokens = lex("fn class const let if for");
        assert_eq!(tokens, vec![
            TokenKind::Fn,
            TokenKind::Class,
            TokenKind::Const,
            TokenKind::Let,
            TokenKind::If,
            TokenKind::For,
        ]);
    }

    #[test]
    fn test_numbers() {
        let tokens = lex("42 3.14 0xFF 0b1010 0o755");
        assert_eq!(tokens, vec![
            TokenKind::IntLiteral,
            TokenKind::FloatLiteral,
            TokenKind::IntLiteral,
            TokenKind::IntLiteral,
            TokenKind::IntLiteral,
        ]);
    }

    #[test]
    fn test_operators() {
        let tokens = lex("+ - * / % ** == != < > <= >= <=> && || ! ?. ?? ?: |> <| $$");
        assert_eq!(tokens, vec![
            TokenKind::Plus,
            TokenKind::Minus,
            TokenKind::Star,
            TokenKind::Slash,
            TokenKind::Percent,
            TokenKind::StarStar,
            TokenKind::Eq,
            TokenKind::Ne,
            TokenKind::Lt,
            TokenKind::Gt,
            TokenKind::Le,
            TokenKind::Ge,
            TokenKind::Spaceship,
            TokenKind::And,
            TokenKind::Or,
            TokenKind::Not,
            TokenKind::QuestionDot,
            TokenKind::QuestionQuestion,
            TokenKind::QuestionColon,
            TokenKind::PipeRight,
            TokenKind::PipeLeft,
            TokenKind::DollarDollar,
        ]);
    }

    #[test]
    fn test_strings() {
        let tokens = lex(r#""hello" 'a' `template ${x}`"#);
        assert_eq!(tokens, vec![
            TokenKind::StringLiteral,
            TokenKind::CharLiteral,
            TokenKind::TemplateLiteral,
        ]);
    }
}
```

- [ ] **Step 2: Test**

```bash
cd crates/coco_lexer && cargo test
```

- [ ] **Step 3: Test on example file**

```bash
cargo run --bin coco lex examples/01-hello.co
```

- [ ] **Step 4: Commit**

```bash
git add crates/coco_lexer/
git commit -m "feat(lexer): implement full tokenizer with Unicode support"
```

---

## Task 6: Test Lexer on All Examples

- [ ] **Step 1: Run lexer on all 21 examples**

```bash
for f in examples/*.co; do
    echo "Lexing $f..."
    cargo run --bin coco lex "$f" > /dev/null || echo "FAILED: $f"
done
```

- [ ] **Step 2: Fix any lexer bugs found**

- [ ] **Step 3: Commit fixes**

```bash
git add crates/coco_lexer/
git commit -m "fix(lexer): resolve issues found in example files"
```

---

## Task 7: coco_syntax — AST Definitions (Part 1: Core Types)

**Files:**
- Create: `crates/coco_syntax/Cargo.toml`
- Create: `crates/coco_syntax/src/lib.rs`
- Create: `crates/coco_syntax/src/ast.rs`

- [ ] **Step 1: Create crates/coco_syntax/Cargo.toml**

```toml
[package]
name = "coco_syntax"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[dependencies]
coco_span = { workspace = true }
```

- [ ] **Step 2: Create crates/coco_syntax/src/lib.rs**

```rust
//! Abstract Syntax Tree definitions for the Coco programming language.

pub mod ast;

pub use ast::*;
```

- [ ] **Step 3: Create crates/coco_syntax/src/ast.rs** (Core types)

```rust
use coco_span::Span;

#[derive(Debug, Clone)]
pub struct Program {
    pub items: Vec<Item>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum Item {
    FnDecl(FnDecl),
    ClassDecl(ClassDecl),
    InterfaceDecl(InterfaceDecl),
    TraitDecl(TraitDecl),
    EnumDecl(EnumDecl),
    ConstDecl(ConstDecl),
    LetDecl(LetDecl),
    TypeAlias(TypeAlias),
    Import(Import),
    Export(Export),
}

#[derive(Debug, Clone)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

// Function Declaration
#[derive(Debug, Clone)]
pub struct FnDecl {
    pub span: Span,
    pub is_async: bool,
    pub name: Ident,
    pub type_params: Option<Vec<TypeParam>>,
    pub params: Vec<Param>,
    pub return_type: Option<Type>,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub span: Span,
    pub name: Ident,
    pub type_ann: Option<Type>,
    pub default_value: Option<Expr>,
}

#[derive(Debug, Clone)]
pub struct TypeParam {
    pub span: Span,
    pub name: Ident,
    pub constraint: Option<Type>,
}

// Class Declaration
#[derive(Debug, Clone)]
pub struct ClassDecl {
    pub span: Span,
    pub name: Ident,
    pub type_params: Option<Vec<TypeParam>>,
    pub extends: Option<Type>,
    pub implements: Vec<Type>,
    pub members: Vec<ClassMember>,
}

#[derive(Debug, Clone)]
pub enum ClassMember {
    Constructor(Constructor),
    Method(Method),
    Property(Property),
    UseTrait(UseTrait),
}

#[derive(Debug, Clone)]
pub struct Constructor {
    pub span: Span,
    pub params: Vec<ConstructorParam>,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub struct ConstructorParam {
    pub span: Span,
    pub modifiers: Vec<Modifier>,
    pub name: Ident,
    pub type_ann: Option<Type>,
    pub default_value: Option<Expr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modifier {
    Public,
    Private,
    Protected,
    Readonly,
    Static,
}

#[derive(Debug, Clone)]
pub struct Method {
    pub span: Span,
    pub modifiers: Vec<Modifier>,
    pub is_async: bool,
    pub name: Ident,
    pub type_params: Option<Vec<TypeParam>>,
    pub params: Vec<Param>,
    pub return_type: Option<Type>,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub struct Property {
    pub span: Span,
    pub modifiers: Vec<Modifier>,
    pub name: Ident,
    pub type_ann: Type,
    pub default_value: Option<Expr>,
}

#[derive(Debug, Clone)]
pub struct UseTrait {
    pub span: Span,
    pub traits: Vec<Ident>,
}

// Interface Declaration
#[derive(Debug, Clone)]
pub struct InterfaceDecl {
    pub span: Span,
    pub name: Ident,
    pub type_params: Option<Vec<TypeParam>>,
    pub extends: Option<Type>,
    pub members: Vec<InterfaceMember>,
}

#[derive(Debug, Clone)]
pub enum InterfaceMember {
    MethodSignature(MethodSignature),
    PropertySignature(PropertySignature),
}

#[derive(Debug, Clone)]
pub struct MethodSignature {
    pub span: Span,
    pub is_async: bool,
    pub name: Ident,
    pub type_params: Option<Vec<TypeParam>>,
    pub params: Vec<Param>,
    pub return_type: Type,
}

#[derive(Debug, Clone)]
pub struct PropertySignature {
    pub span: Span,
    pub name: Ident,
    pub type_ann: Type,
}

// Trait Declaration
#[derive(Debug, Clone)]
pub struct TraitDecl {
    pub span: Span,
    pub name: Ident,
    pub type_params: Option<Vec<TypeParam>>,
    pub members: Vec<TraitMember>,
}

#[derive(Debug, Clone)]
pub enum TraitMember {
    Method(Method),
    MethodSignature(MethodSignature),
    Property(Property),
}

// Enum Declaration
#[derive(Debug, Clone)]
pub struct EnumDecl {
    pub span: Span,
    pub name: Ident,
    pub backing_type: Option<Type>,
    pub variants: Vec<EnumVariant>,
}

#[derive(Debug, Clone)]
pub struct EnumVariant {
    pub span: Span,
    pub name: Ident,
    pub fields: Option<Vec<Type>>,
    pub value: Option<Expr>,
}

// Const/Let Declarations
#[derive(Debug, Clone)]
pub struct ConstDecl {
    pub span: Span,
    pub name: Ident,
    pub type_ann: Option<Type>,
    pub value: Expr,
}

#[derive(Debug, Clone)]
pub struct LetDecl {
    pub span: Span,
    pub name: Ident,
    pub type_ann: Option<Type>,
    pub value: Option<Expr>,
}

// Type Alias
#[derive(Debug, Clone)]
pub struct TypeAlias {
    pub span: Span,
    pub name: Ident,
    pub type_params: Option<Vec<TypeParam>>,
    pub target: Type,
}

// Import/Export
#[derive(Debug, Clone)]
pub struct Import {
    pub span: Span,
    pub items: ImportItems,
    pub source: String,
}

#[derive(Debug, Clone)]
pub enum ImportItems {
    Named(Vec<Ident>),
    Namespace(Ident),
}

#[derive(Debug, Clone)]
pub struct Export {
    pub span: Span,
    pub item: Box<Item>,
}

// Statements
#[derive(Debug, Clone)]
pub enum Stmt {
    Expr(ExprStmt),
    If(IfStmt),
    For(ForStmt),
    While(WhileStmt),
    DoWhile(DoWhileStmt),
    Loop(LoopStmt),
    Return(ReturnStmt),
    Throw(ThrowStmt),
    Try(TryStmt),
    Break(BreakStmt),
    Continue(ContinueStmt),
    Parallel(ParallelStmt),
    Coro(CoroStmt),
    Select(SelectStmt),
    Unsafe(UnsafeStmt),
    Synchronized(SynchronizedStmt),
    Block(Block),
}

#[derive(Debug, Clone)]
pub struct ExprStmt {
    pub span: Span,
    pub expr: Expr,
}

#[derive(Debug, Clone)]
pub struct IfStmt {
    pub span: Span,
    pub condition: Expr,
    pub then_block: Block,
    pub else_ifs: Vec<ElseIf>,
    pub else_block: Option<Block>,
}

#[derive(Debug, Clone)]
pub struct ElseIf {
    pub span: Span,
    pub condition: Expr,
    pub block: Block,
}

#[derive(Debug, Clone)]
pub struct ForStmt {
    pub span: Span,
    pub pattern: Ident,
    pub iterable: Expr,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub struct WhileStmt {
    pub span: Span,
    pub condition: Expr,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub struct DoWhileStmt {
    pub span: Span,
    pub body: Block,
    pub condition: Expr,
}

#[derive(Debug, Clone)]
pub struct LoopStmt {
    pub span: Span,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub struct ReturnStmt {
    pub span: Span,
    pub value: Option<Expr>,
}

#[derive(Debug, Clone)]
pub struct ThrowStmt {
    pub span: Span,
    pub value: Expr,
}

#[derive(Debug, Clone)]
pub struct TryStmt {
    pub span: Span,
    pub body: Block,
    pub catches: Vec<CatchClause>,
    pub finally: Option<Block>,
}

#[derive(Debug, Clone)]
pub struct CatchClause {
    pub span: Span,
    pub param: Ident,
    pub type_ann: Option<Type>,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub struct BreakStmt {
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ContinueStmt {
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct ParallelStmt {
    pub span: Span,
    pub runs: Vec<RunClause>,
}

#[derive(Debug, Clone)]
pub struct RunClause {
    pub span: Span,
    pub expr: Expr,
}

#[derive(Debug, Clone)]
pub struct CoroStmt {
    pub span: Span,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub struct SelectStmt {
    pub span: Span,
    pub cases: Vec<CaseClause>,
}

#[derive(Debug, Clone)]
pub struct CaseClause {
    pub span: Span,
    pub pattern: Ident,
    pub expr: Expr,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone)]
pub struct UnsafeStmt {
    pub span: Span,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub struct SynchronizedStmt {
    pub span: Span,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub struct Block {
    pub span: Span,
    pub stmts: Vec<Stmt>,
}

// Expressions
#[derive(Debug, Clone)]
pub enum Expr {
    Literal(Literal),
    Ident(Ident),
    Binary(Box<BinaryExpr>),
    Unary(Box<UnaryExpr>),
    Call(Box<CallExpr>),
    Index(Box<IndexExpr>),
    Member(Box<MemberExpr>),
    Match(Box<MatchExpr>),
    Lambda(Box<Lambda>),
    Array(ArrayLiteral),
    Object(ObjectLiteral),
    This(Span),
    New(Box<NewExpr>),
}

#[derive(Debug, Clone)]
pub enum Literal {
    Int(i64, Span),
    Float(f64, Span),
    String(String, Span),
    Char(char, Span),
    Bool(bool, Span),
    Null(Span),
}

#[derive(Debug, Clone)]
pub struct BinaryExpr {
    pub span: Span,
    pub left: Expr,
    pub op: BinaryOp,
    pub right: Expr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add, Sub, Mul, Div, Mod, Pow,
    Eq, Ne, Lt, Gt, Le, Ge, Spaceship,
    And, Or,
    BitAnd, BitOr, BitXor,
    Shl, Shr,
    PipeRight, PipeLeft,
    NullCoalesce, Elvis,
    Assign,
    AddAssign, SubAssign, MulAssign, DivAssign, ModAssign, PowAssign,
    ShlAssign, ShrAssign,
    BitAndAssign, BitOrAssign, BitXorAssign,
}

#[derive(Debug, Clone)]
pub struct UnaryExpr {
    pub span: Span,
    pub op: UnaryOp,
    pub expr: Expr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Not, BitNot, Neg,
    Typeof, New, Await, Lazy,
}

#[derive(Debug, Clone)]
pub struct CallExpr {
    pub span: Span,
    pub callee: Expr,
    pub args: Vec<Argument>,
}

#[derive(Debug, Clone)]
pub struct Argument {
    pub span: Span,
    pub name: Option<Ident>,
    pub value: Expr,
}

#[derive(Debug, Clone)]
pub struct IndexExpr {
    pub span: Span,
    pub object: Expr,
    pub index: Expr,
}

#[derive(Debug, Clone)]
pub struct MemberExpr {
    pub span: Span,
    pub object: Expr,
    pub property: Ident,
    pub optional: bool, // for ?.
}

#[derive(Debug, Clone)]
pub struct MatchExpr {
    pub span: Span,
    pub scrutinee: Expr,
    pub arms: Vec<MatchArm>,
}

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub span: Span,
    pub pattern: Pattern,
    pub body: Expr,
}

#[derive(Debug, Clone)]
pub enum Pattern {
    Literal(Literal),
    Ident(Ident),
    IsType(Type),
    Wildcard(Span),
}

#[derive(Debug, Clone)]
pub struct Lambda {
    pub span: Span,
    pub params: Vec<Param>,
    pub return_type: Option<Type>,
    pub body: LambdaBody,
}

#[derive(Debug, Clone)]
pub enum LambdaBody {
    Expr(Expr),
    Block(Block),
}

#[derive(Debug, Clone)]
pub struct ArrayLiteral {
    pub span: Span,
    pub elements: Vec<Expr>,
}

#[derive(Debug, Clone)]
pub struct ObjectLiteral {
    pub span: Span,
    pub fields: Vec<ObjectField>,
}

#[derive(Debug, Clone)]
pub struct ObjectField {
    pub span: Span,
    pub key: ObjectKey,
    pub value: Expr,
}

#[derive(Debug, Clone)]
pub enum ObjectKey {
    Ident(Ident),
    String(String, Span),
}

#[derive(Debug, Clone)]
pub struct NewExpr {
    pub span: Span,
    pub type_name: Ident,
    pub args: Vec<Argument>,
}

// Types
#[derive(Debug, Clone)]
pub enum Type {
    Primitive(PrimitiveType, Span),
    Named(NamedType),
    Union(UnionType),
    Intersection(IntersectionType),
    List(ListType),
    Map(MapType),
    Tuple(TupleType),
    Result(ResultType),
    Function(FunctionType),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimitiveType {
    Int, Uint, Float, Bool, String, Char, Byte,
    Null, Void, Never, Mixed,
}

#[derive(Debug, Clone)]
pub struct NamedType {
    pub span: Span,
    pub name: Ident,
    pub type_args: Option<Vec<Type>>,
}

#[derive(Debug, Clone)]
pub struct UnionType {
    pub span: Span,
    pub types: Vec<Type>,
}

#[derive(Debug, Clone)]
pub struct IntersectionType {
    pub span: Span,
    pub types: Vec<Type>,
}

#[derive(Debug, Clone)]
pub struct ListType {
    pub span: Span,
    pub element_type: Box<Type>,
}

#[derive(Debug, Clone)]
pub struct MapType {
    pub span: Span,
    pub key_type: Box<Type>,
    pub value_type: Box<Type>,
}

#[derive(Debug, Clone)]
pub struct TupleType {
    pub span: Span,
    pub element_types: Vec<Type>,
}

#[derive(Debug, Clone)]
pub struct ResultType {
    pub span: Span,
    pub ok_type: Box<Type>,
    pub err_type: Box<Type>,
}

#[derive(Debug, Clone)]
pub struct FunctionType {
    pub span: Span,
    pub param_types: Vec<Type>,
    pub return_type: Box<Type>,
}
```

- [ ] **Step 4: Commit AST types**

```bash
git add crates/coco_syntax/
git commit -m "feat(syntax): define complete AST node types"
```

---

Due to character limits, I need to continue in the next message. The plan will continue with:

- Task 8-12: Parser implementation
- Task 13-14: Formatter implementation
- Task 15: CLI implementation
- Task 16-17: Testing and benchmarking
- Task 18: Final integration and CI setup

Should I continue?