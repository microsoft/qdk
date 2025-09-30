// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::ast::{
    Attr, Block, CallableBody, CallableDecl, Expr, ExprKind, FieldAccess, FieldAssign, FieldDef,
    FunctorExpr, FunctorExprKind, Ident, ImportKind, ImportOrExportItem, Item, ItemKind, Namespace,
    Package, Pat, PatKind, Path, PathKind, QubitInit, QubitInitKind, SpecBody, SpecDecl, Stmt,
    StmtKind, StringComponent, StructDecl, TopLevelNode, Ty, TyDef, TyDefKind, TyKind,
    TypeParameter,
};
use qsc_data_structures::span::Span;

pub trait MutVisitor: Sized {
    fn visit_package(&mut self, package: &mut Package) {
        walk_package(self, package);
    }

    fn visit_namespace(&mut self, namespace: &mut Namespace) {
        walk_namespace(self, namespace);
    }

    fn visit_item(&mut self, item: &mut Item) {
        walk_item(self, item);
    }

    fn visit_attr(&mut self, attr: &mut Attr) {
        walk_attr(self, attr);
    }

    fn visit_ty_def(&mut self, def: &mut TyDef) {
        walk_ty_def(self, def);
    }

    fn visit_callable_decl(&mut self, decl: &mut CallableDecl) {
        walk_callable_decl(self, decl);
    }

    fn visit_struct_decl(&mut self, decl: &mut StructDecl) {
        walk_struct_decl(self, decl);
    }

    fn visit_import_or_export(&mut self, item: &mut ImportOrExportItem) {
        walk_import_or_export(self, item);
    }

    fn visit_field_def(&mut self, def: &mut FieldDef) {
        walk_field_def(self, def);
    }

    fn visit_spec_decl(&mut self, decl: &mut SpecDecl) {
        walk_spec_decl(self, decl);
    }

    fn visit_functor_expr(&mut self, expr: &mut FunctorExpr) {
        walk_functor_expr(self, expr);
    }

    fn visit_ty(&mut self, ty: &mut Ty) {
        walk_ty(self, ty);
    }

    fn visit_block(&mut self, block: &mut Block) {
        walk_block(self, block);
    }

    fn visit_stmt(&mut self, stmt: &mut Stmt) {
        walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &mut Expr) {
        walk_expr(self, expr);
    }

    fn visit_field_assign(&mut self, assign: &mut FieldAssign) {
        walk_field_assign(self, assign);
    }

    fn visit_pat(&mut self, pat: &mut Pat) {
        walk_pat(self, pat);
    }

    fn visit_qubit_init(&mut self, init: &mut QubitInit) {
        walk_qubit_init(self, init);
    }

    fn visit_path(&mut self, path: &mut Path) {
        walk_path(self, path);
    }

    fn visit_path_kind(&mut self, path: &mut PathKind) {
        walk_path_kind(self, path);
    }

    fn visit_ident(&mut self, ident: &mut Ident) {
        walk_ident(self, ident);
    }

    fn visit_idents(&mut self, ident: &mut [Ident]) {
        walk_idents(self, ident);
    }

    fn visit_span(&mut self, _: &mut Span) {}
}

pub fn walk_package(vis: &mut impl MutVisitor, package: &mut Package) {
    package.nodes.iter_mut().for_each(|n| match n {
        TopLevelNode::Namespace(ns) => vis.visit_namespace(ns),
        TopLevelNode::Stmt(stmt) => vis.visit_stmt(stmt),
    });
    package.entry.iter_mut().for_each(|e| vis.visit_expr(e));
}

pub fn walk_namespace(vis: &mut impl MutVisitor, namespace: &mut Namespace) {
    vis.visit_span(&mut namespace.span);
    vis.visit_idents(&mut namespace.name);

    namespace.items.iter_mut().for_each(|i| vis.visit_item(i));
}

pub fn walk_item(vis: &mut impl MutVisitor, item: &mut Item) {
    vis.visit_span(&mut item.span);
    item.attrs.iter_mut().for_each(|a| vis.visit_attr(a));

    match &mut *item.kind {
        ItemKind::Callable(decl) => vis.visit_callable_decl(decl),
        ItemKind::Err => {}
        ItemKind::Open(ns, alias) => {
            vis.visit_path_kind(ns);
            alias.iter_mut().for_each(|a| vis.visit_ident(a));
        }
        ItemKind::Ty(ident, def) => {
            vis.visit_ident(ident);
            vis.visit_ty_def(def);
        }
        ItemKind::Struct(decl) => vis.visit_struct_decl(decl),
        ItemKind::ImportOrExport(decl) => {
            vis.visit_span(&mut decl.span);
            decl.items
                .iter_mut()
                .for_each(|item| vis.visit_import_or_export(item));
        }
    }
}

