//! Type inference for expressions.

use coco_syntax::*;

use crate::env::TypeEnv;
use crate::types::Ty;
use crate::unify::is_assignable;

/// Infer the type of an expression given the current environment.
/// Returns `Ty::Unknown` for expressions we cannot resolve.
pub fn infer_expr(expr: &Expr, env: &TypeEnv) -> Ty {
    match expr {
        Expr::Literal(lit) => infer_literal(lit),
        Expr::Ident(ident) => env.lookup(&ident.name).cloned().unwrap_or(Ty::Unknown),
        Expr::Binary(bin) => infer_binary(bin, env),
        Expr::Unary(unary) => infer_unary(unary, env),
        Expr::Call(call) => infer_call(call, env),
        Expr::Index(index) => infer_index(index, env),
        Expr::Member(member) => infer_member(member, env),
        Expr::Array(arr) => infer_array(arr, env),
        Expr::Object(object) => infer_object(object, env),
        Expr::Group(inner) => infer_expr(inner, env),
        Expr::Ternary(ternary) => infer_ternary(ternary, env),
        Expr::NullCoalesce(nc) => infer_null_coalesce(nc, env),
        Expr::Elvis(_) => Ty::Unknown,
        Expr::Pipe(_) => Ty::Unknown,
        Expr::Assignment(_) => Ty::Void,
        Expr::Postfix(pf) => {
            match &pf.op {
                PostfixOp::Question => {
                    // expr? unwraps Result<T,E> → T
                    let inner = infer_expr(&pf.object, env);
                    match inner {
                        Ty::Result(ok, _) => *ok,
                        _ => Ty::Unknown,
                    }
                }
                PostfixOp::Bang => infer_expr(&pf.object, env).strip_null(),
                _ => Ty::Unknown,
            }
        }
        Expr::Lambda(lambda) => {
            let params: Vec<Ty> = lambda.params.iter().map(|p| {
                p.type_ann.as_ref().map(crate::convert::ast_type_to_ty).unwrap_or(Ty::Unknown)
            }).collect();
            let ret = lambda.return_type.as_ref().map(crate::convert::ast_type_to_ty).unwrap_or(Ty::Unknown);
            let ret = if lambda.is_async { Ty::Task(Box::new(ret)) } else { ret };
            Ty::Function { params, ret: Box::new(ret) }
        }
        Expr::Match(_) => Ty::Unknown,
        Expr::This(_) | Expr::Dollar(_) => env.current_self().cloned().unwrap_or(Ty::Unknown),
        Expr::DollarDollar(_) => Ty::Unknown,
        Expr::Super(_) => Ty::Unknown,
        Expr::New(new_expr) => Ty::Named(new_expr.type_name.name.clone()),
        Expr::Parallel(_) => Ty::Unknown,
        Expr::Template(_) => Ty::String,
        Expr::Lazy(inner) => {
            let _ = infer_expr(inner, env);
            Ty::Unknown // deferred — wraps in Task<T> when Ty::Task is added
        }
    }
}

fn infer_literal(lit: &Literal) -> Ty {
    match lit {
        Literal::Int(_, _) => Ty::Int,
        Literal::Float(_, _) => Ty::Float,
        Literal::String(_, _) => Ty::String,
        Literal::Char(_, _) => Ty::Char,
        Literal::Bool(_, _) => Ty::Bool,
        Literal::Null(_) => Ty::Null,
    }
}

