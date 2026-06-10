//! Expression-level type checking.

use coco_syntax::*;

use crate::env::TypeEnv;
use crate::errors::TypeckError;
use crate::infer::infer_expr;
use crate::types::Ty;
use crate::unify::is_assignable;

/// Check an expression and return its inferred type, collecting errors.
pub fn check_expr(expr: &Expr, env: &mut TypeEnv, errors: &mut Vec<TypeckError>) -> Ty {
    match expr {
        Expr::Binary(bin) => check_binary(bin, env, errors),
        Expr::Call(call) => check_call(call, env, errors),
        Expr::Array(arr) => check_array(arr, env, errors),
        Expr::Object(object) => check_object(object, env, errors),
        Expr::Member(member) => check_member(member, env, errors),
        Expr::Index(index) => check_index(index, env, errors),
        Expr::New(new_expr) => check_new(new_expr, env, errors),
        Expr::Group(inner) => check_expr(inner, env, errors),
        Expr::Match(m) => check_match(m, env, errors),
        Expr::Postfix(pf) => check_postfix(pf, env, errors),
        Expr::NullCoalesce(nc) => {
            check_expr(&nc.left, env, errors);
            check_expr(&nc.right, env, errors);
            infer_expr(expr, env)
        }
        Expr::Ternary(ternary) => {
            check_expr(&ternary.condition, env, errors);
            check_expr(&ternary.then_expr, env, errors);
            check_expr(&ternary.else_expr, env, errors);
            infer_expr(expr, env)
        }
        Expr::Unary(unary) => {
            check_expr(&unary.expr, env, errors);
            infer_expr(expr, env)
        }
        _ => infer_expr(expr, env),
    }
}

/// Type-check a match expression: check scrutinee, unify arm return types.
fn check_match(m: &MatchExpr, env: &mut TypeEnv, errors: &mut Vec<TypeckError>) -> Ty {
    check_expr(&m.scrutinee, env, errors);
    let mut arm_types: Vec<Ty> = Vec::new();
    for arm in &m.arms {
        check_expr(&arm.body, env, errors);
        let arm_ty = infer_expr(&arm.body, env);
        arm_types.push(arm_ty);
    }
    // Unify arm types: if all same, return that; otherwise union
    if arm_types.is_empty() {
        Ty::Never
    } else if arm_types.len() == 1 {
        arm_types[0].clone()
    } else {
        let mut common = arm_types[0].clone();
        for ty in &arm_types[1..] {
            if !is_assignable(&common, ty) && !is_assignable(ty, &common) {
                common = Ty::Unknown;
                break;
            }
        }
        common
    }
}

/// Type-check postfix expressions. Handles `expr?` (Result unwrap).
fn check_postfix(pf: &PostfixExpr, env: &mut TypeEnv, errors: &mut Vec<TypeckError>) -> Ty {
    let obj_ty = check_expr(&pf.object, env, errors);
    match &pf.op {
        PostfixOp::Question => {
            // expr? — unwrap Result<T, E> to T
            match &obj_ty {
                Ty::Result(ok, _err) => *ok.clone(),
                Ty::Unknown | Ty::Mixed => Ty::Unknown,
                _ => {
                    errors.push(TypeckError::type_mismatch(
                        "Result<_, _>",
                        &obj_ty.to_string(),
                        pf.span,
                    ));
                    Ty::Unknown
                }
            }
        }
        PostfixOp::Bang => obj_ty, // unwrap null — just passthrough
        _ => Ty::Unknown,
    }
}

/// Check an expression against an expected type and report assignment errors.
pub fn check_expr_against(
    expected: &Ty,
    expr: &Expr,
    env: &mut TypeEnv,
    errors: &mut Vec<TypeckError>,
) -> Ty {
    if let (Ty::List(expected_elem), Expr::Array(array)) = (expected, expr) {
        for element in &array.elements {
            let element_ty = check_expr(element, env, errors);
            if !is_assignable(expected_elem, &element_ty) {
                errors.push(TypeckError::type_mismatch(
                    &expected_elem.to_string(),
                    &element_ty.to_string(),
                    element.span(),
                ));
            }
        }
        return infer_expr(expr, env);
    }

    if let (Ty::Map(expected_key, expected_value), Expr::Object(object)) = (expected, expr) {
        for field in &object.fields {
            let key_ty = object_key_type(&field.key);
            if !is_assignable(expected_key, &key_ty) {
                errors.push(TypeckError::type_mismatch(
                    &expected_key.to_string(),
                    &key_ty.to_string(),
                    object_key_span(&field.key),
                ));
            }
            check_expr_against(expected_value, &field.value, env, errors);
        }
        return infer_expr(expr, env);
    }

    let got = check_expr(expr, env, errors);
    if !is_assignable(expected, &got) {
        errors.push(TypeckError::type_mismatch(
            &expected.to_string(),
            &got.to_string(),
            expr.span(),
        ));
    }
    got
}

