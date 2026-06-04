//! Statement-level type checking.

use coco_syntax::*;

use crate::check_expr::{check_expr, check_expr_against};
use crate::convert::ast_type_to_ty;
use crate::env::TypeEnv;
use crate::errors::TypeckError;
use crate::infer::infer_expr;
use crate::types::Ty;
use crate::unify::is_assignable;

type Narrowings = Vec<(String, Ty)>;

/// Check a block of statements.
/// `expected_return` is the declared return type of the enclosing function (if any).
pub fn check_block(
    block: &Block,
    env: &mut TypeEnv,
    expected_return: &Option<Ty>,
    errors: &mut Vec<TypeckError>,
) {
    check_block_with_bindings(block, env, expected_return, errors, &[]);
}

fn check_block_with_bindings(
    block: &Block,
    env: &mut TypeEnv,
    expected_return: &Option<Ty>,
    errors: &mut Vec<TypeckError>,
    bindings: &[(String, Ty)],
) {
    env.push_scope();
    for (name, ty) in bindings {
        env.define(name.clone(), ty.clone());
    }
    for stmt in &block.stmts {
        check_stmt(stmt, env, expected_return, errors);
    }
    env.pop_scope();
}

/// Check a single statement.
pub fn check_stmt(
    stmt: &Stmt,
    env: &mut TypeEnv,
    expected_return: &Option<Ty>,
    errors: &mut Vec<TypeckError>,
) {
    match stmt {
        Stmt::Item(item) => check_item_stmt(item, env, errors),
        Stmt::Expr(expr_stmt) => {
            check_expr(&expr_stmt.expr, env, errors);
        }
        Stmt::If(if_stmt) => check_if(if_stmt, env, expected_return, errors),
        Stmt::For(for_stmt) => check_for(for_stmt, env, expected_return, errors),
        Stmt::While(while_stmt) => check_while(while_stmt, env, expected_return, errors),
        Stmt::DoWhile(do_while) => check_do_while(do_while, env, expected_return, errors),
        Stmt::Loop(loop_stmt) => check_loop(loop_stmt, env, expected_return, errors),
        Stmt::Return(ret) => check_return(ret, env, expected_return, errors),
        Stmt::Throw(throw) => {
            check_expr(&throw.value, env, errors);
        }
        Stmt::Try(try_stmt) => check_try(try_stmt, env, expected_return, errors),
        Stmt::Break(_) | Stmt::Continue(_) => {}
        Stmt::Parallel(_) | Stmt::Coro(_) | Stmt::Select(_) => {}
        Stmt::Unsafe(unsafe_stmt) => {
            check_block(&unsafe_stmt.body, env, expected_return, errors);
        }
        Stmt::Synchronized(sync_stmt) => {
            check_block(&sync_stmt.body, env, expected_return, errors);
        }
    }
}

fn check_item_stmt(item: &Item, env: &mut TypeEnv, errors: &mut Vec<TypeckError>) {
    match item {
        Item::LetDecl(let_decl) => {
            let declared_ty = let_decl.type_ann.as_ref().map(ast_type_to_ty);
            let value_ty = let_decl
                .value
                .as_ref()
                .map(|v| infer_expr(v, env))
                .unwrap_or(Ty::Unknown);

            if let Some(ref declared) = declared_ty {
                if let Some(ref value) = let_decl.value {
                    check_expr_against(declared, value, env, errors);
                }
                env.define(let_decl.name.name.clone(), declared.clone());
            } else {
                // No annotation: infer from value
                env.define(let_decl.name.name.clone(), value_ty);
            }
        }
        Item::ConstDecl(const_decl) => {
            let declared_ty = const_decl.type_ann.as_ref().map(ast_type_to_ty);
            let value_ty = infer_expr(&const_decl.value, env);

            if let Some(ref declared) = declared_ty {
                check_expr_against(declared, &const_decl.value, env, errors);
                env.define(const_decl.name.name.clone(), declared.clone());
            } else {
                env.define(const_decl.name.name.clone(), value_ty);
            }
        }
        // Nested functions inside blocks — just register them
        Item::FnDecl(fn_decl) => {
            let ret_ty = fn_decl
                .return_type
                .as_ref()
                .map(ast_type_to_ty)
                .unwrap_or(Ty::Unknown);
            let params: Vec<Ty> = fn_decl
                .params
                .iter()
                .map(|p| {
                    p.type_ann
                        .as_ref()
                        .map(ast_type_to_ty)
                        .unwrap_or(Ty::Unknown)
                })
                .collect();
            let has_any_annotation = fn_decl.return_type.is_some()
                || fn_decl.params.iter().any(|p| p.type_ann.is_some());
            env.register_fn(
                fn_decl.name.name.clone(),
                crate::env::FnSig {
                    params,
                    ret: ret_ty,
                    is_typed: has_any_annotation,
                },
            );
        }
        _ => {}
    }
}

