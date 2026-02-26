// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::ast::{
    Attr, Block, CallableBody, CallableDecl, Expr, ExprKind, FieldAccess, FieldAssign, FieldDef,
    FunctorExpr, FunctorExprKind, Ident, ImportKind, ImportOrExportItem, Item, ItemKind, Namespace,
    Package, Pat, PatKind, Path, PathKind, QubitInit, QubitInitKind, SpecBody, SpecDecl, Stmt,
    StmtKind, StringComponent, StructDecl, TopLevelNode, Ty, TyDef, TyDefKind, TyKind,
    TypeParameter,
};

pub trait Visitor<'a>: Sized {
    fn visit_package(&mut self, package: &'a Package) {
        walk_package(self, package);
    }

    fn visit_namespace(&mut self, namespace: &'a Namespace) {
        walk_namespace(self, namespace);
    }

    fn visit_item(&mut self, item: &'a Item) {
        walk_item(self, item);
    }

    fn visit_attr(&mut self, attr: &'a Attr) {
        walk_attr(self, attr);
    }

    fn visit_ty_def(&mut self, def: &'a TyDef) {
        walk_ty_def(self, def);
    }

    fn visit_callable_decl(&mut self, decl: &'a CallableDecl) {
        walk_callable_decl(self, decl);
    }

    fn visit_struct_decl(&mut self, decl: &'a StructDecl) {
        walk_struct_decl(self, decl);
    }

    fn visit_import_or_export(&mut self, item: &'a ImportOrExportItem) {
        walk_import_or_export(self, item);
    }

    fn visit_field_def(&mut self, def: &'a FieldDef) {
        walk_field_def(self, def);
    }

    fn visit_spec_decl(&mut self, decl: &'a SpecDecl) {
        walk_spec_decl(self, decl);
    }

    fn visit_functor_expr(&mut self, expr: &'a FunctorExpr) {
        walk_functor_expr(self, expr);
    }

    fn visit_ty(&mut self, ty: &'a Ty) {
        walk_ty(self, ty);
    }

    fn visit_block(&mut self, block: &'a Block) {
        walk_block(self, block);
    }

    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        walk_expr(self, expr);
    }

    fn visit_field_assign(&mut self, assign: &'a FieldAssign) {
        walk_field_assign(self, assign);
    }

    fn visit_pat(&mut self, pat: &'a Pat) {
        walk_pat(self, pat);
    }

    fn visit_qubit_init(&mut self, init: &'a QubitInit) {
        walk_qubit_init(self, init);
    }

    fn visit_path(&mut self, path: &'a Path) {
        walk_path(self, path);
    }

    fn visit_path_kind(&mut self, path: &'a PathKind) {
        walk_path_kind(self, path);
    }

    fn visit_ident(&mut self, _: &'a Ident) {}

    fn visit_idents(&mut self, idents: &'a [Ident]) {
        walk_idents(self, idents);
    }
}

pub fn walk_package<'a>(vis: &mut impl Visitor<'a>, package: &'a Package) {
    package.nodes.iter().for_each(|n| match n {
        TopLevelNode::Namespace(ns) => vis.visit_namespace(ns),
        TopLevelNode::Stmt(stmt) => vis.visit_stmt(stmt),
    });
    package.entry.iter().for_each(|e| vis.visit_expr(e));
}

pub fn walk_namespace<'a>(vis: &mut impl Visitor<'a>, namespace: &'a Namespace) {
    vis.visit_idents(&namespace.name);
    namespace.items.iter().for_each(|i| vis.visit_item(i));
}

