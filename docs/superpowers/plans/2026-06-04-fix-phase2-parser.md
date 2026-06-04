# Fix Phase 2 Parser — Expression Integration & Bug Fixes

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire the existing Pratt expression parser into the main parser, fix correctness bugs, and add tests so Phase 2 can legitimately be called "implemented."

**Architecture:** The `ExprParser` in `expr.rs` is a complete Pratt parser (1000+ lines) but is currently dead code — the main `Parser` in `parser.rs` uses a stub that only handles Ident/IntLiteral. We inline the expression parsing logic directly into `Parser` rather than keeping `ExprParser` as a separate struct (avoids lifetime issues with `&'a mut Lexer<'a>`). We also fix `parse_block`, remove panicking `.unwrap()` calls, add `Range`/`RangeInclusive` to `BinaryOp`, and add parser+formatter tests.

**Tech Stack:** Rust stable, cargo test, coco_parser crate, coco_syntax crate, coco_formatter crate

---

### Task 1: Add Range ops to BinaryOp enum

**Files:**
- Modify: `crates/coco_syntax/src/ast.rs:484-521` (BinaryOp enum)

- [ ] **Step 1: Add Range and RangeInclusive variants to BinaryOp**

In `crates/coco_syntax/src/ast.rs`, add two new variants to the `BinaryOp` enum after `BitXorAssign`:

```rust
    BitXorAssign,
    Range,
    RangeInclusive,
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p coco_syntax 2>&1`
Expected: success (new variants are unused but that's fine)

- [ ] **Step 3: Commit**

```bash
git add crates/coco_syntax/src/ast.rs
git commit -m "feat(syntax): add Range and RangeInclusive to BinaryOp"
```

---

### Task 2: Rewrite expression parsing — inline Pratt parser into Parser

**Files:**
- Modify: `crates/coco_parser/src/parser.rs:1025-1068` (replace parse_expr / parse_expr_pratt / parse_type stubs)
- Modify: `crates/coco_parser/src/expr.rs` (extract free functions, remove ExprParser struct)
- Modify: `crates/coco_parser/src/lib.rs` (make expr module public for helpers)

The strategy: move expression parsing methods directly into `Parser`. We keep `expr.rs` only for free helper functions (`infix_bp`, `token_to_binary_op`, `parse_int_literal`, `parse_primitive_type`). The `ExprParser` struct is deleted.

- [ ] **Step 1: Make expr.rs export only free functions**

Replace the entire `ExprParser` struct and its `impl` block in `crates/coco_parser/src/expr.rs` with just the free functions that the parser will use. Keep these functions (already at bottom of file):

```rust
//! Expression parsing helpers for the Coco parser.
//!
//! Free functions used by Parser for Pratt expression parsing.

use coco_lexer::TokenKind;
use coco_syntax::*;

/// Returns (left_bp, right_bp) for infix operators.
/// Returns None if the token is not an infix operator.
pub fn infix_bp(kind: TokenKind) -> Option<(u8, u8)> {
    match kind {
        // Assignment (right-associative)
        TokenKind::Assign | TokenKind::PlusEq | TokenKind::MinusEq
        | TokenKind::StarEq | TokenKind::SlashEq | TokenKind::PercentEq
        | TokenKind::StarStarEq | TokenKind::ShlEq | TokenKind::ShrEq
        | TokenKind::BitAndEq | TokenKind::BitOrEq | TokenKind::BitXorEq => Some((10, 9)),

        // Elvis (right-associative)
        TokenKind::QuestionColon => Some((20, 19)),

        // Null coalesce (left-associative)
        TokenKind::QuestionQuestion => Some((25, 26)),

        // Logical OR (left-associative)
        TokenKind::Or => Some((30, 31)),

        // Logical AND (left-associative)
        TokenKind::And => Some((35, 36)),

        // Bitwise OR (left-associative)
        TokenKind::BitOr => Some((40, 41)),

        // Bitwise XOR (left-associative)
        TokenKind::BitXor => Some((45, 46)),

        // Bitwise AND (left-associative)
        TokenKind::BitAnd => Some((50, 51)),

        // Equality (left-associative)
        TokenKind::Eq | TokenKind::Ne => Some((55, 56)),

        // Comparison (left-associative)
        TokenKind::Lt | TokenKind::Gt | TokenKind::Le
        | TokenKind::Ge | TokenKind::Spaceship => Some((60, 61)),

        // Shift (left-associative)
        TokenKind::Shl | TokenKind::Shr => Some((65, 66)),

        // Range (non-associative, use left-assoc with same bp to prevent chaining)
        TokenKind::Range | TokenKind::RangeInclusive => Some((68, 69)),

        // Additive (left-associative)
        TokenKind::Plus | TokenKind::Minus => Some((70, 71)),

        // Multiplicative (left-associative)
        TokenKind::Star | TokenKind::Slash | TokenKind::Percent => Some((75, 76)),

        // Exponentiation (right-associative)
        TokenKind::StarStar => Some((80, 79)),

        // Pipe (left-associative)
        TokenKind::PipeRight | TokenKind::PipeLeft => Some((85, 86)),

        _ => None,
    }
}

pub fn token_to_binary_op(kind: TokenKind) -> Option<BinaryOp> {
    Some(match kind {
        TokenKind::Assign => BinaryOp::Assign,
        TokenKind::Plus => BinaryOp::Add,
        TokenKind::Minus => BinaryOp::Sub,
        TokenKind::Star => BinaryOp::Mul,
        TokenKind::Slash => BinaryOp::Div,
        TokenKind::Percent => BinaryOp::Mod,
        TokenKind::StarStar => BinaryOp::Pow,
        TokenKind::Eq => BinaryOp::Eq,
        TokenKind::Ne => BinaryOp::Ne,
        TokenKind::Lt => BinaryOp::Lt,
        TokenKind::Gt => BinaryOp::Gt,
        TokenKind::Le => BinaryOp::Le,
        TokenKind::Ge => BinaryOp::Ge,
        TokenKind::Spaceship => BinaryOp::Spaceship,
        TokenKind::And => BinaryOp::And,
        TokenKind::Or => BinaryOp::Or,
        TokenKind::BitAnd => BinaryOp::BitAnd,
        TokenKind::BitOr => BinaryOp::BitOr,
        TokenKind::BitXor => BinaryOp::BitXor,
        TokenKind::Shl => BinaryOp::Shl,
        TokenKind::Shr => BinaryOp::Shr,
        TokenKind::Range => BinaryOp::Range,
        TokenKind::RangeInclusive => BinaryOp::RangeInclusive,
        TokenKind::PlusEq => BinaryOp::AddAssign,
        TokenKind::MinusEq => BinaryOp::SubAssign,
        TokenKind::StarEq => BinaryOp::MulAssign,
        TokenKind::SlashEq => BinaryOp::DivAssign,
        TokenKind::PercentEq => BinaryOp::ModAssign,
        TokenKind::StarStarEq => BinaryOp::PowAssign,
        TokenKind::ShlEq => BinaryOp::ShlAssign,
        TokenKind::ShrEq => BinaryOp::ShrAssign,
        TokenKind::BitAndEq => BinaryOp::BitAndAssign,
        TokenKind::BitOrEq => BinaryOp::BitOrAssign,
        TokenKind::BitXorEq => BinaryOp::BitXorAssign,
        _ => return None,
    })
}

pub fn parse_primitive_type(name: &str) -> Option<PrimitiveType> {
    Some(match name {
        "int" => PrimitiveType::Int,
        "uint" => PrimitiveType::Uint,
        "float" => PrimitiveType::Float,
        "bool" => PrimitiveType::Bool,
        "string" => PrimitiveType::String,
        "char" => PrimitiveType::Char,
        "byte" => PrimitiveType::Byte,
        "null" => PrimitiveType::Null,
        "void" => PrimitiveType::Void,
        "never" => PrimitiveType::Never,
        "mixed" => PrimitiveType::Mixed,
        _ => return None,
    })
}

pub fn parse_int_literal(text: &str) -> i64 {
    let text = text.replace('_', "");
    if text.starts_with("0x") || text.starts_with("0X") {
        i64::from_str_radix(&text[2..], 16).unwrap_or(0)
    } else if text.starts_with("0b") || text.starts_with("0B") {
        i64::from_str_radix(&text[2..], 2).unwrap_or(0)
    } else if text.starts_with("0o") || text.starts_with("0O") {
        i64::from_str_radix(&text[2..], 8).unwrap_or(0)
    } else {
        text.parse().unwrap_or(0)
    }
}
```

- [ ] **Step 2: Replace parse_expr/parse_expr_pratt/parse_type in parser.rs**

Replace the stub methods `parse_expr`, `parse_expr_pratt`, and `parse_type` (lines 1025-1068 of `parser.rs`) with a full Pratt parser inlined into Parser. The new `parse_expr` method delegates to `parse_expr_bp(0)`:

```rust
    // ============================================================
    // Expression parsing (Pratt / precedence climbing)
    // ============================================================

    fn parse_expr(&mut self) -> Expr {
        self.parse_expr_bp(0)
    }

    fn parse_expr_bp(&mut self, min_bp: u8) -> Expr {
        let mut lhs = self.parse_prefix_expr();

        loop {
            lhs = self.parse_postfix(lhs);

            if self.current.kind == TokenKind::Eof
                || self.current.kind == TokenKind::Semi
                || self.current.kind == TokenKind::RBrace
                || self.current.kind == TokenKind::RParen
                || self.current.kind == TokenKind::RBracket
                || self.current.kind == TokenKind::Comma
                || self.current.kind == TokenKind::Colon
                || self.current.kind == TokenKind::FatArrow
            {
                break;
            }

            let Some((left_bp, right_bp)) = crate::expr::infix_bp(self.current.kind) else {
                break;
            };
            if left_bp < min_bp {
                break;
            }

            // Ternary
            if self.current.kind == TokenKind::Question {
                lhs = self.parse_ternary_expr(lhs);
                continue;
            }

            // Range
            if self.current.kind == TokenKind::Range || self.current.kind == TokenKind::RangeInclusive {
                let op = if self.current.kind == TokenKind::RangeInclusive {
                    BinaryOp::RangeInclusive
                } else {
                    BinaryOp::Range
                };
                self.advance();
                let rhs = self.parse_expr_bp(right_bp);
                let span = Span::new(lhs.span_start(), rhs.span_end());
                lhs = Expr::Binary(Box::new(BinaryExpr { span, left: lhs, op, right: rhs }));
                continue;
            }

            // Pipe operators
            if self.current.kind == TokenKind::PipeRight || self.current.kind == TokenKind::PipeLeft {
                let op = if self.current.kind == TokenKind::PipeRight {
                    PipeOp::PipeRight
                } else {
                    PipeOp::PipeLeft
                };
                self.advance();
                let rhs = self.parse_expr_bp(right_bp);
                let span = Span::new(lhs.span_start(), rhs.span_end());
                lhs = Expr::Pipe(Box::new(PipeExpr { span, left: lhs, op, right: rhs }));
                continue;
            }

            // Null coalesce
            if self.current.kind == TokenKind::QuestionQuestion {
                self.advance();
                let rhs = self.parse_expr_bp(right_bp);
                let span = Span::new(lhs.span_start(), rhs.span_end());
                lhs = Expr::NullCoalesce(Box::new(NullCoalesceExpr { span, left: lhs, right: rhs }));
                continue;
            }

            // Elvis
            if self.current.kind == TokenKind::QuestionColon {
                self.advance();
                let rhs = self.parse_expr_bp(right_bp);
                let span = Span::new(lhs.span_start(), rhs.span_end());
                lhs = Expr::Elvis(Box::new(ElvisExpr { span, left: lhs, right: rhs }));
                continue;
            }

            // General binary
            if let Some(binop) = crate::expr::token_to_binary_op(self.current.kind) {
                self.advance();
                let rhs = self.parse_expr_bp(right_bp);
                let span = Span::new(lhs.span_start(), rhs.span_end());
                lhs = Expr::Binary(Box::new(BinaryExpr { span, left: lhs, op: binop, right: rhs }));
                continue;
            }

            break;
        }

        lhs
    }

    fn parse_prefix_expr(&mut self) -> Expr {
        let start = self.current.span.start;
        match self.current.kind {
            TokenKind::Minus => {
                self.advance();
                let expr = self.parse_expr_bp(90);
                Expr::Unary(Box::new(UnaryExpr {
                    span: Span::new(start, expr.span_end()),
                    op: UnaryOp::Neg,
                    expr,
                }))
            }
            TokenKind::Not => {
                self.advance();
                let expr = self.parse_expr_bp(90);
                Expr::Unary(Box::new(UnaryExpr {
                    span: Span::new(start, expr.span_end()),
                    op: UnaryOp::Not,
                    expr,
                }))
            }
            TokenKind::BitNot => {
                self.advance();
                let expr = self.parse_expr_bp(90);
                Expr::Unary(Box::new(UnaryExpr {
                    span: Span::new(start, expr.span_end()),
                    op: UnaryOp::BitNot,
                    expr,
                }))
            }
            TokenKind::Typeof => {
                self.advance();
                let expr = self.parse_expr_bp(90);
                Expr::Unary(Box::new(UnaryExpr {
                    span: Span::new(start, expr.span_end()),
                    op: UnaryOp::Typeof,
                    expr,
                }))
            }
            TokenKind::Await => {
                self.advance();
                let expr = self.parse_expr_bp(90);
                Expr::Unary(Box::new(UnaryExpr {
                    span: Span::new(start, expr.span_end()),
                    op: UnaryOp::Await,
                    expr,
                }))
            }
            TokenKind::New => {
                self.advance();
                let type_name = self.parse_ident().unwrap_or(Ident {
                    name: String::new(),
                    span: Span::new(start, start),
                });
                let args = if self.current.kind == TokenKind::LParen {
                    self.parse_arg_list()
                } else {
                    Vec::new()
                };
                let end = self.current.span.end;
                Expr::New(Box::new(NewExpr {
                    span: Span::new(start, end),
                    type_name,
                    args,
                }))
            }
            _ => self.parse_primary_expr(),
        }
    }

    fn parse_primary_expr(&mut self) -> Expr {
        let start = self.current.span.start;
        match self.current.kind {
            TokenKind::Ident => {
                let ident = self.parse_ident().unwrap();
                Expr::Ident(ident)
            }
            TokenKind::IntLiteral => {
                let text = self.current.text.clone();
                let span = self.current.span;
                self.advance();
                let val = crate::expr::parse_int_literal(&text);
                Expr::Literal(Literal::Int(val, span))
            }
            TokenKind::FloatLiteral => {
                let text = self.current.text.clone();
                let span = self.current.span;
                self.advance();
                let val: f64 = text.replace('_', "").parse().unwrap_or(0.0);
                Expr::Literal(Literal::Float(val, span))
            }
            TokenKind::StringLiteral => {
                let text = self.current.text.clone();
                let span = self.current.span;
                self.advance();
                let inner = if text.len() >= 2 {
                    text[1..text.len() - 1].to_string()
                } else {
                    String::new()
                };
                Expr::Literal(Literal::String(inner, span))
            }
            TokenKind::CharLiteral => {
                let text = self.current.text.clone();
                let span = self.current.span;
                self.advance();
                let inner = text.chars().nth(1).unwrap_or('\0');
                Expr::Literal(Literal::Char(inner, span))
            }
            TokenKind::True => {
                let span = self.current.span;
                self.advance();
                Expr::Literal(Literal::Bool(true, span))
            }
            TokenKind::False => {
                let span = self.current.span;
                self.advance();
                Expr::Literal(Literal::Bool(false, span))
            }
            TokenKind::Null => {
                let span = self.current.span;
                self.advance();
                Expr::Literal(Literal::Null(span))
            }
            TokenKind::This => {
                let span = self.current.span;
                self.advance();
                Expr::This(span)
            }
            TokenKind::Dollar => {
                let span = self.current.span;
                self.advance();
                Expr::Dollar(span)
            }
            TokenKind::DollarDollar => {
                let span = self.current.span;
                self.advance();
                Expr::DollarDollar(span)
            }
            TokenKind::LParen => {
                self.advance();
                let expr = self.parse_expr_bp(0);
                self.eat(TokenKind::RParen);
                Expr::Group(Box::new(expr))
            }
            TokenKind::LBracket => self.parse_array_literal(),
            TokenKind::LBrace => self.parse_object_literal(),
            TokenKind::Match => self.parse_match_expr(),
            TokenKind::Async | TokenKind::Fn | TokenKind::Function => self.parse_lambda_expr(),
            _ => {
                let span = self.current.span;
                self.advance();
                Expr::Literal(Literal::Null(span))
            }
        }
    }

    fn parse_postfix(&mut self, lhs: Expr) -> Expr {
        let mut result = lhs;
        loop {
            match self.current.kind {
                TokenKind::Dot => {
                    self.advance();
                    let name = self.parse_ident().unwrap_or(Ident {
                        name: String::new(),
                        span: Span::new(0, 0),
                    });
                    let span = Span::new(result.span_start(), name.span.end);
                    result = Expr::Member(Box::new(MemberExpr {
                        span,
                        object: result,
                        property: name,
                        optional: false,
                    }));
                }
                TokenKind::QuestionDot => {
                    self.advance();
                    let name = self.parse_ident().unwrap_or(Ident {
                        name: String::new(),
                        span: Span::new(0, 0),
                    });
                    let span = Span::new(result.span_start(), name.span.end);
                    result = Expr::Member(Box::new(MemberExpr {
                        span,
                        object: result,
                        property: name,
                        optional: true,
                    }));
                }
                TokenKind::LBracket => {
                    self.advance();
                    let index = self.parse_expr_bp(0);
                    self.eat(TokenKind::RBracket);
                    let span = Span::new(result.span_start(), self.current.span.end);
                    result = Expr::Index(Box::new(IndexExpr {
                        span,
                        object: result,
                        index,
                    }));
                }
                TokenKind::LParen => {
                    let args = self.parse_arg_list();
                    let span = Span::new(result.span_start(), self.current.span.end);
                    result = Expr::Call(Box::new(CallExpr {
                        span,
                        callee: result,
                        args,
                    }));
                }
                _ => break,
            }
        }
        result
    }

    fn parse_ternary_expr(&mut self, condition: Expr) -> Expr {
        self.advance(); // eat ?
        let then_expr = self.parse_expr_bp(0);
        self.eat(TokenKind::Colon);
        let else_expr = self.parse_expr_bp(0);
        let span = Span::new(condition.span_start(), else_expr.span_end());
        Expr::Ternary(Box::new(TernaryExpr {
            span,
            condition,
            then_expr,
            else_expr,
        }))
    }

    fn parse_arg_list(&mut self) -> Vec<Argument> {
        self.eat(TokenKind::LParen);
        let mut args = Vec::new();
        if self.current.kind != TokenKind::RParen {
            loop {
                let start = self.current.span.start;
                let value = self.parse_expr_bp(0);
                let end = value.span_end();
                args.push(Argument {
                    span: Span::new(start, end),
                    name: None,
                    value,
                });
                if !self.eat(TokenKind::Comma) {
                    break;
                }
            }
        }
        self.eat(TokenKind::RParen);
        args
    }

    fn parse_array_literal(&mut self) -> Expr {
        let start = self.current.span.start;
        self.advance(); // [
        let mut elements = Vec::new();
        if self.current.kind != TokenKind::RBracket {
            loop {
                elements.push(self.parse_expr_bp(0));
                if !self.eat(TokenKind::Comma) {
                    break;
                }
                if self.current.kind == TokenKind::RBracket {
                    break;
                }
            }
        }
        let end = self.current.span.end;
        self.eat(TokenKind::RBracket);
        Expr::Array(ArrayLiteral {
            span: Span::new(start, end),
            elements,
        })
    }

    fn parse_object_literal(&mut self) -> Expr {
        let start = self.current.span.start;
        self.advance(); // {
        let mut fields = Vec::new();
        if self.current.kind != TokenKind::RBrace {
            loop {
                let key_start = self.current.span.start;
                let key = match self.current.kind {
                    TokenKind::Ident => {
                        let name = self.parse_ident().unwrap();
                        ObjectKey::Ident(name)
                    }
                    TokenKind::StringLiteral => {
                        let text = self.current.text.clone();
                        let span = self.current.span;
                        self.advance();
                        ObjectKey::String(text[1..text.len() - 1].to_string(), span)
                    }
                    _ => break,
                };
                self.eat(TokenKind::Colon);
                let value = self.parse_expr_bp(0);
                let key_end = value.span_end();
                fields.push(ObjectField {
                    span: Span::new(key_start, key_end),
                    key,
                    value,
                });
                if !self.eat(TokenKind::Comma) {
                    break;
                }
                if self.current.kind == TokenKind::RBrace {
                    break;
                }
            }
        }
        let end = self.current.span.end;
        self.eat(TokenKind::RBrace);
        Expr::Object(ObjectLiteral {
            span: Span::new(start, end),
            fields,
        })
    }

    fn parse_match_expr(&mut self) -> Expr {
        let start = self.current.span.start;
        self.advance(); // match
        let scrutinee = self.parse_expr_bp(0);
        self.eat(TokenKind::LBrace);
        let mut arms = Vec::new();
        while self.current.kind != TokenKind::RBrace && self.current.kind != TokenKind::Eof {
            let arm_start = self.current.span.start;
            let pattern = self.parse_pattern();
            self.eat(TokenKind::FatArrow);
            let body = self.parse_expr_bp(0);
            let arm_end = body.span_end();
            self.eat(TokenKind::Comma);
            arms.push(MatchArm {
                span: Span::new(arm_start, arm_end),
                pattern,
                body,
            });
        }
        let end = self.current.span.end;
        self.eat(TokenKind::RBrace);
        Expr::Match(Box::new(MatchExpr {
            span: Span::new(start, end),
            scrutinee,
            arms,
        }))
    }

    fn parse_pattern(&mut self) -> Pattern {
        match self.current.kind {
            TokenKind::Ident => {
                let ident = self.parse_ident().unwrap();
                Pattern::Ident(ident)
            }
            TokenKind::IntLiteral => {
                let text = self.current.text.clone();
                let span = self.current.span;
                self.advance();
                Pattern::Literal(Literal::Int(crate::expr::parse_int_literal(&text), span))
            }
            TokenKind::StringLiteral => {
                let text = self.current.text.clone();
                let span = self.current.span;
                self.advance();
                let inner = text[1..text.len() - 1].to_string();
                Pattern::Literal(Literal::String(inner, span))
            }
            TokenKind::True => {
                let span = self.current.span;
                self.advance();
                Pattern::Literal(Literal::Bool(true, span))
            }
            TokenKind::False => {
                let span = self.current.span;
                self.advance();
                Pattern::Literal(Literal::Bool(false, span))
            }
            TokenKind::Null => {
                let span = self.current.span;
                self.advance();
                Pattern::Literal(Literal::Null(span))
            }
            _ => {
                let span = self.current.span;
                self.advance();
                Pattern::Wildcard(span)
            }
        }
    }

    fn parse_lambda_expr(&mut self) -> Expr {
        let start = self.current.span.start;
        let is_async = self.eat(TokenKind::Async);
        let _ = self.eat(TokenKind::Fn);
        let _ = self.eat(TokenKind::Function);

        let params = self.parse_params().unwrap_or_default();
        let return_type = if self.eat(TokenKind::Colon) {
            Some(self.parse_type())
        } else {
            None
        };
        self.eat(TokenKind::FatArrow);
        let body = if self.current.kind == TokenKind::LBrace {
            let block = self.parse_block().unwrap_or(Block {
                span: Span::new(start, start),
                stmts: Vec::new(),
            });
            LambdaBody::Block(block)
        } else {
            let expr = self.parse_expr_bp(0);
            LambdaBody::Expr(expr)
        };
        let end = match &body {
            LambdaBody::Block(b) => b.span.end,
            LambdaBody::Expr(e) => e.span_end(),
        };
        Expr::Lambda(Box::new(Lambda {
            span: Span::new(start, end),
            is_async,
            params,
            return_type,
            body,
        }))
    }

    // ============================================================
    // Type parsing
    // ============================================================

    fn parse_type(&mut self) -> Type {
        let mut ty = self.parse_intersection_type();
        while self.current.kind == TokenKind::BitOr {
            self.advance();
            let rhs = self.parse_intersection_type();
            ty = Type::Union(UnionType {
                span: ty.span().merge(rhs.span()),
                types: vec![ty, rhs],
            });
        }
        ty
    }

    fn parse_intersection_type(&mut self) -> Type {
        let mut ty = self.parse_primary_type();
        while self.current.kind == TokenKind::BitAnd {
            self.advance();
            let rhs = self.parse_primary_type();
            ty = Type::Intersection(IntersectionType {
                span: ty.span().merge(rhs.span()),
                types: vec![ty, rhs],
            });
        }
        ty
    }

    fn parse_primary_type(&mut self) -> Type {
        let start = self.current.span.start;
        match self.current.kind {
            TokenKind::Ident => {
                let name_text = self.current.text.clone();
                let span = self.current.span;
                self.advance();
                if let Some(pt) = crate::expr::parse_primitive_type(&name_text) {
                    return Type::Primitive(pt, span);
                }
                // Generic built-in types
                if (name_text == "Result" || name_text == "list" || name_text == "map" || name_text == "tuple")
                    && self.current.kind == TokenKind::Lt
                {
                    self.advance(); // <
                    match name_text.as_str() {
                        "Result" => {
                            let ok_type = self.parse_type();
                            self.eat(TokenKind::Comma);
                            let err_type = self.parse_type();
                            let end = self.current.span.end;
                            self.eat(TokenKind::Gt);
                            return Type::Result(ResultType {
                                span: Span::new(start, end),
                                ok_type: Box::new(ok_type),
                                err_type: Box::new(err_type),
                            });
                        }
                        "list" => {
                            let element_type = self.parse_type();
                            let end = self.current.span.end;
                            self.eat(TokenKind::Gt);
                            return Type::List(ListType {
                                span: Span::new(start, end),
                                element_type: Box::new(element_type),
                            });
                        }
                        "map" => {
                            let key_type = self.parse_type();
                            self.eat(TokenKind::Comma);
                            let value_type = self.parse_type();
                            let end = self.current.span.end;
                            self.eat(TokenKind::Gt);
                            return Type::Map(MapType {
                                span: Span::new(start, end),
                                key_type: Box::new(key_type),
                                value_type: Box::new(value_type),
                            });
                        }
                        "tuple" => {
                            let mut element_types = Vec::new();
                            loop {
                                element_types.push(self.parse_type());
                                if !self.eat(TokenKind::Comma) {
                                    break;
                                }
                            }
                            let end = self.current.span.end;
                            self.eat(TokenKind::Gt);
                            return Type::Tuple(TupleType {
                                span: Span::new(start, end),
                                element_types,
                            });
                        }
                        _ => unreachable!(),
                    }
                }
                // Named type with optional type args
                let type_args = if self.current.kind == TokenKind::Lt {
                    self.advance();
                    let mut args = Vec::new();
                    loop {
                        args.push(self.parse_type());
                        if !self.eat(TokenKind::Comma) {
                            break;
                        }
                    }
                    self.eat(TokenKind::Gt);
                    Some(args)
                } else {
                    None
                };
                Type::Named(NamedType {
                    span: Span::new(start, self.current.span.end),
                    name: Ident { name: name_text, span },
                    type_args,
                })
            }
            TokenKind::LParen => {
                self.advance();
                if self.current.kind == TokenKind::RParen {
                    self.advance();
                    self.eat(TokenKind::FatArrow);
                    let return_type = self.parse_type();
                    Type::Function(FunctionType {
                        span: Span::new(start, return_type.span().end),
                        param_types: Vec::new(),
                        return_type: Box::new(return_type),
                    })
                } else {
                    let mut param_types = Vec::new();
                    loop {
                        param_types.push(self.parse_type());
                        if !self.eat(TokenKind::Comma) {
                            break;
                        }
                    }
                    self.eat(TokenKind::RParen);
                    if self.current.kind == TokenKind::FatArrow {
                        self.advance();
                        let return_type = self.parse_type();
                        Type::Function(FunctionType {
                            span: Span::new(start, return_type.span().end),
                            param_types,
                            return_type: Box::new(return_type),
                        })
                    } else {
                        param_types.into_iter().next().unwrap_or(Type::Primitive(
                            PrimitiveType::Void,
                            Span::new(start, start),
                        ))
                    }
                }
            }
            _ => {
                let span = self.current.span;
                Type::Primitive(PrimitiveType::Mixed, span)
            }
        }
    }
```

- [ ] **Step 3: Update lib.rs to make expr public**

In `crates/coco_parser/src/lib.rs`, change `mod expr;` to `pub mod expr;`:

```rust
pub mod parser;
pub mod expr;

pub use parser::Parser;
```

- [ ] **Step 4: Remove the `use crate::expr::ExprParser;` import from parser.rs**

Line 7 of parser.rs currently imports the now-deleted ExprParser. Replace it with nothing (the file now uses `crate::expr::infix_bp` etc. via full path).

- [ ] **Step 5: Verify it compiles**

Run: `cargo build -p coco_parser 2>&1`
Expected: success with possibly some warnings about unused variables

- [ ] **Step 6: Commit**

```bash
git add crates/coco_parser/
git commit -m "feat(parser): wire Pratt expression parser into main Parser"
```

---

### Task 3: Fix parse_block to handle declarations properly

**Files:**
- Modify: `crates/coco_parser/src/parser.rs:945-972` (parse_block method)

- [ ] **Step 1: Replace parse_block implementation**

The current `parse_block` silently discards `parse_item` results and inserts dummy Null. Replace lines 945-972 with:

```rust
    fn parse_block(&mut self) -> Option<Block> {
        let start = self.current.span.start;
        if !self.eat(TokenKind::LBrace) {
            return None;
        }
        let mut stmts = Vec::new();
        while self.current.kind != TokenKind::RBrace && self.current.kind != TokenKind::Eof {
            // Try statement-level keywords first
            if let Some(stmt) = self.parse_block_stmt() {
                stmts.push(stmt);
            } else if self.current.kind != TokenKind::RBrace {
                self.advance();
            }
        }
        let end = self.current.span.end;
        self.eat(TokenKind::RBrace);
        Some(Block {
            span: Span::new(start, end),
            stmts,
        })
    }

    fn parse_block_stmt(&mut self) -> Option<Stmt> {
        match self.current.kind {
            // Local declarations inside blocks become expression statements
            TokenKind::Let | TokenKind::Const => {
                let start = self.current.span.start;
                let item = self.parse_item()?;
                let end = self.current.span.end;
                Some(Stmt::Expr(ExprStmt {
                    span: Span::new(start, end),
                    expr: Expr::Literal(Literal::Null(Span::new(start, end))),
                }))
            }
            _ => self.parse_stmt(),
        }
    }
```

Note: Ideally `let`/`const` inside blocks would be a `Stmt::Let`/`Stmt::Const` variant, but that requires AST changes beyond this fix. For now we parse them via `parse_item` which handles them correctly (including their initializer expressions), and wrap in a placeholder. The key fix is that we no longer discard the result — the `parse_item` call advances the lexer past the declaration properly.

Actually, a better approach for now: add `Stmt` variants or delegate differently. The simplest correct fix is to handle let/const directly in `parse_stmt`:

```rust
    // In parse_stmt, add these cases before the `_` fallback:
    TokenKind::Let => {
        if let Some(decl) = self.parse_let_decl() {
            // Wrap as item statement
            Some(Stmt::Expr(ExprStmt {
                span: decl.span,
                expr: Expr::Ident(decl.name.clone()),
            }))
        } else {
            None
        }
    }
    TokenKind::Const => {
        if let Some(decl) = self.parse_const_decl() {
            Some(Stmt::Expr(ExprStmt {
                span: decl.span,
                expr: Expr::Ident(decl.name.clone()),
            }))
        } else {
            None
        }
    }
```

And simplify parse_block to only call parse_stmt:

```rust
    fn parse_block(&mut self) -> Option<Block> {
        let start = self.current.span.start;
        if !self.eat(TokenKind::LBrace) {
            return None;
        }
        let mut stmts = Vec::new();
        while self.current.kind != TokenKind::RBrace && self.current.kind != TokenKind::Eof {
            if let Some(stmt) = self.parse_stmt() {
                stmts.push(stmt);
            } else if self.current.kind != TokenKind::RBrace {
                self.advance();
            }
        }
        let end = self.current.span.end;
        self.eat(TokenKind::RBrace);
        Some(Block {
            span: Span::new(start, end),
            stmts,
        })
    }
```

- [ ] **Step 2: Verify it compiles and existing tests pass**

Run: `cargo test -p coco_parser 2>&1`
Expected: success

- [ ] **Step 3: Commit**

```bash
git add crates/coco_parser/src/parser.rs
git commit -m "fix(parser): parse_block no longer discards block contents"
```

---

### Task 4: Fix the LBrace case in parse_stmt

**Files:**
- Modify: `crates/coco_parser/src/parser.rs:724-729`

- [ ] **Step 1: Fix LBrace handler to produce proper block expression**

Currently the `TokenKind::LBrace` arm in `parse_stmt` wraps the block in a dummy Null. Fix it to use the block's statements:

Replace:
```rust
            TokenKind::LBrace => {
                let block = self.parse_block()?;
                Some(Stmt::Expr(ExprStmt {
                    span: block.span,
                    expr: Expr::Literal(Literal::Null(block.span)),
                }))
            }
```

With — since Coco doesn't have block-expressions yet, just inline the block's stmts. Simplest correct thing: parse the block and return its stmts wrapped in a synthetic container. But actually, a bare `{ ... }` at statement level is just a block scope. Return first stmt or skip:

```rust
            TokenKind::LBrace => {
                let block = self.parse_block()?;
                // A bare block at statement level — return stmts as first stmt
                // For now, wrap in an ExprStmt with the block's first expression if any
                if let Some(first) = block.stmts.into_iter().next() {
                    Some(first)
                } else {
                    Some(Stmt::Expr(ExprStmt {
                        span: block.span,
                        expr: Expr::Literal(Literal::Null(block.span)),
                    }))
                }
            }
```

Actually — this is object-literal ambiguity at statement level. Since expression statements now handle `LBrace` via `parse_primary_expr` → `parse_object_literal`, we should just fall through to the default `_` arm. Remove the `TokenKind::LBrace` case entirely and let the `_` case handle it as an expression statement.

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p coco_parser 2>&1`
Expected: success

- [ ] **Step 3: Commit**

```bash
git add crates/coco_parser/src/parser.rs
git commit -m "fix(parser): remove special LBrace case, handle via expression parsing"
```

---

### Task 5: Remove panicking unwrap in old parse_ident usage

**Files:**
- Modify: `crates/coco_parser/src/parser.rs`

- [ ] **Step 1: Audit all `.unwrap()` calls on parse_ident and fix**

After Task 2, the old `parse_expr_pratt` stub is gone. But `parse_primary_expr` uses `self.parse_ident().unwrap()` for the `TokenKind::Ident` case — this is safe because we already checked `current.kind == Ident`. However, verify no other `.unwrap()` exists on fallible parse methods.

Search for `.unwrap()` in parser.rs. For any that could fail on user input, replace with `.unwrap_or(...)` or `?`.

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p coco_parser 2>&1`

- [ ] **Step 3: Commit**

```bash
git add crates/coco_parser/src/parser.rs
git commit -m "fix(parser): remove panicking unwrap calls on user input"
```

---

### Task 6: Update formatter to handle Range/RangeInclusive BinaryOp

**Files:**
- Modify: `crates/coco_formatter/src/formatter.rs` (BinaryOp formatting)

- [ ] **Step 1: Find BinaryOp formatting and add Range variants**

Grep for `BinaryOp` in formatter.rs and add:

```rust
BinaryOp::Range => "..",
BinaryOp::RangeInclusive => "..=",
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p coco_formatter 2>&1`
Expected: success

- [ ] **Step 3: Commit**

```bash
git add crates/coco_formatter/src/formatter.rs
git commit -m "feat(formatter): format Range and RangeInclusive operators"
```

---

### Task 7: Add parser tests

**Files:**
- Create: `crates/coco_parser/tests/parse_expr.rs`
- Create: `crates/coco_parser/tests/parse_decl.rs`

- [ ] **Step 1: Create expression parser tests**

Create `crates/coco_parser/tests/parse_expr.rs`:

```rust
use coco_parser::Parser;
use coco_syntax::*;

fn parse(src: &str) -> Program {
    let mut parser = Parser::new(src);
    parser.parse_program()
}

fn parse_expr_stmt(src: &str) -> Expr {
    let program = parse(src);
    match &program.items[0] {
        Item::Stmt(Stmt::Expr(es)) => es.expr.clone(),
        _ => panic!("expected expression statement, got {:?}", program.items[0]),
    }
}

#[test]
fn parse_integer_literal() {
    let expr = parse_expr_stmt("42;");
    assert!(matches!(expr, Expr::Literal(Literal::Int(42, _))));
}

#[test]
fn parse_float_literal() {
    let expr = parse_expr_stmt("3.14;");
    assert!(matches!(expr, Expr::Literal(Literal::Float(f, _)) if (f - 3.14).abs() < f64::EPSILON));
}

#[test]
fn parse_string_literal() {
    let expr = parse_expr_stmt("\"hello\";");
    assert!(matches!(expr, Expr::Literal(Literal::String(s, _)) if s == "hello"));
}

#[test]
fn parse_bool_literals() {
    let expr = parse_expr_stmt("true;");
    assert!(matches!(expr, Expr::Literal(Literal::Bool(true, _))));
    let expr = parse_expr_stmt("false;");
    assert!(matches!(expr, Expr::Literal(Literal::Bool(false, _))));
}

#[test]
fn parse_null_literal() {
    let expr = parse_expr_stmt("null;");
    assert!(matches!(expr, Expr::Literal(Literal::Null(_))));
}

#[test]
fn parse_binary_add() {
    let expr = parse_expr_stmt("1 + 2;");
    match expr {
        Expr::Binary(b) => {
            assert_eq!(b.op, BinaryOp::Add);
            assert!(matches!(b.left, Expr::Literal(Literal::Int(1, _))));
            assert!(matches!(b.right, Expr::Literal(Literal::Int(2, _))));
        }
        _ => panic!("expected binary expr"),
    }
}

#[test]
fn parse_binary_precedence() {
    // 1 + 2 * 3 => Add(1, Mul(2, 3))
    let expr = parse_expr_stmt("1 + 2 * 3;");
    match expr {
        Expr::Binary(b) => {
            assert_eq!(b.op, BinaryOp::Add);
            match *b {
                BinaryExpr { right: Expr::Binary(inner), .. } => {
                    assert_eq!(inner.op, BinaryOp::Mul);
                }
                _ => panic!("expected nested binary"),
            }
        }
        _ => panic!("expected binary expr"),
    }
}

#[test]
fn parse_unary_neg() {
    let expr = parse_expr_stmt("-x;");
    match expr {
        Expr::Unary(u) => {
            assert_eq!(u.op, UnaryOp::Neg);
            assert!(matches!(u.expr, Expr::Ident(_)));
        }
        _ => panic!("expected unary expr"),
    }
}

#[test]
fn parse_member_access() {
    let expr = parse_expr_stmt("foo.bar;");
    match expr {
        Expr::Member(m) => {
            assert_eq!(m.property.name, "bar");
            assert!(!m.optional);
        }
        _ => panic!("expected member expr"),
    }
}

#[test]
fn parse_optional_chain() {
    let expr = parse_expr_stmt("foo?.bar;");
    match expr {
        Expr::Member(m) => {
            assert_eq!(m.property.name, "bar");
            assert!(m.optional);
        }
        _ => panic!("expected member expr"),
    }
}

#[test]
fn parse_function_call() {
    let expr = parse_expr_stmt("foo(1, 2);");
    match expr {
        Expr::Call(c) => {
            assert_eq!(c.args.len(), 2);
        }
        _ => panic!("expected call expr"),
    }
}

#[test]
fn parse_method_chain() {
    let expr = parse_expr_stmt("a.b().c;");
    assert!(matches!(expr, Expr::Member(_)));
}

#[test]
fn parse_array_literal() {
    let expr = parse_expr_stmt("[1, 2, 3];");
    match expr {
        Expr::Array(a) => assert_eq!(a.elements.len(), 3),
        _ => panic!("expected array"),
    }
}

#[test]
fn parse_object_literal() {
    let expr = parse_expr_stmt("{x: 1, y: 2};");
    match expr {
        Expr::Object(o) => assert_eq!(o.fields.len(), 2),
        _ => panic!("expected object"),
    }
}

#[test]
fn parse_pipe_right() {
    let expr = parse_expr_stmt("x |> f;");
    assert!(matches!(expr, Expr::Pipe(_)));
}

#[test]
fn parse_null_coalesce() {
    let expr = parse_expr_stmt("x ?? y;");
    assert!(matches!(expr, Expr::NullCoalesce(_)));
}

#[test]
fn parse_range() {
    let expr = parse_expr_stmt("1..10;");
    match expr {
        Expr::Binary(b) => assert_eq!(b.op, BinaryOp::Range),
        _ => panic!("expected range binary"),
    }
}

#[test]
fn parse_index_access() {
    let expr = parse_expr_stmt("arr[0];");
    assert!(matches!(expr, Expr::Index(_)));
}

#[test]
fn parse_grouped_expr() {
    let expr = parse_expr_stmt("(1 + 2) * 3;");
    match expr {
        Expr::Binary(b) => {
            assert_eq!(b.op, BinaryOp::Mul);
            assert!(matches!(b.left, Expr::Group(_)));
        }
        _ => panic!("expected binary with group"),
    }
}

#[test]
fn no_panic_on_unexpected_token() {
    let mut parser = Parser::new("@@@;");
    let program = parser.parse_program();
    // Should not panic — produces diagnostics or dummy nodes
    assert!(!program.items.is_empty() || !parser.diagnostics().is_empty());
}
```

- [ ] **Step 2: Create declaration parser tests**

Create `crates/coco_parser/tests/parse_decl.rs`:

```rust
use coco_parser::Parser;
use coco_syntax::*;

fn parse(src: &str) -> Program {
    let mut parser = Parser::new(src);
    parser.parse_program()
}

#[test]
fn parse_fn_decl() {
    let program = parse("fn add(x: int, y: int): int { return x + y; }");
    match &program.items[0] {
        Item::FnDecl(f) => {
            assert_eq!(f.name.name, "add");
            assert_eq!(f.params.len(), 2);
            assert!(f.return_type.is_some());
            assert!(!f.body.stmts.is_empty());
        }
        _ => panic!("expected fn decl"),
    }
}

#[test]
fn parse_async_fn() {
    let program = parse("async fn fetch(): string { return x; }");
    match &program.items[0] {
        Item::FnDecl(f) => {
            assert!(f.is_async);
            assert_eq!(f.name.name, "fetch");
        }
        _ => panic!("expected fn decl"),
    }
}

#[test]
fn parse_let_decl() {
    let program = parse("let x: int = 42;");
    match &program.items[0] {
        Item::LetDecl(l) => {
            assert_eq!(l.name.name, "x");
            assert!(l.type_ann.is_some());
            assert!(l.value.is_some());
        }
        _ => panic!("expected let decl"),
    }
}

#[test]
fn parse_const_decl() {
    let program = parse("const PI: float = 3.14;");
    match &program.items[0] {
        Item::ConstDecl(c) => {
            assert_eq!(c.name.name, "PI");
        }
        _ => panic!("expected const decl"),
    }
}

#[test]
fn parse_class_decl() {
    let program = parse("class Dog extends Animal { name: string; fn bark() { } }");
    match &program.items[0] {
        Item::ClassDecl(c) => {
            assert_eq!(c.name.name, "Dog");
            assert!(c.extends.is_some());
            assert_eq!(c.members.len(), 2);
        }
        _ => panic!("expected class decl"),
    }
}

#[test]
fn parse_enum_decl() {
    let program = parse("enum Color { Red, Green, Blue }");
    match &program.items[0] {
        Item::EnumDecl(e) => {
            assert_eq!(e.name.name, "Color");
            assert_eq!(e.variants.len(), 3);
        }
        _ => panic!("expected enum decl"),
    }
}

#[test]
fn parse_interface_decl() {
    let program = parse("interface Printable { print(): void; }");
    match &program.items[0] {
        Item::InterfaceDecl(i) => {
            assert_eq!(i.name.name, "Printable");
            assert_eq!(i.members.len(), 1);
        }
        _ => panic!("expected interface decl"),
    }
}

#[test]
fn parse_import_named() {
    let program = parse("import { foo, bar } from \"module\";");
    match &program.items[0] {
        Item::Import(i) => {
            match &i.items {
                ImportItems::Named(names) => assert_eq!(names.len(), 2),
                _ => panic!("expected named import"),
            }
            assert_eq!(i.source, "module");
        }
        _ => panic!("expected import"),
    }
}

#[test]
fn parse_if_else() {
    let program = parse("fn test() { if x { return 1; } else { return 2; } }");
    match &program.items[0] {
        Item::FnDecl(f) => {
            match &f.body.stmts[0] {
                Stmt::If(i) => {
                    assert!(i.else_block.is_some());
                }
                _ => panic!("expected if stmt"),
            }
        }
        _ => panic!("expected fn decl"),
    }
}

#[test]
fn parse_for_loop() {
    let program = parse("fn test() { for item in items { x; } }");
    match &program.items[0] {
        Item::FnDecl(f) => {
            match &f.body.stmts[0] {
                Stmt::For(s) => {
                    assert_eq!(s.pattern.name, "item");
                }
                _ => panic!("expected for stmt"),
            }
        }
        _ => panic!("expected fn decl"),
    }
}

#[test]
fn parse_return_expr() {
    let program = parse("fn test() { return 1 + 2; }");
    match &program.items[0] {
        Item::FnDecl(f) => {
            match &f.body.stmts[0] {
                Stmt::Return(r) => {
                    assert!(r.value.is_some());
                    assert!(matches!(r.value.as_ref().unwrap(), Expr::Binary(_)));
                }
                _ => panic!("expected return"),
            }
        }
        _ => panic!("expected fn decl"),
    }
}
```

- [ ] **Step 3: Run parser tests**

Run: `cargo test -p coco_parser 2>&1`
Expected: All tests pass

- [ ] **Step 4: Commit**

```bash
git add crates/coco_parser/tests/
git commit -m "test(parser): add expression and declaration parser tests"
```

---

### Task 8: Add formatter round-trip tests

**Files:**
- Create: `crates/coco_formatter/tests/format_roundtrip.rs`

- [ ] **Step 1: Create formatter tests**

Create `crates/coco_formatter/tests/format_roundtrip.rs`:

```rust
use coco_formatter::Formatter;
use coco_parser::Parser;

fn format(src: &str) -> String {
    let mut parser = Parser::new(src);
    let program = parser.parse_program();
    let mut formatter = Formatter::new();
    formatter.format(&program)
}

fn assert_idempotent(src: &str) {
    let first = format(src);
    let second = format(&first);
    assert_eq!(first, second, "Formatter is not idempotent");
}

#[test]
fn format_fn_decl() {
    let output = format("fn add(x: int, y: int): int { return x + y; }");
    assert!(output.contains("fn add"));
    assert!(output.contains("return"));
}

#[test]
fn format_idempotent_fn() {
    assert_idempotent("fn add(x: int, y: int): int {\n    return x + y;\n}\n");
}

#[test]
fn format_class_decl() {
    let output = format("class Dog { name: string; fn bark() { } }");
    assert!(output.contains("class Dog"));
    assert!(output.contains("name: string"));
}

#[test]
fn format_let_const() {
    let output = format("let x: int = 42;");
    assert!(output.contains("let x"));
}

#[test]
fn format_enum() {
    let output = format("enum Color { Red, Green, Blue }");
    assert!(output.contains("enum Color"));
    assert!(output.contains("Red"));
}

#[test]
fn format_binary_expr() {
    let output = format("let x = 1 + 2 * 3;");
    // Should format without crashing
    assert!(!output.is_empty());
}

#[test]
fn format_pipe_expr() {
    let output = format("let x = data |> transform;");
    assert!(output.contains("|>"));
}
```

- [ ] **Step 2: Run formatter tests**

Run: `cargo test -p coco_formatter 2>&1`
Expected: All tests pass

- [ ] **Step 3: Commit**

```bash
git add crates/coco_formatter/tests/
git commit -m "test(formatter): add formatting and idempotency tests"
```

---

### Task 9: Final verification

- [ ] **Step 1: Run full test suite**

Run: `cargo test 2>&1`
Expected: All tests pass (lexer, span, diagnostics, parser, formatter)

- [ ] **Step 2: Run clippy**

Run: `cargo clippy 2>&1`
Expected: No errors (warnings acceptable for now)

- [ ] **Step 3: Test with an example file**

Run: `cargo run -- parse examples/hello.co 2>&1`
Expected: Produces AST summary without panicking

- [ ] **Step 4: Test formatting round-trip**

Run: `cargo run -- fmt examples/hello.co 2>&1`
Expected: Produces formatted output without panicking

- [ ] **Step 5: Final commit if any fixups needed**

```bash
git add -A
git commit -m "fix: address clippy warnings and final polish"
```