fn check_if(
    if_stmt: &IfStmt,
    env: &mut TypeEnv,
    expected_return: &Option<Ty>,
    errors: &mut Vec<TypeckError>,
) {
    check_expr(&if_stmt.condition, env, errors);
    let (then_bindings, else_bindings) = null_narrowings(&if_stmt.condition, env);
    check_block_with_bindings(
        &if_stmt.then_block,
        env,
        expected_return,
        errors,
        &then_bindings,
    );
    for else_if in &if_stmt.else_ifs {
        check_expr(&else_if.condition, env, errors);
        check_block(&else_if.block, env, expected_return, errors);
    }
    if let Some(ref else_block) = if_stmt.else_block {
        check_block_with_bindings(else_block, env, expected_return, errors, &else_bindings);
    }
}

fn null_narrowings(condition: &Expr, env: &TypeEnv) -> (Narrowings, Narrowings) {
    let Expr::Binary(bin) = condition else {
        return (Vec::new(), Vec::new());
    };

    let Some(name) = null_checked_ident(&bin.left, &bin.right) else {
        return (Vec::new(), Vec::new());
    };

    let Some(current_ty) = env.lookup(name) else {
        return (Vec::new(), Vec::new());
    };
    if !current_ty.is_nullable() {
        return (Vec::new(), Vec::new());
    }

    let narrowed = current_ty.strip_null();
    if narrowed.is_unknown() || matches!(narrowed, Ty::Never) {
        return (Vec::new(), Vec::new());
    }

    match bin.op {
        BinaryOp::Ne => (vec![(name.to_string(), narrowed)], Vec::new()),
        BinaryOp::Eq => (Vec::new(), vec![(name.to_string(), narrowed)]),
        _ => (Vec::new(), Vec::new()),
    }
}

fn null_checked_ident<'a>(left: &'a Expr, right: &'a Expr) -> Option<&'a str> {
    match (left, right) {
        (Expr::Ident(ident), Expr::Literal(Literal::Null(_))) => Some(ident.name.as_str()),
        (Expr::Literal(Literal::Null(_)), Expr::Ident(ident)) => Some(ident.name.as_str()),
        _ => None,
    }
}

fn check_for(
    for_stmt: &ForStmt,
    env: &mut TypeEnv,
    expected_return: &Option<Ty>,
    errors: &mut Vec<TypeckError>,
) {
    check_expr(&for_stmt.iterable, env, errors);
    env.push_scope();
    env.define(for_stmt.pattern.name.clone(), Ty::Unknown);
    for stmt in &for_stmt.body.stmts {
        check_stmt(stmt, env, expected_return, errors);
    }
    env.pop_scope();
}

fn check_while(
    while_stmt: &WhileStmt,
    env: &mut TypeEnv,
    expected_return: &Option<Ty>,
    errors: &mut Vec<TypeckError>,
) {
    check_expr(&while_stmt.condition, env, errors);
    check_block(&while_stmt.body, env, expected_return, errors);
}

fn check_do_while(
    do_while: &DoWhileStmt,
    env: &mut TypeEnv,
    expected_return: &Option<Ty>,
    errors: &mut Vec<TypeckError>,
) {
    check_block(&do_while.body, env, expected_return, errors);
    check_expr(&do_while.condition, env, errors);
}

fn check_loop(
    loop_stmt: &LoopStmt,
    env: &mut TypeEnv,
    expected_return: &Option<Ty>,
    errors: &mut Vec<TypeckError>,
) {
    check_block(&loop_stmt.body, env, expected_return, errors);
}

fn check_return(
    ret: &ReturnStmt,
    env: &mut TypeEnv,
    expected_return: &Option<Ty>,
    errors: &mut Vec<TypeckError>,
) {
    if let Some(ref expected) = expected_return {
        if let Some(ref value) = ret.value {
            let value_ty = check_expr(value, env, errors);
            if !is_assignable(expected, &value_ty) {
                errors.push(TypeckError::type_mismatch(
                    &expected.to_string(),
                    &value_ty.to_string(),
                    ret.span,
                ));
            }
        } else if !matches!(expected, Ty::Void) && !expected.is_unknown() {
            errors.push(TypeckError::missing_return(ret.span));
        }
    }
}

fn check_try(
    try_stmt: &TryStmt,
    env: &mut TypeEnv,
    expected_return: &Option<Ty>,
    errors: &mut Vec<TypeckError>,
) {
    check_block(&try_stmt.body, env, expected_return, errors);
    for catch in &try_stmt.catches {
        env.push_scope();
        let catch_ty = catch
            .type_ann
            .as_ref()
            .map(ast_type_to_ty)
            .unwrap_or(Ty::Unknown);
        env.define(catch.param.name.clone(), catch_ty);
        check_block(&catch.body, env, expected_return, errors);
        env.pop_scope();
    }
    if let Some(ref finally) = try_stmt.finally {
        check_block(finally, env, expected_return, errors);
    }
}
