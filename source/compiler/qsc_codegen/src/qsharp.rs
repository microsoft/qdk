// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod spec_decls;

#[cfg(test)]
mod tests;

#[cfg(test)]
pub mod test_utils;

use std::io::Write;
use std::vec;

use qsc_ast::ast::{
    self, Attr, BinOp, Block, CallableBody, CallableDecl, CallableKind, Expr, ExprKind,
    FieldAccess, Functor, FunctorExpr, FunctorExprKind, Ident, Idents, ImportKind,
    ImportOrExportItem, Item, ItemKind, Lit, Mutability, Pat, PatKind, Path, PathKind, Pauli,
    QubitInit, QubitInitKind, QubitSource, SetOp, SpecBody, SpecDecl, SpecGen, Stmt, StmtKind,
    StringComponent, TernOp, TopLevelNode, Ty, TyDef, TyDefKind, TyKind, UnOp,
};
use qsc_ast::ast::{Namespace, Package};
use qsc_ast::visit::Visitor;
use qsc_formatter::formatter::format_str;
use qsc_frontend::compile::PackageStore;

fn write<W: Write>(output: W, packages: &[&Package]) {
    let mut r#gen = QSharpGen::new(output);
    for package in packages {
        r#gen.visit_package(package);
    }
}

pub fn write_store<W: Write>(output: W, store: &PackageStore) {
    let mut r#gen = QSharpGen::new(output);
    for (_, unit) in store {
        r#gen.visit_package(&unit.ast.package);
    }
}

#[must_use]
pub fn write_store_string(store: &PackageStore) -> Vec<String> {
    let mut package_strings: Vec<_> = vec![];
    for (_, unit) in store {
        package_strings.push(write_package_string(&unit.ast.package));
    }
    package_strings
}

#[must_use]
pub fn write_package_string(package: &Package) -> String {
    let mut output = Vec::new();
    write(&mut output, &[package]);
    let s = match std::str::from_utf8(&output) {
        Ok(v) => v.to_owned(),
        Err(e) => format!("Invalid UTF-8 sequence: {e}"),
    };

    output.clear();
    format_str(&s)
}

#[must_use]
pub fn write_stmt_string(stmt: &ast::Stmt) -> String {
    let mut output = Vec::new();
    let mut r#gen = QSharpGen::new(&mut output);
    r#gen.visit_stmt(stmt);
    let s = match std::str::from_utf8(&output) {
        Ok(v) => v.to_owned(),
        Err(e) => format!("Invalid UTF-8 sequence: {e}"),
    };

    output.clear();
    format_str(&s)
}

struct QSharpGen<W: Write> {
    pub(crate) output: W,
}

impl<W> QSharpGen<W>
where
    W: Write,
{
    pub fn new(output: W) -> Self {
        Self { output }
    }

    pub fn write(&mut self, args: &str) {
        write!(&mut self.output, "{args}").expect("write failed");
    }

    pub fn writeln(&mut self, args: &str) {
        self.write(args);
        self.write("\n");
    }

    /// special case for tuple with one element
    /// otherwise we are changing the semantics of the program
    fn ensure_trailing_comma_for_arity_one_tuples<T>(&mut self, most: &[T]) {
        if most.is_empty() {
            self.write(",");
        }
    }
}