pub fn walk_item<'a>(vis: &mut impl Visitor<'a>, item: &'a Item) {
    item.attrs.iter().for_each(|a| vis.visit_attr(a));
    match &*item.kind {
        ItemKind::Err => {}
        ItemKind::Callable(decl) => vis.visit_callable_decl(decl),
        ItemKind::Open(ns, alias) => {
            vis.visit_path_kind(ns);
            if let Some(a) = alias.as_ref() {
                vis.visit_ident(a);
            }
        }
        ItemKind::Ty(ident, def) => {
            vis.visit_ident(ident);
            vis.visit_ty_def(def);
        }
        ItemKind::Struct(decl) => vis.visit_struct_decl(decl),
        ItemKind::ImportOrExport(decl) => {
            decl.items
                .iter()
                .for_each(|item| vis.visit_import_or_export(item));
        }
    }
}
pub fn walk_attr<'a>(vis: &mut impl Visitor<'a>, attr: &'a Attr) {
    vis.visit_ident(&attr.name);
    vis.visit_expr(&attr.arg);
}

pub fn walk_ty_def<'a>(vis: &mut impl Visitor<'a>, def: &'a TyDef) {
    match &*def.kind {
        TyDefKind::Field(name, ty, _) => {
            if let Some(n) = name.as_ref() {
                vis.visit_ident(n);
            }
            vis.visit_ty(ty);
        }
        TyDefKind::Paren(def) => vis.visit_ty_def(def),
        TyDefKind::Tuple(defs) => defs.iter().for_each(|d| vis.visit_ty_def(d)),
        TyDefKind::Err => {}
    }
}

pub fn walk_callable_decl<'a>(vis: &mut impl Visitor<'a>, decl: &'a CallableDecl) {
    vis.visit_ident(&decl.name);
    decl.generics.iter().for_each(|p| {
        vis.visit_ident(&p.ty);
        p.constraints.0.iter().for_each(|b| {
            vis.visit_ident(&b.name);
            b.parameters
                .iter()
                .for_each(|crate::ast::ConstraintParameter { ty, .. }| {
                    vis.visit_ty(ty);
                });
        });
    });
    vis.visit_pat(&decl.input);
    vis.visit_ty(&decl.output);
    decl.functors.iter().for_each(|f| vis.visit_functor_expr(f));
    match &*decl.body {
        CallableBody::Block(block) => vis.visit_block(block),
        CallableBody::Specs(specs) => specs.iter().for_each(|s| vis.visit_spec_decl(s)),
    }
}

pub fn walk_struct_decl<'a>(vis: &mut impl Visitor<'a>, decl: &'a StructDecl) {
    vis.visit_ident(&decl.name);
    decl.fields.iter().for_each(|f| vis.visit_field_def(f));
}

pub fn walk_import_or_export<'a>(vis: &mut impl Visitor<'a>, item: &'a ImportOrExportItem) {
    vis.visit_path_kind(&item.path);
    if let ImportKind::Direct {
        alias: Some(ref alias),
    } = item.kind
    {
        vis.visit_ident(alias);
    }
}

pub fn walk_field_def<'a>(vis: &mut impl Visitor<'a>, def: &'a FieldDef) {
    vis.visit_ident(&def.name);
    vis.visit_ty(&def.ty);
}

pub fn walk_spec_decl<'a>(vis: &mut impl Visitor<'a>, decl: &'a SpecDecl) {
    match &decl.body {
        SpecBody::Gen(_) => {}
        SpecBody::Impl(pat, block) => {
            vis.visit_pat(pat);
            vis.visit_block(block);
        }
    }
}

pub fn walk_functor_expr<'a>(vis: &mut impl Visitor<'a>, expr: &'a FunctorExpr) {
    match &*expr.kind {
        FunctorExprKind::BinOp(_, lhs, rhs) => {
            vis.visit_functor_expr(lhs);
            vis.visit_functor_expr(rhs);
        }
        FunctorExprKind::Lit(_) => {}
        FunctorExprKind::Paren(expr) => vis.visit_functor_expr(expr),
    }
}

