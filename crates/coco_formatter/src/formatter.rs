//! Pretty-printer: walks the AST and produces formatted Coco source code.

use coco_syntax::*;

pub struct Formatter {
    output: String,
    indent_level: usize,
    indent_str: &'static str,
    _max_width: usize,
    current_line_length: usize,
}

impl Formatter {
    pub fn new() -> Self {
        Self {
            output: String::new(),
            indent_level: 0,
            indent_str: "    ",
            _max_width: 100,
            current_line_length: 0,
        }
    }

    pub fn format(&mut self, program: &Program) -> String {
        self.output.clear();
        self.indent_level = 0;
        self.current_line_length = 0;

        for (i, item) in program.items.iter().enumerate() {
            if i > 0 {
                self.write_newline();
            }
            self.format_item(item);
        }

        if !self.output.ends_with('\n') {
            self.output.push('\n');
        }

        self.output.clone()
    }

    fn indent(&mut self) {
        self.indent_level += 1;
    }

    fn dedent(&mut self) {
        if self.indent_level > 0 {
            self.indent_level -= 1;
        }
    }

    fn write_indent(&mut self) {
        for _ in 0..self.indent_level {
            self.output.push_str(self.indent_str);
        }
        self.current_line_length = self.indent_level * self.indent_str.len();
    }

    fn write_newline(&mut self) {
        self.output.push('\n');
        self.current_line_length = 0;
    }

    fn write_str(&mut self, s: &str) {
        self.output.push_str(s);
        self.current_line_length += s.len();
    }

    fn write_space(&mut self) {
        self.write_str(" ");
    }

    fn write_line(&mut self, s: &str) {
        self.write_indent();
        self.write_str(s);
        self.write_newline();
    }

    fn format_item(&mut self, item: &Item) {
        match item {
            Item::FnDecl(f) => self.format_fn_decl(f),
            Item::ClassDecl(c) => self.format_class_decl(c),
            Item::InterfaceDecl(i) => self.format_interface_decl(i),
            Item::TraitDecl(t) => self.format_trait_decl(t),
            Item::EnumDecl(e) => self.format_enum_decl(e),
            Item::ConstDecl(c) => self.format_const_decl(c),
            Item::LetDecl(l) => self.format_let_decl(l),
            Item::TypeAlias(t) => self.format_type_alias(t),
            Item::Import(i) => self.format_import(i),
            Item::Export(e) => self.format_export(e),
            Item::ExprStmt(es) => self.format_expr_stmt(es),
            Item::Stmt(s) => self.format_stmt(s),
        }
    }

    fn format_fn_decl(&mut self, f: &FnDecl) {
        self.write_indent();
        if f.is_async {
            self.write_str("async ");
        }
        self.write_str("fn ");
        self.write_str(&f.name.name);
        if let Some(ref tp) = &f.type_params {
            self.format_type_params(tp);
        }
        self.write_str("(");
        self.format_params(&f.params);
        self.write_str(")");
        if let Some(ref rt) = &f.return_type {
            self.write_str(": ");
            self.format_type(rt);
        }
        self.write_space();
        self.format_block(&f.body);
        self.write_newline();
    }

    fn format_class_decl(&mut self, c: &ClassDecl) {
        self.write_indent();
        self.write_str("class ");
        self.write_str(&c.name.name);
        if let Some(ref tp) = &c.type_params {
            self.format_type_params(tp);
        }
        if let Some(ref e) = &c.extends {
            self.write_str(" extends ");
            self.format_type(e);
        }
        if !c.implements.is_empty() {
            self.write_str(" implements ");
            for (i, iface) in c.implements.iter().enumerate() {
                if i > 0 {
                    self.write_str(", ");
                }
                self.format_type(iface);
            }
        }
        self.write_str(" {");
        self.write_newline();
        self.indent();
        for member in &c.members {
            self.format_class_member(member);
        }
        self.dedent();
        self.write_line("}");
        self.write_newline();
    }

