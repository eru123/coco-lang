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
        other => panic!("expected FnDecl, got {:?}", other),
    }
}

#[test]
fn parse_async_fn() {
    let program = parse("async fn fetch(): string { return x; }");
    match &program.items[0] {
        Item::FnDecl(f) => {
            assert!(f.is_async);
            assert_eq!(f.name.name, "fetch");
            assert!(f.return_type.is_some());
        }
        other => panic!("expected async FnDecl, got {:?}", other),
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
        other => panic!("expected LetDecl, got {:?}", other),
    }
}

#[test]
fn parse_null_type_in_union() {
    let program = parse("let x: int|null = null;");
    match &program.items[0] {
        Item::LetDecl(l) => {
            let Some(Type::Union(union)) = &l.type_ann else {
                panic!("expected union type, got {:?}", l.type_ann);
            };
            assert!(matches!(
                &union.types[1],
                Type::Primitive(PrimitiveType::Null, _)
            ));
        }
        other => panic!("expected LetDecl, got {:?}", other),
    }
}

#[test]
fn parse_const_decl() {
    let program = parse("const PI: float = 3.14;");
    match &program.items[0] {
        Item::ConstDecl(c) => {
            assert_eq!(c.name.name, "PI");
            assert!(c.type_ann.is_some());
        }
        other => panic!("expected ConstDecl, got {:?}", other),
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
        other => panic!("expected ClassDecl, got {:?}", other),
    }
}

#[test]
fn parse_enum_decl() {
    let program = parse("enum Color { Red, Green, Blue }");
    match &program.items[0] {
        Item::EnumDecl(e) => {
            assert_eq!(e.name.name, "Color");
            assert_eq!(e.variants.len(), 3);
            assert_eq!(e.variants[0].name.name, "Red");
            assert_eq!(e.variants[1].name.name, "Green");
            assert_eq!(e.variants[2].name.name, "Blue");
        }
        other => panic!("expected EnumDecl, got {:?}", other),
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
        other => panic!("expected InterfaceDecl, got {:?}", other),
    }
}

#[test]
fn parse_import_named() {
    let program = parse("import { foo, bar } from \"module\";");
    match &program.items[0] {
        Item::Import(i) => {
            match &i.items {
                ImportItems::Named(names) => {
                    assert_eq!(names.len(), 2);
                    assert_eq!(names[0].name, "foo");
                    assert_eq!(names[1].name, "bar");
                }
                other => panic!("expected Named import items, got {:?}", other),
            }
            assert_eq!(i.source, "module");
        }
        other => panic!("expected Import, got {:?}", other),
    }
}

#[test]
fn parse_if_else() {
    let program = parse("fn test() { if x { return 1; } else { return 2; } }");
    match &program.items[0] {
        Item::FnDecl(f) => {
            assert_eq!(f.name.name, "test");
            // The body should contain an If statement
            let stmt = &f.body.stmts[0];
            match stmt {
                Stmt::If(if_stmt) => {
                    assert!(if_stmt.else_block.is_some());
                }
                other => panic!("expected If statement, got {:?}", other),
            }
        }
        other => panic!("expected FnDecl, got {:?}", other),
    }
}

#[test]
fn parse_for_loop() {
    let program = parse("fn test() { for item in items { x; } }");
    match &program.items[0] {
        Item::FnDecl(f) => {
            let stmt = &f.body.stmts[0];
            match stmt {
                Stmt::For(for_stmt) => {
                    assert_eq!(for_stmt.pattern.name, "item");
                }
                other => panic!("expected For statement, got {:?}", other),
            }
        }
        other => panic!("expected FnDecl, got {:?}", other),
    }
}

#[test]
fn parse_return_expr() {
    let program = parse("fn test() { return 1 + 2; }");
    match &program.items[0] {
        Item::FnDecl(f) => {
            let stmt = &f.body.stmts[0];
            match stmt {
                Stmt::Return(ret) => {
                    assert!(ret.value.is_some());
                    match ret.value.as_ref().unwrap() {
                        Expr::Binary(b) => {
                            assert_eq!(b.op, BinaryOp::Add);
                        }
                        other => panic!("expected Binary expr in return, got {:?}", other),
                    }
                }
                other => panic!("expected Return statement, got {:?}", other),
            }
        }
        other => panic!("expected FnDecl, got {:?}", other),
    }
}
