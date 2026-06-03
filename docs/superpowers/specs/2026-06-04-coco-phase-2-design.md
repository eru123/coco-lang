# Coco Language — Phase 2 Design Spec

> Date: 2026-06-04
> Scope: Lexer, Parser, Formatter implementation in Rust
> Status: Approved

---

## 1. Overview

Phase 2 implements the foundational compilation pipeline: lexical analysis (lexer), syntax analysis (parser), and code formatting (formatter). All components are written in Rust and produce a working CLI tool that can tokenize, parse, and format Coco source code.

**Goal:** By the end of Phase 2, `coco fmt`, `coco lex`, and `coco parse` commands work on all 21 example programs without errors.

---

## 2. Architecture

### 2.1 Component Overview

```
coco-lang/
├── crates/
│   ├── coco_span/         # Source location tracking (line, column, byte offset)
│   ├── coco_diagnostics/  # Error reporting with beautiful messages (ariadne)
│   ├── coco_lexer/        # Tokenization with Unicode support
│   ├── coco_syntax/       # AST node definitions, visitors, lossless tokens
│   ├── coco_parser/       # Recursive descent + Pratt parsing, error recovery
│   ├── coco_formatter/    # Pretty-printer with configurable style
│   └── coco_cli/          # CLI binary (lex, parse, fmt commands)
├── Cargo.toml             # Workspace manifest
└── examples/              # 21 .co test files
```

### 2.2 Data Flow

```
Source Code (.co file)
    ↓
[Lexer] → Token Stream
    ↓
[Parser] → AST (Abstract Syntax Tree)
    ↓
[Formatter] → Formatted Source Code
```

### 2.3 Key Design Decisions

#### Lossless AST
The AST preserves ALL source information:
- Whitespace between tokens
- Comments (line and block)
- Token spans (line, column, byte offset)

**Why:** The formatter needs exact source layout to preserve comments and produce idiomatic output. A lossy AST would require heuristics.

#### Recursive Descent + Pratt Parsing
- **Recursive descent** for statements, declarations, blocks
- **Pratt parsing** (precedence climbing) for expressions

**Why:** Recursive descent is simple and matches grammar structure. Pratt parsing handles operator precedence elegantly without deeply nested functions.

#### Error Recovery
The parser continues after syntax errors, producing a partial AST with error markers.

**Why:** IDEs and LSPs need partial ASTs to provide completions/diagnostics even in invalid code.

#### Beautiful Diagnostics
Errors use `ariadne` for colored, annotated source excerpts with suggestions.

**Why:** Developer experience matters. Clear error messages reduce frustration and speed up learning.

---

## 3. Crate Breakdown

### 3.1 `coco_span`

**Purpose:** Track source locations (file, line, column, byte offset).

**Types:**
- `Span`: byte range in a source file
- `Location`: line + column position
- `SourceFile`: file path + content
- `SourceMap`: registry of all loaded files

**API:**
```rust
pub struct Span {
    pub start: usize,
    pub end: usize,
}

pub struct Location {
    pub line: usize,
    pub column: usize,
}

pub struct SourceFile {
    pub path: PathBuf,
    pub content: String,
}

pub struct SourceMap {
    files: Vec<SourceFile>,
}

impl SourceMap {
    pub fn add_file(&mut self, path: PathBuf, content: String) -> FileId;
    pub fn get_location(&self, file: FileId, offset: usize) -> Location;
    pub fn get_span_text(&self, file: FileId, span: Span) -> &str;
}
```

**Dependencies:** `std` only (no external deps)

---

### 3.2 `coco_diagnostics`

**Purpose:** Emit beautiful error messages with source context.

**Types:**
- `Diagnostic`: error/warning with span, message, labels, notes
- `DiagnosticLevel`: Error, Warning, Note
- `Label`: annotated span with message

**API:**
```rust
pub enum DiagnosticLevel {
    Error,
    Warning,
    Note,
}

pub struct Diagnostic {
    pub level: DiagnosticLevel,
    pub message: String,
    pub labels: Vec<Label>,
    pub notes: Vec<String>,
}

pub struct Label {
    pub span: Span,
    pub message: String,
}

pub fn emit(diagnostic: &Diagnostic, source_map: &SourceMap);
```

