//! Conversion from AST Type nodes to internal Ty representation.

use coco_syntax::{PrimitiveType, Type};

use crate::types::Ty;

/// Convert an AST `Type` node to an internal `Ty`.
pub fn ast_type_to_ty(ast_type: &Type) -> Ty {
    match ast_type {
        Type::Primitive(prim, _) => primitive_to_ty(prim),
        Type::Named(named) => {
            // Check if it is a well-known name that maps to a primitive
            match named.name.name.as_str() {
                "int" => Ty::Int,
                "uint" => Ty::Uint,
                "float" => Ty::Float,
                "bool" => Ty::Bool,
                "string" => Ty::String,
                "char" => Ty::Char,
                "byte" => Ty::Byte,
                "null" => Ty::Null,
                "void" => Ty::Void,
                "never" => Ty::Never,
                "mixed" => Ty::Mixed,
                _ => Ty::Named(named.name.name.clone()),
            }
        }
        Type::Union(union_type) => {
            let types: Vec<Ty> = union_type.types.iter().map(ast_type_to_ty).collect();
            Ty::Union(types)
        }
        Type::Intersection(_) => {
            // For now, intersection types are treated as mixed
            Ty::Mixed
        }
        Type::List(list_type) => {
            let elem = ast_type_to_ty(&list_type.element_type);
            Ty::List(Box::new(elem))
        }
        Type::Map(map_type) => {
            let key = ast_type_to_ty(&map_type.key_type);
            let value = ast_type_to_ty(&map_type.value_type);
            Ty::Map(Box::new(key), Box::new(value))
        }
        Type::Tuple(tuple_type) => {
            let types: Vec<Ty> = tuple_type
                .element_types
                .iter()
                .map(ast_type_to_ty)
                .collect();
            Ty::Tuple(types)
        }
        Type::Result(result_type) => {
            let ok = ast_type_to_ty(&result_type.ok_type);
            let err = ast_type_to_ty(&result_type.err_type);
            Ty::Result(Box::new(ok), Box::new(err))
        }
        Type::Function(fn_type) => {
            let params: Vec<Ty> = fn_type.param_types.iter().map(ast_type_to_ty).collect();
            let ret = ast_type_to_ty(&fn_type.return_type);
            Ty::Function {
                params,
                ret: Box::new(ret),
            }
        }
    }
}

/// Convert a primitive type AST node to a Ty.
fn primitive_to_ty(prim: &PrimitiveType) -> Ty {
    match prim {
        PrimitiveType::Int => Ty::Int,
        PrimitiveType::Uint => Ty::Uint,
        PrimitiveType::Float => Ty::Float,
        PrimitiveType::Bool => Ty::Bool,
        PrimitiveType::String => Ty::String,
        PrimitiveType::Char => Ty::Char,
        PrimitiveType::Byte => Ty::Byte,
        PrimitiveType::Null => Ty::Null,
        PrimitiveType::Void => Ty::Void,
        PrimitiveType::Never => Ty::Never,
        PrimitiveType::Mixed => Ty::Mixed,
    }
}
