use coco_span::Span;

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
    pub text: String,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span, text: String) -> Self {
        Self { kind, span, text }
    }

    pub fn eof(offset: usize) -> Self {
        Self {
            kind: TokenKind::Eof,
            span: Span::new(offset, offset),
            text: String::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    // Keywords
    As,
    Async,
    Await,
    Break,
    Case,
    Catch,
    Class,
    Const,
    Constructor,
    Continue,
    Coro,
    Do,
    Else,
    Enum,
    Export,
    Extends,
    False,
    Finally,
    Fn,
    For,
    From,
    Function,
    If,
    Implements,
    Import,
    In,
    Interface,
    Is,
    Lazy,
    Let,
    Loop,
    Match,
    New,
    Null,
    Of,
    Parallel,
    Private,
    Protected,
    Public,
    Readonly,
    Return,
    Run,
    Select,
    Static,
    Synchronized,
    This,
    Throw,
    Trait,
    True,
    Try,
    Type,
    Typeof,
    Unsafe,
    Use,
    Void,
    While,
    Ok,
    Err,
    Result,

    // Identifiers and Literals
    Ident,
    IntLiteral,
    FloatLiteral,
    StringLiteral,
    CharLiteral,
    TemplateLiteral,
    BoolLiteral,

    // Operators
    Assign,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    StarStar,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    Spaceship,
    And,
    Or,
    Not,
    BitAnd,
    BitOr,
    BitXor,
    BitNot,
    Shl,
    Shr,
    Question,
    QuestionDot,
    QuestionQuestion,
    QuestionColon,
    PipeRight,
    PipeLeft,
    DollarDollar,

    // Compound assignment operators
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    PercentEq,
    StarStarEq,
    ShlEq,
    ShrEq,
    BitAndEq,
    BitOrEq,
    BitXorEq,

    // Delimiters
    LBrace,
    RBrace,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
    Semi,
    Colon,
    Dot,
    Arrow,
    FatArrow,
    Range,
    RangeInclusive,

    // Special
    Dollar,
    Eof,
    Error,
}

impl TokenKind {
    /// Look up a keyword by string, returning the corresponding TokenKind.
    pub fn keyword_from_str(s: &str) -> Option<Self> {
        Some(match s {
            "as" => Self::As,
            "async" => Self::Async,
            "await" => Self::Await,
            "break" => Self::Break,
            "case" => Self::Case,
            "catch" => Self::Catch,
            "class" => Self::Class,
            "const" => Self::Const,
            "constructor" => Self::Constructor,
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
            "from" => Self::From,
            "function" => Self::Function,
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
        false // Whitespace and comments are skipped by the lexer, never emitted as tokens
    }

    /// Check if this token kind is a synchronisation point for error recovery.
    pub fn is_sync_point(&self) -> bool {
        matches!(
            self,
            Self::Semi
                | Self::RBrace
                | Self::Fn
                | Self::Class
                | Self::Const
                | Self::Let
                | Self::If
                | Self::For
                | Self::While
                | Self::Loop
                | Self::Return
                | Self::Enum
                | Self::Interface
                | Self::Trait
                | Self::Import
                | Self::Export
                | Self::Async
                | Self::Eof
        )
    }
}