pub fn walk_ty<'a>(vis: &mut impl Visitor<'a>, ty: &'a Ty) {
    match &*ty.kind {
        TyKind::Array(item) => vis.visit_ty(item),
        TyKind::Arrow(_, lhs, rhs, functors) => {
            vis.visit_ty(lhs);
            vis.visit_ty(rhs);
            if let Some(f) = functors.as_ref() {
                vis.visit_functor_expr(f);
            }
        }
        TyKind::Hole | TyKind::Err => {}
        TyKind::Paren(ty) => vis.visit_ty(ty),
        TyKind::Path(path) => vis.visit_path_kind(path),
        TyKind::Param(TypeParameter {
            ty,
            constraints: bounds,
            ..
        }) => {
            for bound in &bounds.0 {
                vis.visit_ident(&bound.name);

                bound.parameters.iter().for_each(
                    |crate::ast::ConstraintParameter { ty, .. }| {
                        vis.visit_ty(ty);
                    },
                );
            }
            vis.visit_ident(ty);
        }
        TyKind::Tuple(tys) => tys.iter().for_each(|t| vis.visit_ty(t)),
    }
}

pub fn walk_block<'a>(vis: &mut impl Visitor<'a>, block: &'a Block) {
    block.stmts.iter().for_each(|s| vis.visit_stmt(s));
}

pub fn walk_stmt<'a>(vis: &mut impl Visitor<'a>, stmt: &'a Stmt) {
    match &*stmt.kind {
        StmtKind::Empty | StmtKind::Err => {}
        StmtKind::Expr(expr) | StmtKind::Semi(expr) => vis.visit_expr(expr),
        StmtKind::Item(item) => vis.visit_item(item),
        StmtKind::Local(_, pat, value) => {
            vis.visit_pat(pat);
            vis.visit_expr(value);
        }
        StmtKind::Qubit(_, pat, init, block) => {
            vis.visit_pat(pat);
            vis.visit_qubit_init(init);
            if let Some(b) = block.as_ref() {
                vis.visit_block(b);
            }
        }
    }
}

#[allow(clippy::too_many_lines)]
pub fn walk_expr<'a>(vis: &mut impl Visitor<'a>, expr: &'a Expr) {
    match &*expr.kind {
        ExprKind::Array(exprs) => exprs.iter().for_each(|e| vis.visit_expr(e)),
        ExprKind::ArrayRepeat(item, size) => {
            vis.visit_expr(item);
            vis.visit_expr(size);
        }
        ExprKind::Assign(lhs, rhs)
        | ExprKind::AssignOp(_, lhs, rhs)
        | ExprKind::BinOp(_, lhs, rhs) => {
            vis.visit_expr(lhs);
            vis.visit_expr(rhs);
        }
        ExprKind::AssignUpdate(record, index, value) => {
            vis.visit_expr(record);
            vis.visit_expr(index);
            vis.visit_expr(value);
        }
        ExprKind::Block(block) => vis.visit_block(block),
        ExprKind::Call(callee, arg) => {
            vis.visit_expr(callee);
            vis.visit_expr(arg);
        }
        ExprKind::Conjugate(within, apply) => {
            vis.visit_block(within);
            vis.visit_block(apply);
        }
        ExprKind::Fail(msg) => vis.visit_expr(msg),
        ExprKind::Field(record, name) => {
            vis.visit_expr(record);
            if let FieldAccess::Ok(name) = name {
                vis.visit_ident(name);
            }
        }
        ExprKind::For(pat, iter, block) => {
            vis.visit_pat(pat);
            vis.visit_expr(iter);
            vis.visit_block(block);
        }
        ExprKind::If(cond, body, otherwise) => {
            vis.visit_expr(cond);
            vis.visit_block(body);
            if let Some(e) = otherwise.as_ref() {
                vis.visit_expr(e);
            }
        }
        ExprKind::Index(array, index) => {
            vis.visit_expr(array);
            vis.visit_expr(index);
        }
        ExprKind::Interpolate(components) => {
            for component in components.as_ref() {
                match component {
                    StringComponent::Expr(expr) => vis.visit_expr(expr.as_ref()),
                    StringComponent::Lit(_) => {}
                }
            }
        }
        ExprKind::Lambda(_, pat, expr) => {
            vis.visit_pat(pat);
            vis.visit_expr(expr);
        }
        ExprKind::Paren(expr) | ExprKind::Return(expr) | ExprKind::UnOp(_, expr) => {
            vis.visit_expr(expr);
        }
        ExprKind::Path(path) => vis.visit_path_kind(path),
        ExprKind::Range(start, step, end) => {
            if let Some(s) = start.as_ref() {
                vis.visit_expr(s);
            }
            if let Some(s) = step.as_ref() {
                vis.visit_expr(s);
            }
            if let Some(e) = end.as_ref() {
                vis.visit_expr(e);
            }
        }
        ExprKind::Repeat(body, until, fixup) => {
            vis.visit_block(body);
            vis.visit_expr(until);
            if let Some(f) = fixup.as_ref() {
                vis.visit_block(f);
            }
        }
        ExprKind::Struct(name, copy, fields) => {
            vis.visit_path_kind(name);
            if let Some(c) = copy.as_ref() {
                vis.visit_expr(c);
            }
            fields.iter().for_each(|f| vis.visit_field_assign(f));
        }
        ExprKind::TernOp(_, e1, e2, e3) => {
            vis.visit_expr(e1);
            vis.visit_expr(e2);
            vis.visit_expr(e3);
        }
        ExprKind::Tuple(exprs) => exprs.iter().for_each(|e| vis.visit_expr(e)),
        ExprKind::While(cond, block) => {
            vis.visit_expr(cond);
            vis.visit_block(block);
        }
        ExprKind::Err | ExprKind::Hole | ExprKind::Lit(_) => {}
    }
}

