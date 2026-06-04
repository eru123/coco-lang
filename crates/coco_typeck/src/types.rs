//! Internal type representation for the Coco type checker.

use std::fmt;

/// Internal representation of types used during type checking.
#[derive(Debug, Clone, PartialEq)]
pub enum Ty {
    Int,
    Uint,
    Float,
    Bool,
    String,
    Char,
    Byte,
    Null,
    Void,
    Never,
    Mixed,
    List(Box<Ty>),
    Map(Box<Ty>, Box<Ty>),
    Tuple(Vec<Ty>),
    Union(Vec<Ty>),
    Function {
        params: Vec<Ty>,
        ret: Box<Ty>,
    },
    Named(std::string::String),
    Result(Box<Ty>, Box<Ty>),
    /// Used when a type cannot be determined (e.g., unannotated parameter).
    Unknown,
}

impl Ty {
    /// Returns true if this type is numeric (int, uint, or float).
    pub fn is_numeric(&self) -> bool {
        matches!(self, Ty::Int | Ty::Uint | Ty::Float)
    }

    /// Returns true if this type is `Mixed`.
    pub fn is_mixed(&self) -> bool {
        matches!(self, Ty::Mixed)
    }

    /// Returns true if this type can hold null (is Null, a union containing Null, or Mixed).
    pub fn is_nullable(&self) -> bool {
        match self {
            Ty::Null => true,
            Ty::Mixed => true,
            Ty::Union(types) => types.iter().any(|t| t.is_nullable()),
            _ => false,
        }
    }

    /// Returns a new type with Null removed from unions. If not a union, returns self.
    pub fn strip_null(&self) -> Ty {
        match self {
            Ty::Union(types) => {
                let filtered: Vec<Ty> = types
                    .iter()
                    .filter(|t| !matches!(t, Ty::Null))
                    .cloned()
                    .collect();
                match filtered.len() {
                    0 => Ty::Never,
                    1 => filtered.into_iter().next().unwrap(),
                    _ => Ty::Union(filtered),
                }
            }
            Ty::Null => Ty::Never,
            other => other.clone(),
        }
    }

    /// Returns true if the type is Unknown (unannotated / cannot be determined).
    pub fn is_unknown(&self) -> bool {
        matches!(self, Ty::Unknown)
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Int => write!(f, "int"),
            Ty::Uint => write!(f, "uint"),
            Ty::Float => write!(f, "float"),
            Ty::Bool => write!(f, "bool"),
            Ty::String => write!(f, "string"),
            Ty::Char => write!(f, "char"),
            Ty::Byte => write!(f, "byte"),
            Ty::Null => write!(f, "null"),
            Ty::Void => write!(f, "void"),
            Ty::Never => write!(f, "never"),
            Ty::Mixed => write!(f, "mixed"),
            Ty::List(elem) => write!(f, "list<{}>", elem),
            Ty::Map(k, v) => write!(f, "map<{}, {}>", k, v),
            Ty::Tuple(elems) => {
                write!(f, "(")?;
                for (i, e) in elems.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", e)?;
                }
                write!(f, ")")
            }
            Ty::Union(types) => {
                for (i, t) in types.iter().enumerate() {
                    if i > 0 {
                        write!(f, "|")?;
                    }
                    write!(f, "{}", t)?;
                }
                Ok(())
            }
            Ty::Function { params, ret } => {
                write!(f, "fn(")?;
                for (i, p) in params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", p)?;
                }
                write!(f, "): {}", ret)
            }
            Ty::Named(name) => write!(f, "{}", name),
            Ty::Result(ok, err) => write!(f, "Result<{}, {}>", ok, err),
            Ty::Unknown => write!(f, "unknown"),
        }
    }
}
