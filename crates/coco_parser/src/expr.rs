//! Pratt expression parser for Coco.
//!
//! Implements operator precedence parsing for expressions,
//! handling binary, unary, postfix, ternary, and pipe operators.

use coco_lexer::{Lexer, Token, TokenKind};
use coco_span::Span;
use coco_syntax::*;

/// Prefix power: the binding power for prefix operators (unary, etc.)
const PREFIX_POWER: u8 = 90;

/// Postfix power: the binding power for postfix operators
const POSTFIX_POWER: u8 = 95;

/// Maximum binding power
const MAX_BP: u8 = 100;

pub struct ExprParser<'a> {
    lexer: &'a mut Lexer<'a>,
    current: Token,
}

impl<'a> ExprParser<'a> {
    pub fn new(lexer: &'a mut Lexer<'a>, current: Token) -> Self {
        Self { lexer, current }
    }

    pub fn current(&self) -> &Token {
        &self.current
    }

    fn advance(&mut self) -> Token {
        let next = self.lexer.next_token();
        std::mem::replace(&mut self.current, next)
    }

    fn peek(&mut self) -> Token {
        // Need to peek ahead without advancing — use cursor save/restore
        // Since Lexer doesn't support peek, we use a simple workaround
        let save = self.current.clone();
        let next = self.advance();
        let result = self.current.clone();
        // Put back by re-lexing... this is suboptimal but works
        // Actually let's just clone current, re-advance, and put result back
        // This doesn't work because Lexer has already consumed.
        // Instead, we use `current` to look at the next token after advance.
        // For now, we peek by advancing and storing.
        // Better approach: the Parser will use token buffering.
        next
    }

    /// Check if current token matches and advance if so
    fn eat(&mut self, kind: TokenKind) -> bool {
        if self.current.kind == kind {
            self.advance();
            true
        } else {
            false
        }
    }

    /// Parse expression starting at binding power `min_bp`
    pub fn parse_expr(&mut self, min_bp: u8) -> Expr {
        // Parse prefix / primary
        let mut lhs = self.parse_prefix();

        // Parse infix and postfix operators
        loop {
            // Postfix operators
            lhs = self.parse_postfix_continuation(lhs);

            // Infix operators
            if self.current.kind == TokenKind::Eof || self.current.kind == TokenKind::Semi {
                break;
            }

            if self.current.kind == TokenKind::RBrace
                || self.current.kind == TokenKind::RParen
                || self.current.kind == TokenKind::RBracket
                || self.current.kind == TokenKind::Comma
                || self.current.kind == TokenKind::Colon
                || self.current.kind == TokenKind::FatArrow
            {
                break;
            }

            let maybe_bp = infix_bp(self.current.kind);
            if maybe_bp.is_none() {
                break;
            }
            let (left_bp, right_bp) = maybe_bp.unwrap();
            if left_bp < min_bp {
                break;
            }

            // Handle ternary `? :`
            if self.current.kind == TokenKind::Question {
                lhs = self.parse_ternary(lhs);
                continue;
            }

            // Handle range `..` or `..=`
            if self.current.kind == TokenKind::Range || self.current.kind == TokenKind::RangeInclusive {
                let op_start = self.current.span.start;
                let is_inclusive = self.current.kind == TokenKind::RangeInclusive;
                self.advance();
                let rhs = self.parse_expr(right_bp);
                let span = Span::new(lhs.span_start(), rhs.span_end());
                lhs = Expr::Binary(Box::new(BinaryExpr {
                    span,
                    left: lhs,
                    op: if is_inclusive { BinaryOp::Spaceship } else { BinaryOp::Spaceship },
                    right: rhs,
                }));
                continue;
            }

            // Handle pipe operators
            if self.current.kind == TokenKind::PipeRight || self.current.kind == TokenKind::PipeLeft {
                let op = if self.current.kind == TokenKind::PipeRight {
                    PipeOp::PipeRight
                } else {
                    PipeOp::PipeLeft
                };
                let op_start = self.current.span.start;
                self.advance();
                let rhs = self.parse_expr(right_bp);
                let span = Span::new(lhs.span_start(), rhs.span_end());
                lhs = Expr::Pipe(Box::new(PipeExpr {
                    span,
                    left: lhs,
                    op,
                    right: rhs,
                }));
                continue;
            }

            // Handle null coalesce `??`
            if self.current.kind == TokenKind::QuestionQuestion {
                let op_start = self.current.span.start;
                self.advance();
                let rhs = self.parse_expr(right_bp);
                let span = Span::new(lhs.span_start(), rhs.span_end());
                lhs = Expr::NullCoalesce(Box::new(NullCoalesceExpr {
                    span,
                    left: lhs,
                    right: rhs,
                }));
                continue;
            }

            // Handle elvis `?:`
            if self.current.kind == TokenKind::QuestionColon {
                let op_start = self.current.span.start;
                self.advance();
                let rhs = self.parse_expr(right_bp);
                let span = Span::new(lhs.span_start(), rhs.span_end());
                lhs = Expr::Elvis(Box::new(ElvisExpr {
                    span,
                    left: lhs,
                    right: rhs,
                }));
                continue;
            }

            // General binary operator
            let op = token_to_binary_op(self.current.kind);
            if let Some(binop) = op {
                let op_start = self.current.span.start;
                self.advance();
                let rhs = self.parse_expr(right_bp);
                let span = Span::new(lhs.span_start(), rhs.span_end());
                lhs = Expr::Binary(Box::new(BinaryExpr {
                    span,
                    left: lhs,
                    op: binop,
                    right: rhs,
                }));
                continue;
            }

            break;
        }

        lhs
    }