fn infer_binary(bin: &BinaryExpr, env: &TypeEnv) -> Ty {
    let left = infer_expr(&bin.left, env);
    let right = infer_expr(&bin.right, env);

    match bin.op {
        // Arithmetic operators
        BinaryOp::Add
        | BinaryOp::Sub
        | BinaryOp::Mul
        | BinaryOp::Div
        | BinaryOp::Mod
        | BinaryOp::Pow => {
            // String concatenation
            if bin.op == BinaryOp::Add
                && (matches!(left, Ty::String) || matches!(right, Ty::String))
            {
                return Ty::String;
            }

            // Numeric promotion
            if left.is_unknown() || right.is_unknown() || left.is_mixed() || right.is_mixed() {
                return Ty::Unknown;
            }
            if matches!(left, Ty::Float) || matches!(right, Ty::Float) {
                Ty::Float
            } else if left.is_numeric() && right.is_numeric() {
                Ty::Int
            } else {
                // Might be an error, but we report that in check_expr
                Ty::Unknown
            }
        }
        // Comparison operators
        BinaryOp::Eq
        | BinaryOp::Ne
        | BinaryOp::Lt
        | BinaryOp::Gt
        | BinaryOp::Le
        | BinaryOp::Ge
        | BinaryOp::Spaceship => Ty::Bool,
        // Logical operators
        BinaryOp::And | BinaryOp::Or => Ty::Bool,
        // Bitwise operators
        BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor | BinaryOp::Shl | BinaryOp::Shr => {
            if left.is_unknown() || right.is_unknown() || left.is_mixed() || right.is_mixed() {
                Ty::Unknown
            } else {
                Ty::Int
            }
        }
        // Null coalesce
        BinaryOp::NullCoalesce => {
            if left.is_nullable() {
                let stripped = left.strip_null();
                if stripped == Ty::Never {
                    right
                } else {
                    stripped
                }
            } else if left.is_unknown() || left.is_mixed() {
                right
            } else {
                left
            }
        }
        BinaryOp::Elvis => Ty::Unknown,
        // Pipe operators
        BinaryOp::PipeRight | BinaryOp::PipeLeft => Ty::Unknown,
        // Assignment ops return void
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
        | BinaryOp::BitXorAssign => Ty::Void,
        // Range
        BinaryOp::Range | BinaryOp::RangeInclusive => Ty::Unknown,
        // Is type test
        BinaryOp::Is => Ty::Bool,
    }
}

fn infer_unary(unary: &UnaryExpr, env: &TypeEnv) -> Ty {
    let inner = infer_expr(&unary.expr, env);
    match unary.op {
        UnaryOp::Not => Ty::Bool,
        UnaryOp::Neg => {
            if inner.is_unknown() || inner.is_mixed() {
                Ty::Unknown
            } else {
                inner
            }
        }
        UnaryOp::BitNot => Ty::Int,
        UnaryOp::Typeof => Ty::String,
        UnaryOp::Await => inner,
        _ => Ty::Unknown,
    }
}