fn check_binary(bin: &BinaryExpr, env: &mut TypeEnv, errors: &mut Vec<TypeckError>) -> Ty {
    if is_assignment_op(bin.op) {
        return check_assignment(bin, env, errors);
    }

    let left_ty = check_expr(&bin.left, env, errors);
    let right_ty = check_expr(&bin.right, env, errors);

    match bin.op {
        BinaryOp::Add
        | BinaryOp::Sub
        | BinaryOp::Mul
        | BinaryOp::Div
        | BinaryOp::Mod
        | BinaryOp::Pow => {
            // Skip check if either side is unknown/mixed (gradual boundary)
            if left_ty.is_unknown()
                || right_ty.is_unknown()
                || left_ty.is_mixed()
                || right_ty.is_mixed()
            {
                return infer_expr(
                    &Expr::Binary(Box::new(BinaryExpr {
                        span: bin.span,
                        left: bin.left.clone(),
                        op: bin.op,
                        right: bin.right.clone(),
                    })),
                    env,
                );
            }

            // String concatenation with +
            // Spec: string + any (non-error) operand produces string.
            // `"count: " + 42` → `"count: 42"`.
            if bin.op == BinaryOp::Add
                && (matches!(left_ty, Ty::String) || matches!(right_ty, Ty::String))
            {
                // Only error if the non-string operand is truly incompatible
                // (e.g., never/void types — anything else coerces via toString)
                let non_string = if matches!(left_ty, Ty::String) { &right_ty } else { &left_ty };
                if matches!(non_string, Ty::Never | Ty::Void) {
                    errors.push(TypeckError::incompatible_operands(
                        "+",
                        &left_ty.to_string(),
                        &right_ty.to_string(),
                        bin.span,
                    ));
                    return Ty::Unknown;
                }
                return Ty::String;
            }

            // Numeric operations
            if left_ty.is_numeric() && right_ty.is_numeric() {
                if matches!(left_ty, Ty::Float) || matches!(right_ty, Ty::Float) {
                    return Ty::Float;
                }
                return Ty::Int;
            }

            // Error: incompatible operands
            let op_str = match bin.op {
                BinaryOp::Add => "+",
                BinaryOp::Sub => "-",
                BinaryOp::Mul => "*",
                BinaryOp::Div => "/",
                BinaryOp::Mod => "%",
                BinaryOp::Pow => "**",
                _ => "?",
            };
            errors.push(TypeckError::incompatible_operands(
                op_str,
                &left_ty.to_string(),
                &right_ty.to_string(),
                bin.span,
            ));
            Ty::Unknown
        }
        BinaryOp::Eq
        | BinaryOp::Ne
        | BinaryOp::Lt
        | BinaryOp::Gt
        | BinaryOp::Le
        | BinaryOp::Ge
        | BinaryOp::Spaceship => Ty::Bool,
        BinaryOp::And | BinaryOp::Or => Ty::Bool,
        BinaryOp::Is => Ty::Bool,
        BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor | BinaryOp::Shl | BinaryOp::Shr => {
            Ty::Int
        }
        _ => infer_expr(
            &Expr::Binary(Box::new(BinaryExpr {
                span: bin.span,
                left: bin.left.clone(),
                op: bin.op,
                right: bin.right.clone(),
            })),
            env,
        ),
    }
}

