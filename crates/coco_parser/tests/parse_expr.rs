use coco_parser::Parser;
use coco_syntax::*;

fn parse_expr_stmt(src: &str) -> Expr {
    let mut parser = Parser::new(src);
    let program = parser.parse_program();
    assert!(
        parser.diagnostics().is_empty(),
        "unexpected diagnostics: {:?}",
        parser.diagnostics()
    );
    match &program.items[0] {
        Item::Stmt(Stmt::Expr(es)) => es.expr.clone(),
        other => panic!("expected expression statement, got {:?}", other),
    }
}

fn parse_const_init(src: &str) -> Expr {
    let mut parser = Parser::new(src);
    let program = parser.parse_program();
    assert!(
        parser.diagnostics().is_empty(),
        "unexpected diagnostics: {:?}",
        parser.diagnostics()
    );
    match &program.items[0] {
        Item::ConstDecl(c) => c.value.clone(),
        other => panic!("expected const declaration, got {:?}", other),
    }
}

#[test]
fn parse_integer_literal() {
    let expr = parse_expr_stmt("42;");
    match expr {
        Expr::Literal(Literal::Int(s, _)) => assert_eq!(s, "42"),
        other => panic!("expected int literal, got {:?}", other),
    }
}

#[test]
fn parse_float_literal() {
    let expr = parse_expr_stmt("3.14;");
    match expr {
        Expr::Literal(Literal::Float(v, _)) => {
            assert!((v - 3.14).abs() < f64::EPSILON);
        }
        other => panic!("expected float literal, got {:?}", other),
    }
}

#[test]
fn parse_string_literal() {
    let expr = parse_expr_stmt("\"hello\";");
    match expr {
        Expr::Literal(Literal::String(s, _)) => assert_eq!(s, "hello"),
        other => panic!("expected string literal, got {:?}", other),
    }
}

#[test]
fn parse_bool_true() {
    let expr = parse_expr_stmt("true;");
    assert!(matches!(expr, Expr::Literal(Literal::Bool(true, _))));
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
        Expr::Binary(ref b) => {
            assert_eq!(b.op, BinaryOp::Add);
                    match &b.left {
                Expr::Literal(Literal::Int(s, _)) => assert_eq!(s, "1"),
                other => panic!("expected int literal, got {:?}", other),
            }
            match &b.right {
                Expr::Literal(Literal::Int(s, _)) => assert_eq!(s, "2"),
                other => panic!("expected int literal, got {:?}", other),
            }
        }
        other => panic!("expected binary add, got {:?}", other),
    }
}

#[test]
fn parse_binary_precedence() {
    // 1 + 2 * 3 should parse as Add(1, Mul(2, 3))
    let expr = parse_expr_stmt("1 + 2 * 3;");
    match expr {
        Expr::Binary(ref b) => {
            assert_eq!(b.op, BinaryOp::Add);
            match &b.left {
                Expr::Literal(Literal::Int(s, _)) => assert_eq!(s, "1"),
                other => panic!("expected int literal, got {:?}", other),
            }
            match &b.right {
                Expr::Binary(inner) => {
                    assert_eq!(inner.op, BinaryOp::Mul);
                    match &inner.left {
                        Expr::Literal(Literal::Int(s, _)) => assert_eq!(s, "2"),
                        other => panic!("expected int literal, got {:?}", other),
                    }
                    match &inner.right {
                        Expr::Literal(Literal::Int(s, _)) => assert_eq!(s, "3"),
                        other => panic!("expected int literal, got {:?}", other),
                    }
                }
                other => panic!("expected binary mul on right, got {:?}", other),
            }
        }
        other => panic!("expected binary add, got {:?}", other),
    }
}

#[test]
fn parse_unary_neg() {
    let expr = parse_expr_stmt("-x;");
    match expr {
        Expr::Unary(ref u) => {
            assert_eq!(u.op, UnaryOp::Neg);
            match &u.expr {
                Expr::Ident(i) => assert_eq!(i.name, "x"),
                other => panic!("expected ident x, got {:?}", other),
            }
        }
        other => panic!("expected unary neg, got {:?}", other),
    }
}