fn infer_call(call: &CallExpr, env: &TypeEnv) -> Ty {
    // Resolve the callee to a function name
    if let Expr::Ident(ident) = &call.callee {
        if let Some(sig) = env.lookup_fn(&ident.name) {
            if sig.type_params.is_empty() {
                return sig.ret.clone();
            }
            // Generic instantiation: infer type args from call arguments
            let mut inferred_args: Vec<Ty> = Vec::new();
            for param_name in &sig.type_params {
                // For each type param, try to find a matching argument
                let mut found = None;
                for (i, param_ty) in sig.params.iter().enumerate() {
                    if let Ty::Named(name) = param_ty {
                        if name == param_name {
                            // This param's type is the type param — infer from arg
                            if let Some(arg) = call.args.get(i) {
                                let arg_ty = infer_expr(&arg.value, env);
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
            return sig.ret.substitute(&sig.type_params, &inferred_args);
        }
    }
    if let Ty::Function { ret, .. } = infer_expr(&call.callee, env) {
        return *ret;
    }
    Ty::Unknown
}

fn infer_index(index: &IndexExpr, env: &TypeEnv) -> Ty {
    let object_ty = infer_expr(&index.object, env);
    match object_ty {
        Ty::List(element) => *element,
        Ty::Map(_, value) => *value,
        Ty::Union(types) => {
            let indexed = types
                .iter()
                .map(|ty| match ty {
                    Ty::List(element) => (**element).clone(),
                    Ty::Map(_, value) => (**value).clone(),
                    _ => Ty::Unknown,
                })
                .collect();
            Ty::union(indexed)
        }
        _ => Ty::Unknown,
    }
}

fn infer_member(member: &MemberExpr, env: &TypeEnv) -> Ty {
    let object_ty = infer_expr(&member.object, env);
    let base_ty = object_ty.strip_null();
    let member_ty = match base_ty {
        Ty::Named(name) => env
            .lookup_shape(&name)
            .and_then(|shape| shape.member_type(&member.property.name))
            .unwrap_or(Ty::Unknown),
        // Enum variant access: Direction.North → Direction
        Ty::Enum(enum_name, variants) => {
            let variant = &member.property.name;
            if variants.iter().any(|v| v == variant) {
                Ty::Enum(enum_name.clone(), variants.clone())
            } else {
                Ty::Unknown
            }
        }
        Ty::Union(types) => {
            let member_types = types
                .iter()
                .map(|ty| match ty.strip_null() {
                    Ty::Named(name) => env
                        .lookup_shape(&name)
                        .and_then(|shape| shape.member_type(&member.property.name))
                        .unwrap_or(Ty::Unknown),
                    Ty::Enum(enum_name, variants) => {
                        let variant = &member.property.name;
                        if variants.iter().any(|v| v == variant) {
                            Ty::Enum(enum_name.clone(), variants.clone())
                        } else {
                            Ty::Unknown
                        }
                    }
                    _ => Ty::Unknown,
                })
                .collect();
            Ty::union(member_types)
        }
        _ => Ty::Unknown,
    };

    if member.optional && object_ty.is_nullable() {
        Ty::union(vec![member_ty, Ty::Null])
    } else {
        member_ty
    }
}

fn infer_array(arr: &ArrayLiteral, env: &TypeEnv) -> Ty {
    if arr.elements.is_empty() {
        return Ty::List(Box::new(Ty::Unknown));
    }
    let first = infer_expr(&arr.elements[0], env);
    // Check if all elements share a common type
    let mut common = first;
    for elem in arr.elements.iter().skip(1) {
        let elem_ty = infer_expr(elem, env);
        if elem_ty != common {
            if is_assignable(&common, &elem_ty) {
                // keep common
            } else if is_assignable(&elem_ty, &common) {
                common = elem_ty;
            } else {
                common = Ty::Mixed;
                break;
            }
        }
    }
    Ty::List(Box::new(common))
}

fn infer_object(object: &ObjectLiteral, env: &TypeEnv) -> Ty {
    if object.fields.is_empty() {
        return Ty::Map(Box::new(Ty::String), Box::new(Ty::Unknown));
    }

    let mut common = infer_expr(&object.fields[0].value, env);
    for field in object.fields.iter().skip(1) {
        let value_ty = infer_expr(&field.value, env);
        if value_ty != common {
            if is_assignable(&common, &value_ty) {
                // Keep current common type.
            } else if is_assignable(&value_ty, &common) {
                common = value_ty;
            } else {
                common = Ty::Mixed;
                break;
            }
        }
    }

    Ty::Map(Box::new(Ty::String), Box::new(common))
}

fn infer_ternary(ternary: &TernaryExpr, env: &TypeEnv) -> Ty {
    let then_ty = infer_expr(&ternary.then_expr, env);
    let else_ty = infer_expr(&ternary.else_expr, env);
    if then_ty == else_ty || is_assignable(&then_ty, &else_ty) {
        then_ty
    } else if is_assignable(&else_ty, &then_ty) {
        else_ty
    } else {
        Ty::union(vec![then_ty, else_ty])
    }
}

fn infer_null_coalesce(nc: &NullCoalesceExpr, env: &TypeEnv) -> Ty {
    let left = infer_expr(&nc.left, env);
    let right = infer_expr(&nc.right, env);
    // x ?? y: if x is nullable, result is strip_null(x) | type(y)
    if left.is_nullable() {
        let stripped = left.strip_null();
        if stripped == Ty::Never || stripped.is_unknown() {
            right
        } else {
            stripped
        }
    } else if left.is_unknown() || left.is_mixed() {
        right
    } else {
        left
    }
}