fn check_call(call: &CallExpr, env: &mut TypeEnv, errors: &mut Vec<TypeckError>) -> Ty {
    // Resolve function name
    if let Expr::Ident(ident) = &call.callee {
        if let Some(sig) = env.lookup_fn(&ident.name) {
            let sig = sig.clone();
            // Skip type checking for untyped functions
            if !sig.is_typed {
                for arg in &call.args {
                    check_expr(&arg.value, env, errors);
                }
                return sig.ret.clone();
            }

            // Special case: variadic-like builtins (print accepts any args)
            if ident.name == "print" {
                for arg in &call.args {
                    check_expr(&arg.value, env, errors);
                }
                return sig.ret.clone();
            }

            // Generic instantiation: infer type args and substitute
            if !sig.type_params.is_empty() {
                let mut inferred_args: Vec<Ty> = Vec::new();
                for param_name in &sig.type_params {
                    let mut found = None;
                    for (i, param_ty) in sig.params.iter().enumerate() {
                        if let Ty::Named(name) = param_ty {
                            if name == param_name {
                                if let Some(arg) = call.args.get(i) {
                                    let arg_ty = check_expr(&arg.value, env, errors);
                                    if !arg_ty.is_unknown() {
                                        found = Some(arg_ty);
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    inferred_args.push(found.unwrap_or(Ty::Unknown));
                }
                let subst_params: Vec<Ty> = sig.params.iter()
                    .map(|p| p.substitute(&sig.type_params, &inferred_args))
                    .collect();
                let subst_ret = sig.ret.substitute(&sig.type_params, &inferred_args);
                check_args_against(&subst_params, &call.args, call.span, env, errors);
                return subst_ret;
            }

            check_args_against(&sig.params, &call.args, call.span, env, errors);
            return sig.ret.clone();
        }
    }

    let callee_ty = check_expr(&call.callee, env, errors);
    if let Ty::Function { params, ret } = callee_ty {
        check_args_against(&params, &call.args, call.span, env, errors);
        return *ret;
    }

    for arg in &call.args {
        check_expr(&arg.value, env, errors);
    }
    Ty::Unknown
}

fn check_array(arr: &ArrayLiteral, env: &mut TypeEnv, errors: &mut Vec<TypeckError>) -> Ty {
    // Just check each element
    for elem in &arr.elements {
        check_expr(elem, env, errors);
    }
    infer_expr(&Expr::Array(arr.clone()), env)
}

fn check_object(object: &ObjectLiteral, env: &mut TypeEnv, errors: &mut Vec<TypeckError>) -> Ty {
    for field in &object.fields {
        check_expr(&field.value, env, errors);
    }
    infer_expr(&Expr::Object(object.clone()), env)
}

fn check_index(index: &IndexExpr, env: &mut TypeEnv, errors: &mut Vec<TypeckError>) -> Ty {
    let object_ty = check_expr(&index.object, env, errors);
    let index_ty = check_expr(&index.index, env, errors);

    match object_ty.strip_null() {
        Ty::List(element) => {
            if !is_assignable(&Ty::Int, &index_ty) {
                errors.push(TypeckError::type_mismatch(
                    "int",
                    &index_ty.to_string(),
                    index.index.span(),
                ));
            }
            *element
        }
        Ty::Map(key, value) => {
            if !is_assignable(&key, &index_ty) {
                errors.push(TypeckError::type_mismatch(
                    &key.to_string(),
                    &index_ty.to_string(),
                    index.index.span(),
                ));
            }
            *value
        }
        Ty::Mixed | Ty::Unknown => Ty::Unknown,
        other => {
            errors.push(TypeckError::property_not_found(
                &other.to_string(),
                "[]",
                index.span,
            ));
            Ty::Unknown
        }
    }
}

fn check_member(member: &MemberExpr, env: &mut TypeEnv, errors: &mut Vec<TypeckError>) -> Ty {
    let object_ty = check_expr(&member.object, env, errors);
    if object_ty.is_nullable() && !member.optional {
        errors.push(TypeckError::null_access(member.span));
    }

    let member_ty = lookup_member_type(&object_ty.strip_null(), &member.property, env, errors);
    if member.optional && object_ty.is_nullable() {
        Ty::union(vec![member_ty, Ty::Null])
    } else {
        member_ty
    }
}

fn lookup_member_type(
    object_ty: &Ty,
    property: &Ident,
    env: &TypeEnv,
    errors: &mut Vec<TypeckError>,
) -> Ty {
    match object_ty {
        Ty::Named(name) => {
            if let Some(shape) = env.lookup_shape(name) {
                shape.member_type(&property.name).unwrap_or_else(|| {
                    errors.push(TypeckError::property_not_found(
                        name,
                        &property.name,
                        property.span,
                    ));
                    Ty::Unknown
                })
            } else {
                Ty::Unknown
            }
        }
        Ty::Union(types) => {
            let mut member_types = Vec::new();
            for ty in types {
                member_types.push(lookup_member_type(&ty.strip_null(), property, env, errors));
            }
            Ty::union(member_types)
        }
        Ty::Mixed | Ty::Unknown | Ty::Never => Ty::Unknown,
        other => {
            errors.push(TypeckError::property_not_found(
                &other.to_string(),
                &property.name,
                property.span,
            ));
            Ty::Unknown
        }
    }
}

fn check_new(new_expr: &NewExpr, env: &mut TypeEnv, errors: &mut Vec<TypeckError>) -> Ty {
    let class_ty = Ty::Named(new_expr.type_name.name.clone());
    if let Some(shape) = env.lookup_shape(&new_expr.type_name.name) {
        if let Some(constructor) = shape.constructor.clone() {
            if constructor.is_typed {
                check_args_against(
                    &constructor.params,
                    &new_expr.args,
                    new_expr.span,
                    env,
                    errors,
                );
            } else {
                for arg in &new_expr.args {
                    check_expr(&arg.value, env, errors);
                }
            }
        } else if !new_expr.args.is_empty() {
            errors.push(TypeckError::arg_count(
                0,
                new_expr.args.len(),
                new_expr.span,
            ));
        }
    } else {
        for arg in &new_expr.args {
            check_expr(&arg.value, env, errors);
        }
    }
    class_ty
}

fn check_assignment(bin: &BinaryExpr, env: &mut TypeEnv, errors: &mut Vec<TypeckError>) -> Ty {
    let value_ty = check_expr(&bin.right, env, errors);
    let target_ty = assignment_target_type(&bin.left, env, errors);

    if !is_assignable(&target_ty, &value_ty) {
        errors.push(TypeckError::type_mismatch(
            &target_ty.to_string(),
            &value_ty.to_string(),
            bin.right.span(),
        ));
    }

    if let Expr::Ident(ident) = &bin.left {
        if target_ty.is_unknown() {
            env.assign(&ident.name, value_ty);
        }
    }

    Ty::Void
}

fn assignment_target_type(target: &Expr, env: &mut TypeEnv, errors: &mut Vec<TypeckError>) -> Ty {
    match target {
        Expr::Ident(ident) => env.lookup(&ident.name).cloned().unwrap_or(Ty::Unknown),
        Expr::Member(member) => check_member(member, env, errors),
        Expr::Index(index) => check_index(index, env, errors),
        _ => {
            check_expr(target, env, errors);
            Ty::Unknown
        }
    }
}

fn check_args_against(
    params: &[Ty],
    args: &[Argument],
    call_span: coco_span::Span,
    env: &mut TypeEnv,
    errors: &mut Vec<TypeckError>,
) {
    if args.len() != params.len() {
        errors.push(TypeckError::arg_count(params.len(), args.len(), call_span));
        for arg in args {
            check_expr(&arg.value, env, errors);
        }
        return;
    }

    for (arg, param_ty) in args.iter().zip(params.iter()) {
        let arg_ty = check_expr(&arg.value, env, errors);
        if !is_assignable(param_ty, &arg_ty) {
            errors.push(TypeckError::type_mismatch(
                &param_ty.to_string(),
                &arg_ty.to_string(),
                arg.span,
            ));
        }
    }
}

fn is_assignment_op(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::Assign
            | BinaryOp::AddAssign
            | BinaryOp::SubAssign
            | BinaryOp::MulAssign
            | BinaryOp::DivAssign
            | BinaryOp::ModAssign
            | BinaryOp::PowAssign
            | BinaryOp::ShlAssign
            | BinaryOp::ShrAssign
            | BinaryOp::BitAndAssign
            | BinaryOp::BitOrAssign
            | BinaryOp::BitXorAssign
    )
}

fn object_key_type(_key: &ObjectKey) -> Ty {
    Ty::String
}

fn object_key_span(key: &ObjectKey) -> coco_span::Span {
    match key {
        ObjectKey::Ident(ident) => ident.span,
        ObjectKey::String(_, span) => *span,
    }
}
