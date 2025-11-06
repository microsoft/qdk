// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::hir::{
    Block, CallableDecl, Expr, ExprKind, FieldAssign, Ident, Item, ItemKind, Package, Pat, PatKind,
    QubitInit, QubitInitKind, SpecBody, SpecDecl, Stmt, StmtKind, StringComponent,
};
use qsc_data_structures::span::Span;

pub trait MutVisitor: Sized {
    fn visit_package(&mut self, package: &mut Package) {
        walk_package(self, package);
    }

    fn visit_item(&mut self, item: &mut Item) {
        walk_item(self, item);
    }

    fn visit_callable_decl(&mut self, decl: &mut CallableDecl) {
        walk_callable_decl(self, decl);
    }

    fn visit_spec_decl(&mut self, decl: &mut SpecDecl) {
        walk_spec_decl(self, decl);
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

    fn visit_ident(&mut self, ident: &mut Ident) {
        walk_ident(self, ident);
    }

    fn visit_idents(&mut self, ident: &mut crate::hir::Idents) {
        walk_idents(self, ident);
    }

    fn visit_span(&mut self, _: &mut Span) {}
}

pub fn walk_package(vis: &mut impl MutVisitor, package: &mut Package) {
    package.items.values_mut().for_each(|i| vis.visit_item(i));
    package.stmts.iter_mut().for_each(|s| vis.visit_stmt(s));
    package.entry.iter_mut().for_each(|e| vis.visit_expr(e));
}

pub fn walk_item(vis: &mut impl MutVisitor, item: &mut Item) {
    vis.visit_span(&mut item.span);

    match &mut item.kind {
        ItemKind::Callable(decl) => vis.visit_callable_decl(decl),
        ItemKind::Namespace(name, _) => vis.visit_idents(name),
        ItemKind::Ty(name, _) | ItemKind::Export(name, _) => vis.visit_ident(name),
    }
}

pub fn walk_callable_decl(vis: &mut impl MutVisitor, decl: &mut CallableDecl) {
    vis.visit_span(&mut decl.span);
    vis.visit_ident(&mut decl.name);
    vis.visit_pat(&mut decl.input);
    vis.visit_spec_decl(&mut decl.body);
    decl.adj
        .iter_mut()
        .for_each(|spec| vis.visit_spec_decl(spec));
    decl.ctl
        .iter_mut()
        .for_each(|spec| vis.visit_spec_decl(spec));
    decl.ctl_adj
        .iter_mut()
        .for_each(|spec| vis.visit_spec_decl(spec));
}

pub fn walk_spec_decl(vis: &mut impl MutVisitor, decl: &mut SpecDecl) {
    vis.visit_span(&mut decl.span);

    match &mut decl.body {
        SpecBody::Gen(_) => {}
        SpecBody::Impl(pat, block) => {
            for pat in pat.iter_mut() {
                vis.visit_pat(pat);
            }
            vis.visit_block(block);
        }
    }
}

pub fn walk_block(vis: &mut impl MutVisitor, block: &mut Block) {
    vis.visit_span(&mut block.span);
    block.stmts.iter_mut().for_each(|s| vis.visit_stmt(s));
}

pub fn walk_stmt(vis: &mut impl MutVisitor, stmt: &mut Stmt) {
    vis.visit_span(&mut stmt.span);

    match &mut stmt.kind {
        StmtKind::Item(_) => {}
        StmtKind::Expr(expr) | StmtKind::Semi(expr) => vis.visit_expr(expr),
        StmtKind::Local(_, pat, value) => {
            vis.visit_pat(pat);
            vis.visit_expr(value);
        }
        StmtKind::Qubit(_, pat, init, block) => {
            vis.visit_pat(pat);
            vis.visit_qubit_init(init);
            for b in block.iter_mut() {
                vis.visit_block(b);
            }
        }
    }
}

pub fn walk_expr(vis: &mut impl MutVisitor, expr: &mut Expr) {
    vis.visit_span(&mut expr.span);

    match &mut expr.kind {
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
        ExprKind::AssignField(record, _, replace) | ExprKind::UpdateField(record, _, replace) => {
            vis.visit_expr(record);
            vis.visit_expr(replace);
        }
        ExprKind::AssignIndex(array, index, replace) => {
            vis.visit_expr(array);
            vis.visit_expr(index);
            vis.visit_expr(replace);
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
        ExprKind::Field(record, _) => vis.visit_expr(record),
        ExprKind::For(pat, iter, block) => {
            vis.visit_pat(pat);
            vis.visit_expr(iter);
            vis.visit_block(block);
        }
        ExprKind::If(cond, body, otherwise) => {
            vis.visit_expr(cond);
            vis.visit_expr(body);
            for e in otherwise.iter_mut() {
                vis.visit_expr(e);
            }
        }
        ExprKind::Index(array, index) => {
            vis.visit_expr(array);
            vis.visit_expr(index);
        }
        ExprKind::Return(expr) | ExprKind::UnOp(_, expr) => {
            vis.visit_expr(expr);
        }
        ExprKind::Range(start, step, end) => {
            for s in start.iter_mut() {
                vis.visit_expr(s);
            }
            for s in step.iter_mut() {
                vis.visit_expr(s);
            }
            for e in end.iter_mut() {
                vis.visit_expr(e);
            }
        }
        ExprKind::Repeat(body, until, fixup) => {
            vis.visit_block(body);
            vis.visit_expr(until);
            for f in fixup.iter_mut() {
                vis.visit_block(f);
            }
        }
        ExprKind::Struct(_, copy, fields) => {
            for c in copy.iter_mut() {
                vis.visit_expr(c);
            }
            fields.iter_mut().for_each(|f| vis.visit_field_assign(f));
        }
        ExprKind::String(components) => {
            for component in components {
                match component {
                    StringComponent::Expr(expr) => vis.visit_expr(expr),
                    StringComponent::Lit(_) => {}
                }
            }
        }
        ExprKind::UpdateIndex(e1, e2, e3) => {
            vis.visit_expr(e1);
            vis.visit_expr(e2);
            vis.visit_expr(e3);
        }
        ExprKind::Tuple(exprs) => exprs.iter_mut().for_each(|e| vis.visit_expr(e)),
        ExprKind::While(cond, block) => {
            vis.visit_expr(cond);
            vis.visit_block(block);
        }
        ExprKind::Closure(_, _)
        | ExprKind::Err
        | ExprKind::Hole
        | ExprKind::Lit(_)
        | ExprKind::Var(_, _) => {}
    }
}

pub fn walk_field_assign(vis: &mut impl MutVisitor, assign: &mut FieldAssign) {
    vis.visit_span(&mut assign.span);
    vis.visit_expr(&mut assign.value);
}

pub fn walk_pat(vis: &mut impl MutVisitor, pat: &mut Pat) {
    vis.visit_span(&mut pat.span);

    match &mut pat.kind {
        PatKind::Bind(name) => vis.visit_ident(name),
        PatKind::Discard | PatKind::Err => {}
        PatKind::Tuple(pats) => pats.iter_mut().for_each(|p| vis.visit_pat(p)),
    }
}

pub fn walk_qubit_init(vis: &mut impl MutVisitor, init: &mut QubitInit) {
    vis.visit_span(&mut init.span);

    match &mut init.kind {
        QubitInitKind::Array(len) => vis.visit_expr(len),
        QubitInitKind::Single | QubitInitKind::Err => {}
        QubitInitKind::Tuple(inits) => inits.iter_mut().for_each(|i| vis.visit_qubit_init(i)),
    }
}

pub fn walk_ident(vis: &mut impl MutVisitor, ident: &mut Ident) {
    vis.visit_span(&mut ident.span);
}

pub fn walk_idents(vis: &mut impl MutVisitor, ident: &mut crate::hir::Idents) {
    for ref mut ident in &mut *ident.0 {
        vis.visit_ident(ident);
    }
}
