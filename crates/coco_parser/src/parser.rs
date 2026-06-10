//! Recursive descent parser for Coco declarations and statements.

use coco_diagnostics::Diagnostic;
use coco_lexer::{Lexer, Token, TokenKind};
use coco_span::Span;
use coco_syntax::*;

pub struct Parser<'a> {
    lexer: Lexer<'a>,
    current: Token,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Parser<'a> {
    pub fn new(source: &'a str) -> Self {
        let mut lexer = Lexer::new(source);
        let current = lexer.next_token();
        Self {
            lexer,
            current,
            diagnostics: Vec::new(),
        }
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    fn advance(&mut self) -> Token {
        let next = self.lexer.next_token();
        std::mem::replace(&mut self.current, next)
    }

    fn expect(&mut self, kind: TokenKind) -> bool {
        if self.current.kind == kind {
            self.advance();
            true
        } else {
            self.error(&format!("expected {:?}, got {:?}", kind, self.current.kind));
            false
        }
    }

    fn error(&mut self, msg: &str) {
        let span = self.current.span;
        self.diagnostics.push(
            Diagnostic::error(
                coco_span::FileId(0),
                msg.to_string(),
            )
            .with_label(span, "here", true),
        );
        self.synchronize();
    }

    fn synchronize(&mut self) {
        while !self.current.kind.is_sync_point() && self.current.kind != TokenKind::Eof {
            self.advance();
        }
    }

    fn eat(&mut self, kind: TokenKind) -> bool {
        if self.current.kind == kind {
            self.advance();
            true
        } else {
            false
        }
    }

    // ============================================================
    // Top-level parsing
    // ============================================================

    pub fn parse_program(&mut self) -> Program {
        let start = self.current.span.start;
        let mut items = Vec::new();

        while self.current.kind != TokenKind::Eof {
            match self.parse_item() {
                Some(item) => items.push(item),
                None => {
                    if self.current.kind != TokenKind::Eof {
                        self.advance();
                    }
                }
            }
        }

        let end = self.current.span.end;
        Program {
            items,
            span: Span::new(start, end),
        }
    }

    fn parse_item(&mut self) -> Option<Item> {
        match self.current.kind {
            TokenKind::Import => self.parse_import().map(Item::Import),
            TokenKind::Export => self.parse_export().map(Item::Export),
            TokenKind::Fn | TokenKind::Function | TokenKind::Async => {
                self.parse_fn_decl().map(Item::FnDecl)
            }
            TokenKind::Class => self.parse_class_decl().map(Item::ClassDecl),
            TokenKind::Interface => self.parse_interface_decl().map(Item::InterfaceDecl),
            TokenKind::Trait => self.parse_trait_decl().map(Item::TraitDecl),
            TokenKind::Enum => self.parse_enum_decl().map(Item::EnumDecl),
            TokenKind::Const => self.parse_const_decl().map(Item::ConstDecl),
            TokenKind::Let => self.parse_let_decl().map(Item::LetDecl),
            TokenKind::Type => self.parse_type_alias().map(Item::TypeAlias),
            _ => self.parse_stmt().map(Item::Stmt),
        }
    }

    // ============================================================
    // Declarations
    // ============================================================

    fn parse_fn_decl(&mut self) -> Option<FnDecl> {
        let start = self.current.span.start;
        let is_async = self.eat(TokenKind::Async);
        // Skip fn/function/f keyword
        let _ = self.eat(TokenKind::Fn);
        let _ = self.eat(TokenKind::Function);

        let name = self.parse_ident()?;
        let type_params = self.parse_type_params();

        let params = self.parse_params()?;
        let return_type = if self.eat(TokenKind::Colon) {
            Some(self.parse_type())
        } else {
            None
        };
        let body = self.parse_block()?;

        let end = body.span.end;
        Some(FnDecl {
            span: Span::new(start, end),
            is_async,
            name,
            type_params,
            params,
            return_type,
            body,
        })
    }

    fn parse_class_decl(&mut self) -> Option<ClassDecl> {
        let start = self.current.span.start;
        self.expect(TokenKind::Class);
        let name = self.parse_ident()?;
        let type_params = self.parse_type_params();
        let extends = if self.eat(TokenKind::Extends) {
            Some(self.parse_type())
        } else {
            None
        };
        let implements = if self.eat(TokenKind::Implements) {
            let mut ifaces = Vec::new();
            loop {
                ifaces.push(self.parse_type());
                if !self.eat(TokenKind::Comma) {
                    break;
                }
            }
            ifaces
        } else {
            Vec::new()
        };

        self.expect(TokenKind::LBrace);
        let mut members = Vec::new();
        while self.current.kind != TokenKind::RBrace && self.current.kind != TokenKind::Eof {
            if let Some(member) = self.parse_class_member() {
                members.push(member);
            }
        }
        let end = self.current.span.end;
        self.expect(TokenKind::RBrace);

        Some(ClassDecl {
            span: Span::new(start, end),
            name,
            type_params,
            extends,
            implements,
            members,
        })
    }

    fn parse_class_member(&mut self) -> Option<ClassMember> {
        let modifiers = self.parse_modifiers();

        match self.current.kind {
            TokenKind::Constructor => Some(ClassMember::Constructor(self.parse_constructor()?)),
            TokenKind::Use => {
                self.advance();
                let mut traits = Vec::new();
                loop {
                    if let Some(ident) = self.parse_ident() {
                        traits.push(ident);
                    }
                    if !self.eat(TokenKind::Comma) {
                        break;
                    }
                }
                self.expect(TokenKind::Semi);
                Some(ClassMember::UseTrait(UseTrait {
                    span: Span::new(self.current.span.start, self.current.span.end),
                    traits,
                }))
            }
            TokenKind::Ident => {
                // Could be method (with parens) or property (with colon)
                self.parse_method_or_property(modifiers)
            }
            TokenKind::Fn | TokenKind::Function => {
                self.parse_method(modifiers).map(ClassMember::Method)
            }
            TokenKind::Async => self.parse_method(modifiers).map(ClassMember::Method),
            TokenKind::Public
            | TokenKind::Private
            | TokenKind::Protected
            | TokenKind::Readonly
            | TokenKind::Static => {
                // Already handled modifiers, recurse
                self.parse_class_member()
            }
            _ => {
                self.advance();
                None
            }
        }
    }

    fn parse_constructor(&mut self) -> Option<Constructor> {
        let start = self.current.span.start;
        self.expect(TokenKind::Constructor);
        self.expect(TokenKind::LParen);
        let params = self.parse_constructor_params();
        self.expect(TokenKind::RParen);
        let body = self.parse_block()?;
        let end = body.span.end;
        Some(Constructor {
            span: Span::new(start, end),
            params,
            body,
        })
    }

    fn parse_constructor_params(&mut self) -> Vec<ConstructorParam> {
        let mut params = Vec::new();
        if self.current.kind != TokenKind::RParen {
            loop {
                let modifiers = self.parse_modifiers();
                if let Some(name) = self.parse_ident() {
                    let p_start = name.span.start;
                    let type_ann = if self.eat(TokenKind::Colon) {
                        Some(self.parse_type())
                    } else {
                        None
                    };
                    let default_value = if self.eat(TokenKind::Assign) {
                        Some(self.parse_expr())
                    } else {
                        None
                    };
                    params.push(ConstructorParam {
                        span: Span::new(p_start, self.current.span.end),
                        modifiers,
                        name,
                        type_ann,
                        default_value,
                    });
                }
                if !self.eat(TokenKind::Comma) {
                    break;
                }
            }
        }
        params
    }

    fn parse_method_or_property(&mut self, modifiers: Vec<Modifier>) -> Option<ClassMember> {
        // We need to look ahead to determine if this is a method or property.
        // Method: ident(...), Property: ident: Type
        // Since we can't peek easily, save name and check next token
        let name = self.parse_ident()?;
        if self.current.kind == TokenKind::LParen || self.current.kind == TokenKind::Lt {
            // Method with type params or direct parens
            Some(ClassMember::Method(
                self.finish_parse_method(modifiers, name)?,
            ))
        } else if self.eat(TokenKind::Colon) {
            // Property
            let type_ann = self.parse_type();
            let default_value = if self.eat(TokenKind::Assign) {
                Some(self.parse_expr())
            } else {
                None
            };
            self.expect(TokenKind::Semi);
            let end = self.current.span.end;
            Some(ClassMember::Property(Property {
                span: Span::new(name.span.start, end),
                modifiers,
                name,
                type_ann,
                default_value,
            }))
        } else {
            None
        }
    }

    fn parse_method(&mut self, modifiers: Vec<Modifier>) -> Option<Method> {
        let start = self.current.span.start;
        let is_async = self.eat(TokenKind::Async);
        let _ = self.eat(TokenKind::Fn);
        let _ = self.eat(TokenKind::Function);
        let name = self.parse_ident()?;
        self.finish_parse_method_detail(modifiers, is_async, name, start)
    }

    fn finish_parse_method(&mut self, modifiers: Vec<Modifier>, name: Ident) -> Option<Method> {
        let start = name.span.start;
        self.finish_parse_method_detail(modifiers, false, name, start)
    }

    fn finish_parse_method_detail(
        &mut self,
        modifiers: Vec<Modifier>,
        is_async: bool,
        name: Ident,
        start: usize,
    ) -> Option<Method> {
        let type_params = self.parse_type_params();
        let params = self.parse_params()?;
        let return_type = if self.eat(TokenKind::Colon) {
            Some(self.parse_type())
        } else {
            None
        };
        let body = self.parse_block()?;
        let end = body.span.end;
        Some(Method {
            span: Span::new(start, end),
            modifiers,
            is_async,
            name,
            type_params,
            params,
            return_type,
            body,
        })
    }

    fn parse_interface_decl(&mut self) -> Option<InterfaceDecl> {
        let start = self.current.span.start;
        self.expect(TokenKind::Interface);
        let name = self.parse_ident()?;
        let type_params = self.parse_type_params();
        let extends = if self.eat(TokenKind::Extends) {
            Some(self.parse_type())
        } else {
            None
        };

        self.expect(TokenKind::LBrace);
        let mut members = Vec::new();
        while self.current.kind != TokenKind::RBrace && self.current.kind != TokenKind::Eof {
            if self.current.kind == TokenKind::Ident {
                let m_name = self.parse_ident()?;
                if self.current.kind == TokenKind::LParen || self.current.kind == TokenKind::Lt {
                    // Method signature
                    let type_params = self.parse_type_params();
                    let params = self.parse_params()?;
                    self.expect(TokenKind::Colon);
                    let return_type = self.parse_type();
                    self.expect(TokenKind::Semi);
                    let end = self.current.span.end;
                    members.push(InterfaceMember::MethodSignature(MethodSignature {
                        span: Span::new(start, end),
                        is_async: false,
                        name: m_name,
                        type_params,
                        params,
                        return_type,
                    }));
                } else if self.eat(TokenKind::Colon) {
                    let type_ann = self.parse_type();
                    self.expect(TokenKind::Semi);
                    let end = self.current.span.end;
                    members.push(InterfaceMember::PropertySignature(PropertySignature {
                        span: Span::new(start, end),
                        name: m_name,
                        type_ann,
                    }));
                }
            } else {
                self.advance();
            }
        }
        let end = self.current.span.end;
        self.expect(TokenKind::RBrace);

        Some(InterfaceDecl {
            span: Span::new(start, end),
            name,
            type_params,
            extends,
            members,
        })
    }

    fn parse_trait_decl(&mut self) -> Option<TraitDecl> {
        let start = self.current.span.start;
        self.expect(TokenKind::Trait);
        let name = self.parse_ident()?;
        let type_params = self.parse_type_params();

        self.expect(TokenKind::LBrace);
        let mut members = Vec::new();
        while self.current.kind != TokenKind::RBrace && self.current.kind != TokenKind::Eof {
            if let Some(m) = self.parse_trait_member() {
                members.push(m);
            } else {
                self.advance();
            }
        }
        let end = self.current.span.end;
        self.expect(TokenKind::RBrace);

        Some(TraitDecl {
            span: Span::new(start, end),
            name,
            type_params,
            members,
        })
    }

    fn parse_trait_member(&mut self) -> Option<TraitMember> {
        let _modifiers = self.parse_modifiers();
        // Try as method
        if let Some(class_member) = self.parse_class_member() {
            match class_member {
                ClassMember::Method(m) => Some(TraitMember::Method(m)),
                ClassMember::Property(p) => Some(TraitMember::Property(p)),
                _ => None,
            }
        } else {
            None
        }
    }

    fn parse_enum_decl(&mut self) -> Option<EnumDecl> {
        let start = self.current.span.start;
        self.expect(TokenKind::Enum);
        let name = self.parse_ident()?;
        let backing_type = if self.eat(TokenKind::Colon) {
            Some(self.parse_type())
        } else {
            None
        };

        self.expect(TokenKind::LBrace);
        let mut variants = Vec::new();
        while self.current.kind != TokenKind::RBrace && self.current.kind != TokenKind::Eof {
            if self.current.kind == TokenKind::Ident {
                let v_start = self.current.span.start;
                let v_name = self.parse_ident()?;
                let fields = if self.current.kind == TokenKind::LParen {
                    self.advance();
                    let mut types = Vec::new();
                    loop {
                        types.push(self.parse_type());
                        if !self.eat(TokenKind::Comma) {
                            break;
                        }
                    }
                    self.expect(TokenKind::RParen);
                    Some(types)
                } else {
                    None
                };
                let value = if self.eat(TokenKind::Assign) {
                    Some(self.parse_expr())
                } else {
                    None
                };
                variants.push(EnumVariant {
                    span: Span::new(v_start, self.current.span.end),
                    name: v_name,
                    fields,
                    value,
                });
                if !self.eat(TokenKind::Comma) {
                    break;
                }
            } else {
                self.advance();
            }
        }
        let end = self.current.span.end;
        self.expect(TokenKind::RBrace);

        Some(EnumDecl {
            span: Span::new(start, end),
            name,
            backing_type,
            variants,
        })
    }

    fn parse_const_decl(&mut self) -> Option<ConstDecl> {
        let start = self.current.span.start;
        self.expect(TokenKind::Const);
        let name = self.parse_ident()?;
        let type_ann = if self.eat(TokenKind::Colon) {
            Some(self.parse_type())
        } else {
            None
        };
        self.expect(TokenKind::Assign);
        let value = self.parse_expr();
        self.expect(TokenKind::Semi);
        let end = self.current.span.end;
        Some(ConstDecl {
            span: Span::new(start, end),
            name,
            type_ann,
            value,
        })
    }

    fn parse_let_decl(&mut self) -> Option<LetDecl> {
        let start = self.current.span.start;
        self.expect(TokenKind::Let);
        let name = self.parse_ident()?;
        let type_ann = if self.eat(TokenKind::Colon) {
            Some(self.parse_type())
        } else {
            None
        };
        let value = if self.eat(TokenKind::Assign) {
            Some(self.parse_expr())
        } else {
            None
        };
        self.expect(TokenKind::Semi);
        let end = self.current.span.end;
        Some(LetDecl {
            span: Span::new(start, end),
            name,
            type_ann,
            value,
        })
    }

    fn parse_type_alias(&mut self) -> Option<TypeAlias> {
        let start = self.current.span.start;
        self.expect(TokenKind::Type);
        let name = self.parse_ident()?;
        let type_params = self.parse_type_params();
        self.expect(TokenKind::Assign);
        let target = self.parse_type();
        self.expect(TokenKind::Semi);
        let end = self.current.span.end;
        Some(TypeAlias {
            span: Span::new(start, end),
            name,
            type_params,
            target,
        })
    }

    fn parse_import(&mut self) -> Option<Import> {
        let start = self.current.span.start;
        self.expect(TokenKind::Import);
        let items = if self.current.kind == TokenKind::LBrace {
            self.advance();
            let mut names = Vec::new();
            loop {
                if let Some(ident) = self.parse_ident() {
                    names.push(ident);
                }
                if !self.eat(TokenKind::Comma) {
                    break;
                }
            }
            self.expect(TokenKind::RBrace);
            ImportItems::Named(names)
        } else if self.current.kind == TokenKind::Star {
            self.advance();
            self.expect(TokenKind::As);
            let alias = self.parse_ident()?;
            ImportItems::Namespace(alias)
        } else {
            return None;
        };
        self.expect(TokenKind::From);
        let source = self.parse_string_literal();
        self.expect(TokenKind::Semi);
        let end = self.current.span.end;
        Some(Import {
            span: Span::new(start, end),
            items,
            source,
        })
    }

    fn parse_export(&mut self) -> Option<Export> {
        let start = self.current.span.start;
        self.expect(TokenKind::Export);
        let item = self.parse_item()?;
        Some(Export {
            span: Span::new(start, self.current.span.end),
            item: Box::new(item),
        })
    }

    // ============================================================
    // Statements
    // ============================================================

    fn parse_stmt(&mut self) -> Option<Stmt> {
        match self.current.kind {
            TokenKind::If => Some(Stmt::If(self.parse_if_stmt()?)),
            TokenKind::For => Some(Stmt::For(self.parse_for_stmt()?)),
            TokenKind::While => Some(Stmt::While(self.parse_while_stmt()?)),
            TokenKind::Do => Some(Stmt::DoWhile(self.parse_do_while_stmt()?)),
            TokenKind::Loop => Some(Stmt::Loop(self.parse_loop_stmt()?)),
            TokenKind::Return => Some(Stmt::Return(self.parse_return_stmt()?)),
            TokenKind::Throw => Some(Stmt::Throw(self.parse_throw_stmt()?)),
            TokenKind::Try => Some(Stmt::Try(self.parse_try_stmt()?)),
            TokenKind::Break => {
                let span = self.current.span;
                self.advance();
                self.expect(TokenKind::Semi);
                Some(Stmt::Break(BreakStmt { span }))
            }
            TokenKind::Continue => {
                let span = self.current.span;
                self.advance();
                self.expect(TokenKind::Semi);
                Some(Stmt::Continue(ContinueStmt { span }))
            }
            TokenKind::Await => {
                // Could be await parallel or await expression
                self.advance();
                if self.eat(TokenKind::Parallel) {
                    Some(Stmt::Parallel(self.parse_parallel_stmt_body()?))
                } else {
                    // Fall back to expression
                    let expr = self.parse_expr();
                    let span = expr.span();
                    self.expect(TokenKind::Semi);
                    Some(Stmt::Expr(ExprStmt { span, expr }))
                }
            }
            TokenKind::Parallel => Some(Stmt::Parallel(self.parse_parallel_stmt()?)),
            TokenKind::Coro => {
                let span = self.current.span;
                self.advance();
                let body = self.parse_block()?;
                self.eat(TokenKind::Semi); // optional trailing semicolon
                Some(Stmt::Coro(CoroStmt { span, body }))
            }
            TokenKind::Select => {
                self.advance();
                self.expect(TokenKind::LBrace);
                let mut cases = Vec::new();
                while self.current.kind != TokenKind::RBrace && self.current.kind != TokenKind::Eof
                {
                    if self.current.kind == TokenKind::Case {
                        self.advance();
                        let pat = self.parse_ident()?;
                        self.expect(TokenKind::Assign);
                        let expr = self.parse_expr();
                        self.expect(TokenKind::Colon);
                        let mut body = Vec::new();
                        while self.current.kind != TokenKind::Case
                            && self.current.kind != TokenKind::RBrace
                            && self.current.kind != TokenKind::Eof
                        {
                            if let Some(s) = self.parse_stmt() {
                                body.push(s);
                            } else {
                                break;
                            }
                        }
                        cases.push(CaseClause {
                            span: Span::new(self.current.span.start, self.current.span.end),
                            pattern: pat,
                            expr,
                            body,
                        });
                    } else {
                        self.advance();
                    }
                }
                let end = self.current.span.end;
                self.expect(TokenKind::RBrace);
                Some(Stmt::Select(SelectStmt {
                    span: Span::new(self.current.span.start, end),
                    cases,
                }))
            }
            TokenKind::Unsafe => {
                let span = self.current.span;
                self.advance();
                let body = self.parse_block()?;
                Some(Stmt::Unsafe(UnsafeStmt { span, body }))
            }
            TokenKind::Synchronized => {
                let span = self.current.span;
                self.advance();
                let body = self.parse_block()?;
                Some(Stmt::Synchronized(SynchronizedStmt { span, body }))
            }
            _ => {
                // Expression statement
                let start = self.current.span.start;
                let expr = self.parse_expr();
                let end = expr.span_end();
                self.expect(TokenKind::Semi);
                Some(Stmt::Expr(ExprStmt {
                    span: Span::new(start, end),
                    expr,
                }))
            }
        }
    }

    fn parse_if_stmt(&mut self) -> Option<IfStmt> {
        let start = self.current.span.start;
        self.expect(TokenKind::If);
        let condition = self.parse_expr();
        let then_block = self.parse_block()?;

        let mut else_ifs = Vec::new();
        let mut else_block = None;

        while self.current.kind == TokenKind::Else {
            self.advance();
            if self.current.kind == TokenKind::If {
                self.advance();
                let ei_start = self.current.span.start;
                let condition = self.parse_expr();
                let block = self.parse_block()?;
                else_ifs.push(ElseIf {
                    span: Span::new(ei_start, block.span.end),
                    condition,
                    block,
                });
            } else {
                else_block = self.parse_block();
                break;
            }
        }

        let end = else_block
            .as_ref()
            .map(|b| b.span.end)
            .or_else(|| else_ifs.last().map(|e| e.block.span.end))
            .unwrap_or(then_block.span.end);

        Some(IfStmt {
            span: Span::new(start, end),
            condition,
            then_block,
            else_ifs,
            else_block,
        })
    }

    fn parse_for_stmt(&mut self) -> Option<ForStmt> {
        let start = self.current.span.start;
        self.expect(TokenKind::For);
        let pattern = self.parse_ident()?;
        self.expect(TokenKind::In);
        let iterable = self.parse_expr();
        let body = self.parse_block()?;
        Some(ForStmt {
            span: Span::new(start, body.span.end),
            pattern,
            iterable,
            body,
        })
    }

    fn parse_while_stmt(&mut self) -> Option<WhileStmt> {
        let start = self.current.span.start;
        self.expect(TokenKind::While);
        let condition = self.parse_expr();
        let body = self.parse_block()?;
        Some(WhileStmt {
            span: Span::new(start, body.span.end),
            condition,
            body,
        })
    }

    fn parse_do_while_stmt(&mut self) -> Option<DoWhileStmt> {
        let start = self.current.span.start;
        self.expect(TokenKind::Do);
        let body = self.parse_block()?;
        self.expect(TokenKind::While);
        let condition = self.parse_expr();
        self.expect(TokenKind::Semi);
        Some(DoWhileStmt {
            span: Span::new(start, self.current.span.end),
            body,
            condition,
        })
    }

    /// Parse a template literal: splits `text ${expr} more` into alternating
    /// static string parts and expression holes.
    fn parse_template(&mut self, raw: String, span: Span) -> Expr {
        let mut parts: Vec<TemplatePart> = Vec::new();
        let mut current = String::new();
        let mut chars = raw.chars().peekable();
        // The raw text includes the backtick delimiters; strip them.
        let inner = if raw.starts_with('`') && raw.ends_with('`') {
            &raw[1..raw.len() - 1]
        } else {
            &raw
        };

        let mut i = 0;
        let bytes = inner.as_bytes();
        while i < bytes.len() {
            if i + 1 < bytes.len() && bytes[i] == b'$' && bytes[i + 1] == b'{' {
                // End current static part
                if !current.is_empty() {
                    parts.push(TemplatePart::Static(current.clone()));
                    current.clear();
                }
                // Find matching }
                i += 2; // skip ${
                let mut depth = 1;
                let expr_start = i;
                while i < bytes.len() && depth > 0 {
                    if bytes[i] == b'{' {
                        depth += 1;
                    } else if bytes[i] == b'}' {
                        depth -= 1;
                    }
                    if depth > 0 {
                        i += 1;
                    }
                }
                let expr_end = i;
                let expr_str = std::str::from_utf8(&bytes[expr_start..expr_end]).unwrap_or("");
                // Parse the expression
                let mut expr_parser = Parser::new(expr_str);
                let parsed = expr_parser.parse_expr();
                parts.push(TemplatePart::Expr(parsed));
                i += 1; // skip closing }
            } else {
                current.push(bytes[i] as char);
                i += 1;
            }
        }
        if !current.is_empty() {
            parts.push(TemplatePart::Static(current));
        }

        Expr::Template(Box::new(TemplateExpr { span, parts }))
    }

    fn parse_loop_stmt(&mut self) -> Option<LoopStmt> {
        let start = self.current.span.start;
        self.expect(TokenKind::Loop);
        let body = self.parse_block()?;
        Some(LoopStmt {
            span: Span::new(start, body.span.end),
            body,
        })
    }

    fn parse_return_stmt(&mut self) -> Option<ReturnStmt> {
        let start = self.current.span.start;
        self.expect(TokenKind::Return);
        let value = if self.current.kind != TokenKind::Semi {
            Some(self.parse_expr())
        } else {
            None
        };
        self.expect(TokenKind::Semi);
        Some(ReturnStmt {
            span: Span::new(start, self.current.span.end),
            value,
        })
    }

    fn parse_throw_stmt(&mut self) -> Option<ThrowStmt> {
        let start = self.current.span.start;
        self.expect(TokenKind::Throw);
        let value = self.parse_expr();
        self.expect(TokenKind::Semi);
        Some(ThrowStmt {
            span: Span::new(start, self.current.span.end),
            value,
        })
    }

    fn parse_try_stmt(&mut self) -> Option<TryStmt> {
        let start = self.current.span.start;
        self.expect(TokenKind::Try);
        let body = self.parse_block()?;

        let mut catches = Vec::new();
        while self.current.kind == TokenKind::Catch {
            let c_start = self.current.span.start;
            self.advance();
            self.expect(TokenKind::LParen);
            let param = self.parse_ident()?;
            let type_ann = if self.eat(TokenKind::Colon) {
                Some(self.parse_type())
            } else {
                None
            };
            self.expect(TokenKind::RParen);
            let body = self.parse_block()?;
            catches.push(CatchClause {
                span: Span::new(c_start, body.span.end),
                param,
                type_ann,
                body,
            });
        }

        let finally = if self.eat(TokenKind::Finally) {
            self.parse_block()
        } else {
            None
        };

        let end = finally
            .as_ref()
            .map(|b| b.span.end)
            .or_else(|| catches.last().map(|c| c.body.span.end))
            .unwrap_or(body.span.end);

        Some(TryStmt {
            span: Span::new(start, end),
            body,
            catches,
            finally,
        })
    }

    fn parse_parallel_stmt(&mut self) -> Option<ParallelStmt> {
        let _start = self.current.span.start;
        self.expect(TokenKind::Parallel);
        self.parse_parallel_stmt_body()
    }

    fn parse_parallel_stmt_body(&mut self) -> Option<ParallelStmt> {
        self.expect(TokenKind::LBrace);
        let mut runs = Vec::new();
        while self.current.kind != TokenKind::RBrace && self.current.kind != TokenKind::Eof {
            if self.current.kind == TokenKind::Run {
                let r_start = self.current.span.start;
                self.advance();
                let expr = self.parse_expr();
                self.expect(TokenKind::Semi);
                runs.push(RunClause {
                    span: Span::new(r_start, self.current.span.end),
                    expr,
                });
            } else {
                self.advance();
            }
        }
        let end = self.current.span.end;
        self.expect(TokenKind::RBrace);
        Some(ParallelStmt {
            span: Span::new(self.current.span.start, end),
            runs,
        })
    }

    // ============================================================
    // Block and params
    // ============================================================

    fn parse_block(&mut self) -> Option<Block> {
        let start = self.current.span.start;
        if !self.eat(TokenKind::LBrace) {
            return None;
        }
        let mut stmts = Vec::new();
        while self.current.kind != TokenKind::RBrace && self.current.kind != TokenKind::Eof {
            match self.current.kind {
                TokenKind::Let
                | TokenKind::Const
                | TokenKind::Fn
                | TokenKind::Function
                | TokenKind::Async => {
                    if let Some(item) = self.parse_item() {
                        stmts.push(Stmt::Item(Box::new(item)));
                    } else {
                        self.advance();
                    }
                }
                _ => {
                    if let Some(stmt) = self.parse_stmt() {
                        stmts.push(stmt);
                    } else if self.current.kind != TokenKind::RBrace {
                        self.advance();
                    }
                }
            }
        }
        let end = self.current.span.end;
        self.eat(TokenKind::RBrace);
        Some(Block {
            span: Span::new(start, end),
            stmts,
        })
    }

    fn parse_params(&mut self) -> Option<Vec<Param>> {
        self.expect(TokenKind::LParen);
        let mut params = Vec::new();
        if self.current.kind != TokenKind::RParen {
            loop {
                let p_start = self.current.span.start;
                let name = self.parse_ident()?;
                let type_ann = if self.eat(TokenKind::Colon) {
                    Some(self.parse_type())
                } else {
                    None
                };
                let default_value = if self.eat(TokenKind::Assign) {
                    Some(self.parse_expr())
                } else {
                    None
                };
                params.push(Param {
                    span: Span::new(p_start, self.current.span.end),
                    name,
                    type_ann,
                    default_value,
                });
                if !self.eat(TokenKind::Comma) {
                    break;
                }
            }
        }
        self.expect(TokenKind::RParen);
        Some(params)
    }

    // ============================================================
    // Helpers
    // ============================================================

    fn parse_ident(&mut self) -> Option<Ident> {
        if self.current.kind == TokenKind::Ident
            || self.current.kind == TokenKind::True
            || self.current.kind == TokenKind::False
            || self.current.kind == TokenKind::Null
        {
            let span = self.current.span;
            let name = self.current.text.clone();
            self.advance();
            Some(Ident { name, span })
        } else {
            None
        }
    }

    // ============================================================
    // Expression parsing (Pratt parser)
    // ============================================================

    fn parse_expr(&mut self) -> Expr {
        self.parse_expr_bp(0)
    }

    fn parse_expr_bp(&mut self, min_bp: u8) -> Expr {
        let mut lhs = self.parse_prefix_expr();

        // Postfix operators (member access, indexing, calls)
        lhs = self.parse_postfix(lhs);

        loop {
            // Break conditions
            match self.current.kind {
                TokenKind::Eof
                | TokenKind::Semi
                | TokenKind::RBrace
                | TokenKind::RParen
                | TokenKind::RBracket
                | TokenKind::Comma
                | TokenKind::Colon
                | TokenKind::FatArrow => break,
                _ => {}
            }

            let Some((left_bp, right_bp)) = crate::expr::infix_bp(self.current.kind) else {
                break;
            };
            if left_bp < min_bp {
                break;
            }

            // Handle ternary `? ... : ...` vs postfix `?`
            if self.current.kind == TokenKind::Question {
                // Clone lexer to peek at the next token.
                let mut peek_lexer = self.lexer.clone();
                let next = peek_lexer.next_token();
                let is_postfix_q = matches!(
                    next.kind,
                    TokenKind::Semi
                        | TokenKind::RParen
                        | TokenKind::RBrace
                        | TokenKind::Comma
                        | TokenKind::RBracket
                        | TokenKind::Colon
                        | TokenKind::Eof
                );
                if is_postfix_q {
                    let span = Span::new(lhs.span_start(), self.current.span.end);
                    self.advance();
                    lhs = Expr::Postfix(Box::new(PostfixExpr {
                        span,
                        object: lhs,
                        op: PostfixOp::Question,
                    }));
                    lhs = self.parse_postfix(lhs);
                    continue;
                }
                lhs = self.parse_ternary_expr(lhs);
                lhs = self.parse_postfix(lhs);
                continue;
            }

            // Handle range `..` or `..=`
            if self.current.kind == TokenKind::Range
                || self.current.kind == TokenKind::RangeInclusive
            {
                let is_inclusive = self.current.kind == TokenKind::RangeInclusive;
                self.advance();
                let rhs = self.parse_expr_bp(right_bp);
                let span = Span::new(lhs.span_start(), rhs.span_end());
                lhs = Expr::Binary(Box::new(BinaryExpr {
                    span,
                    left: lhs,
                    op: if is_inclusive {
                        BinaryOp::RangeInclusive
                    } else {
                        BinaryOp::Range
                    },
                    right: rhs,
                }));
                lhs = self.parse_postfix(lhs);
                continue;
            }

            // Handle pipe operators
            if self.current.kind == TokenKind::PipeRight || self.current.kind == TokenKind::PipeLeft
            {
                let op = if self.current.kind == TokenKind::PipeRight {
                    PipeOp::PipeRight
                } else {
                    PipeOp::PipeLeft
                };
                self.advance();
                let rhs = self.parse_expr_bp(right_bp);
                let span = Span::new(lhs.span_start(), rhs.span_end());
                lhs = Expr::Pipe(Box::new(PipeExpr {
                    span,
                    left: lhs,
                    op,
                    right: rhs,
                }));
                lhs = self.parse_postfix(lhs);
                continue;
            }

            // Handle null coalesce `??`
            if self.current.kind == TokenKind::QuestionQuestion {
                self.advance();
                let rhs = self.parse_expr_bp(right_bp);
                let span = Span::new(lhs.span_start(), rhs.span_end());
                lhs = Expr::NullCoalesce(Box::new(NullCoalesceExpr {
                    span,
                    left: lhs,
                    right: rhs,
                }));
                lhs = self.parse_postfix(lhs);
                continue;
            }

            // Handle elvis `?:`
            if self.current.kind == TokenKind::QuestionColon {
                self.advance();
                let rhs = self.parse_expr_bp(right_bp);
                let span = Span::new(lhs.span_start(), rhs.span_end());
                lhs = Expr::Elvis(Box::new(ElvisExpr {
                    span,
                    left: lhs,
                    right: rhs,
                }));
                lhs = self.parse_postfix(lhs);
                continue;
            }

            // General binary operator
            let op = crate::expr::token_to_binary_op(self.current.kind);
            if let Some(binop) = op {
                self.advance();
                let rhs = self.parse_expr_bp(right_bp);
                let span = Span::new(lhs.span_start(), rhs.span_end());
                lhs = Expr::Binary(Box::new(BinaryExpr {
                    span,
                    left: lhs,
                    op: binop,
                    right: rhs,
                }));
                lhs = self.parse_postfix(lhs);
                continue;
            }

            break;
        }

        lhs
    }

    fn parse_prefix_expr(&mut self) -> Expr {
        let start_pos = self.current.span.start;

        match self.current.kind {
            TokenKind::Minus => {
                self.advance();
                let expr = self.parse_expr_bp(90);
                let end_pos = expr.span_end();
                Expr::Unary(Box::new(UnaryExpr {
                    span: Span::new(start_pos, end_pos),
                    op: UnaryOp::Neg,
                    expr,
                }))
            }
            TokenKind::Not => {
                self.advance();
                let expr = self.parse_expr_bp(90);
                let end_pos = expr.span_end();
                Expr::Unary(Box::new(UnaryExpr {
                    span: Span::new(start_pos, end_pos),
                    op: UnaryOp::Not,
                    expr,
                }))
            }
            TokenKind::BitNot => {
                self.advance();
                let expr = self.parse_expr_bp(90);
                let end_pos = expr.span_end();
                Expr::Unary(Box::new(UnaryExpr {
                    span: Span::new(start_pos, end_pos),
                    op: UnaryOp::BitNot,
                    expr,
                }))
            }
            TokenKind::Typeof => {
                self.advance();
                let expr = self.parse_expr_bp(90);
                let end_pos = expr.span_end();
                Expr::Unary(Box::new(UnaryExpr {
                    span: Span::new(start_pos, end_pos),
                    op: UnaryOp::Typeof,
                    expr,
                }))
            }
            TokenKind::Await => {
                self.advance();
                let expr = self.parse_expr_bp(90);
                let end_pos = expr.span_end();
                Expr::Unary(Box::new(UnaryExpr {
                    span: Span::new(start_pos, end_pos),
                    op: UnaryOp::Await,
                    expr,
                }))
            }
            TokenKind::New => {
                self.advance();
                let type_name = self.parse_ident().unwrap_or(Ident {
                    name: String::new(),
                    span: Span::new(start_pos, start_pos),
                });
                let args = self.parse_arg_list();
                let end_pos = self.current.span.start;
                Expr::New(Box::new(NewExpr {
                    span: Span::new(start_pos, end_pos),
                    type_name,
                    args,
                }))
            }
            _ => self.parse_primary_expr(),
        }
    }

    fn parse_primary_expr(&mut self) -> Expr {
        let start_pos = self.current.span.start;

        match self.current.kind {
            TokenKind::Ident => {
                let ident = self.parse_ident().unwrap_or(Ident {
                    name: String::new(),
                    span: Span::new(start_pos, start_pos),
                });
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
            TokenKind::Super => {
                let span = self.current.span;
                self.advance();
                Expr::Super(span)
            }
            TokenKind::LParen => {
                if self.starts_parenthesized_lambda() {
                    return self.parse_lambda_expr();
                }
                self.advance();
                let expr = self.parse_expr_bp(0);
                self.expect(TokenKind::RParen);
                Expr::Group(Box::new(expr))
            }
            TokenKind::LBracket => self.parse_array_literal(),
            TokenKind::LBrace => self.parse_object_literal(),
            TokenKind::Match => self.parse_match_expr(),
            TokenKind::Async | TokenKind::Fn | TokenKind::Function => self.parse_lambda_expr(),
            TokenKind::Parallel => {
                let start = self.current.span.start;
                self.advance();
                if let Some(parallel_stmt) = self.parse_parallel_stmt_body() {
                    let end = parallel_stmt.span.end;
                    Expr::Parallel(Box::new(ParallelExpr {
                        span: Span::new(start, end),
                        runs: parallel_stmt.runs,
                    }))
                } else {
                    Expr::Literal(Literal::Null(Span::new(start, start)))
                }
            }
            TokenKind::Lazy => {
                let span = self.current.span;
                self.advance();
                let expr = self.parse_expr_bp(0);
                Expr::Lazy(Box::new(expr))
            }
            TokenKind::TemplateLiteral => {
                let text = self.current.text.clone();
                let span = self.current.span;
                self.advance();
                self.parse_template(text, span)
            }
            // Keywords that are valid as identifiers in expression context
            TokenKind::Ok | TokenKind::Err | TokenKind::Result
            | TokenKind::Typeof | TokenKind::As | TokenKind::Is
            | TokenKind::From | TokenKind::Use
            | TokenKind::Await
            | TokenKind::Coro | TokenKind::Select => {
                let text = self.current.text.clone();
                let span = self.current.span;
                self.advance();
                Expr::Ident(Ident {
                    name: text,
                    span,
                })
            }
            _ => {
                let span = self.current.span;
                self.advance();
                Expr::Literal(Literal::Null(span))
            }
        }
    }

    fn starts_parenthesized_lambda(&self) -> bool {
        if self.current.kind != TokenKind::LParen {
            return false;
        }

        let mut probe = Self {
            lexer: self.lexer.clone(),
            current: self.current.clone(),
            diagnostics: Vec::new(),
        };

        let _ = probe.parse_lambda_params();
        if !probe.diagnostics.is_empty() {
            return false;
        }

        if probe.eat(TokenKind::Colon) {
            let _ = probe.parse_type();
            if !probe.diagnostics.is_empty() {
                return false;
            }
        }

        probe.current.kind == TokenKind::FatArrow
    }

    fn parse_postfix(&mut self, lhs: Expr) -> Expr {
        let mut result = lhs;
        loop {
            match self.current.kind {
                TokenKind::Dot => {
                    self.advance();
                    let name = self.parse_ident().unwrap_or(Ident {
                        name: String::new(),
                        span: Span::new(self.current.span.start, self.current.span.start),
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
                        span: Span::new(self.current.span.start, self.current.span.start),
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
                    let end = self.current.span.end;
                    self.expect(TokenKind::RBracket);
                    let span = Span::new(result.span_start(), end);
                    result = Expr::Index(Box::new(IndexExpr {
                        span,
                        object: result,
                        index,
                    }));
                }
                TokenKind::LParen => {
                    let args = self.parse_arg_list();
                    let span = Span::new(result.span_start(), self.current.span.start);
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
        self.advance(); // eat `?`
        let then_expr = self.parse_expr_bp(0);
        self.expect(TokenKind::Colon);
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
        self.expect(TokenKind::LParen);
        let mut args = Vec::new();
        if self.current.kind != TokenKind::RParen && self.current.kind != TokenKind::Eof {
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
                if self.current.kind == TokenKind::RParen {
                    break; // trailing comma
                }
            }
        }
        self.expect(TokenKind::RParen);
        args
    }

    fn parse_array_literal(&mut self) -> Expr {
        let start = self.current.span.start;
        self.advance(); // [
        let mut elements = Vec::new();
        if self.current.kind != TokenKind::RBracket && self.current.kind != TokenKind::Eof {
            loop {
                elements.push(self.parse_expr_bp(0));
                if !self.eat(TokenKind::Comma) {
                    break;
                }
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
        if self.current.kind != TokenKind::RBrace && self.current.kind != TokenKind::Eof {
            loop {
                let key_start = self.current.span.start;
                let key = match self.current.kind {
                    TokenKind::Ident => {
                        let name = self.parse_ident().unwrap_or(Ident {
                            name: String::new(),
                            span: Span::new(key_start, key_start),
                        });
                        ObjectKey::Ident(name)
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
                        ObjectKey::String(inner, span)
                    }
                    _ => break,
                };
                self.expect(TokenKind::Colon);
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
        let scrutinee = self.parse_expr_bp(0);
        self.expect(TokenKind::LBrace);
        let mut arms = Vec::new();
        while self.current.kind != TokenKind::RBrace && self.current.kind != TokenKind::Eof {
            let arm_start = self.current.span.start;
            let pattern = self.parse_pattern();
            self.expect(TokenKind::FatArrow);
            let body = self.parse_expr_bp(0);
            let arm_end = body.span_end();
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
                let ident = self.parse_ident().unwrap_or(Ident {
                    name: String::new(),
                    span: self.current.span,
                });
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
                let val = crate::expr::parse_int_literal(&text);
                Pattern::Literal(Literal::Int(val, span))
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

    fn parse_lambda_expr(&mut self) -> Expr {
        let start = self.current.span.start;
        let is_async = self.eat(TokenKind::Async);

        // Skip fn/function keyword if present
        let _ = self.eat(TokenKind::Fn);
        let _ = self.eat(TokenKind::Function);

        if self.current.kind == TokenKind::LParen {
            // Lambda: (params) => body or (params): Type => body
            let params = self.parse_lambda_params();
            let return_type = if self.eat(TokenKind::Colon) {
                Some(self.parse_type())
            } else {
                None
            };
            self.expect(TokenKind::FatArrow);
            let body = if self.current.kind == TokenKind::LBrace {
                let block = self.parse_block().unwrap_or(Block {
                    span: Span::new(self.current.span.start, self.current.span.start),
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
        } else {
            // Single-param arrow: ident => body
            let name = self.parse_ident().unwrap_or(Ident {
                name: String::new(),
                span: Span::new(start, start),
            });
            let param = Param {
                span: name.span,
                name: name.clone(),
                type_ann: None,
                default_value: None,
            };
            self.expect(TokenKind::FatArrow);
            let body = if self.current.kind == TokenKind::LBrace {
                let block = self.parse_block().unwrap_or(Block {
                    span: Span::new(self.current.span.start, self.current.span.start),
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
                params: vec![param],
                return_type: None,
                body,
            }))
        }
    }

    fn parse_lambda_params(&mut self) -> Vec<Param> {
        self.expect(TokenKind::LParen);
        let mut params = Vec::new();
        if self.current.kind != TokenKind::RParen && self.current.kind != TokenKind::Eof {
            loop {
                let p_start = self.current.span.start;
                let name = self.parse_ident().unwrap_or(Ident {
                    name: String::new(),
                    span: Span::new(p_start, p_start),
                });
                let type_ann = if self.eat(TokenKind::Colon) {
                    Some(self.parse_type())
                } else {
                    None
                };
                let default_value = if self.eat(TokenKind::Assign) {
                    Some(self.parse_expr_bp(0))
                } else {
                    None
                };
                params.push(Param {
                    span: Span::new(p_start, self.current.span.end),
                    name,
                    type_ann,
                    default_value,
                });
                if !self.eat(TokenKind::Comma) {
                    break;
                }
            }
        }
        self.expect(TokenKind::RParen);
        params
    }

    // ============================================================
    // Type parsing
    // ============================================================

    fn parse_type(&mut self) -> Type {
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
            TokenKind::Ident | TokenKind::Result => {
                let name = self.current.text.clone();
                let span = self.current.span;
                self.advance();
                // Check for primitive types
                if let Some(pt) = crate::expr::parse_primitive_type(&name) {
                    return Type::Primitive(pt, span);
                }
                // Check for Result<T, E>
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
                // Check for list<T>
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
                // Check for map<K, V>
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
                // Check for tuple<T, ...>
                if name == "tuple" && self.current.kind == TokenKind::Lt {
                    self.advance();
                    let mut element_types = Vec::new();
                    loop {
                        element_types.push(self.parse_type());
                        if !self.eat(TokenKind::Comma) {
                            break;
                        }
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
                        if !self.eat(TokenKind::Comma) {
                            break;
                        }
                    }
                    self.expect(TokenKind::Gt);
                    Some(args)
                } else {
                    None
                };
                let end = self.current.span.start;
                Type::Named(NamedType {
                    span: Span::new(start, end),
                    name: Ident { name, span },
                    type_args,
                })
            }
            TokenKind::LParen => {
                // Function type or grouped type: (T, T) => T
                self.advance();
                if self.current.kind == TokenKind::RParen {
                    self.advance();
                    self.expect(TokenKind::FatArrow);
                    let return_type = self.parse_type();
                    let end = return_type.span().end;
                    Type::Function(FunctionType {
                        span: Span::new(start, end),
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
                    self.expect(TokenKind::RParen);
                    if self.current.kind == TokenKind::FatArrow {
                        self.advance();
                        let return_type = self.parse_type();
                        let end = return_type.span().end;
                        Type::Function(FunctionType {
                            span: Span::new(start, end),
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
            TokenKind::Null => {
                let span = self.current.span;
                self.advance();
                Type::Primitive(PrimitiveType::Null, span)
            }
            TokenKind::Void => {
                let span = self.current.span;
                self.advance();
                Type::Primitive(PrimitiveType::Void, span)
            }
            _ => {
                let span = self.current.span;
                self.advance();
                Type::Primitive(PrimitiveType::Void, span)
            }
        }
    }

    fn parse_type_params(&mut self) -> Option<Vec<TypeParam>> {
        if !self.eat(TokenKind::Lt) {
            return None;
        }
        let mut params = Vec::new();
        loop {
            if let Some(name) = self.parse_ident() {
                let constraint = if self.eat(TokenKind::Colon) {
                    Some(self.parse_type())
                } else {
                    None
                };
                params.push(TypeParam {
                    span: Span::new(name.span.start, self.current.span.end),
                    name,
                    constraint,
                });
            }
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::Gt);
        Some(params)
    }

    fn parse_modifiers(&mut self) -> Vec<Modifier> {
        let mut mods = Vec::new();
        loop {
            match self.current.kind {
                TokenKind::Public => {
                    self.advance();
                    mods.push(Modifier::Public);
                }
                TokenKind::Private => {
                    self.advance();
                    mods.push(Modifier::Private);
                }
                TokenKind::Protected => {
                    self.advance();
                    mods.push(Modifier::Protected);
                }
                TokenKind::Readonly => {
                    self.advance();
                    mods.push(Modifier::Readonly);
                }
                TokenKind::Static => {
                    self.advance();
                    mods.push(Modifier::Static);
                }
                _ => break,
            }
        }
        mods
    }

    fn parse_string_literal(&mut self) -> String {
        let text = self.current.text.clone();
        if self.current.kind == TokenKind::StringLiteral {
            self.advance();
            text[1..text.len() - 1].to_string()
        } else {
            String::new()
        }
    }
}
