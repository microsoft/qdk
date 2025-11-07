// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::hir::{
    Block, CallableDecl, Expr, ExprKind, FieldAssign, Ident, Idents, Item, ItemKind, Package, Pat,
    PatKind, QubitInit, QubitInitKind, SpecBody, SpecDecl, Stmt, StmtKind, StringComponent,
};

pub trait Visitor<'a>: Sized {
    fn visit_package(&mut self, package: &'a Package) {
        walk_package(self, package);
    }

    fn visit_item(&mut self, item: &'a Item) {
        walk_item(self, item);
    }

    fn visit_callable_decl(&mut self, decl: &'a CallableDecl) {
        walk_callable_decl(self, decl);
    }

    fn visit_spec_decl(&mut self, decl: &'a SpecDecl) {
        walk_spec_decl(self, decl);
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

    fn visit_ident(&mut self, _: &'a Ident) {}

    fn visit_idents(&mut self, idents: &'a Idents) {
        walk_idents(self, idents);
    }
}

pub fn walk_package<'a>(vis: &mut impl Visitor<'a>, package: &'a Package) {
    package.items.values().for_each(|i| vis.visit_item(i));
    package.stmts.iter().for_each(|s| vis.visit_stmt(s));
    package.entry.iter().for_each(|e| vis.visit_expr(e));
}

pub fn walk_item<'a>(vis: &mut impl Visitor<'a>, item: &'a Item) {
    match &item.kind {
        ItemKind::Callable(decl) => vis.visit_callable_decl(decl),
        ItemKind::Namespace(name, _) => vis.visit_idents(name),
        ItemKind::Ty(name, _) | ItemKind::Export(name, _) => vis.visit_ident(name),
    }
}

pub fn walk_callable_decl<'a>(vis: &mut impl Visitor<'a>, decl: &'a CallableDecl) {
    vis.visit_ident(&decl.name);
    vis.visit_pat(&decl.input);
    vis.visit_spec_decl(&decl.body);
    decl.adj.iter().for_each(|spec| vis.visit_spec_decl(spec));
    decl.ctl.iter().for_each(|spec| vis.visit_spec_decl(spec));
    decl.ctl_adj
        .iter()
        .for_each(|spec| vis.visit_spec_decl(spec));
}

pub fn walk_spec_decl<'a>(vis: &mut impl Visitor<'a>, decl: &'a SpecDecl) {
    match &decl.body {
        SpecBody::Gen(_) => {}
        SpecBody::Impl(pat, block) => {
            if let Some(pat) = pat {
                vis.visit_pat(pat);
            }
            vis.visit_block(block);
        }
    }
}

pub fn walk_block<'a>(vis: &mut impl Visitor<'a>, block: &'a Block) {
    block.stmts.iter().for_each(|s| vis.visit_stmt(s));
}

pub fn walk_stmt<'a>(vis: &mut impl Visitor<'a>, stmt: &'a Stmt) {
    match &stmt.kind {
        StmtKind::Item(_) => {}
        StmtKind::Expr(expr) | StmtKind::Semi(expr) => vis.visit_expr(expr),
        StmtKind::Local(_, pat, value) => {
            vis.visit_pat(pat);
            vis.visit_expr(value);
        }
        StmtKind::Qubit(_, pat, init, block) => {
            vis.visit_pat(pat);
            vis.visit_qubit_init(init);
            if let Some(b) = block {
                vis.visit_block(b);
            }
        }
    }
}

pub fn walk_expr<'a>(vis: &mut impl Visitor<'a>, expr: &'a Expr) {
    match &expr.kind {
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
            if let Some(e) = otherwise {
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
            if let Some(s) = start {
                vis.visit_expr(s);
            }
            if let Some(s) = step {
                vis.visit_expr(s);
            }
            if let Some(e) = end {
                vis.visit_expr(e);
            }
        }
        ExprKind::Repeat(body, until, fixup) => {
            vis.visit_block(body);
            vis.visit_expr(until);
            if let Some(f) = fixup {
                vis.visit_block(f);
            }
        }
        ExprKind::Struct(_, copy, fields) => {
            if let Some(c) = copy {
                vis.visit_expr(c);
            }
            fields.iter().for_each(|f| vis.visit_field_assign(f));
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
        ExprKind::Tuple(exprs) => exprs.iter().for_each(|e| vis.visit_expr(e)),
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

pub fn walk_field_assign<'a>(vis: &mut impl Visitor<'a>, assign: &'a FieldAssign) {
    vis.visit_expr(&assign.value);
}

pub fn walk_pat<'a>(vis: &mut impl Visitor<'a>, pat: &'a Pat) {
    match &pat.kind {
        PatKind::Bind(name) => vis.visit_ident(name),
        PatKind::Discard | PatKind::Err => {}
        PatKind::Tuple(pats) => pats.iter().for_each(|p| vis.visit_pat(p)),
    }
}

pub fn walk_qubit_init<'a>(vis: &mut impl Visitor<'a>, init: &'a QubitInit) {
    match &init.kind {
        QubitInitKind::Array(len) => vis.visit_expr(len),
        QubitInitKind::Single | QubitInitKind::Err => {}
        QubitInitKind::Tuple(inits) => inits.iter().for_each(|i| vis.visit_qubit_init(i)),
    }
}

pub fn walk_idents<'a>(vis: &mut impl Visitor<'a>, idents: &'a Idents) {
    idents.iter().for_each(|i| vis.visit_ident(i));
}