pub fn walk_attr(vis: &mut impl MutVisitor, attr: &mut Attr) {
    vis.visit_span(&mut attr.span);
    vis.visit_ident(&mut attr.name);
    vis.visit_expr(&mut attr.arg);
}

pub fn walk_ty_def(vis: &mut impl MutVisitor, def: &mut TyDef) {
    vis.visit_span(&mut def.span);

    match &mut *def.kind {
        TyDefKind::Field(name, ty) => {
            name.iter_mut().for_each(|n| vis.visit_ident(n));
            vis.visit_ty(ty);
        }
        TyDefKind::Paren(def) => vis.visit_ty_def(def),
        TyDefKind::Tuple(defs) => defs.iter_mut().for_each(|d| vis.visit_ty_def(d)),
        TyDefKind::Err => {}
    }
}

pub fn walk_callable_decl(vis: &mut impl MutVisitor, decl: &mut CallableDecl) {
    vis.visit_span(&mut decl.span);
    vis.visit_ident(&mut decl.name);
    decl.generics.iter_mut().for_each(|p| {
        vis.visit_ident(&mut p.ty);
        p.constraints.0.iter_mut().for_each(|b| {
            vis.visit_ident(&mut b.name);
            b.parameters
                .iter_mut()
                .for_each(|crate::ast::ConstraintParameter { ty, .. }| {
                    vis.visit_ty(ty);
                });
        });
    });
    vis.visit_pat(&mut decl.input);
    vis.visit_ty(&mut decl.output);
    decl.functors
        .iter_mut()
        .for_each(|f| vis.visit_functor_expr(f));

    match &mut *decl.body {
        CallableBody::Block(block) => vis.visit_block(block),
        CallableBody::Specs(specs) => specs.iter_mut().for_each(|s| vis.visit_spec_decl(s)),
    }
}

pub fn walk_struct_decl(vis: &mut impl MutVisitor, decl: &mut StructDecl) {
    vis.visit_span(&mut decl.span);
    vis.visit_ident(&mut decl.name);
    decl.fields.iter_mut().for_each(|f| vis.visit_field_def(f));
}

pub fn walk_import_or_export(vis: &mut impl MutVisitor, item: &mut ImportOrExportItem) {
    vis.visit_span(&mut item.span);
    vis.visit_path_kind(&mut item.path);
    if let ImportKind::Direct { alias: Some(alias) } = &mut item.kind {
        vis.visit_ident(alias);
    }
}

pub fn walk_field_def(vis: &mut impl MutVisitor, def: &mut FieldDef) {
    vis.visit_span(&mut def.span);
    vis.visit_ident(&mut def.name);
    vis.visit_ty(&mut def.ty);
}

pub fn walk_spec_decl(vis: &mut impl MutVisitor, decl: &mut SpecDecl) {
    vis.visit_span(&mut decl.span);

    match &mut decl.body {
        SpecBody::Gen(_) => {}
        SpecBody::Impl(pat, block) => {
            vis.visit_pat(pat);
            vis.visit_block(block);
        }
    }
}

pub fn walk_functor_expr(vis: &mut impl MutVisitor, expr: &mut FunctorExpr) {
    vis.visit_span(&mut expr.span);

    match &mut *expr.kind {
        FunctorExprKind::BinOp(_, lhs, rhs) => {
            vis.visit_functor_expr(lhs);
            vis.visit_functor_expr(rhs);
        }
        FunctorExprKind::Lit(_) => {}
        FunctorExprKind::Paren(expr) => vis.visit_functor_expr(expr),
    }
}

pub fn walk_ty(vis: &mut impl MutVisitor, ty: &mut Ty) {
    vis.visit_span(&mut ty.span);

    match &mut *ty.kind {
        TyKind::SizedArray(item, _) | TyKind::Array(item) => vis.visit_ty(item),
        TyKind::Arrow(_, lhs, rhs, functors) => {
            vis.visit_ty(lhs);
            vis.visit_ty(rhs);
            functors.iter_mut().for_each(|f| vis.visit_functor_expr(f));
        }
        TyKind::Hole | TyKind::Err => {}
        TyKind::Paren(ty) => vis.visit_ty(ty),
        TyKind::Param(TypeParameter {
            ty,
            constraints: bounds,
            ..
        }) => {
            for bound in &mut bounds.0 {
                vis.visit_ident(&mut bound.name);
                bound.parameters.iter_mut().for_each(
                    |crate::ast::ConstraintParameter { ty, .. }| {
                        vis.visit_ty(ty);
                    },
                );
            }
            vis.visit_ident(ty);
        }
        TyKind::Path(path) => vis.visit_path_kind(path),
        TyKind::Tuple(tys) => tys.iter_mut().for_each(|t| vis.visit_ty(t)),
    }
}

