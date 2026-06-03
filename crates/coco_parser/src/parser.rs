//! Recursive descent parser for Coco declarations and statements.

use coco_lexer::{Lexer, Token, TokenKind};
use coco_span::Span;
use coco_syntax::*;

use crate::expr::ExprParser;

pub struct Parser<'a> {
    lexer: Lexer<'a>,
    current: Token,
    diagnostics: Vec<String>,
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

    pub fn diagnostics(&self) -> &[String] {
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
        self.diagnostics.push(msg.to_string());
        self.synchronize();
    }

    fn synchronize(&mut self) {
        while !self.current.kind.is_sync_point()
            && self.current.kind != TokenKind::Eof
        {
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
            TokenKind::Constructor => {
                Some(ClassMember::Constructor(self.parse_constructor()?))
            }
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
            TokenKind::Async => {
                self.parse_method(modifiers).map(ClassMember::Method)
            }
            TokenKind::Public | TokenKind::Private | TokenKind::Protected
            | TokenKind::Readonly | TokenKind::Static => {
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

    fn parse_method_or_property(
        &mut self,
        modifiers: Vec<Modifier>,
    ) -> Option<ClassMember> {
        // We need to look ahead to determine if this is a method or property.
        // Method: ident(...), Property: ident: Type
        // Since we can't peek easily, save name and check next token
        let name = self.parse_ident()?;
        if self.current.kind == TokenKind::LParen || self.current.kind == TokenKind::Lt {
            // Method with type params or direct parens
            Some(ClassMember::Method(self.finish_parse_method(
                modifiers, name,
            )?))
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

    fn finish_parse_method(
        &mut self,
        modifiers: Vec<Modifier>,
        name: Ident,
    ) -> Option<Method> {
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
        let modifiers = self.parse_modifiers();
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
                Some(Stmt::Coro(CoroStmt { span, body }))
            }
            TokenKind::Select => {
                self.advance();
                self.expect(TokenKind::LBrace);
                let mut cases = Vec::new();
                while self.current.kind != TokenKind::RBrace && self.current.kind != TokenKind::Eof {
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
            TokenKind::LBrace => {
                let block = self.parse_block()?;
                Some(Stmt::Expr(ExprStmt {
                    span: block.span,
                    expr: Expr::Literal(Literal::Null(block.span)),
                }))
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
        let start = self.current.span.start;
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
            if let Some(stmt) = self.parse_stmt() {
                stmts.push(stmt);
            } else if self.current.kind != TokenKind::RBrace {
                // Try as declaration
                if let Some(item) = self.parse_item() {
                    stmts.push(Stmt::Expr(ExprStmt {
                        span: Span::new(self.current.span.start, self.current.span.end),
                        expr: Expr::Literal(Literal::Null(Span::new(0, 0))),
                    }));
                } else {
                    self.advance();
                }
            }
        }
        let end = self.current.span.end;
        self.expect(TokenKind::RBrace);
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

    fn parse_expr(&mut self) -> Expr {
        // Temporarily create expr parser with owned values
        // We'll use the current state directly
        self.parse_expr_pratt(0)
    }

    fn parse_expr_pratt(&mut self, min_bp: u8) -> Expr {
        // Simplified expression parsing - delegate to actual implementation
        // For now, parse a simple identifier or literal
        match self.current.kind {
            TokenKind::Ident => {
                let ident = self.parse_ident().unwrap();
                Expr::Ident(ident)
            }
            TokenKind::IntLiteral => {
                let span = self.current.span;
                let text = self.current.text.clone();
                self.advance();
                let value = text.parse().unwrap_or(0);
                Expr::Literal(Literal::Int(value, span))
            }
            _ => {
                // Placeholder - return a dummy expression
                Expr::Literal(Literal::Null(self.current.span))
            }
        }
    }

    fn parse_type(&mut self) -> Type {
        // Simplified type parsing
        match self.current.kind {
            TokenKind::Ident => {
                let name = self.parse_ident().unwrap();
                Type::Named(NamedType {
                    span: name.span,
                    name,
                    type_args: None,
                })
            }
            _ => {
                Type::Primitive(PrimitiveType::Mixed, self.current.span)
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