#[test]
fn parse_member_access() {
    let expr = parse_expr_stmt("foo.bar;");
    match expr {
        Expr::Member(ref m) => {
            assert_eq!(m.property.name, "bar");
            assert!(!m.optional);
            match &m.object {
                Expr::Ident(i) => assert_eq!(i.name, "foo"),
                other => panic!("expected ident foo, got {:?}", other),
            }
        }
        other => panic!("expected member access, got {:?}", other),
    }
}

#[test]
fn parse_optional_chain() {
    let expr = parse_expr_stmt("foo?.bar;");
    match expr {
        Expr::Member(ref m) => {
            assert_eq!(m.property.name, "bar");
            assert!(m.optional);
        }
        other => panic!("expected optional member access, got {:?}", other),
    }
}

#[test]
fn parse_function_call() {
    let expr = parse_expr_stmt("foo(1, 2);");
    match expr {
        Expr::Call(ref c) => {
            assert_eq!(c.args.len(), 2);
            match &c.callee {
                Expr::Ident(i) => assert_eq!(i.name, "foo"),
                other => panic!("expected ident foo, got {:?}", other),
            }
        }
        other => panic!("expected call, got {:?}", other),
    }
}

#[test]
fn parse_method_chain() {
    // a.b().c should parse as Member(Call(Member(a, b), []), c)
    let expr = parse_expr_stmt("a.b().c;");
    match expr {
        Expr::Member(ref m) => {
            assert_eq!(m.property.name, "c");
            assert!(!m.optional);
            // The object should be a Call expression
            match &m.object {
                Expr::Call(c) => {
                    // The callee should be Member(a, b)
                    match &c.callee {
                        Expr::Member(inner) => {
                            assert_eq!(inner.property.name, "b");
                            match &inner.object {
                                Expr::Ident(i) => assert_eq!(i.name, "a"),
                                other => panic!("expected ident a, got {:?}", other),
                            }
                        }
                        other => panic!("expected member a.b, got {:?}", other),
                    }
                }
                other => panic!("expected call, got {:?}", other),
            }
        }
        other => panic!("expected member access .c, got {:?}", other),
    }
}

#[test]
fn parse_array_literal() {
    let expr = parse_expr_stmt("[1, 2, 3];");
    match expr {
        Expr::Array(ref a) => {
            assert_eq!(a.elements.len(), 3);
        }
        other => panic!("expected array literal, got {:?}", other),
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
        Expr::Binary(ref b) => {
            assert_eq!(b.op, BinaryOp::Range);
        }
        other => panic!("expected binary range, got {:?}", other),
    }
}

#[test]
fn parse_index_access() {
    let expr = parse_expr_stmt("arr[0];");
    assert!(matches!(expr, Expr::Index(_)));
}

#[test]
fn parse_grouped_expr() {
    // (1 + 2) * 3 should parse as Mul(Group(Add(1, 2)), 3)
    let expr = parse_expr_stmt("(1 + 2) * 3;");
    match expr {
        Expr::Binary(ref b) => {
            assert_eq!(b.op, BinaryOp::Mul);
            assert!(matches!(b.left, Expr::Group(_)));
            match &b.right {
                Expr::Literal(Literal::Int(s, _)) => assert_eq!(s, "3"),
                other => panic!("expected int literal, got {:?}", other),
            }
        }
        other => panic!("expected binary mul with group, got {:?}", other),
    }
}

#[test]
fn parse_empty_param_arrow_expr_body() {
    let expr = parse_const_init("const x = () => 1;");
    match expr {
        Expr::Lambda(lambda) => {
            assert!(lambda.params.is_empty());
            assert!(lambda.return_type.is_none());
            assert!(matches!(lambda.body, LambdaBody::Expr(_)));
        }
        other => panic!("expected lambda expression, got {:?}", other),
    }
}

#[test]
fn parse_typed_empty_param_arrow_expr_body() {
    let expr = parse_const_init("const x = (): int => 1;");
    match expr {
        Expr::Lambda(lambda) => {
            assert!(lambda.params.is_empty());
            assert!(lambda.return_type.is_some());
            assert!(matches!(lambda.body, LambdaBody::Expr(_)));
        }
        other => panic!("expected lambda expression, got {:?}", other),
    }
}

#[test]
fn no_panic_on_unexpected_token() {
    // Parser should not panic on invalid input
    let mut parser = Parser::new("@@@;");
    let program = parser.parse_program();
    // Just check it doesn't panic — we don't care about the exact output
    let _ = program.items;
}