pub fn walk_field_assign<'a>(vis: &mut impl Visitor<'a>, assign: &'a FieldAssign) {
    vis.visit_ident(&assign.field);
    vis.visit_expr(&assign.value);
}

pub fn walk_pat<'a>(vis: &mut impl Visitor<'a>, pat: &'a Pat) {
    match &*pat.kind {
        PatKind::Bind(name, ty) => {
            vis.visit_ident(name);
            if let Some(t) = ty.as_ref() {
                vis.visit_ty(t);
            }
        }
        PatKind::Discard(ty) => ty.iter().for_each(|t| vis.visit_ty(t)),
        PatKind::Elided | PatKind::Err => {}
        PatKind::Paren(pat) => vis.visit_pat(pat),
        PatKind::Tuple(pats) => pats.iter().for_each(|p| vis.visit_pat(p)),
    }
}

pub fn walk_qubit_init<'a>(vis: &mut impl Visitor<'a>, init: &'a QubitInit) {
    match &*init.kind {
        QubitInitKind::Array(len) => vis.visit_expr(len),
        QubitInitKind::Paren(init) => vis.visit_qubit_init(init),
        QubitInitKind::Single | QubitInitKind::Err => {}
        QubitInitKind::Tuple(inits) => inits.iter().for_each(|i| vis.visit_qubit_init(i)),
    }
}

pub fn walk_path<'a>(vis: &mut impl Visitor<'a>, path: &'a Path) {
    if let Some(ref parts) = path.segments {
        vis.visit_idents(parts);
    }
    vis.visit_ident(&path.name);
}

pub fn walk_path_kind<'a>(vis: &mut impl Visitor<'a>, path: &'a PathKind) {
    match path {
        PathKind::Ok(path) => vis.visit_path(path),
        PathKind::Err(Some(incomplete_path)) => {
            vis.visit_idents(&incomplete_path.segments);
        }
        PathKind::Err(None) => {}
    }
}

pub fn walk_idents<'a>(vis: &mut impl Visitor<'a>, idents: &'a [Ident]) {
    for i in idents {
        vis.visit_ident(i);
    }
}