**Example Output:**
```
error: expected ';' after variable declaration
  ┌─ examples/02-variables.co:5:20
  │
5 │     let counter = 0
  │                    ^ expected ';' here
  │
  = help: add a semicolon to complete the statement
```

**Dependencies:** `ariadne`, `coco_span`

---

### 3.3 `coco_lexer`

**Purpose:** Tokenize Coco source code into a stream of tokens.

**Tokens:**
- Keywords: `fn`, `class`, `const`, `let`, `if`, `for`, `async`, `await`, etc.
- Identifiers: `userName`, `_temp`, `value123`
- Literals: integers, floats, strings, chars, booleans, `null`
- Operators: `+`, `-`, `*`, `/`, `==`, `!=`, `?.`, `??`, `?:`, `|>`, `<|`, `$$`
- Delimiters: `{`, `}`, `(`, `)`, `[`, `]`, `,`, `;`, `:`, `.`
- Whitespace (preserved for lossless AST)
- Comments (line `//`, block `/* */`)

**API:**
```rust
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
    pub text: String,
}

pub enum TokenKind {
    // Keywords
    Fn, Class, Const, Let, If, Else, For, While, Return, /* ... */,
    
    // Identifiers and literals
    Ident, IntLiteral, FloatLiteral, StringLiteral, CharLiteral, BoolLiteral, Null,
    
    // Operators
    Plus, Minus, Star, Slash, Percent, StarStar,
    Eq, Ne, Lt, Gt, Le, Ge, Spaceship,
    And, Or, Not,
    Question, QuestionDot, QuestionQuestion, QuestionColon,
    PipeRight, PipeLeft, DollarDollar,
    
    // Delimiters
    LBrace, RBrace, LParen, RParen, LBracket, RBracket,
    Comma, Semi, Colon, Dot, Arrow,
    
    // Trivia
    Whitespace, LineComment, BlockComment,
    
    // Special
    Eof, Error,
}

pub struct Lexer<'a> {
    source: &'a str,
    cursor: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self;
    pub fn next_token(&mut self) -> Token;
}
```

**Features:**
- Unicode support (UTF-8 source, Unicode identifiers allowed)
- String escape sequences: `\n`, `\t`, `\x##`, `\u{...}`
- Template literals: `` `Hello, ${name}` ``
- Integer literals: decimal, hex `0x`, binary `0b`, octal `0o`
- Float literals: `3.14`, `1e10`, `2.5e-3`
- Underscore separators: `1_000_000`, `0xFF_FF`

**Error Handling:**
- Invalid characters → `TokenKind::Error` with diagnostic
- Unterminated strings → error + recovery (skip to next quote or newline)
- Invalid escape sequences → error + continue

**Dependencies:** `coco_span`, `unicode-xid` (for identifier validation)

---

### 3.4 `coco_syntax`

**Purpose:** Define AST node types and visitor patterns.

**Node Types:**
- `Program`: top-level declarations
- `Decl`: function, class, interface, trait, enum, const, let, type alias
- `Stmt`: expression, if, for, while, loop, return, throw, try, parallel, coro, block
- `Expr`: literals, identifiers, binary ops, unary ops, call, index, member access, match, lambda
- `Type`: primitive, named, union, intersection, list, map, tuple, Result, function types
- `Pattern`: literal, identifier, `is Type`, wildcard `_`

**Lossless Tokens:**
Every AST node stores its leading/trailing trivia (whitespace, comments).

**API:**
```rust
pub struct Program {
    pub items: Vec<Item>,
}

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

pub struct FnDecl {
    pub span: Span,
    pub is_async: bool,
    pub name: Ident,
    pub type_params: Option<TypeParams>,
    pub params: Vec<Param>,
    pub return_type: Option<Type>,
    pub body: Block,
}

pub struct ClassDecl {
    pub span: Span,
    pub name: Ident,
    pub type_params: Option<TypeParams>,
    pub extends: Option<Type>,
    pub implements: Vec<Type>,
    pub members: Vec<ClassMember>,
}

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
    // ... more
}

pub struct BinaryExpr {
    pub span: Span,
    pub left: Expr,
    pub op: BinaryOp,
    pub right: Expr,
}

pub enum BinaryOp {
    Add, Sub, Mul, Div, Mod, Pow,
    Eq, Ne, Lt, Gt, Le, Ge, Spaceship,
    And, Or,
    BitAnd, BitOr, BitXor,
    Shl, Shr,
    PipeRight, PipeLeft,
    NullCoalesce, Elvis,
    // ... more
}

// Visitor pattern:
pub trait Visitor {
    fn visit_program(&mut self, program: &Program);
    fn visit_fn_decl(&mut self, fn_decl: &FnDecl);
    fn visit_expr(&mut self, expr: &Expr);
    // ... more
}
```