    fn format_class_member(&mut self, m: &ClassMember) {
        match m {
            ClassMember::Constructor(c) => {
                self.write_indent();
                self.write_str("constructor(");
                for (i, p) in c.params.iter().enumerate() {
                    if i > 0 {
                        self.write_str(", ");
                    }
                    self.format_modifiers(&p.modifiers);
                    self.write_str(&p.name.name);
                    if let Some(ref t) = &p.type_ann {
                        self.write_str(": ");
                        self.format_type(t);
                    }
                    if let Some(ref v) = &p.default_value {
                        self.write_str(" = ");
                        self.format_expr(v);
                    }
                }
                self.write_str(") ");
                self.format_block(&c.body);
                self.write_newline();
            }
            ClassMember::Method(m) => {
                self.write_indent();
                self.format_modifiers(&m.modifiers);
                if m.is_async {
                    self.write_str("async ");
                }
                self.write_str("fn ");
                self.write_str(&m.name.name);
                if let Some(ref tp) = &m.type_params {
                    self.format_type_params(tp);
                }
                self.write_str("(");
                self.format_params(&m.params);
                self.write_str(")");
                if let Some(ref rt) = &m.return_type {
                    self.write_str(": ");
                    self.format_type(rt);
                }
                self.write_space();
                self.format_block(&m.body);
                self.write_newline();
            }
            ClassMember::Property(p) => {
                self.write_indent();
                self.format_modifiers(&p.modifiers);
                self.write_str(&p.name.name);
                self.write_str(": ");
                self.format_type(&p.type_ann);
                if let Some(ref v) = &p.default_value {
                    self.write_str(" = ");
                    self.format_expr(v);
                }
                self.write_str(";");
                self.write_newline();
            }
            ClassMember::UseTrait(u) => {
                self.write_indent();
                self.write_str("use ");
                for (i, t) in u.traits.iter().enumerate() {
                    if i > 0 {
                        self.write_str(", ");
                    }
                    self.write_str(&t.name);
                }
                self.write_str(";");
                self.write_newline();
            }
        }
    }

    fn format_interface_decl(&mut self, i: &InterfaceDecl) {
        self.write_indent();
        self.write_str("interface ");
        self.write_str(&i.name.name);
        if let Some(ref tp) = &i.type_params {
            self.format_type_params(tp);
        }
        if let Some(ref e) = &i.extends {
            self.write_str(" extends ");
            self.format_type(e);
        }
        self.write_str(" {");
        self.write_newline();
        self.indent();
        for member in &i.members {
            match member {
                InterfaceMember::MethodSignature(ms) => {
                    self.write_indent();
                    if ms.is_async {
                        self.write_str("async ");
                    }
                    self.write_str("fn ");
                    self.write_str(&ms.name.name);
                    if let Some(ref tp) = &ms.type_params {
                        self.format_type_params(tp);
                    }
                    self.write_str("(");
                    self.format_params(&ms.params);
                    self.write_str("): ");
                    self.format_type(&ms.return_type);
                    self.write_str(";");
                    self.write_newline();
                }
                InterfaceMember::PropertySignature(ps) => {
                    self.write_indent();
                    self.write_str(&ps.name.name);
                    self.write_str(": ");
                    self.format_type(&ps.type_ann);
                    self.write_str(";");
                    self.write_newline();
                }
            }
        }
        self.dedent();
        self.write_line("}");
        self.write_newline();
    }

