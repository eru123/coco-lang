//! Item-level type checking (top-level declarations).

use coco_syntax::*;

use crate::check_expr::check_expr_against;
use crate::check_stmt::check_stmt;
use crate::convert::ast_type_to_ty;
use crate::env::{FnSig, TypeEnv, TypeKind, TypeShape};
use crate::errors::TypeckError;
use crate::infer::infer_expr;
use crate::types::Ty;
use crate::unify::is_assignable;

/// First pass: collect function signatures, top-level bindings, and named type shapes.
pub fn collect_items(items: &[Item], env: &mut TypeEnv) {
    for item in items {
        match item {
            Item::InterfaceDecl(interface_decl) => collect_interface_shape(interface_decl, env),
            Item::TraitDecl(trait_decl) => collect_trait_shape(trait_decl, env),
            Item::Export(export) => collect_items(&[(*export.item).clone()], env),
            _ => {}
        }
    }

    for item in items {
        match item {
            Item::ClassDecl(class_decl) => collect_class_shape(class_decl, env),
            Item::Export(export) => collect_items(&[(*export.item).clone()], env),
            _ => {}
        }
    }

    for item in items {
        match item {
            Item::EnumDecl(enum_decl) => {
                let variants: Vec<String> = enum_decl
                    .variants
                    .iter()
                    .map(|v| v.name.name.clone())
                    .collect();
                env.register_enum(enum_decl.name.name.clone(), variants);
            }
            Item::FnDecl(fn_decl) => {
                env.register_fn(fn_decl.name.name.clone(), fn_sig_from_fn(fn_decl));
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
            Item::ClassDecl(class_decl) => {
                check_class_decl(class_decl, env, errors);
            }
            Item::TraitDecl(trait_decl) => {
                check_trait_decl(trait_decl, env, errors);
            }
            Item::InterfaceDecl(_) => {}
            Item::Export(export) => {
                check_items(&[(*export.item).clone()], env, errors);
            }
            _ => {}
        }
    }
}

fn collect_interface_shape(interface_decl: &InterfaceDecl, env: &mut TypeEnv) {
    let mut shape = TypeShape::new(interface_decl.name.name.clone(), TypeKind::Interface);
    if let Some(ref extends) = interface_decl.extends {
        merge_named_shape(&mut shape, extends, env);
    }

    for member in &interface_decl.members {
        match member {
            InterfaceMember::PropertySignature(property) => {
                shape.properties.insert(
                    property.name.name.clone(),
                    ast_type_to_ty(&property.type_ann),
                );
            }
            InterfaceMember::MethodSignature(method) => {
                shape.methods.insert(
                    method.name.name.clone(),
                    fn_sig_from_method_signature(method),
                );
            }
        }
    }

    env.register_shape(shape);
}

fn collect_trait_shape(trait_decl: &TraitDecl, env: &mut TypeEnv) {
    let mut shape = TypeShape::new(trait_decl.name.name.clone(), TypeKind::Trait);

    for member in &trait_decl.members {
        match member {
            TraitMember::Property(property) => {
                shape.properties.insert(
                    property.name.name.clone(),
                    ast_type_to_ty(&property.type_ann),
                );
            }
            TraitMember::Method(method) => {
                shape
                    .methods
                    .insert(method.name.name.clone(), fn_sig_from_method(method));
            }
            TraitMember::MethodSignature(method) => {
                shape.methods.insert(
                    method.name.name.clone(),
                    fn_sig_from_method_signature(method),
                );
            }
        }
    }

    env.register_shape(shape);
}

fn collect_class_shape(class_decl: &ClassDecl, env: &mut TypeEnv) {
    let mut shape = TypeShape::new(class_decl.name.name.clone(), TypeKind::Class);
    if let Some(ref extends) = class_decl.extends {
        merge_named_shape(&mut shape, extends, env);
    }

    for member in &class_decl.members {
        if let ClassMember::UseTrait(use_trait) = member {
            for trait_name in &use_trait.traits {
                if let Some(trait_shape) = env.lookup_shape(&trait_name.name).cloned() {
                    merge_shape_members(&mut shape, &trait_shape);
                }
            }
        }
    }

    for member in &class_decl.members {
        match member {
            ClassMember::Constructor(constructor) => {
                shape.constructor = Some(fn_sig_from_constructor(constructor));
                for param in &constructor.params {
                    if !param.modifiers.is_empty() {
                        let ty = param
                            .type_ann
                            .as_ref()
                            .map(ast_type_to_ty)
                            .unwrap_or(Ty::Unknown);
                        shape.properties.insert(param.name.name.clone(), ty);
                    }
                }
            }
            ClassMember::Method(method) => {
                shape
                    .methods
                    .insert(method.name.name.clone(), fn_sig_from_method(method));
            }
            ClassMember::Property(property) => {
                shape.properties.insert(
                    property.name.name.clone(),
                    ast_type_to_ty(&property.type_ann),
                );
            }
            ClassMember::UseTrait(_) => {}
        }
    }

    env.register_shape(shape);
}

fn merge_named_shape(target: &mut TypeShape, ty: &Type, env: &TypeEnv) {
    if let Some(name) = named_type_name(ty) {
        if let Some(shape) = env.lookup_shape(name) {
            merge_shape_members(target, shape);
        }
    }
}

fn merge_shape_members(target: &mut TypeShape, source: &TypeShape) {
    for (name, ty) in &source.properties {
        target
            .properties
            .entry(name.clone())
            .or_insert_with(|| ty.clone());
    }
    for (name, sig) in &source.methods {
        target
            .methods
            .entry(name.clone())
            .or_insert_with(|| sig.clone());
    }
}

fn named_type_name(ty: &Type) -> Option<&str> {
    match ty {
        Type::Named(named) => Some(named.name.name.as_str()),
        Type::Primitive(_, _) => None,
        _ => None,
    }
}

fn fn_sig_from_fn(fn_decl: &FnDecl) -> FnSig {
    let ret = fn_decl
        .return_type
        .as_ref()
        .map(ast_type_to_ty)
        .unwrap_or(Ty::Unknown);
    let params = fn_decl
        .params
        .iter()
        .map(|p| {
            p.type_ann
                .as_ref()
                .map(ast_type_to_ty)
                .unwrap_or(Ty::Unknown)
        })
        .collect();
    let is_typed =
        fn_decl.return_type.is_some() || fn_decl.params.iter().any(|p| p.type_ann.is_some());
    let type_params = fn_decl
        .type_params
        .as_ref()
        .map(|tps| tps.iter().map(|tp| tp.name.name.clone()).collect())
        .unwrap_or_default();
    // Async functions return Task<T> instead of T
    let ret = if fn_decl.is_async {
        Ty::Task(Box::new(ret))
    } else {
        ret
    };
    FnSig {
        params,
        ret,
        is_typed,
        type_params,
    }
}

fn fn_sig_from_method(method: &Method) -> FnSig {
    let ret = method
        .return_type
        .as_ref()
        .map(ast_type_to_ty)
        .unwrap_or(Ty::Unknown);
    let params = method
        .params
        .iter()
        .map(|p| {
            p.type_ann
                .as_ref()
                .map(ast_type_to_ty)
                .unwrap_or(Ty::Unknown)
        })
        .collect();
    let is_typed =
        method.return_type.is_some() || method.params.iter().any(|p| p.type_ann.is_some());
    FnSig {
        params,
        ret,
        is_typed,
        type_params: vec![],
    }
}

fn fn_sig_from_method_signature(method: &MethodSignature) -> FnSig {
    FnSig {
        params: method
            .params
            .iter()
            .map(|p| {
                p.type_ann
                    .as_ref()
                    .map(ast_type_to_ty)
                    .unwrap_or(Ty::Unknown)
            })
            .collect(),
        ret: ast_type_to_ty(&method.return_type),
        is_typed: true,
        type_params: vec![],
    }
}

fn fn_sig_from_constructor(constructor: &Constructor) -> FnSig {
    FnSig {
        params: constructor
            .params
            .iter()
            .map(|p| {
                p.type_ann
                    .as_ref()
                    .map(ast_type_to_ty)
                    .unwrap_or(Ty::Unknown)
            })
            .collect(),
        ret: Ty::Void,
        is_typed: constructor.params.iter().any(|p| p.type_ann.is_some()),
        type_params: vec![],
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

fn check_class_decl(class_decl: &ClassDecl, env: &mut TypeEnv, errors: &mut Vec<TypeckError>) {
    check_interface_implementations(class_decl, env, errors);

    let self_ty = Ty::Named(class_decl.name.name.clone());
    for member in &class_decl.members {
        match member {
            ClassMember::Constructor(constructor) => {
                check_constructor(constructor, self_ty.clone(), env, errors);
            }
            ClassMember::Method(method) => {
                check_method(method, self_ty.clone(), env, errors);
            }
            ClassMember::Property(property) => {
                check_property(property, env, errors);
            }
            ClassMember::UseTrait(_) => {}
        }
    }
}

fn check_trait_decl(trait_decl: &TraitDecl, env: &mut TypeEnv, errors: &mut Vec<TypeckError>) {
    let self_ty = Ty::Named(trait_decl.name.name.clone());
    for member in &trait_decl.members {
        match member {
            TraitMember::Method(method) => check_method(method, self_ty.clone(), env, errors),
            TraitMember::Property(property) => check_property(property, env, errors),
            TraitMember::MethodSignature(_) => {}
        }
    }
}

fn check_property(property: &Property, env: &mut TypeEnv, errors: &mut Vec<TypeckError>) {
    if let Some(ref default_value) = property.default_value {
        let property_ty = ast_type_to_ty(&property.type_ann);
        check_expr_against(&property_ty, default_value, env, errors);
    }
}

fn check_constructor(
    constructor: &Constructor,
    self_ty: Ty,
    env: &mut TypeEnv,
    errors: &mut Vec<TypeckError>,
) {
    env.push_self(self_ty);
    env.push_scope();

    for param in &constructor.params {
        let param_ty = param
            .type_ann
            .as_ref()
            .map(ast_type_to_ty)
            .unwrap_or(Ty::Mixed);
        if let Some(ref default_value) = param.default_value {
            check_expr_against(&param_ty, default_value, env, errors);
        }
        env.define(param.name.name.clone(), param_ty);
    }

    for stmt in &constructor.body.stmts {
        check_stmt(stmt, env, &Some(Ty::Void), errors);
    }

    env.pop_scope();
    env.pop_self();
}

fn check_method(method: &Method, self_ty: Ty, env: &mut TypeEnv, errors: &mut Vec<TypeckError>) {
    let ret_ty = method.return_type.as_ref().map(ast_type_to_ty);

    env.push_self(self_ty);
    env.push_scope();

    for param in &method.params {
        let param_ty = param
            .type_ann
            .as_ref()
            .map(ast_type_to_ty)
            .unwrap_or(Ty::Mixed);
        if let Some(ref default_value) = param.default_value {
            check_expr_against(&param_ty, default_value, env, errors);
        }
        env.define(param.name.name.clone(), param_ty);
    }

    for stmt in &method.body.stmts {
        check_stmt(stmt, env, &ret_ty, errors);
    }

    env.pop_scope();
    env.pop_self();
}

fn check_interface_implementations(
    class_decl: &ClassDecl,
    env: &TypeEnv,
    errors: &mut Vec<TypeckError>,
) {
    let Some(class_shape) = env.lookup_shape(&class_decl.name.name) else {
        return;
    };

    for implemented in &class_decl.implements {
        let Some(interface_name) = named_type_name(implemented) else {
            continue;
        };
        let Some(interface_shape) = env.lookup_shape(interface_name) else {
            continue;
        };

        for (name, interface_ty) in &interface_shape.properties {
            let Some(class_ty) = class_shape.properties.get(name) else {
                errors.push(TypeckError::property_not_found(
                    &class_decl.name.name,
                    name,
                    implemented.span(),
                ));
                continue;
            };
            if !is_assignable(interface_ty, class_ty) {
                errors.push(TypeckError::type_mismatch(
                    &interface_ty.to_string(),
                    &class_ty.to_string(),
                    implemented.span(),
                ));
            }
        }

        for (name, interface_sig) in &interface_shape.methods {
            let Some(class_sig) = class_shape.methods.get(name) else {
                errors.push(TypeckError::property_not_found(
                    &class_decl.name.name,
                    name,
                    implemented.span(),
                ));
                continue;
            };
            if class_sig.params.len() != interface_sig.params.len() {
                errors.push(TypeckError::arg_count(
                    interface_sig.params.len(),
                    class_sig.params.len(),
                    implemented.span(),
                ));
                continue;
            }
            for (interface_param, class_param) in
                interface_sig.params.iter().zip(class_sig.params.iter())
            {
                if !is_assignable(class_param, interface_param) {
                    errors.push(TypeckError::type_mismatch(
                        &interface_param.to_string(),
                        &class_param.to_string(),
                        implemented.span(),
                    ));
                }
            }
            if !is_assignable(&interface_sig.ret, &class_sig.ret) {
                errors.push(TypeckError::type_mismatch(
                    &interface_sig.ret.to_string(),
                    &class_sig.ret.to_string(),
                    implemented.span(),
                ));
            }
        }
    }
}