**Dependencies:** `coco_span`

---

### 3.5 `coco_parser`

**Purpose:** Parse token stream into AST with error recovery.

**Algorithm:**
- **Recursive descent** for statements, declarations
- **Pratt parsing** for expressions (operator precedence)
- **Error recovery:** on unexpected token, skip until sync point (`;`, `}`, `fn`, `class`)

**Operator Precedence (lowest to highest):**
1. Assignment: `=`, `+=`, `-=`, etc.
2. Elvis: `?:`
3. Null coalescing: `??`
4. Logical OR: `||`
5. Logical AND: `&&`
6. Bitwise OR: `|`
7. Bitwise XOR: `^`
8. Bitwise AND: `&`
9. Equality: `==`, `!=`
10. Comparison: `<`, `>`, `<=`, `>=`, `<=>`
11. Shift: `<<`, `>>`
12. Additive: `+`, `-`
13. Multiplicative: `*`, `/`, `%`
14. Exponentiation: `**` (right-associative)
15. Pipe: `|>`, `<|` (left-associative)
16. Unary: `!`, `~`, `-`, `typeof`, `new`, `await`, `lazy`
17. Postfix: `.`, `?.`, `[]`, `()`, `!`, `?`, `++`, `--`

**Pipe Operator Validation:**
- `|>` (forward): left-to-right, `$$` allowed in right operand
- `<|` (backward): right-to-left, `$$` allowed in left operand
- `$$` scope tracking: only valid inside pipe expressions
- Nested pipes: track direction, validate `$$` placement

**API:**
```rust
pub struct Parser<'a> {
    lexer: Lexer<'a>,
    current: Token,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Parser<'a> {
    pub fn new(source: &'a str) -> Self;
    pub fn parse_program(&mut self) -> Program;
    pub fn diagnostics(&self) -> &[Diagnostic];
}

// Internal methods:
impl<'a> Parser<'a> {
    fn parse_item(&mut self) -> Item;
    fn parse_fn_decl(&mut self) -> FnDecl;
    fn parse_class_decl(&mut self) -> ClassDecl;
    fn parse_stmt(&mut self) -> Stmt;
    fn parse_expr(&mut self) -> Expr;
    fn parse_expr_bp(&mut self, min_bp: u8) -> Expr; // Pratt parsing
    
    // Error recovery:
    fn expect(&mut self, kind: TokenKind) -> Result<Token, ()>;
    fn synchronize(&mut self); // skip to sync point
}
```

**Error Recovery Strategy:**
- On unexpected token: emit diagnostic, skip to sync point (`;`, `}`, keyword)
- Partial AST nodes marked with `Error` variant
- Continue parsing remaining items

**Dependencies:** `coco_lexer`, `coco_syntax`, `coco_diagnostics`

---

### 3.6 `coco_formatter`

**Purpose:** Pretty-print AST back to formatted source code.

**Style:**
- Indent: 4 spaces (configurable)
- Max line length: 100 chars (configurable)
- Trailing commas: multiline arrays/objects
- Space after keywords: `if (...)`, `fn name(...)`
- No space before `:` in types, space after: `name: Type`
- Binary operators: space around (`a + b`)
- Unary operators: no space (`!flag`, `-value`)

**Idempotence:**
Running `coco fmt` twice on the same file produces identical output.

**Comment Preservation:**
- Line comments stay on their original line
- Block comments preserve internal formatting
- Comments attached to AST nodes via trivia

