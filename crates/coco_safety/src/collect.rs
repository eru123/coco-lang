//! First pass: collect variable bindings from top-level items.
//!
//! Registers `const` (immutable) and `let` (mutable) declarations in the
//! safety environment so downstream passes can answer mutability queries.

use coco_syntax::*;

use crate::env::SafetyEnv;

/// Walk top-level items and register all const/let declarations.
pub fn collect_bindings(items: &[Item], env: &mut SafetyEnv) {
    for item in items {
        collect_item(item, env);
    }
}

fn collect_item(item: &Item, env: &mut SafetyEnv) {
    match item {
        Item::ConstDecl(d) => {
            env.define(
                d.name.name.clone(),
                false, // const = immutable
                true,  // const always has initializer
                d.span,
            );
        }
        Item::LetDecl(d) => {
            env.define(
                d.name.name.clone(),
                true,                      // let = mutable
                d.value.is_some(),         // initialized if value present
                d.span,
            );
        }
        Item::FnDecl(d) => {
            env.define(
                d.name.name.clone(),
                false, // function names are immutable
                true,
                d.span,
            );
        }
        Item::ClassDecl(d) => {
            env.define(
                d.name.name.clone(),
                false,
                true,
                d.span,
            );
        }
        Item::Export(e) => {
            collect_item(&e.item, env);
        }
        // EnumDecl, InterfaceDecl, TraitDecl, TypeAlias, Import — skip
        _ => {}
    }
}