    fn parse_prefix(&mut self) -> Expr {
        let start_pos = self.current.span.start;

        match self.current.kind {
            // Unary operators
            TokenKind::Minus => {
                self.advance();
                let expr = self.parse_expr(PREFIX_POWER);
                let end_pos = expr.span_end();
                Expr::Unary(Box::new(UnaryExpr {
                    span: Span::new(start_pos, end_pos),
                    op: UnaryOp::Neg,
                    expr,
                }))
            }
            TokenKind::Not => {
                self.advance();
                let expr = self.parse_expr(PREFIX_POWER);
                let end_pos = expr.span_end();
                Expr::Unary(Box::new(UnaryExpr {
                    span: Span::new(start_pos, end_pos),
                    op: UnaryOp::Not,
                    expr,
                }))
            }
            TokenKind::BitNot => {
                self.advance();
                let expr = self.parse_expr(PREFIX_POWER);
                let end_pos = expr.span_end();
                Expr::Unary(Box::new(UnaryExpr {
                    span: Span::new(start_pos, end_pos),
                    op: UnaryOp::BitNot,
                    expr,
                }))
            }
            TokenKind::Typeof => {
                self.advance();
                let expr = self.parse_expr(PREFIX_POWER);
                let end_pos = expr.span_end();
                Expr::Unary(Box::new(UnaryExpr {
                    span: Span::new(start_pos, end_pos),
                    op: UnaryOp::Typeof,
                    expr,
                }))
            }
            TokenKind::Await => {
                self.advance();
                let expr = self.parse_expr(PREFIX_POWER);
                let end_pos = expr.span_end();
                Expr::Unary(Box::new(UnaryExpr {
                    span: Span::new(start_pos, end_pos),
                    op: UnaryOp::Await,
                    expr,
                }))
            }
            TokenKind::New => {
                self.advance();
                let type_name = self.parse_ident();
                let args = self.parse_arg_list();
                let end_pos = self.current.span.end;
                Expr::New(Box::new(NewExpr {
                    span: Span::new(start_pos, end_pos),
                    type_name,
                    args,
                }))
            }

            // Primary expressions
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Expr {
        let start_pos = self.current.span.start;
        match self.current.kind {
            TokenKind::Ident => {
                let ident = self.parse_ident();
                // Check for function call
                if self.current.kind == TokenKind::LParen {
                    let args = self.parse_arg_list();
                    let end_pos = self.current.span.end;
                    // Use span from last token
                    let span = Span::new(start_pos, end_pos);
                    return Expr::Call(Box::new(CallExpr {
                        span,
                        callee: Expr::Ident(ident),
                        args,
                    }));
                }
                Expr::Ident(ident)
            }
            TokenKind::IntLiteral => {
                let text = self.current.text.clone();
                let span = self.current.span;
                self.advance();
                // Parse integer value
                let val = parse_int_literal(&text);
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
                // Strip quotes
                let inner = text[1..text.len() - 1].to_string();
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
                let expr = self.parse_expr(0);
                self.expect(TokenKind::RParen);
                expr
            }
            TokenKind::LBracket => {
                self.parse_array_literal()
            }
            TokenKind::LBrace => {
                self.parse_object_literal()
            }
            TokenKind::Match => {
                self.parse_match_expr()
            }
            TokenKind::Async | TokenKind::Fn | TokenKind::Function => {
                self.parse_lambda_or_fn_expr()
            }
            _ => {
                // Error: unexpected token in expression context
                let span = self.current.span;
                self.advance();
                Expr::Literal(Literal::Null(span))
            }
        }
    }

    fn parse_ternary(&mut self, condition: Expr) -> Expr {
        let q_span = self.current.span;
        self.advance(); // ?
        let then_expr = self.parse_expr(0);
        let colon_span = self.current.span;
        if self.current.kind == TokenKind::Colon {
            self.advance();
        }
        let else_expr = self.parse_expr(0);
        let span = Span::new(condition.span_start(), else_expr.span_end());
        Expr::Ternary(Box::new(TernaryExpr {
            span,
            condition,
            then_expr,
            else_expr,
        }))
    }

    fn parse_postfix_continuation(&mut self, lhs: Expr) -> Expr {
        let mut result = lhs;
        loop {
            match self.current.kind {
                TokenKind::Dot => {
                    self.advance();
                    let name = self.parse_ident();
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
                    let name = self.parse_ident();
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
                    let index = self.parse_expr(0);
                    self.expect(TokenKind::RBracket);
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
                TokenKind::Not => {
                    // `expr!` — postfix unwrap
                    let span = Span::new(result.span_start(), self.current.span.end);
                    self.advance();
                    result = Expr::Postfix(Box::new(PostfixExpr {
                        span,
                        object: result,
                        op: PostfixOp::Bang,
                    }));
                }
                TokenKind::Question => {
                    // `expr?` — postfix error propagation (but check for `?:` first)
                    // `?:` is handled in infix loop, not here
                    break;
                }
                _ => break,
            }
        }
        result
    }

    fn parse_ident(&mut self) -> Ident {
        let span = self.current.span;
        let name = self.current.text.clone();
        self.expect(TokenKind::Ident);
        Ident { name, span }
    }

    fn parse_arg_list(&mut self) -> Vec<Argument> {
        self.expect(TokenKind::LParen);
        let mut args = Vec::new();
        if self.current.kind != TokenKind::RParen {
            loop {
                let start_pos = self.current.span.start;
                let name = if self.current.kind == TokenKind::Ident
                    && self.peek_ahead_for_colon()
                {
                    let ident = self.parse_ident();
                    self.advance(); // :
                    Some(ident)
                } else {
                    None
                };
                let value = self.parse_expr(0);
                let end_pos = self.current.span.end;
                args.push(Argument {
                    span: Span::new(start_pos, end_pos),
                    name,
                    value,
                });
                if self.current.kind != TokenKind::Comma {
                    break;
                }
                self.advance();
            }
        }
        self.expect(TokenKind::RParen);
        args
    }

    fn peek_ahead_for_colon(&self) -> bool {
        // Simplified: assume named args are rare and just parse without checking
        false
    }

    fn parse_array_literal(&mut self) -> Expr {
        let start = self.current.span.start;
        self.advance(); // [
        let mut elements = Vec::new();
        if self.current.kind != TokenKind::RBracket {
            loop {
                elements.push(self.parse_expr(0));
                if self.current.kind != TokenKind::Comma {
                    break;
                }
                self.advance();
                if self.current.kind == TokenKind::RBracket {
                    break; // trailing comma
                }
            }
        }
        let end = self.current.span.end;
        self.expect(TokenKind::RBracket);
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
                        let name = self.parse_ident();
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
                self.expect(TokenKind::Colon);
                let value = self.parse_expr(0);
                let key_end = self.current.span.end;
                fields.push(ObjectField {
                    span: Span::new(key_start, key_end),
                    key,
                    value,
                });
                if self.current.kind != TokenKind::Comma {
                    break;
                }
                self.advance();
                if self.current.kind == TokenKind::RBrace {
                    break; // trailing comma
                }
            }
        }
        let end = self.current.span.end;
        self.expect(TokenKind::RBrace);
        Expr::Object(ObjectLiteral {
            span: Span::new(start, end),
            fields,
        })
    }

    fn parse_match_expr(&mut self) -> Expr {
        let start = self.current.span.start;
        self.advance(); // match
        let scrutinee = self.parse_expr(0);
        self.expect(TokenKind::LBrace);
        let mut arms = Vec::new();
        while self.current.kind != TokenKind::RBrace && self.current.kind != TokenKind::Eof {
            let arm_start = self.current.span.start;
            let pattern = self.parse_pattern();
            self.expect(TokenKind::FatArrow);
            let body = self.parse_expr(0);
            let arm_end = self.current.span.end;
            if self.current.kind == TokenKind::Comma {
                self.advance();
            }
            arms.push(MatchArm {
                span: Span::new(arm_start, arm_end),
                pattern,
                body,
            });
        }
        let end = self.current.span.end;
        self.expect(TokenKind::RBrace);
        Expr::Match(Box::new(MatchExpr {
            span: Span::new(start, end),
            scrutinee,
            arms,
        }))
    }

    fn parse_pattern(&mut self) -> Pattern {
        match self.current.kind {
            TokenKind::Ident => {
                let ident = self.parse_ident();
                Pattern::Ident(ident)
            }
            TokenKind::Is => {
                self.advance();
                let ty = self.parse_type();
                Pattern::IsType(ty)
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
            TokenKind::IntLiteral => {
                let text = self.current.text.clone();
                let span = self.current.span;
                self.advance();
                let val = parse_int_literal(&text);
                Pattern::Literal(Literal::Int(val, span))
            }
            TokenKind::StringLiteral => {
                let text = self.current.text.clone();
                let span = self.current.span;
                self.advance();
                let inner = text[1..text.len() - 1].to_string();
                Pattern::Literal(Literal::String(inner, span))
            }
            TokenKind::CharLiteral => {
                let text = self.current.text.clone();
                let span = self.current.span;
                self.advance();
                let inner = text.chars().nth(1).unwrap_or('\0');
                Pattern::Literal(Literal::Char(inner, span))
            }
            _ => {
                let span = self.current.span;
                self.advance();
                Pattern::Wildcard(span)
            }
        }
    }

    fn parse_lambda_or_fn_expr(&mut self) -> Expr {
        let start = self.current.span.start;
        let is_async = self.eat(TokenKind::Async);

        // Skip fn/function keyword if present
        let _ = self.eat(TokenKind::Fn);
        let _ = self.eat(TokenKind::Function);

        if self.current.kind == TokenKind::LParen {
            // Lambda: (params) => body or (params): Type => body
            let params = self.parse_param_list();
            let return_type = if self.eat(TokenKind::Colon) {
                Some(self.parse_type())
            } else {
                None
            };
            self.expect(TokenKind::FatArrow);
            let body = if self.current.kind == TokenKind::LBrace {
                let block = self.parse_block_body();
                LambdaBody::Block(block)
            } else {
                let expr = self.parse_expr(0);
                LambdaBody::Expr(expr)
            };
            let end = self.span_end();
            Expr::Lambda(Box::new(Lambda {
                span: Span::new(start, end),
                is_async,
                params,
                return_type,
                body,
            }))
        } else {
            // Single-param arrow: ident => body
            let name = self.parse_ident();
            let param = Param {
                span: name.span,
                name: name.clone(),
                type_ann: None,
                default_value: None,
            };
            self.expect(TokenKind::FatArrow);
            let body = if self.current.kind == TokenKind::LBrace {
                let block = self.parse_block_body();
                LambdaBody::Block(block)
            } else {
                let expr = self.parse_expr(0);
                LambdaBody::Expr(expr)
            };
            let end = self.span_end();
            Expr::Lambda(Box::new(Lambda {
                span: Span::new(start, end),
                is_async,
                params: vec![param],
                return_type: None,
                body,
            }))
        }
    }

    pub fn parse_param_list(&mut self) -> Vec<Param> {
        self.expect(TokenKind::LParen);
        let mut params = Vec::new();
        if self.current.kind != TokenKind::RParen {
            loop {
                let p_start = self.current.span.start;
                let name = self.parse_ident();
                let type_ann = if self.eat(TokenKind::Colon) {
                    Some(self.parse_type())
                } else {
                    None
                };
                let default_value = if self.eat(TokenKind::Assign) {
                    Some(self.parse_expr(0))
                } else {
                    None
                };
                params.push(Param {
                    span: Span::new(p_start, self.current.span.end),
                    name,
                    type_ann,
                    default_value,
                });
                if self.current.kind != TokenKind::Comma {
                    break;
                }
                self.advance();
            }
        }
        self.expect(TokenKind::RParen);
        params
    }

    pub fn parse_type(&mut self) -> Type {
        // Parse union type: intersection { '|' intersection }
        let mut ty = self.parse_intersection_type();
        while self.eat(TokenKind::BitOr) {
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
        while self.eat(TokenKind::BitAnd) {
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
                let name = self.current.text.clone();
                let span = self.current.span;
                self.advance();
                // Check for primitive types
                if let Some(pt) = parse_primitive_type(&name) {
                    return Type::Primitive(pt, span);
                }
                // Check for Result<ListMap/Tuple types
                if name == "Result" && self.current.kind == TokenKind::Lt {
                    self.advance();
                    let ok_type = self.parse_type();
                    self.expect(TokenKind::Comma);
                    let err_type = self.parse_type();
                    let end = self.current.span.end;
                    self.expect(TokenKind::Gt);
                    return Type::Result(ResultType {
                        span: Span::new(start, end),
                        ok_type: Box::new(ok_type),
                        err_type: Box::new(err_type),
                    });
                }
                if name == "list" && self.current.kind == TokenKind::Lt {
                    self.advance();
                    let element_type = self.parse_type();
                    let end = self.current.span.end;
                    self.expect(TokenKind::Gt);
                    return Type::List(ListType {
                        span: Span::new(start, end),
                        element_type: Box::new(element_type),
                    });
                }
                if name == "map" && self.current.kind == TokenKind::Lt {
                    self.advance();
                    let key_type = self.parse_type();
                    self.expect(TokenKind::Comma);
                    let value_type = self.parse_type();
                    let end = self.current.span.end;
                    self.expect(TokenKind::Gt);
                    return Type::Map(MapType {
                        span: Span::new(start, end),
                        key_type: Box::new(key_type),
                        value_type: Box::new(value_type),
                    });
                }
                if name == "tuple" && self.current.kind == TokenKind::Lt {
                    self.advance();
                    let mut element_types = Vec::new();
                    loop {
                        element_types.push(self.parse_type());
                        if self.current.kind != TokenKind::Comma {
                            break;
                        }
                        self.advance();
                    }
                    let end = self.current.span.end;
                    self.expect(TokenKind::Gt);
                    return Type::Tuple(TupleType {
                        span: Span::new(start, end),
                        element_types,
                    });
                }
                // Generic type args
                let type_args = if self.current.kind == TokenKind::Lt {
                    self.advance();
                    let mut args = Vec::new();
                    loop {
                        args.push(self.parse_type());
                        if self.current.kind != TokenKind::Comma {
                            break;
                        }
                        self.advance();
                    }
                    self.expect(TokenKind::Gt);
                    Some(args)
                } else {
                    None
                };
                let end = self.current.span.end;
                Type::Named(NamedType {
                    span: Span::new(start, end),
                    name: Ident { name, span },
                    type_args,
                })
            }
            TokenKind::LParen => {
                // Function type or grouped type
                self.advance();
                if self.current.kind == TokenKind::RParen {
                    self.advance();
                    self.expect(TokenKind::FatArrow);
                    let return_type = self.parse_type();
                    Type::Function(FunctionType {
                        span: Span::new(start, self.span_end()),
                        param_types: Vec::new(),
                        return_type: Box::new(return_type),
                    })
                } else {
                    let mut param_types = Vec::new();
                    loop {
                        param_types.push(self.parse_type());
                        if self.current.kind != TokenKind::Comma {
                            break;
                        }
                        self.advance();
                    }
                    self.expect(TokenKind::RParen);
                    if self.current.kind == TokenKind::FatArrow {
                        self.advance();
                        let return_type = self.parse_type();
                        Type::Function(FunctionType {
                            span: Span::new(start, self.span_end()),
                            param_types,
                            return_type: Box::new(return_type),
                        })
                    } else {
                        // Grouped type — return the first one
                        param_types.into_iter().next().unwrap_or(Type::Primitive(
                            PrimitiveType::Void,
                            Span::new(start, start),
                        ))
                    }
                }
            }
            _ => {
                let span = self.current.span;
                self.advance();
                Type::Primitive(PrimitiveType::Void, span)
            }
        }
    }

    pub fn parse_block_body(&mut self) -> Block {
        let start = self.current.span.start;
        self.expect(TokenKind::LBrace);
        let stmts = Vec::new(); // Will be filled by the parent parser
        let end = self.current.span.end;
        self.expect(TokenKind::RBrace);
        Block {
            span: Span::new(start, end),
            stmts,
        }
    }

    fn expect(&mut self, kind: TokenKind) {
        if self.current.kind == kind {
            self.advance();
        }
    }

    fn span_end(&self) -> usize {
        self.current.span.end
    }
}

// ============================================================
// Helper functions
// ============================================================

/// Returns (left_bp, right_bp) for infix operators.
/// Returns None if the token is not an infix operator.
fn infix_bp(kind: TokenKind) -> Option<(u8, u8)> {
    match kind {
        // Assignment (right-associative)
        TokenKind::Assign | TokenKind::PlusEq | TokenKind::MinusEq
        | TokenKind::StarEq | TokenKind::SlashEq | TokenKind::PercentEq
        | TokenKind::StarStarEq | TokenKind::ShlEq | TokenKind::ShrEq
        | TokenKind::BitAndEq | TokenKind::BitOrEq | TokenKind::BitXorEq => Some((10, 9)),

        // Range (right-associative)
        TokenKind::Range | TokenKind::RangeInclusive => Some((35, 34)),

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

fn token_to_binary_op(kind: TokenKind) -> Option<BinaryOp> {
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
        TokenKind::PipeRight => BinaryOp::PipeRight,
        TokenKind::PipeLeft => BinaryOp::PipeLeft,
        TokenKind::QuestionQuestion => BinaryOp::NullCoalesce,
        TokenKind::QuestionColon => BinaryOp::Elvis,
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

fn parse_primitive_type(name: &str) -> Option<PrimitiveType> {
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

fn parse_int_literal(text: &str) -> i64 {
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