**API:**
```rust
pub struct Formatter {
    pub config: FormatterConfig,
}

pub struct FormatterConfig {
    pub indent_width: usize,
    pub max_line_length: usize,
    pub trailing_commas: bool,
}

impl Formatter {
    pub fn new(config: FormatterConfig) -> Self;
    pub fn format(&self, program: &Program) -> String;
}
```

**Algorithm:**
1. Walk AST with visitor
2. Build intermediate representation (IR) of formatting decisions
3. Apply line-breaking algorithm (greedy or optimal)
4. Emit formatted string with trivia

**Dependencies:** `coco_syntax`, `coco_span`

---

### 3.7 `coco_cli`

**Purpose:** Command-line tool exposing lexer, parser, formatter.

**Commands:**
```bash
coco lex <file>       # Tokenize and print tokens
coco parse <file>     # Parse and print AST (debug format)
coco fmt <file>       # Format and print to stdout
coco fmt -w <file>    # Format and write in-place
coco check <file>     # Parse and report diagnostics
```

**API:**
```rust
use clap::Parser;

#[derive(Parser)]
enum Cli {
    Lex { file: PathBuf },
    Parse { file: PathBuf },
    Fmt { file: PathBuf, write: bool },
    Check { file: PathBuf },
}

fn main() {
    let cli = Cli::parse();
    match cli {
        Cli::Lex { file } => run_lex(file),
        Cli::Parse { file } => run_parse(file),
        Cli::Fmt { file, write } => run_fmt(file, write),
        Cli::Check { file } => run_check(file),
    }
}
```

**Dependencies:** `clap`, `coco_lexer`, `coco_parser`, `coco_formatter`, `coco_diagnostics`

---

## 4. Implementation Timeline

**Estimated effort:** 6-8 weeks

### Week 1: Foundation
- Setup Cargo workspace
- Implement `coco_span` (source locations)
- Implement `coco_diagnostics` (error reporting with ariadne)
- **Milestone:** Can emit beautiful errors with source context

### Week 2: Lexer
- Implement `coco_lexer` (tokenization)
- Keywords, identifiers, operators, literals
- Unicode support, string escapes, template literals
- **Milestone:** All 21 examples tokenize without errors

### Week 3: Syntax Definitions
- Implement `coco_syntax` (AST nodes)
- Define all node types (Decl, Stmt, Expr, Type, Pattern)
- Lossless token storage (trivia)
- **Milestone:** AST types cover full grammar

### Week 4-5: Parser
- Implement `coco_parser` (recursive descent + Pratt)
- Parse declarations, statements, expressions
- Operator precedence table
- Pipe operator validation (direction, `$$` scope)
- Error recovery
- **Milestone:** All 21 examples parse into valid AST

### Week 6: Formatter Foundation
- Implement `coco_formatter` (pretty-printer)
- Basic formatting (no line-breaking yet)
- Comment preservation
- **Milestone:** Simple programs format correctly

### Week 7: Formatter Polish
- Line-breaking algorithm (wrap long lines)
- Idempotence testing
- Style configurability
- **Milestone:** All 21 examples format correctly and idempotently

### Week 8: CLI and Testing
- Implement `coco_cli` (lex, parse, fmt, check commands)
- Integration tests on all 21 examples
- Benchmark performance (lex 1000 lines < 1ms, parse < 5ms, fmt < 10ms)
- **Milestone:** Phase 2 complete

---

## 5. Success Criteria

Phase 2 is complete when:

1. **All 21 example programs parse without errors**
   - No syntax errors reported by `coco check`
   - Full AST produced for each file

2. **Formatter is idempotent**
   - `coco fmt file.co | coco fmt` produces same output
   - Comments and whitespace preserved correctly

3. **Error messages are helpful**
   - Span highlighting with ariadne
   - Suggestions for common mistakes
   - No cryptic compiler jargon

4. **Performance targets met**
   - Lex 1000 lines: < 1ms
   - Parse 1000 lines: < 5ms
   - Format 1000 lines: < 10ms

5. **Test coverage > 90%**
   - Unit tests for lexer, parser, formatter
   - Integration tests on all examples
   - Fuzzing for parser robustness

6. **Documentation complete**
   - Architecture documented (this spec)
   - API docs for public types
   - Examples in doc comments

---

## 6. Non-Scope for Phase 2

These are explicitly deferred to later phases:

- **Semantic analysis:** Type checking, name resolution (Phase 4)
- **Interpreter:** Execution of Coco programs (Phase 3)
- **Code generation:** Bytecode or native compilation (Phase 7+)
- **LSP server:** IDE integration (post-v1)
- **Package manager:** Dependency resolution (post-v1)
- **Standard library:** Runtime APIs (Phase 10)

Phase 2 only implements **syntax** — not semantics.

---

## 7. Open Questions

### 7.1 Template Literal Parsing
Should template literals be parsed as a single token with embedded expressions, or multiple tokens?

**Decision:** Single token with expression slots. Simplifies lexer, parser handles expressions.

### 7.2 Error Recovery Aggressiveness
How aggressively should the parser recover? Skip entire declarations or try to salvage?

**Decision:** Skip to sync points (`;`, `}`, keyword). Emit partial AST where possible.

### 7.3 Formatter Line-Breaking Algorithm
Greedy (fast, simple) or optimal (prettier, slower)?

**Decision:** Start with greedy. Optimize later if needed.

---

## 8. Dependencies

### External Crates
- `ariadne`: Beautiful error messages
- `unicode-xid`: Unicode identifier validation
- `clap`: CLI argument parsing

### Internal Crates
- `coco_span` ← (no deps)
- `coco_diagnostics` ← `ariadne`, `coco_span`
- `coco_lexer` ← `unicode-xid`, `coco_span`
- `coco_syntax` ← `coco_span`
- `coco_parser` ← `coco_lexer`, `coco_syntax`, `coco_diagnostics`
- `coco_formatter` ← `coco_syntax`, `coco_span`
- `coco_cli` ← `clap`, all coco_* crates

---

## 9. Testing Strategy

### Unit Tests
- Lexer: token kinds, spans, edge cases (unterminated strings, invalid escapes)
- Parser: each grammar production, error recovery
- Formatter: idempotence, comment preservation

### Integration Tests
- All 21 examples must parse and format
- Golden output tests (compare formatted output to expected)

### Fuzzing
- Fuzz lexer with random bytes (should not panic)
- Fuzz parser with random token streams (should not panic)

### Performance Tests
- Benchmark lexer, parser, formatter on large files
- Ensure targets met (1ms lex, 5ms parse, 10ms fmt per 1000 lines)

---

## 10. Deliverables

```
coco-lang/
├── Cargo.toml                      # Workspace manifest
├── crates/
│   ├── coco_span/
│   │   ├── Cargo.toml
│   │   ├── src/lib.rs             # Span, Location, SourceMap
│   │   └── tests/
│   ├── coco_diagnostics/
│   │   ├── Cargo.toml
│   │   ├── src/lib.rs             # Diagnostic, emit()
│   │   └── tests/
│   ├── coco_lexer/
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs             # Lexer, Token, TokenKind
│   │   │   ├── token.rs
│   │   │   └── lexer.rs
│   │   └── tests/
│   ├── coco_syntax/
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs             # AST node definitions
│   │   │   ├── ast.rs
│   │   │   └── visitor.rs
│   │   └── tests/
│   ├── coco_parser/
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs             # Parser
│   │   │   ├── parser.rs
│   │   │   ├── pratt.rs           # Pratt parsing
│   │   │   └── recovery.rs        # Error recovery
│   │   └── tests/
│   ├── coco_formatter/
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── lib.rs             # Formatter
│   │   │   ├── format.rs
│   │   │   └── config.rs
│   │   └── tests/
│   └── coco_cli/
│       ├── Cargo.toml
│       ├── src/
│       │   └── main.rs            # CLI commands
│       └── tests/
└── examples/                       # 21 .co test files
    ├── 01-hello.co
    ├── ...
    └── 21-pipe-operator.co
```

---

## 11. Exit Criteria

Phase 2 is complete when:

1. All 7 crates compile without warnings
2. `cargo test` passes with >90% coverage
3. All 21 examples parse and format correctly
4. `coco lex`, `coco parse`, `coco fmt`, `coco check` work
5. Performance benchmarks meet targets
6. Documentation complete (rustdoc + README)
7. CI pipeline set up (GitHub Actions: build, test, clippy, fmt)
