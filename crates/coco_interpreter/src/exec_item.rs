use std::path::Path;

use coco_parser::Parser;
use coco_syntax::*;

use crate::error::{IResult, RuntimeError, Signal};
use crate::value::{Function, Value};
use crate::Interpreter;

impl Interpreter {
    /// Execute a top-level item (declaration/statement).
    pub(crate) fn exec_item(&mut self, item: &Item) -> IResult {
        match item {
            Item::FnDecl(fn_decl) => self.exec_fn_decl(fn_decl),
            Item::LetDecl(let_decl) => self.exec_let_decl(let_decl),
            Item::ConstDecl(const_decl) => self.exec_const_decl(const_decl),
            Item::ExprStmt(expr_stmt) => self.eval_expr(&expr_stmt.expr),
            Item::Stmt(stmt) => self.exec_stmt(stmt),
            Item::ClassDecl(class_decl) => self.exec_class_decl(class_decl),
            Item::Import(import) => self.exec_import(import),
            _ => Ok(Value::Null),
        }
    }

    fn exec_fn_decl(&mut self, fn_decl: &FnDecl) -> IResult {
        let name = fn_decl.name.name.clone();
        let params: Vec<String> = fn_decl.params.iter().map(|p| p.name.name.clone()).collect();
        let func = Function {
            name: name.clone(),
            params,
            body: fn_decl.body.clone(),
        };
        self.env.define(name, Value::Function(func), false);
        Ok(Value::Null)
    }

    fn exec_let_decl(&mut self, let_decl: &LetDecl) -> IResult {
        let value = if let Some(expr) = &let_decl.value {
            self.eval_expr(expr)?
        } else {
            Value::Null
        };
        self.env
            .define(let_decl.name.name.clone(), value, true);
        Ok(Value::Null)
    }

    fn exec_const_decl(&mut self, const_decl: &ConstDecl) -> IResult {
        let value = self.eval_expr(&const_decl.value)?;
        self.env
            .define(const_decl.name.name.clone(), value, false);
        Ok(Value::Null)
    }

    fn exec_class_decl(&mut self, class_decl: &ClassDecl) -> IResult {
        // Store class as a map with metadata
        let mut class_map = std::collections::HashMap::new();

        // Class name marker
        class_map.insert(
            "__class__".to_string(),
            Value::String(class_decl.name.name.clone()),
        );

        // Inheritance: store parent class reference
        if let Some(parent) = &class_decl.extends {
            // Extract the class name from the type annotation
            let parent_name = match parent {
                Type::Named(named) => &named.name.name,
                _ => {
                    return Err(Signal::Error(RuntimeError::new(
                        "extends clause must be a named class",
                    )));
                }
            };
            if let Some(parent_val) = self.env.get(parent_name) {
                class_map.insert(
                    "__parent__".to_string(),
                    parent_val.clone(),
                );
            }
        }

        // Process members: Constructor, Method, Property
        for member in &class_decl.members {
            match member {
                ClassMember::Constructor(ctor) => {
                    let params: Vec<String> = ctor
                        .params
                        .iter()
                        .map(|p| p.name.name.clone())
                        .collect();
                    let ctor_func = Function {
                        name: format!("{}.constructor", class_decl.name.name),
                        params,
                        body: ctor.body.clone(),
                    };
                    class_map.insert(
                        "__constructor__".to_string(),
                        Value::Function(ctor_func),
                    );
                }
                ClassMember::Method(method) => {
                    let params: Vec<String> = method
                        .params
                        .iter()
                        .map(|p| p.name.name.clone())
                        .collect();
                    let meth_func = Function {
                        name: format!("{}.{}", class_decl.name.name, method.name.name),
                        params,
                        body: method.body.clone(),
                    };
                    class_map
                        .insert(method.name.name.clone(), Value::Function(meth_func));
                }
                _ => {} // Property and UseTrait deferred
            }
        }

        let class_val = self.alloc_map(class_map);
        self.env
            .define(class_decl.name.name.clone(), class_val, false);
        Ok(Value::Null)
    }

    /// Execute an import statement — resolve the module, parse it,
    /// execute its items, and import the requested names.
    fn exec_import(&mut self, import: &Import) -> IResult {
        // Build the module file path
        let module_path = match &self.source_file {
            Some(base) => {
                let parent = base.parent().unwrap_or(Path::new("."));
                parent.join(&import.source)
            }
            None => Path::new(&import.source).to_path_buf(),
        };

        // Read the module file
        let src = match std::fs::read_to_string(&module_path) {
            Ok(s) => s,
            Err(e) => {
                return Err(Signal::Error(RuntimeError::new(format!(
                    "cannot read module '{}': {}",
                    module_path.display(),
                    e
                ))));
            }
        };

        // Parse the module
        let mut parser = Parser::new(&src);
        let program = parser.parse_program();

        // Extract requested import names
        let export_names: Vec<String> = match &import.items {
            ImportItems::Named(idents) => idents.iter().map(|i| i.name.clone()).collect(),
            ImportItems::Namespace(_) => Vec::new(), // `import * as name` deferred
        };

        // Execute the module's items in a fresh scope to collect exports
        let saved_file = self.source_file.clone();
        self.source_file = Some(module_path);
        self.env.push_scope();

        for item in &program.items {
            if let Err(e) = self.exec_item(item) {
                self.env.pop_scope();
                self.source_file = saved_file;
                return Err(e);
            }
        }

        // Collect exported values before popping module scope
        let mut exports: Vec<(String, Value)> = Vec::new();
        for name in &export_names {
            if let Some(val) = self.env.get_current_scope(name) {
                exports.push((name.clone(), val));
            }
        }

        // Pop the module scope
        self.env.pop_scope();

        // Define exports in parent (now-current) scope
        for (name, val) in exports {
            self.env.define(name, val, true);
        }

        self.source_file = saved_file;

        Ok(Value::Null)
    }
}