pub fn walk_block(vis: &mut impl MutVisitor, block: &mut Block) {
    vis.visit_span(&mut block.span);
    block.stmts.iter_mut().for_each(|s| vis.visit_stmt(s));
}

pub fn walk_stmt(vis: &mut impl MutVisitor, stmt: &mut Stmt) {
    vis.visit_span(&mut stmt.span);

    match &mut *stmt.kind {
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
            block.iter_mut().for_each(|b| vis.visit_block(b));
        }
    }
}

pub fn walk_expr(vis: &mut impl MutVisitor, expr: &mut Expr) {
    vis.visit_span(&mut expr.span);

    match &mut *expr.kind {
        ExprKind::Array(exprs) => exprs.iter_mut().for_each(|e| vis.visit_expr(e)),
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
            otherwise.iter_mut().for_each(|e| vis.visit_expr(e));
        }
        ExprKind::Index(array, index) => {
            vis.visit_expr(array);
            vis.visit_expr(index);
        }
        ExprKind::Interpolate(components) => {
            for component in components.iter_mut() {
                match component {
                    StringComponent::Expr(expr) => vis.visit_expr(expr.as_mut()),
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
            start.iter_mut().for_each(|s| vis.visit_expr(s));
            step.iter_mut().for_each(|s| vis.visit_expr(s));
            end.iter_mut().for_each(|e| vis.visit_expr(e));
        }
        ExprKind::Repeat(body, until, fixup) => {
            vis.visit_block(body);
            vis.visit_expr(until);
            fixup.iter_mut().for_each(|f| vis.visit_block(f));
        }
        ExprKind::Struct(name, copy, fields) => {
            vis.visit_path_kind(name);
            copy.iter_mut().for_each(|c| vis.visit_expr(c));
            fields.iter_mut().for_each(|f| vis.visit_field_assign(f));
        }
        ExprKind::TernOp(_, e1, e2, e3) => {
            vis.visit_expr(e1);
            vis.visit_expr(e2);
            vis.visit_expr(e3);
        }
        ExprKind::Tuple(exprs) => exprs.iter_mut().for_each(|e| vis.visit_expr(e)),
        ExprKind::While(cond, block) => {
            vis.visit_expr(cond);
            vis.visit_block(block);
        }
        ExprKind::Err | ExprKind::Hole | ExprKind::Lit(_) => {}
    }
}

pub fn walk_field_assign(vis: &mut impl MutVisitor, assign: &mut FieldAssign) {
    vis.visit_span(&mut assign.span);
    vis.visit_ident(&mut assign.field);
    vis.visit_expr(&mut assign.value);
}

pub fn walk_pat(vis: &mut impl MutVisitor, pat: &mut Pat) {
    vis.visit_span(&mut pat.span);

    match &mut *pat.kind {
        PatKind::Bind(name, ty) => {
            vis.visit_ident(name);
            ty.iter_mut().for_each(|t| vis.visit_ty(t));
        }
        PatKind::Discard(ty) => ty.iter_mut().for_each(|t| vis.visit_ty(t)),
        PatKind::Elided | PatKind::Err => {}
        PatKind::Paren(pat) => vis.visit_pat(pat),
        PatKind::Tuple(pats) => pats.iter_mut().for_each(|p| vis.visit_pat(p)),
    }
}

pub fn walk_qubit_init(vis: &mut impl MutVisitor, init: &mut QubitInit) {
    vis.visit_span(&mut init.span);

    match &mut *init.kind {
        QubitInitKind::Array(len) => vis.visit_expr(len),
        QubitInitKind::Paren(init) => vis.visit_qubit_init(init),
        QubitInitKind::Single | QubitInitKind::Err => {}
        QubitInitKind::Tuple(inits) => inits.iter_mut().for_each(|i| vis.visit_qubit_init(i)),
    }
}

pub fn walk_path(vis: &mut impl MutVisitor, path: &mut Path) {
    vis.visit_span(&mut path.span);
    if let Some(ref mut parts) = path.segments {
        vis.visit_idents(parts);
    }
    vis.visit_ident(&mut path.name);
}

pub fn walk_path_kind(vis: &mut impl MutVisitor, path: &mut PathKind) {
    match path {
        PathKind::Ok(path) => vis.visit_path(path),
        PathKind::Err(Some(incomplete_path)) => {
            vis.visit_span(&mut incomplete_path.span);

            for ref mut ident in &mut incomplete_path.segments {
                vis.visit_ident(ident);
            }
        }
        PathKind::Err(None) => {}
    }
}

pub fn walk_ident(vis: &mut impl MutVisitor, ident: &mut Ident) {
    vis.visit_span(&mut ident.span);
}

pub fn walk_idents(vis: &mut impl MutVisitor, idents: &mut [Ident]) {
    for ref mut ident in idents {
        vis.visit_ident(ident);
    }
}
