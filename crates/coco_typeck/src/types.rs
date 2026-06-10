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
    /// An async task that yields a value of the inner type.
    Task(Box<Ty>),
    /// A nominal enum type with its variant names.
    Enum(String, Vec<String>),
    /// Used when a type cannot be determined (e.g., unannotated parameter).
    Unknown,
}

impl Ty {
    /// Build a flattened, deduplicated union. A one-member union collapses to that member.
    pub fn union(types: Vec<Ty>) -> Ty {
        let mut flattened = Vec::new();
        for ty in types {
            match ty {
                Ty::Union(inner) => flattened.extend(inner),
                other => flattened.push(other),
            }
        }

        let mut unique = Vec::new();
        for ty in flattened {
            if !unique.contains(&ty) {
                unique.push(ty);
            }
        }

        match unique.len() {
            0 => Ty::Never,
            1 => unique.into_iter().next().unwrap(),
            _ => Ty::Union(unique),
        }
    }

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
                    .map(Ty::strip_null)
                    .filter(|t| !matches!(t, Ty::Null | Ty::Never))
                    .collect();
                Ty::union(filtered)
            }
            Ty::Null => Ty::Never,
            other => other.clone(),
        }
    }

    /// Returns true if the type is Unknown (unannotated / cannot be determined).
    pub fn is_unknown(&self) -> bool {
        matches!(self, Ty::Unknown)
    }

    /// Substitute type parameter names with concrete types.
    /// `params` and `args` must be the same length.
    pub fn substitute(&self, params: &[String], args: &[Ty]) -> Ty {
        if params.is_empty() || args.is_empty() {
            return self.clone();
        }
        let pairs: Vec<(&String, &Ty)> = params.iter().zip(args.iter()).collect();
        self.subst_impl(&pairs)
    }

    fn subst_impl(&self, pairs: &[(&String, &Ty)]) -> Ty {
        match self {
            Ty::Named(name) => {
                for (param, arg) in pairs {
                    if *param == name {
                        return (*arg).clone();
                    }
                }
                self.clone()
            }
            Ty::List(elem) => Ty::List(Box::new(elem.subst_impl(pairs))),
            Ty::Map(k, v) => Ty::Map(Box::new(k.subst_impl(pairs)), Box::new(v.subst_impl(pairs))),
            Ty::Tuple(types) => Ty::Tuple(types.iter().map(|t| t.subst_impl(pairs)).collect()),
            Ty::Union(types) => Ty::Union(types.iter().map(|t| t.subst_impl(pairs)).collect()),
            Ty::Function { params, ret } => Ty::Function {
                params: params.iter().map(|t| t.subst_impl(pairs)).collect(),
                ret: Box::new(ret.subst_impl(pairs)),
            },
            Ty::Result(ok, err) => Ty::Result(
                Box::new(ok.subst_impl(pairs)),
                Box::new(err.subst_impl(pairs)),
            ),
            Ty::Task(inner) => Ty::Task(Box::new(inner.subst_impl(pairs))),
            Ty::Enum(name, variants) => Ty::Enum(name.clone(), variants.clone()),
            other => other.clone(),
        }
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
            Ty::Task(inner) => write!(f, "Task<{}>", inner),
            Ty::Enum(name, _) => write!(f, "{}", name),
            Ty::Unknown => write!(f, "unknown"),
        }
    }
}
