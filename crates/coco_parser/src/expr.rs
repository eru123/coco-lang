//! Expression parsing helpers for the Coco parser.

use coco_lexer::TokenKind;
use coco_syntax::*;

/// Returns (left_bp, right_bp) for infix operators.
/// Returns None if the token is not an infix operator.
pub fn infix_bp(kind: TokenKind) -> Option<(u8, u8)> {
    match kind {
        // Assignment (right-associative)
        TokenKind::Assign | TokenKind::PlusEq | TokenKind::MinusEq
        | TokenKind::StarEq | TokenKind::SlashEq | TokenKind::PercentEq
        | TokenKind::StarStarEq | TokenKind::ShlEq | TokenKind::ShrEq
        | TokenKind::BitAndEq | TokenKind::BitOrEq | TokenKind::BitXorEq => Some((10, 9)),

        // Ternary (right-associative)
        TokenKind::Question => Some((15, 14)),

        // Elvis (right-associative)
        TokenKind::QuestionColon => Some((20, 19)),

        // Null coalesce (left-associative)
        TokenKind::QuestionQuestion => Some((25, 26)),

        // Logical OR (left-associative)
        TokenKind::Or => Some((30, 31)),

        // Logical AND (left-associative)
        TokenKind::And => Some((35, 36)),

        // Bitwise OR (left-associative)
        TokenKind::BitOr => Some((40, 41)),

        // Bitwise XOR (left-associative)
        TokenKind::BitXor => Some((45, 46)),

        // Bitwise AND (left-associative)
        TokenKind::BitAnd => Some((50, 51)),

        // Equality (left-associative)
        TokenKind::Eq | TokenKind::Ne => Some((55, 56)),

        // Comparison (left-associative)
        TokenKind::Lt | TokenKind::Gt | TokenKind::Le
        | TokenKind::Ge | TokenKind::Spaceship => Some((60, 61)),

        // Shift (left-associative)
        TokenKind::Shl | TokenKind::Shr => Some((65, 66)),

        // Range (right-associative)
        TokenKind::Range | TokenKind::RangeInclusive => Some((68, 69)),

        // Additive (left-associative)
        TokenKind::Plus | TokenKind::Minus => Some((70, 71)),

        // Multiplicative (left-associative)
        TokenKind::Star | TokenKind::Slash | TokenKind::Percent => Some((75, 76)),

        // Exponentiation (right-associative)
        TokenKind::StarStar => Some((80, 79)),

        // Pipe (left-associative)
        TokenKind::PipeRight | TokenKind::PipeLeft => Some((85, 86)),

        _ => None,
    }
}

pub fn token_to_binary_op(kind: TokenKind) -> Option<BinaryOp> {
    Some(match kind {
        TokenKind::Assign => BinaryOp::Assign,
        TokenKind::Plus => BinaryOp::Add,
        TokenKind::Minus => BinaryOp::Sub,
        TokenKind::Star => BinaryOp::Mul,
        TokenKind::Slash => BinaryOp::Div,
        TokenKind::Percent => BinaryOp::Mod,
        TokenKind::StarStar => BinaryOp::Pow,
        TokenKind::Eq => BinaryOp::Eq,
        TokenKind::Ne => BinaryOp::Ne,
        TokenKind::Lt => BinaryOp::Lt,
        TokenKind::Gt => BinaryOp::Gt,
        TokenKind::Le => BinaryOp::Le,
        TokenKind::Ge => BinaryOp::Ge,
        TokenKind::Spaceship => BinaryOp::Spaceship,
        TokenKind::And => BinaryOp::And,
        TokenKind::Or => BinaryOp::Or,
        TokenKind::BitAnd => BinaryOp::BitAnd,
        TokenKind::BitOr => BinaryOp::BitOr,
        TokenKind::BitXor => BinaryOp::BitXor,
        TokenKind::Shl => BinaryOp::Shl,
        TokenKind::Shr => BinaryOp::Shr,
        TokenKind::Range => BinaryOp::Range,
        TokenKind::RangeInclusive => BinaryOp::RangeInclusive,
        TokenKind::PipeRight => BinaryOp::PipeRight,
        TokenKind::PipeLeft => BinaryOp::PipeLeft,
        TokenKind::QuestionQuestion => BinaryOp::NullCoalesce,
        TokenKind::QuestionColon => BinaryOp::Elvis,
        TokenKind::PlusEq => BinaryOp::AddAssign,
        TokenKind::MinusEq => BinaryOp::SubAssign,
        TokenKind::StarEq => BinaryOp::MulAssign,
        TokenKind::SlashEq => BinaryOp::DivAssign,
        TokenKind::PercentEq => BinaryOp::ModAssign,
        TokenKind::StarStarEq => BinaryOp::PowAssign,
        TokenKind::ShlEq => BinaryOp::ShlAssign,
        TokenKind::ShrEq => BinaryOp::ShrAssign,
        TokenKind::BitAndEq => BinaryOp::BitAndAssign,
        TokenKind::BitOrEq => BinaryOp::BitOrAssign,
        TokenKind::BitXorEq => BinaryOp::BitXorAssign,
        _ => return None,
    })
}

pub fn parse_primitive_type(name: &str) -> Option<PrimitiveType> {
    Some(match name {
        "int" => PrimitiveType::Int,
        "uint" => PrimitiveType::Uint,
        "float" => PrimitiveType::Float,
        "bool" => PrimitiveType::Bool,
        "string" => PrimitiveType::String,
        "char" => PrimitiveType::Char,
        "byte" => PrimitiveType::Byte,
        "null" => PrimitiveType::Null,
        "void" => PrimitiveType::Void,
        "never" => PrimitiveType::Never,
        "mixed" => PrimitiveType::Mixed,
        _ => return None,
    })
}

pub fn parse_int_literal(text: &str) -> i64 {
    let text = text.replace('_', "");
    if text.starts_with("0x") || text.starts_with("0X") {
        i64::from_str_radix(&text[2..], 16).unwrap_or(0)
    } else if text.starts_with("0b") || text.starts_with("0B") {
        i64::from_str_radix(&text[2..], 2).unwrap_or(0)
    } else if text.starts_with("0o") || text.starts_with("0O") {
        i64::from_str_radix(&text[2..], 8).unwrap_or(0)
    } else {
        text.parse().unwrap_or(0)
    }
}