    fn format_trait_decl(&mut self, t: &TraitDecl) {
        self.write_indent();
        self.write_str("trait ");
        self.write_str(&t.name.name);
        if let Some(ref tp) = &t.type_params {
            self.format_type_params(tp);
        }
        self.write_str(" {");
        self.write_newline();
        self.indent();
        for member in &t.members {
            match member {
                TraitMember::Method(m) => {
                    self.write_indent();
                    self.format_modifiers(&m.modifiers);
                    if m.is_async {
                        self.write_str("async ");
                    }
                    self.write_str("fn ");
                    self.write_str(&m.name.name);
                    if let Some(ref tp) = &m.type_params {
                        self.format_type_params(tp);
                    }
                    self.write_str("(");
                    self.format_params(&m.params);
                    self.write_str(")");
                    if let Some(ref rt) = &m.return_type {
                        self.write_str(": ");
                        self.format_type(rt);
                    }
                    self.write_space();
                    self.format_block(&m.body);
                    self.write_newline();
                }
                TraitMember::MethodSignature(ms) => {
                    self.write_indent();
                    if ms.is_async {
                        self.write_str("async ");
                    }
                    self.write_str("fn ");
                    self.write_str(&ms.name.name);
                    if let Some(ref tp) = &ms.type_params {
                        self.format_type_params(tp);
                    }
                    self.write_str("(");
                    self.format_params(&ms.params);
                    self.write_str("): ");
                    self.format_type(&ms.return_type);
                    self.write_str(";");
                    self.write_newline();
                }
                TraitMember::Property(p) => {
                    self.write_indent();
                    self.format_modifiers(&p.modifiers);
                    self.write_str(&p.name.name);
                    self.write_str(": ");
                    self.format_type(&p.type_ann);
                    if let Some(ref v) = &p.default_value {
                        self.write_str(" = ");
                        self.format_expr(v);
                    }
                    self.write_str(";");
                    self.write_newline();
                }
            }
        }
        self.dedent();
        self.write_line("}");
        self.write_newline();
    }

    fn format_enum_decl(&mut self, e: &EnumDecl) {
        self.write_indent();
        self.write_str("enum ");
        self.write_str(&e.name.name);
        if let Some(ref bt) = &e.backing_type {
            self.write_str(": ");
            self.format_type(bt);
        }
        self.write_str(" {");
        self.write_newline();
        self.indent();
        for (i, v) in e.variants.iter().enumerate() {
            self.write_indent();
            self.write_str(&v.name.name);
            if let Some(ref fields) = &v.fields {
                self.write_str("(");
                for (j, f) in fields.iter().enumerate() {
                    if j > 0 {
                        self.write_str(", ");
                    }
                    self.format_type(f);
                }
                self.write_str(")");
            }
            if let Some(ref val) = &v.value {
                self.write_str(" = ");
                self.format_expr(val);
            }
            if i < e.variants.len() - 1 {
                self.write_str(",");
            }
            self.write_newline();
        }
        self.dedent();
        self.write_line("}");
        self.write_newline();
    }

    fn format_const_decl(&mut self, c: &ConstDecl) {
        self.write_indent();
        self.write_str("const ");
        self.write_str(&c.name.name);
        if let Some(ref t) = &c.type_ann {
            self.write_str(": ");
            self.format_type(t);
        }
        self.write_str(" = ");
        self.format_expr(&c.value);
        self.write_str(";");
        self.write_newline();
    }

    fn format_let_decl(&mut self, l: &LetDecl) {
        self.write_indent();
        self.write_str("let ");
        self.write_str(&l.name.name);
        if let Some(ref t) = &l.type_ann {
            self.write_str(": ");
            self.format_type(t);
        }
        if let Some(ref v) = &l.value {
            self.write_str(" = ");
            self.format_expr(v);
        }
        self.write_str(";");
        self.write_newline();
    }

    fn format_type_alias(&mut self, t: &TypeAlias) {
        self.write_indent();
        self.write_str("type ");
        self.write_str(&t.name.name);
        if let Some(ref tp) = &t.type_params {
            self.format_type_params(tp);
        }
        self.write_str(" = ");
        self.format_type(&t.target);
        self.write_str(";");
        self.write_newline();
    }

