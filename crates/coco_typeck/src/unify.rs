//! Type unification and assignability checks.

use crate::types::Ty;

/// Check if a value of type `value` can be assigned to a target of type `target`.
///
/// Rules:
/// - Mixed is compatible with everything (gradual typing boundary).
/// - Unknown is compatible with everything (unannotated code).
/// - Never is assignable to anything (bottom type).
/// - int can be promoted to float.
/// - null can be assigned to nullable types (unions containing null).
/// - Union membership: value assignable to target union if assignable to any member.
/// - Value union: all members must be assignable to target.
/// - List/Map covariance.
pub fn is_assignable(target: &Ty, value: &Ty) -> bool {
    // Same type is always fine
    if target == value {
        return true;
    }

    // Mixed accepts anything, and anything accepts Mixed (gradual typing boundary)
    if target.is_mixed() || value.is_mixed() {
        return true;
    }

    // Unknown (unannotated) is compatible with everything
    if target.is_unknown() || value.is_unknown() {
        return true;
    }

    // Never (bottom type) is assignable to anything
    if matches!(value, Ty::Never) {
        return true;
    }

    // int -> float promotion
    if matches!(target, Ty::Float) && matches!(value, Ty::Int | Ty::Uint) {
        return true;
    }

    // null -> nullable target
    if matches!(value, Ty::Null) && target.is_nullable() {
        return true;
    }

    // Target is a union: value must be assignable to at least one member
    if let Ty::Union(target_types) = target {
        return target_types.iter().any(|t| is_assignable(t, value));
    }

    // Value is a union: all members must be assignable to target
    if let Ty::Union(value_types) = value {
        return value_types.iter().all(|v| is_assignable(target, v));
    }

    // List covariance: list<A> assignable to list<B> if A assignable to B
    if let (Ty::List(target_elem), Ty::List(value_elem)) = (target, value) {
        return is_assignable(target_elem, value_elem);
    }

    // Map covariance
    if let (Ty::Map(tk, tv), Ty::Map(vk, vv)) = (target, value) {
        return is_assignable(tk, vk) && is_assignable(tv, vv);
    }

    // Tuple: same length and element-wise assignable
    if let (Ty::Tuple(target_elems), Ty::Tuple(value_elems)) = (target, value) {
        if target_elems.len() != value_elems.len() {
            return false;
        }
        return target_elems
            .iter()
            .zip(value_elems.iter())
            .all(|(t, v)| is_assignable(t, v));
    }

    // Function subtyping (simplified): param count matches + return assignable
    if let (
        Ty::Function {
            params: tp,
            ret: tr,
        },
        Ty::Function {
            params: vp,
            ret: vr,
        },
    ) = (target, value)
    {
        if tp.len() != vp.len() {
            return false;
        }
        // Contravariant params, covariant return
        let params_ok = tp.iter().zip(vp.iter()).all(|(t, v)| is_assignable(v, t));
        let ret_ok = is_assignable(tr, vr);
        return params_ok && ret_ok;
    }

    false
}
