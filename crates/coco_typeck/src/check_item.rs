//! Item-level type checking (top-level declarations).

use coco_syntax::*;

use crate::check_expr::check_expr_against;
use crate::check_stmt::check_stmt;
use crate::convert::ast_type_to_ty;
use crate::env::{FnSig, TypeEnv};
use crate::errors::TypeckError;
use crate::infer::infer_expr;
use crate::types::Ty;

/// First pass: collect function signatures from top-level items.
pub fn collect_items(items: &[Item], env: &mut TypeEnv) {
    for item in items {
        match item {
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
                    FnSig {
                        params,
                        ret: ret_ty,
                        is_typed: has_any_annotation,
                    },
                );
            }
            Item::ConstDecl(const_decl) => {
                let ty = const_decl
                    .type_ann
                    .as_ref()
                    .map(ast_type_to_ty)
                    .unwrap_or_else(|| infer_expr(&const_decl.value, env));
                env.define(const_decl.name.name.clone(), ty);
            }
            Item::LetDecl(let_decl) => {
                let ty = if let Some(ref ann) = let_decl.type_ann {
                    ast_type_to_ty(ann)
                } else if let Some(ref value) = let_decl.value {
                    infer_expr(value, env)
                } else {
                    Ty::Unknown
                };
                env.define(let_decl.name.name.clone(), ty);
            }
            Item::Export(export) => {
                // Recurse into the exported item
                collect_items(&[(*export.item).clone()], env);
            }
            _ => {}
        }
    }
}

/// Second pass: check function bodies and other items.
pub fn check_items(items: &[Item], env: &mut TypeEnv, errors: &mut Vec<TypeckError>) {
    for item in items {
        match item {
            Item::FnDecl(fn_decl) => {
                check_fn_decl(fn_decl, env, errors);
            }
            Item::LetDecl(let_decl) => {
                check_let_decl(let_decl, env, errors);
            }
            Item::ConstDecl(const_decl) => {
                check_const_decl(const_decl, env, errors);
            }
            Item::Export(export) => {
                check_items(&[(*export.item).clone()], env, errors);
            }
            // Classes, interfaces, traits, enums: skip for now
            _ => {}
        }
    }
}

fn check_fn_decl(fn_decl: &FnDecl, env: &mut TypeEnv, errors: &mut Vec<TypeckError>) {
    let ret_ty = fn_decl.return_type.as_ref().map(ast_type_to_ty);

    // Push scope for function body
    env.push_scope();

    // Define parameters in scope
    for param in &fn_decl.params {
        let param_ty = param
            .type_ann
            .as_ref()
            .map(ast_type_to_ty)
            .unwrap_or(Ty::Mixed);
        env.define(param.name.name.clone(), param_ty);
    }

    // Check the body statements
    for stmt in &fn_decl.body.stmts {
        check_stmt(stmt, env, &ret_ty, errors);
    }

    env.pop_scope();
}

fn check_let_decl(let_decl: &LetDecl, env: &mut TypeEnv, errors: &mut Vec<TypeckError>) {
    // Only check if there's a type annotation
    if let Some(ref type_ann) = let_decl.type_ann {
        let declared_ty = ast_type_to_ty(type_ann);
        if let Some(ref value) = let_decl.value {
            check_expr_against(&declared_ty, value, env, errors);
        }
    }
}

fn check_const_decl(const_decl: &ConstDecl, env: &mut TypeEnv, errors: &mut Vec<TypeckError>) {
    if let Some(ref type_ann) = const_decl.type_ann {
        let declared_ty = ast_type_to_ty(type_ann);
        check_expr_against(&declared_ty, &const_decl.value, env, errors);
    }
}
