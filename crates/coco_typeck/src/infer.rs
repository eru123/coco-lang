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
        Expr::Index(_) => Ty::Unknown,
        Expr::Member(_) => Ty::Unknown,
        Expr::Array(arr) => infer_array(arr, env),
        Expr::Object(_) => Ty::Unknown,
        Expr::Group(inner) => infer_expr(inner, env),
        Expr::Ternary(ternary) => infer_ternary(ternary, env),
        Expr::NullCoalesce(nc) => infer_null_coalesce(nc, env),
        Expr::Elvis(_) => Ty::Unknown,
        Expr::Pipe(_) => Ty::Unknown,
        Expr::Assignment(_) => Ty::Void,
        Expr::Postfix(_) => Ty::Unknown,
        Expr::Lambda(_) => Ty::Unknown,
        Expr::Match(_) => Ty::Unknown,
        Expr::This(_) | Expr::Dollar(_) | Expr::DollarDollar(_) => Ty::Unknown,
        Expr::New(_) => Ty::Unknown,
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
            return sig.ret.clone();
        }
    }
    Ty::Unknown
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

fn infer_ternary(ternary: &TernaryExpr, env: &TypeEnv) -> Ty {
    let then_ty = infer_expr(&ternary.then_expr, env);
    let else_ty = infer_expr(&ternary.else_expr, env);
    if then_ty == else_ty || is_assignable(&then_ty, &else_ty) {
        then_ty
    } else if is_assignable(&else_ty, &then_ty) {
        else_ty
    } else {
        Ty::Union(vec![then_ty, else_ty])
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
