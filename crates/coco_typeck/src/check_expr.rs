//! Expression-level type checking.

use coco_syntax::*;

use crate::env::TypeEnv;
use crate::errors::TypeckError;
use crate::infer::infer_expr;
use crate::types::Ty;
use crate::unify::is_assignable;

/// Check an expression and return its inferred type, collecting errors.
pub fn check_expr(expr: &Expr, env: &TypeEnv, errors: &mut Vec<TypeckError>) -> Ty {
    match expr {
        Expr::Binary(bin) => check_binary(bin, env, errors),
        Expr::Call(call) => check_call(call, env, errors),
        Expr::Array(arr) => check_array(arr, env, errors),
        Expr::Group(inner) => check_expr(inner, env, errors),
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

/// Check an expression against an expected type and report assignment errors.
pub fn check_expr_against(
    expected: &Ty,
    expr: &Expr,
    env: &TypeEnv,
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

fn check_binary(bin: &BinaryExpr, env: &TypeEnv, errors: &mut Vec<TypeckError>) -> Ty {
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
            if bin.op == BinaryOp::Add
                && (matches!(left_ty, Ty::String) || matches!(right_ty, Ty::String))
            {
                if matches!(left_ty, Ty::String) && matches!(right_ty, Ty::String) {
                    return Ty::String;
                }
                // string + non-string that isn't also string -> error
                if matches!(left_ty, Ty::String)
                    && !matches!(right_ty, Ty::String)
                    && !right_ty.is_numeric()
                {
                    // For the test: "a + \"x\"" where a is int - this is an error
                    // But "\"a\" + \"b\"" is fine
                    // Actually string + anything is string concat in many languages
                    // But per spec, we error on int + string (incompatible arithmetic)
                    errors.push(TypeckError::incompatible_operands(
                        "+",
                        &left_ty.to_string(),
                        &right_ty.to_string(),
                        bin.span,
                    ));
                    return Ty::Unknown;
                }
                if matches!(right_ty, Ty::String)
                    && !matches!(left_ty, Ty::String)
                    && !left_ty.is_numeric()
                {
                    errors.push(TypeckError::incompatible_operands(
                        "+",
                        &left_ty.to_string(),
                        &right_ty.to_string(),
                        bin.span,
                    ));
                    return Ty::Unknown;
                }
                // int + string or string + int -> error (incompatible operands)
                if (left_ty.is_numeric() && matches!(right_ty, Ty::String))
                    || (matches!(left_ty, Ty::String) && right_ty.is_numeric())
                {
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

fn check_call(call: &CallExpr, env: &TypeEnv, errors: &mut Vec<TypeckError>) -> Ty {
    // Check argument expressions
    for arg in &call.args {
        check_expr(&arg.value, env, errors);
    }

    // Resolve function name
    if let Expr::Ident(ident) = &call.callee {
        if let Some(sig) = env.lookup_fn(&ident.name) {
            let sig = sig.clone();
            // Skip type checking for untyped functions
            if !sig.is_typed {
                return sig.ret.clone();
            }

            // Special case: variadic-like builtins (print accepts any args)
            if ident.name == "print" {
                return sig.ret.clone();
            }

            // Check argument count
            if call.args.len() != sig.params.len() {
                errors.push(TypeckError::arg_count(
                    sig.params.len(),
                    call.args.len(),
                    call.span,
                ));
                return sig.ret.clone();
            }

            // Check argument types
            for (i, arg) in call.args.iter().enumerate() {
                let arg_ty = infer_expr(&arg.value, env);
                let param_ty = &sig.params[i];
                if !is_assignable(param_ty, &arg_ty) {
                    errors.push(TypeckError::type_mismatch(
                        &param_ty.to_string(),
                        &arg_ty.to_string(),
                        arg.span,
                    ));
                }
            }

            return sig.ret.clone();
        }
    }

    Ty::Unknown
}

fn check_array(arr: &ArrayLiteral, env: &TypeEnv, errors: &mut Vec<TypeckError>) -> Ty {
    // Just check each element
    for elem in &arr.elements {
        check_expr(elem, env, errors);
    }
    infer_expr(&Expr::Array(arr.clone()), env)
}
