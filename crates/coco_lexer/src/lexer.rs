use crate::token::{Token, TokenKind};
use coco_span::Span;
use unicode_xid::UnicodeXID;

#[derive(Clone)]
pub struct Lexer<'a> {
    source: &'a str,
    cursor: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        Self { source, cursor: 0 }
    }

    pub fn next_token(&mut self) -> Token {
        let start = self.cursor;

        // Skip whitespace and comments
        loop {
            if self.is_eof() {
                return Token::eof(start);
            }
            match self.current_char() {
                ' ' | '\t' | '\r' | '\n' => {
                    self.advance();
                }
                '/' if self.peek_char(1) == '/' => {
                    self.skip_line_comment();
                }
                '/' if self.peek_char(1) == '*' => {
                    self.skip_block_comment();
                }
                _ => break,
            }
        }

        let start = self.cursor;
        let ch = self.current_char();

        let kind = if is_id_start(ch) {
            self.lex_ident_or_keyword()
        } else if ch.is_ascii_digit() {
            self.lex_number()
        } else if ch == '"' {
            self.lex_string()
        } else if ch == '\'' {
            self.lex_char()
        } else if ch == '`' {
            self.lex_template_literal()
        } else {
            self.lex_operator_or_punctuation()
        };

        let end = self.cursor;
        let text = self.source[start..end].to_string();
        Token::new(kind, Span::new(start, end), text)
    }

    pub fn cursor_pos(&self) -> usize {
        self.cursor
    }

    fn is_eof(&self) -> bool {
        self.cursor >= self.source.len()
    }

    fn current_char(&self) -> char {
        self.source[self.cursor..].chars().next().unwrap_or('\0')
    }

    fn peek_char(&self, offset: usize) -> char {
        self.source[self.cursor..]
            .chars()
            .nth(offset)
            .unwrap_or('\0')
    }

    fn advance(&mut self) {
        if let Some(ch) = self.source[self.cursor..].chars().next() {
            self.cursor += ch.len_utf8();
        }
    }

    fn advance_by(&mut self, n: usize) {
        for _ in 0..n {
            self.advance();
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
        let mut depth = 1u32;
        while !self.is_eof() && depth > 0 {
            if self.current_char() == '/' && self.peek_char(1) == '*' {
                self.advance_by(2);
                depth += 1;
            } else if self.current_char() == '*' && self.peek_char(1) == '/' {
                self.advance_by(2);
                depth = depth.saturating_sub(1);
            } else {
                self.advance();
            }
        }
    }

    // --- Identifiers and keywords ---

    fn lex_ident_or_keyword(&mut self) -> TokenKind {
        let start = self.cursor;
        while !self.is_eof() && is_id_continue(self.current_char()) {
            self.advance();
        }
        let ident = &self.source[start..self.cursor];
        if let Some(kw) = TokenKind::keyword_from_str(ident) {
            return kw;
        }
        match ident.to_ascii_lowercase().as_str() {
            "and" => TokenKind::And,
            "or" => TokenKind::Or,
            "not" => TokenKind::Not,
            "xor" => TokenKind::BitXor,
            _ => TokenKind::Ident,
        }
    }

    // --- Numbers ---

    fn lex_number(&mut self) -> TokenKind {
        if self.current_char() == '0' {
            match self.peek_char(1) {
                'x' | 'X' => return self.lex_hex_number(),
                'b' | 'B' => return self.lex_bin_number(),
                'o' | 'O' => return self.lex_oct_number(),
                _ => {}
            }
        }
        self.lex_decimal_number()
    }

    fn lex_hex_number(&mut self) -> TokenKind {
        self.advance_by(2); // 0x
        while !self.is_eof()
            && (self.current_char().is_ascii_hexdigit() || self.current_char() == '_')
        {
            self.advance();
        }
        TokenKind::IntLiteral
    }

    fn lex_bin_number(&mut self) -> TokenKind {
        self.advance_by(2); // 0b
        while !self.is_eof()
            && (self.current_char() == '0'
                || self.current_char() == '1'
                || self.current_char() == '_')
        {
            self.advance();
        }
        TokenKind::IntLiteral
    }

    fn lex_oct_number(&mut self) -> TokenKind {
        self.advance_by(2); // 0o
        while !self.is_eof() && (self.current_char().is_digit(8) || self.current_char() == '_') {
            self.advance();
        }
        TokenKind::IntLiteral
    }

    fn lex_decimal_number(&mut self) -> TokenKind {
        while !self.is_eof() && (self.current_char().is_ascii_digit() || self.current_char() == '_')
        {
            self.advance();
        }
        if self.current_char() == '.' && self.peek_char(1).is_ascii_digit() {
            self.advance(); // .
            while !self.is_eof()
                && (self.current_char().is_ascii_digit() || self.current_char() == '_')
            {
                self.advance();
            }
            self.lex_exponent_part();
            return TokenKind::FloatLiteral;
        }
        if self.current_char() == 'e' || self.current_char() == 'E' {
            self.lex_exponent_part();
            return TokenKind::FloatLiteral;
        }
        TokenKind::IntLiteral
    }

    fn lex_exponent_part(&mut self) {
        if self.current_char() == 'e' || self.current_char() == 'E' {
            self.advance();
            if self.current_char() == '+' || self.current_char() == '-' {
                self.advance();
            }
            while !self.is_eof() && self.current_char().is_ascii_digit() {
                self.advance();
            }
        }
    }

    // --- Strings ---

    fn lex_string(&mut self) -> TokenKind {
        self.advance(); // opening "
        while !self.is_eof() && self.current_char() != '"' {
            if self.current_char() == '\\' {
                self.advance(); // backslash
                if !self.is_eof() {
                    self.advance(); // escaped char
                }
            } else {
                self.advance();
            }
        }
        if !self.is_eof() {
            self.advance(); // closing "
        }
        TokenKind::StringLiteral
    }

    fn lex_char(&mut self) -> TokenKind {
        self.advance(); // opening '
        if !self.is_eof() && self.current_char() != '\'' {
            if self.current_char() == '\\' {
                self.advance(); // backslash
                if !self.is_eof() {
                    self.advance(); // escaped char
                }
            } else {
                self.advance(); // content
            }
        }
        if !self.is_eof() {
            self.advance(); // closing '
        }
        TokenKind::CharLiteral
    }

    fn lex_template_literal(&mut self) -> TokenKind {
        self.advance(); // opening `
        while !self.is_eof() && self.current_char() != '`' {
            if self.current_char() == '\\' {
                self.advance();
                if !self.is_eof() {
                    self.advance();
                }
            } else if self.current_char() == '$' && self.peek_char(1) == '{' {
                self.advance_by(2); // ${
                let mut brace_depth = 1u32;
                while !self.is_eof() && brace_depth > 0 {
                    match self.current_char() {
                        '{' => brace_depth += 1,
                        '}' => brace_depth = brace_depth.saturating_sub(1),
                        '"' => {
                            self.advance();
                            while !self.is_eof() && self.current_char() != '"' {
                                if self.current_char() == '\\' {
                                    self.advance();
                                }
                                self.advance();
                            }
                            continue;
                        }
                        '\'' => {
                            self.advance();
                            if !self.is_eof() {
                                self.advance();
                                if !self.is_eof() {
                                    self.advance();
                                }
                            }
                            continue;
                        }
                        '`' => {
                            // nested template; skip for now
                        }
                        _ => {}
                    }
                    if !self.is_eof() {
                        self.advance();
                    }
                }
            } else {
                self.advance();
            }
        }
        if !self.is_eof() {
            self.advance(); // closing `
        }
        TokenKind::TemplateLiteral
    }

    // --- Operators and punctuation ---

    fn lex_operator_or_punctuation(&mut self) -> TokenKind {
        let ch = self.current_char();
        self.advance();
        match ch {
            '+' => match self.current_char() {
                '=' => {
                    self.advance();
                    TokenKind::PlusEq
                }
                '+' => {
                    self.advance();
                    TokenKind::Error
                } // ++ not valid as standalone token
                _ => TokenKind::Plus,
            },
            '-' => match self.current_char() {
                '=' => {
                    self.advance();
                    TokenKind::MinusEq
                }
                '-' => {
                    self.advance();
                    TokenKind::Error
                } // -- not valid as standalone
                _ => TokenKind::Minus,
            },
            '*' => match self.current_char() {
                '=' => {
                    self.advance();
                    TokenKind::StarEq
                }
                '*' => {
                    self.advance();
                    if self.current_char() == '=' {
                        self.advance();
                        TokenKind::StarStarEq
                    } else {
                        TokenKind::StarStar
                    }
                }
                _ => TokenKind::Star,
            },
            '/' => match self.current_char() {
                '=' => {
                    self.advance();
                    TokenKind::SlashEq
                }
                _ => TokenKind::Slash,
            },
            '%' => match self.current_char() {
                '=' => {
                    self.advance();
                    TokenKind::PercentEq
                }
                _ => TokenKind::Percent,
            },
            '=' => match self.current_char() {
                '=' => {
                    self.advance();
                    TokenKind::Eq
                }
                '>' => {
                    self.advance();
                    TokenKind::FatArrow
                }
                _ => TokenKind::Assign,
            },
            '!' => match self.current_char() {
                '=' => {
                    self.advance();
                    TokenKind::Ne
                }
                _ => TokenKind::Not,
            },
            '<' => match self.current_char() {
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
                '|' => {
                    self.advance();
                    TokenKind::PipeLeft
                }
                _ => TokenKind::Lt,
            },
            '>' => match self.current_char() {
                '=' => {
                    self.advance();
                    TokenKind::Ge
                }
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
            },
            '&' => match self.current_char() {
                '&' => {
                    self.advance();
                    TokenKind::And
                }
                '=' => {
                    self.advance();
                    TokenKind::BitAndEq
                }
                _ => TokenKind::BitAnd,
            },
            '|' => match self.current_char() {
                '|' => {
                    self.advance();
                    TokenKind::Or
                }
                '=' => {
                    self.advance();
                    TokenKind::BitOrEq
                }
                '>' => {
                    self.advance();
                    TokenKind::PipeRight
                }
                _ => TokenKind::BitOr,
            },
            '^' => match self.current_char() {
                '=' => {
                    self.advance();
                    TokenKind::BitXorEq
                }
                _ => TokenKind::BitXor,
            },
            '~' => TokenKind::BitNot,
            '?' => match self.current_char() {
                '.' => {
                    self.advance();
                    TokenKind::QuestionDot
                }
                '?' => {
                    self.advance();
                    TokenKind::QuestionQuestion
                }
                ':' => {
                    self.advance();
                    TokenKind::QuestionColon
                }
                _ => TokenKind::Question,
            },
            '$' => {
                if self.current_char() == '$' {
                    self.advance();
                    TokenKind::DollarDollar
                } else {
                    TokenKind::Dollar
                }
            }
            '{' => TokenKind::LBrace,
            '}' => TokenKind::RBrace,
            '(' => TokenKind::LParen,
            ')' => TokenKind::RParen,
            '[' => TokenKind::LBracket,
            ']' => TokenKind::RBracket,
            ',' => TokenKind::Comma,
            ';' => TokenKind::Semi,
            ':' => TokenKind::Colon,
            '.' => {
                if self.current_char() == '.' {
                    self.advance();
                    if self.current_char() == '=' {
                        self.advance();
                        TokenKind::RangeInclusive
                    } else {
                        TokenKind::Range
                    }
                } else {
                    TokenKind::Dot
                }
            }
            _ => TokenKind::Error,
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

    fn lex_all(source: &str) -> Vec<TokenKind> {
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
        let tokens = lex_all("fn class const let if for async await return while match");
        assert_eq!(
            tokens,
            vec![
                TokenKind::Fn,
                TokenKind::Class,
                TokenKind::Const,
                TokenKind::Let,
                TokenKind::If,
                TokenKind::For,
                TokenKind::Async,
                TokenKind::Await,
                TokenKind::Return,
                TokenKind::While,
                TokenKind::Match,
            ]
        );
    }

    #[test]
    fn test_identifiers() {
        let tokens = lex_all("hello world foo_bar x1 _temp");
        assert_eq!(
            tokens,
            vec![
                TokenKind::Ident,
                TokenKind::Ident,
                TokenKind::Ident,
                TokenKind::Ident,
                TokenKind::Ident,
            ]
        );
    }

    #[test]
    fn test_numbers() {
        let tokens = lex_all("42 3.14 0xFF 0b1010 0o755 1_000_000");
        assert_eq!(
            tokens,
            vec![
                TokenKind::IntLiteral,
                TokenKind::FloatLiteral,
                TokenKind::IntLiteral,
                TokenKind::IntLiteral,
                TokenKind::IntLiteral,
                TokenKind::IntLiteral,
            ]
        );
    }

    #[test]
    fn test_operators() {
        let tokens = lex_all("+ - * / % ** == != < > <= >= <=> && || ! ?. ?? ?: |> <| $$");
        assert_eq!(
            tokens,
            vec![
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
            ]
        );
    }

    #[test]
    fn test_strings() {
        let tokens = lex_all(r#""hello" 'a' `template ${x}`"#);
        assert_eq!(
            tokens,
            vec![
                TokenKind::StringLiteral,
                TokenKind::CharLiteral,
                TokenKind::TemplateLiteral,
            ]
        );
    }

    #[test]
    fn test_delimiters() {
        let tokens = lex_all("{ } ( ) [ ] , ; : .");
        assert_eq!(
            tokens,
            vec![
                TokenKind::LBrace,
                TokenKind::RBrace,
                TokenKind::LParen,
                TokenKind::RParen,
                TokenKind::LBracket,
                TokenKind::RBracket,
                TokenKind::Comma,
                TokenKind::Semi,
                TokenKind::Colon,
                TokenKind::Dot,
            ]
        );
    }

    #[test]
    fn test_compound_assignment() {
        let tokens = lex_all("+= -= *= /= %= **= <<= >>= &= |= ^=");
        assert_eq!(
            tokens,
            vec![
                TokenKind::PlusEq,
                TokenKind::MinusEq,
                TokenKind::StarEq,
                TokenKind::SlashEq,
                TokenKind::PercentEq,
                TokenKind::StarStarEq,
                TokenKind::ShlEq,
                TokenKind::ShrEq,
                TokenKind::BitAndEq,
                TokenKind::BitOrEq,
                TokenKind::BitXorEq,
            ]
        );
    }

    #[test]
    fn test_comment_skipping() {
        let tokens = lex_all("x // comment\ny");
        assert_eq!(tokens, vec![TokenKind::Ident, TokenKind::Ident,]);
    }

    #[test]
    fn test_block_comment_skipping() {
        let tokens = lex_all("x /* block */ y");
        assert_eq!(tokens, vec![TokenKind::Ident, TokenKind::Ident,]);
    }

    #[test]
    fn test_dollar() {
        let tokens = lex_all("$ $$");
        assert_eq!(tokens, vec![TokenKind::Dollar, TokenKind::DollarDollar,]);
    }
}