impl<W: Write> Visitor<'_> for QSharpGen<W> {
    fn visit_package(&mut self, package: &'_ Package) {
        package.nodes.iter().for_each(|n| match n {
            TopLevelNode::Namespace(ns) => {
                self.visit_namespace(ns);
            }
            TopLevelNode::Stmt(stmt) => self.visit_stmt(stmt),
        });
        package.entry.iter().for_each(|e| self.visit_expr(e));
    }

    fn visit_namespace(&mut self, namespace: &'_ Namespace) {
        self.write("namespace ");
        self.visit_idents(&namespace.name);
        self.writeln("{");
        namespace.items.iter().for_each(|i| {
            self.visit_item(i);
        });
        self.write("}");
    }

    fn visit_item(&mut self, item: &'_ Item) {
        item.attrs.iter().for_each(|a| self.visit_attr(a));
        match &*item.kind {
            ItemKind::Err => {
                unreachable!()
            }
            ItemKind::Callable(decl) => self.visit_callable_decl(decl),
            ItemKind::Open(ns, alias) => {
                self.write("open ");
                self.visit_path_kind(ns);
                if let Some(alias) = alias {
                    self.write(" as ");
                    self.visit_ident(alias);
                }
                self.writeln(";");
            }
            ItemKind::Ty(ident, def) => {
                self.write("newtype ");
                self.visit_ident(ident);
                self.write(" = ");
                self.visit_ty_def(def);
                self.writeln(";");
            }
            ItemKind::Struct(decl) => self.visit_struct_decl(decl),
            ItemKind::ImportOrExport(decl) => {
                if decl.is_export() {
                    self.write("export ");
                } else {
                    self.write("import ");
                }

                for (
                    ix,
                    ImportOrExportItem {
                        span: _,
                        path,
                        kind,
                    },
                ) in decl.items.iter().enumerate()
                {
                    let is_last = ix == decl.items.len() - 1;
                    self.visit_path_kind(path);

                    if let ImportKind::Wildcard = kind {
                        self.write(".*");
                    }

                    if let ImportKind::Direct { alias: Some(alias) } = kind {
                        self.write(&format!(" as {}", alias.name));
                    }

                    if !is_last {
                        self.write(", ");
                    }
                }

                self.write(";");
            }
        }
    }

    fn visit_attr(&mut self, attr: &'_ Attr) {
        self.write("@");
        self.visit_ident(&attr.name);
        self.visit_expr(&attr.arg);
        self.writeln("");
    }

    fn visit_ty_def(&mut self, def: &'_ TyDef) {
        match &*def.kind {
            TyDefKind::Field(name, ty) => {
                if let Some(n) = name {
                    self.visit_ident(n);
                    self.write(": ");
                }
                self.visit_ty(ty);
            }
            TyDefKind::Paren(def) => self.visit_ty_def(def),
            TyDefKind::Tuple(defs) => {
                self.write("(");
                if let Some((last, most)) = defs.split_last() {
                    for i in most {
                        self.visit_ty_def(i);
                        self.write(", ");
                    }
                    self.visit_ty_def(last);
                    self.ensure_trailing_comma_for_arity_one_tuples(most);
                }
                self.write(")");
            }
            TyDefKind::Err => {}
        }
    }

    fn visit_callable_decl(&mut self, decl: &'_ CallableDecl) {
        match decl.kind {
            CallableKind::Function => self.write("function "),
            CallableKind::Operation => self.write("operation "),
        }
        self.visit_ident(&decl.name);
        if !decl.generics.is_empty() {
            self.write("<");
            if let Some((last, most)) = decl.generics.split_last() {
                for i in most {
                    self.visit_ident(&i.ty);
                    self.write(", ");
                }
                self.visit_ident(&last.ty);
            }

            self.write(">");
        }

        self.visit_pat(&decl.input);
        self.write(" : ");
        self.visit_ty(&decl.output);
        if let Some(functors) = decl.functors.as_deref() {
            self.write(" is ");
            self.visit_functor_expr(functors);
        }

        match &*decl.body {
            CallableBody::Block(block) => {
                self.visit_block(block);
            }
            CallableBody::Specs(specs) => {
                self.writeln("{");
                specs.iter().for_each(|s| self.visit_spec_decl(s));
                self.writeln("}");
            }
        }
    }

    fn visit_struct_decl(&mut self, decl: &'_ ast::StructDecl) {
        self.write("struct ");
        self.visit_ident(&decl.name);
        self.writeln(" {");
        if let Some((last, most)) = decl.fields.split_last() {
            for i in most {
                self.visit_field_def(i);
                self.write(", ");
            }
            self.visit_field_def(last);
        }
        self.writeln("}");
    }

    fn visit_field_def(&mut self, def: &'_ ast::FieldDef) {
        self.visit_ident(&def.name);
        self.write(" : ");
        self.visit_ty(&def.ty);
    }

    fn visit_spec_decl(&mut self, decl: &'_ SpecDecl) {
        match decl.spec {
            ast::Spec::Body => self.write("body "),
            ast::Spec::Adj => self.write("adjoint "),
            ast::Spec::Ctl => self.write("controlled "),
            ast::Spec::CtlAdj => self.write("controlled adjoint "),
        }
        match &decl.body {
            SpecBody::Gen(spec) => match spec {
                SpecGen::Auto => self.writeln("auto;"),
                SpecGen::Distribute => self.writeln("distribute;"),
                SpecGen::Intrinsic => self.writeln("intrinsic;"),
                SpecGen::Invert => self.writeln("invert;"),
                SpecGen::Slf => self.writeln("self;"),
            },
            SpecBody::Impl(pat, block) => {
                self.visit_pat(pat);
                self.visit_block(block);
            }
        }
    }

    fn visit_functor_expr(&mut self, expr: &'_ FunctorExpr) {
        match &*expr.kind {
            FunctorExprKind::BinOp(op, lhs, rhs) => {
                self.visit_functor_expr(lhs);
                match op {
                    SetOp::Union => self.write(" + "),
                    SetOp::Intersect => self.write(" * "),
                }
                self.visit_functor_expr(rhs);
            }
            FunctorExprKind::Lit(functor) => match functor {
                Functor::Adj => self.write("Adj"),
                Functor::Ctl => self.write("Ctl"),
            },
            FunctorExprKind::Paren(expr) => {
                self.write("(");
                self.visit_functor_expr(expr);
                self.write(")");
            }
        }
    }

    fn visit_ty(&mut self, ty: &'_ Ty) {
        match &*ty.kind {
            TyKind::Array(item) => {
                self.visit_ty(item);
                self.write("[]");
            }
            TyKind::Arrow(kind, lhs, rhs, functors) => {
                self.visit_ty(lhs);
                match kind {
                    CallableKind::Function => self.write(" -> "),
                    CallableKind::Operation => self.write(" => "),
                }
                self.visit_ty(rhs);
                if let Some(functors) = functors.as_deref() {
                    self.write(" is ");
                    self.visit_functor_expr(functors);
                }
            }
            TyKind::Hole => self.write("_"),
            TyKind::Paren(ty) => {
                self.write("(");
                self.visit_ty(ty);
                self.write(")");
            }
            TyKind::Path(path) => self.visit_path_kind(path),
            TyKind::Param(name) => self.visit_ident(&name.ty),
            TyKind::Tuple(tys) => {
                if tys.is_empty() {
                    self.write("()");
                } else {
                    self.write("(");
                    if let Some((last, most)) = tys.split_last() {
                        for t in most {
                            self.visit_ty(t);
                            self.write(", ");
                        }
                        self.visit_ty(last);
                        self.ensure_trailing_comma_for_arity_one_tuples(most);
                    }
                    self.write(")");
                }
            }
            TyKind::Err => unreachable!(),
        }
    }

    fn visit_block(&mut self, block: &'_ Block) {
        self.writeln(" {");
        block.stmts.iter().for_each(|s| {
            self.visit_stmt(s);
        });
        self.writeln("}");
    }

    fn visit_stmt(&mut self, stmt: &'_ Stmt) {
        match &*stmt.kind {
            StmtKind::Empty | StmtKind::Err => {}
            StmtKind::Semi(expr) => {
                self.visit_expr(expr);
                self.writeln(";");
            }
            StmtKind::Expr(expr) => {
                self.visit_expr(expr);
            }
            StmtKind::Item(item) => self.visit_item(item),
            StmtKind::Local(mutability, pat, value) => {
                match mutability {
                    Mutability::Mutable => self.write("mutable "),
                    Mutability::Immutable => self.write("let "),
                }
                self.visit_pat(pat);
                self.write(" = ");
                self.visit_expr(value);
                self.writeln(";");
            }
            StmtKind::Qubit(source, pat, init, block) => {
                match source {
                    QubitSource::Dirty => self.write("borrow "),
                    QubitSource::Fresh => self.write("use "),
                }
                self.visit_pat(pat);
                self.write(" = ");
                self.visit_qubit_init(init);
                if let Some(b) = block {
                    self.visit_block(b);
                } else {
                    self.writeln(";");
                }
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    fn visit_expr(&mut self, expr: &'_ Expr) {
        match &*expr.kind {
            ExprKind::Array(exprs) => {
                self.write("[");
                if let Some((last, most)) = exprs.split_last() {
                    for e in most {
                        self.visit_expr(e);
                        self.write(", ");
                    }
                    self.visit_expr(last);
                }
                self.write("]");
            }
            ExprKind::ArrayRepeat(item, size) => {
                self.write("[");
                self.visit_expr(item);
                self.write(", size = ");
                self.visit_expr(size);
                self.write("]");
            }
            ExprKind::Assign(lhs, rhs) => {
                self.write("set ");
                self.visit_expr(lhs);
                self.write(" = ");
                self.visit_expr(rhs);
            }
            ExprKind::AssignOp(op, lhs, rhs) => {
                self.write("set ");
                self.visit_expr(lhs);
                self.write(" ");
                let op_str = binop_as_str(op);
                self.write(op_str);
                self.write("= ");
                self.visit_expr(rhs);
            }
            ExprKind::BinOp(op, lhs, rhs) => {
                self.visit_expr(lhs);
                self.write(" ");
                let op_str = binop_as_str(op);
                self.write(op_str);
                self.write(" ");
                self.visit_expr(rhs);
            }
            ExprKind::AssignUpdate(record, index, value) => {
                self.write("set ");
                self.visit_expr(record);
                self.write(" w/= ");
                self.visit_expr(index);
                self.write(" <- ");
                self.visit_expr(value);
            }
            ExprKind::Block(block) => self.visit_block(block),
            ExprKind::Call(callee, arg) => {
                self.visit_expr(callee);
                self.visit_expr(arg);
            }
            ExprKind::Conjugate(within, apply) => {
                self.write("within");
                self.visit_block(within);
                self.write("apply");
                self.visit_block(apply);
            }
            ExprKind::Fail(msg) => {
                self.write("fail ");
                self.visit_expr(msg);
            }
            ExprKind::Field(record, ast::FieldAccess::Ok(name)) => {
                self.visit_expr(record);
                self.write(".");
                self.visit_ident(name);
            }
            ExprKind::For(pat, iter, block) => {
                self.write("for ");
                self.visit_pat(pat);
                self.write(" in ");
                self.visit_expr(iter);
                self.write(" ");
                self.visit_block(block);
            }
            ExprKind::If(cond, body, otherwise) => {
                self.write("if ");
                self.visit_expr(cond);
                self.write(" ");
                self.visit_block(body);
                if let Some(expr) = otherwise {
                    if matches!(*expr.kind, ExprKind::If(..)) {
                        // visiting expr as if writes 'if' to make 'elif'
                        self.write(" el");
                    } else {
                        self.write(" else ");
                    }
                    self.visit_expr(expr);
                }
            }
            ExprKind::Index(array, index) => {
                self.visit_expr(array);
                self.write("[");
                self.visit_expr(index);
                self.write("]");
            }
            ExprKind::Interpolate(components) => {
                self.write("$\"");
                for component in components.as_ref() {
                    match component {
                        StringComponent::Expr(expr) => {
                            self.write("{");
                            self.visit_expr(expr.as_ref());
                            self.write("}");
                        }
                        StringComponent::Lit(lit) => {
                            self.write(lit);
                        }
                    }
                }
                self.write("\"");
            }
            ExprKind::Lambda(kind, pat, expr) => {
                self.visit_pat(pat);
                match kind {
                    CallableKind::Function => self.write(" -> "),
                    CallableKind::Operation => self.write(" => "),
                }
                self.visit_expr(expr);
            }
            ExprKind::Paren(expr) => {
                self.write("(");
                self.visit_expr(expr);
                self.write(")");
            }
            ExprKind::Return(expr) => {
                self.write("return ");
                self.visit_expr(expr);
            }
            ExprKind::Struct(PathKind::Ok(path), copy, assigns) => {
                self.write("new ");
                self.visit_path(path);
                self.writeln(" {");
                if let Some(copy) = copy {
                    self.write("...");
                    self.visit_expr(copy);
                    if !assigns.is_empty() {
                        self.writeln(",");
                    }
                }
                if let Some((last, most)) = assigns.split_last() {
                    for assign in most {
                        self.visit_field_assign(assign);
                        self.writeln(",");
                    }
                    self.visit_field_assign(last);
                    self.writeln("");
                }
                self.writeln("}");
            }
            ExprKind::UnOp(op, expr) => {
                let op_str = unop_as_str(op);
                if op == &UnOp::Unwrap {
                    self.visit_expr(expr);
                    self.write(op_str);
                } else {
                    self.write(op_str);
                    self.visit_expr(expr);
                }
            }
            ExprKind::Path(PathKind::Ok(path)) => self.visit_path(path),
            ExprKind::Range(start, step, end) => {
                // A range: `start..step..end`, `start..end`, `start...`, `...end`, or `...`.
                match (start, step, end) {
                    (None, None, None) => {
                        self.write("...");
                    }
                    (None, None, Some(end)) => {
                        self.write("...");
                        self.visit_expr(end);
                    }
                    (None, Some(step), None) => {
                        self.write("...");
                        self.visit_expr(step);
                        self.write("...");
                    }
                    (None, Some(step), Some(end)) => {
                        self.write("...");
                        self.visit_expr(step);
                        self.write("..");
                        self.visit_expr(end);
                    }
                    (Some(start), None, None) => {
                        self.visit_expr(start);
                        self.write("...");
                    }
                    (Some(start), None, Some(end)) => {
                        self.visit_expr(start);
                        self.write("..");
                        self.visit_expr(end);
                    }
                    (Some(start), Some(step), None) => {
                        self.visit_expr(start);
                        self.write("..");
                        self.visit_expr(step);
                        self.write("...");
                    }
                    (Some(start), Some(step), Some(end)) => {
                        self.visit_expr(start);
                        self.write("..");
                        self.visit_expr(step);
                        self.write("..");
                        self.visit_expr(end);
                    }
                }
            }
            ExprKind::Repeat(body, until, fixup) => {
                self.write("repeat ");
                self.visit_block(body);
                self.write("until ");
                self.visit_expr(until);
                if let Some(fixup) = fixup {
                    self.write(" fixup ");
                    self.visit_block(fixup);
                }
            }
            ExprKind::TernOp(op, e1, e2, e3) => {
                match op {
                    TernOp::Cond => {
                        // Conditional: `a ? b | c`.
                        self.visit_expr(e1);
                        self.write(" ? ");
                        self.visit_expr(e2);
                        self.write(" | ");
                        self.visit_expr(e3);
                    }
                    TernOp::Update => {
                        // Aggregate update: `a w/ b <- c`.
                        self.visit_expr(e1);
                        self.write(" w/ ");
                        self.visit_expr(e2);
                        self.write(" <- ");
                        self.visit_expr(e3);
                    }
                }
            }
            ExprKind::Tuple(exprs) => {
                self.write("(");
                if let Some((last, most)) = exprs.split_last() {
                    for e in most {
                        self.visit_expr(e);
                        self.write(", ");
                    }
                    self.visit_expr(last);
                    self.ensure_trailing_comma_for_arity_one_tuples(most);
                }
                self.write(")");
            }
            ExprKind::While(cond, block) => {
                self.write("while ");
                self.visit_expr(cond);
                self.visit_block(block);
            }
            ExprKind::Lit(lit) => match lit.as_ref() {
                Lit::BigInt(value) => {
                    self.write(value.to_string().as_str());
                    self.write("L");
                }
                Lit::Bool(value) => {
                    if *value {
                        self.write("true");
                    } else {
                        self.write("false");
                    }
                }
                Lit::Double(value) => {
                    let num_str = if value.fract() == 0.0 {
                        format!("{value}.")
                    } else {
                        format!("{value}")
                    };
                    self.write(&num_str);
                }
                Lit::Imaginary(value) => {
                    let num_str = if value.fract() == 0.0 {
                        format!("{value}.i")
                    } else {
                        format!("{value}i")
                    };
                    self.write(&num_str);
                }
                Lit::Int(value) => self.write(&value.to_string()),
                Lit::Pauli(value) => match value {
                    Pauli::I => self.write("PauliI"),
                    Pauli::X => self.write("PauliX"),
                    Pauli::Y => self.write("PauliY"),
                    Pauli::Z => self.write("PauliZ"),
                },
                Lit::Result(value) => match value {
                    ast::Result::One => self.write("One"),
                    ast::Result::Zero => self.write("Zero"),
                },
                Lit::String(value) => {
                    self.write("\"");
                    self.write(value.as_ref());
                    self.write("\"");
                }
            },
            ExprKind::Hole => {
                self.write("_");
            }
            ExprKind::Err => {}
            ExprKind::Path(PathKind::Err(_))
            | ExprKind::Struct(PathKind::Err(_), ..)
            | ExprKind::Field(_, FieldAccess::Err) => {
                unreachable!();
            }
        }
    }

    fn visit_field_assign(&mut self, assign: &'_ ast::FieldAssign) {
        self.visit_ident(&assign.field);
        self.write(" = ");
        self.visit_expr(&assign.value);
    }

    fn visit_pat(&mut self, pat: &'_ Pat) {
        match &*pat.kind {
            PatKind::Bind(name, ty) => {
                self.visit_ident(name);

                if let Some(t) = ty {
                    self.write(": ");
                    self.visit_ty(t);
                }
            }
            PatKind::Discard(ty) => {
                self.write("_");
                if let Some(t) = ty {
                    self.write(": ");
                    self.visit_ty(t);
                }
            }
            PatKind::Elided => {
                self.write("...");
            }
            PatKind::Paren(pat) => {
                self.write("(");
                self.visit_pat(pat);
                self.write(")");
            }
            PatKind::Tuple(pats) => {
                self.write("(");
                if let Some((last, most)) = pats.split_last() {
                    for pat in most {
                        self.visit_pat(pat);
                        self.write(", ");
                    }
                    self.visit_pat(last);
                    self.ensure_trailing_comma_for_arity_one_tuples(most);
                }
                self.write(")");
            }
            PatKind::Err => {
                unreachable!();
            }
        }
    }

    fn visit_qubit_init(&mut self, init: &'_ QubitInit) {
        match &*init.kind {
            QubitInitKind::Array(len) => {
                self.write("Qubit[");
                self.visit_expr(len);
                self.write("]");
            }
            QubitInitKind::Paren(init) => self.visit_qubit_init(init),
            QubitInitKind::Single => {
                self.write("Qubit()");
            }
            QubitInitKind::Tuple(inits) => {
                self.write("(");
                if let Some((last, most)) = inits.split_last() {
                    for init in most {
                        self.visit_qubit_init(init);
                        self.write(", ");
                    }
                    self.visit_qubit_init(last);
                    self.ensure_trailing_comma_for_arity_one_tuples(most);
                }
                self.write(")");
            }
            QubitInitKind::Err => unreachable!(),
        }
    }

    fn visit_path(&mut self, path: &'_ Path) {
        if let Some(parts) = &path.segments {
            self.visit_idents(parts);
            self.write(".");
        }
        self.visit_ident(&path.name);
    }

    fn visit_ident(&mut self, id: &'_ Ident) {
        self.write(&id.name);
    }

    fn visit_idents(&mut self, idents: &'_ [Ident]) {
        self.write(&idents.full_name());
    }
}

fn binop_as_str(op: &BinOp) -> &str {
    match op {
        BinOp::Add => "+",
        BinOp::AndB => "&&&",
        BinOp::AndL => "and",
        BinOp::Div => "/",
        BinOp::Eq => "==",
        BinOp::Exp => "^",
        BinOp::Gt => ">",
        BinOp::Gte => ">=",
        BinOp::Lt => "<",
        BinOp::Lte => "<=",
        BinOp::Mod => "%",
        BinOp::Mul => "*",
        BinOp::Neq => "!=",
        BinOp::OrB => "|||",
        BinOp::OrL => "or",
        BinOp::Shl => "<<<",
        BinOp::Shr => ">>>",
        BinOp::Sub => "-",
        BinOp::XorB => "^^^",
    }
}

fn unop_as_str(op: &UnOp) -> &str {
    match op {
        UnOp::Functor(functor) => match functor {
            Functor::Adj => "Adjoint ",
            Functor::Ctl => "Controlled ",
        },
        UnOp::Neg => "-",
        UnOp::NotB => "~~~",
        UnOp::NotL => "not ",
        UnOp::Pos => "+",
        UnOp::Unwrap => "!",
    }
}