    fn format_import(&mut self, i: &Import) {
        self.write_indent();
        self.write_str("import ");
        match &i.items {
            ImportItems::Named(names) => {
                self.write_str("{ ");
                for (j, n) in names.iter().enumerate() {
                    if j > 0 {
                        self.write_str(", ");
                    }
                    self.write_str(&n.name);
                }
                self.write_str(" }");
            }
            ImportItems::Namespace(n) => {
                self.write_str("* as ");
                self.write_str(&n.name);
            }
        }
        self.write_str(" from ");
        self.write_str("\"");
        self.write_str(&i.source);
        self.write_str("\"");
        self.write_str(";");
        self.write_newline();
    }

    fn format_export(&mut self, e: &Export) {
        self.write_indent();
        self.write_str("export ");
        self.format_item(&e.item);
    }

    fn format_expr_stmt(&mut self, es: &ExprStmt) {
        self.write_indent();
        self.format_expr(&es.expr);
        self.write_str(";");
        self.write_newline();
    }

    fn format_stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Item(item) => self.format_item(item),
            Stmt::Expr(es) => self.format_expr_stmt(es),
            Stmt::If(i) => self.format_if_stmt(i),
            Stmt::For(f) => self.format_for_stmt(f),
            Stmt::While(w) => self.format_while_stmt(w),
            Stmt::DoWhile(d) => self.format_do_while_stmt(d),
            Stmt::Loop(l) => {
                self.write_indent();
                self.write_str("loop ");
                self.format_block(&l.body);
                self.write_newline();
            }
            Stmt::Return(r) => {
                self.write_indent();
                self.write_str("return");
                if let Some(ref v) = &r.value {
                    self.write_space();
                    self.format_expr(v);
                }
                self.write_str(";");
                self.write_newline();
            }
            Stmt::Throw(t) => {
                self.write_indent();
                self.write_str("throw ");
                self.format_expr(&t.value);
                self.write_str(";");
                self.write_newline();
            }
            Stmt::Try(t) => self.format_try_stmt(t),
            Stmt::Break(_) => {
                self.write_line("break;");
            }
            Stmt::Continue(_) => {
                self.write_line("continue;");
            }
            Stmt::Parallel(p) => {
                self.write_indent();
                self.write_str("await parallel {");
                self.write_newline();
                self.indent();
                for r in &p.runs {
                    self.write_indent();
                    self.write_str("run ");
                    self.format_expr(&r.expr);
                    self.write_str(";");
                    self.write_newline();
                }
                self.dedent();
                self.write_line("}");
            }
            Stmt::Coro(c) => {
                self.write_indent();
                self.write_str("coro ");
                self.format_block(&c.body);
                self.write_newline();
            }
            Stmt::Select(s) => {
                self.write_indent();
                self.write_str("select {");
                self.write_newline();
                self.indent();
                for c in &s.cases {
                    self.write_indent();
                    self.write_str("case ");
                    self.write_str(&c.pattern.name);
                    self.write_str(" = ");
                    self.format_expr(&c.expr);
                    self.write_str(":");
                    self.write_newline();
                    self.indent();
                    for stmt in &c.body {
                        self.format_stmt(stmt);
                    }
                    self.dedent();
                }
                self.dedent();
                self.write_line("}");
            }
            Stmt::Unsafe(u) => {
                self.write_indent();
                self.write_str("unsafe ");
                self.format_block(&u.body);
                self.write_newline();
            }
            Stmt::Synchronized(s) => {
                self.write_indent();
                self.write_str("synchronized ");
                self.format_block(&s.body);
                self.write_newline();
            }
        }
    }

    fn format_if_stmt(&mut self, i: &IfStmt) {
        self.write_indent();
        self.write_str("if ");
        self.format_expr(&i.condition);
        self.write_space();
        self.format_block(&i.then_block);

        for ei in &i.else_ifs {
            self.write_indent();
            self.write_str("else if ");
            self.format_expr(&ei.condition);
            self.write_space();
            self.format_block(&ei.block);
        }

        if let Some(ref else_block) = &i.else_block {
            self.write_indent();
            self.write_str("else ");
            self.format_block(else_block);
        }
    }

    fn format_for_stmt(&mut self, f: &ForStmt) {
        self.write_indent();
        self.write_str("for ");
        self.write_str(&f.pattern.name);
        self.write_str(" in ");
        self.format_expr(&f.iterable);
        self.write_space();
        self.format_block(&f.body);
        self.write_newline();
    }

    fn format_while_stmt(&mut self, w: &WhileStmt) {
        self.write_indent();
        self.write_str("while ");
        self.format_expr(&w.condition);
        self.write_space();
        self.format_block(&w.body);
        self.write_newline();
    }

    fn format_do_while_stmt(&mut self, d: &DoWhileStmt) {
        self.write_indent();
        self.write_str("do ");
        self.format_block(&d.body);
        self.write_str(" while ");
        self.format_expr(&d.condition);
        self.write_str(";");
        self.write_newline();
    }

    fn format_try_stmt(&mut self, t: &TryStmt) {
        self.write_indent();
        self.write_str("try ");
        self.format_block(&t.body);
        self.write_newline();

        for c in &t.catches {
            self.write_indent();
            self.write_str("catch (");
            self.write_str(&c.param.name);
            if let Some(ref ty) = &c.type_ann {
                self.write_str(": ");
                self.format_type(ty);
            }
            self.write_str(") ");
            self.format_block(&c.body);
            self.write_newline();
        }

        if let Some(ref f) = &t.finally {
            self.write_indent();
            self.write_str("finally ");
            self.format_block(f);
            self.write_newline();
        }
    }

    fn format_block(&mut self, b: &Block) {
        self.write_str("{");
        self.write_newline();
        self.indent();
        for stmt in &b.stmts {
            self.format_stmt(stmt);
        }
        self.dedent();
        self.write_indent();
        self.write_str("}");
    }

    fn format_params(&mut self, params: &[Param]) {
        for (i, p) in params.iter().enumerate() {
            if i > 0 {
                self.write_str(", ");
            }
            self.write_str(&p.name.name);
            if let Some(ref t) = &p.type_ann {
                self.write_str(": ");
                self.format_type(t);
            }
            if let Some(ref v) = &p.default_value {
                self.write_str(" = ");
                self.format_expr(v);
            }
        }
    }

    fn format_type_params(&mut self, params: &[TypeParam]) {
        self.write_str("<");
        for (i, p) in params.iter().enumerate() {
            if i > 0 {
                self.write_str(", ");
            }
            self.write_str(&p.name.name);
            if let Some(ref c) = &p.constraint {
                self.write_str(": ");
                self.format_type(c);
            }
        }
        self.write_str(">");
    }

    fn format_modifiers(&mut self, mods: &[Modifier]) {
        for m in mods {
            match m {
                Modifier::Public => self.write_str("public "),
                Modifier::Private => self.write_str("private "),
                Modifier::Protected => self.write_str("protected "),
                Modifier::Readonly => self.write_str("readonly "),
                Modifier::Static => self.write_str("static "),
            }
        }
    }

    fn format_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Literal(l) => self.format_literal(l),
            Expr::Ident(i) => self.write_str(&i.name),
            Expr::Binary(b) => {
                self.format_expr(&b.left);
                self.write_space();
                self.write_str(self.binary_op_str(b.op));
                self.write_space();
                self.format_expr(&b.right);
            }
            Expr::Unary(u) => {
                self.write_str(self.unary_op_str(u.op));
                self.format_expr(&u.expr);
            }
            Expr::Call(c) => {
                self.format_expr(&c.callee);
                self.write_str("(");
                for (i, a) in c.args.iter().enumerate() {
                    if i > 0 {
                        self.write_str(", ");
                    }
                    if let Some(ref n) = &a.name {
                        self.write_str(&n.name);
                        self.write_str(": ");
                    }
                    self.format_expr(&a.value);
                }
                self.write_str(")");
            }
            Expr::Index(i) => {
                self.format_expr(&i.object);
                self.write_str("[");
                self.format_expr(&i.index);
                self.write_str("]");
            }
            Expr::Member(m) => {
                self.format_expr(&m.object);
                if m.optional {
                    self.write_str("?.");
                } else {
                    self.write_str(".");
                }
                self.write_str(&m.property.name);
            }
            Expr::Match(m) => {
                self.write_str("match ");
                self.format_expr(&m.scrutinee);
                self.write_str(" { ");
                for arm in &m.arms {
                    self.format_pattern(&arm.pattern);
                    self.write_str(" => ");
                    self.format_expr(&arm.body);
                    self.write_str(", ");
                }
                self.write_str("}");
            }
            Expr::Lambda(l) => {
                if l.is_async {
                    self.write_str("async ");
                }
                self.write_str("(");
                self.format_params(&l.params);
                self.write_str(")");
                if let Some(ref rt) = &l.return_type {
                    self.write_str(": ");
                    self.format_type(rt);
                }
                self.write_str(" => ");
                match &l.body {
                    LambdaBody::Expr(e) => self.format_expr(e),
                    LambdaBody::Block(b) => self.format_block(b),
                }
            }
            Expr::Array(a) => {
                self.write_str("[");
                for (i, e) in a.elements.iter().enumerate() {
                    if i > 0 {
                        self.write_str(", ");
                    }
                    self.format_expr(e);
                }
                self.write_str("]");
            }
            Expr::Object(o) => {
                self.write_str("{ ");
                for (i, f) in o.fields.iter().enumerate() {
                    if i > 0 {
                        self.write_str(", ");
                    }
                    match &f.key {
                        ObjectKey::Ident(id) => self.write_str(&id.name),
                        ObjectKey::String(s, _) => {
                            self.write_str("\"");
                            self.write_str(s);
                            self.write_str("\"");
                        }
                    }
                    self.write_str(": ");
                    self.format_expr(&f.value);
                }
                self.write_str(" }");
            }
            Expr::This(_) => self.write_str("this"),
            Expr::Dollar(_) => self.write_str("$"),
            Expr::DollarDollar(_) => self.write_str("$$"),
            Expr::Super(_) => self.write_str("super"),
            Expr::Parallel(_) => self.write_str("<parallel>"),
            Expr::Template(t) => {
                self.write_str("`");
                for part in &t.parts {
                    match part {
                        TemplatePart::Static(s) => self.write_str(s),
                        TemplatePart::Expr(e) => {
                            self.write_str("${");
                            self.format_expr(e);
                            self.write_str("}");
                        }
                    }
                }
                self.write_str("`");
            }
            Expr::Lazy(e) => {
                self.write_str("lazy ");
                self.format_expr(e);
            }
            Expr::New(n) => {
                self.write_str("new ");
                self.write_str(&n.type_name.name);
                self.write_str("(");
                for (i, a) in n.args.iter().enumerate() {
                    if i > 0 {
                        self.write_str(", ");
                    }
                    if let Some(ref name) = &a.name {
                        self.write_str(&name.name);
                        self.write_str(": ");
                    }
                    self.format_expr(&a.value);
                }
                self.write_str(")");
            }
            Expr::Ternary(t) => {
                self.format_expr(&t.condition);
                self.write_str(" ? ");
                self.format_expr(&t.then_expr);
                self.write_str(" : ");
                self.format_expr(&t.else_expr);
            }
            Expr::NullCoalesce(n) => {
                self.format_expr(&n.left);
                self.write_str(" ?? ");
                self.format_expr(&n.right);
            }
            Expr::Elvis(e) => {
                self.format_expr(&e.left);
                self.write_str(" ?: ");
                self.format_expr(&e.right);
            }
            Expr::Pipe(p) => {
                self.format_expr(&p.left);
                self.write_space();
                self.write_str(match p.op {
                    PipeOp::PipeRight => "|>",
                    PipeOp::PipeLeft => "<|",
                });
                self.write_space();
                self.format_expr(&p.right);
            }
            Expr::Assignment(a) => {
                self.format_expr(&a.target);
                self.write_space();
                self.write_str(self.assignment_op_str(a.op));
                self.write_space();
                self.format_expr(&a.value);
            }
            Expr::Postfix(p) => {
                self.format_expr(&p.object);
                match &p.op {
                    PostfixOp::Dot(id) => {
                        self.write_str(".");
                        self.write_str(&id.name);
                    }
                    PostfixOp::QuestionDot(id) => {
                        self.write_str("?.");
                        self.write_str(&id.name);
                    }
                    PostfixOp::Index(e) => {
                        self.write_str("[");
                        self.format_expr(e);
                        self.write_str("]");
                    }
                    PostfixOp::Call(args) => {
                        self.write_str("(");
                        for (i, a) in args.iter().enumerate() {
                            if i > 0 {
                                self.write_str(", ");
                            }
                            if let Some(ref n) = &a.name {
                                self.write_str(&n.name);
                                self.write_str(": ");
                            }
                            self.format_expr(&a.value);
                        }
                        self.write_str(")");
                    }
                    PostfixOp::Bang => self.write_str("!"),
                    PostfixOp::Question => self.write_str("?"),
                }
            }
            Expr::Group(e) => {
                self.write_str("(");
                self.format_expr(e);
                self.write_str(")");
            }
        }
    }

    fn format_literal(&mut self, l: &Literal) {
        match l {
            Literal::Int(v, _) => {
                let s = format!("{}", v);
                self.write_str(&s);
            }
            Literal::Float(v, _) => {
                let s = format!("{}", v);
                self.write_str(&s);
            }
            Literal::String(s, _) => {
                self.write_str("\"");
                self.write_str(s);
                self.write_str("\"");
            }
            Literal::Char(c, _) => {
                self.write_str("'");
                self.write_str(&c.to_string());
                self.write_str("'");
            }
            Literal::Bool(b, _) => {
                self.write_str(if *b { "true" } else { "false" });
            }
            Literal::Null(_) => self.write_str("null"),
        }
    }

    fn format_pattern(&mut self, p: &Pattern) {
        match p {
            Pattern::Literal(l) => self.format_literal(l),
            Pattern::Ident(i) => self.write_str(&i.name),
            Pattern::IsType(t) => {
                self.write_str("is ");
                self.format_type(t);
            }
            Pattern::Wildcard(_) => self.write_str("_"),
        }
    }

    fn format_type(&mut self, t: &Type) {
        match t {
            Type::Primitive(pt, _) => {
                self.write_str(primitive_type_str(*pt));
            }
            Type::Named(n) => {
                self.write_str(&n.name.name);
                if let Some(ref args) = &n.type_args {
                    self.write_str("<");
                    for (i, a) in args.iter().enumerate() {
                        if i > 0 {
                            self.write_str(", ");
                        }
                        self.format_type(a);
                    }
                    self.write_str(">");
                }
            }
            Type::Union(u) => {
                for (i, ty) in u.types.iter().enumerate() {
                    if i > 0 {
                        self.write_str(" | ");
                    }
                    self.format_type(ty);
                }
            }
            Type::Intersection(i) => {
                for (j, ty) in i.types.iter().enumerate() {
                    if j > 0 {
                        self.write_str(" & ");
                    }
                    self.format_type(ty);
                }
            }
            Type::List(l) => {
                self.write_str("list<");
                self.format_type(&l.element_type);
                self.write_str(">");
            }
            Type::Map(m) => {
                self.write_str("map<");
                self.format_type(&m.key_type);
                self.write_str(", ");
                self.format_type(&m.value_type);
                self.write_str(">");
            }
            Type::Tuple(tu) => {
                self.write_str("tuple<");
                for (i, ty) in tu.element_types.iter().enumerate() {
                    if i > 0 {
                        self.write_str(", ");
                    }
                    self.format_type(ty);
                }
                self.write_str(">");
            }
            Type::Result(r) => {
                self.write_str("Result<");
                self.format_type(&r.ok_type);
                self.write_str(", ");
                self.format_type(&r.err_type);
                self.write_str(">");
            }
            Type::Function(f) => {
                self.write_str("(");
                for (i, ty) in f.param_types.iter().enumerate() {
                    if i > 0 {
                        self.write_str(", ");
                    }
                    self.format_type(ty);
                }
                self.write_str(") => ");
                self.format_type(&f.return_type);
            }
        }
    }

    fn binary_op_str(&self, op: BinaryOp) -> &'static str {
        match op {
            BinaryOp::Add => "+",
            BinaryOp::Sub => "-",
            BinaryOp::Mul => "*",
            BinaryOp::Div => "/",
            BinaryOp::Mod => "%",
            BinaryOp::Pow => "**",
            BinaryOp::Eq => "==",
            BinaryOp::Ne => "!=",
            BinaryOp::Lt => "<",
            BinaryOp::Gt => ">",
            BinaryOp::Le => "<=",
            BinaryOp::Ge => ">=",
            BinaryOp::Spaceship => "<=>",
            BinaryOp::And => "&&",
            BinaryOp::Or => "||",
            BinaryOp::BitAnd => "&",
            BinaryOp::BitOr => "|",
            BinaryOp::BitXor => "^",
            BinaryOp::Shl => "<<",
            BinaryOp::Shr => ">>",
            BinaryOp::PipeRight => "|>",
            BinaryOp::PipeLeft => "<|",
            BinaryOp::NullCoalesce => "??",
            BinaryOp::Elvis => "?:",
            BinaryOp::Assign => "=",
            BinaryOp::AddAssign => "+=",
            BinaryOp::SubAssign => "-=",
            BinaryOp::MulAssign => "*=",
            BinaryOp::DivAssign => "/=",
            BinaryOp::ModAssign => "%=",
            BinaryOp::PowAssign => "**=",
            BinaryOp::ShlAssign => "<<=",
            BinaryOp::ShrAssign => ">>=",
            BinaryOp::BitAndAssign => "&=",
            BinaryOp::BitOrAssign => "|=",
            BinaryOp::BitXorAssign => "^=",
            BinaryOp::Range => "..",
            BinaryOp::RangeInclusive => "..=",
        }
    }

    fn unary_op_str(&self, op: UnaryOp) -> &'static str {
        match op {
            UnaryOp::Not => "!",
            UnaryOp::BitNot => "~",
            UnaryOp::Neg => "-",
            UnaryOp::Typeof => "typeof ",
            UnaryOp::New => "new ",
            UnaryOp::Await => "await ",
            UnaryOp::Lazy => "lazy ",
        }
    }

    fn assignment_op_str(&self, op: AssignmentOp) -> &'static str {
        match op {
            AssignmentOp::Assign => "=",
            AssignmentOp::AddAssign => "+=",
            AssignmentOp::SubAssign => "-=",
            AssignmentOp::MulAssign => "*=",
            AssignmentOp::DivAssign => "/=",
            AssignmentOp::ModAssign => "%=",
            AssignmentOp::PowAssign => "**=",
            AssignmentOp::ShlAssign => "<<=",
            AssignmentOp::ShrAssign => ">>=",
            AssignmentOp::BitAndAssign => "&=",
            AssignmentOp::BitOrAssign => "|=",
            AssignmentOp::BitXorAssign => "^=",
        }
    }
}

impl Default for Formatter {
    fn default() -> Self {
        Self::new()
    }
}

fn primitive_type_str(pt: PrimitiveType) -> &'static str {
    match pt {
        PrimitiveType::Int => "int",
        PrimitiveType::Uint => "uint",
        PrimitiveType::Float => "float",
        PrimitiveType::Bool => "bool",
        PrimitiveType::String => "string",
        PrimitiveType::Char => "char",
        PrimitiveType::Byte => "byte",
        PrimitiveType::Null => "null",
        PrimitiveType::Void => "void",
        PrimitiveType::Never => "never",
        PrimitiveType::Mixed => "mixed",
    }
}
