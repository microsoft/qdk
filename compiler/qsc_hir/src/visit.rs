// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::hir::{
    Attr, Block, CallableBody, CallableDecl, Expr, ExprKind, FunctorExpr, FunctorExprKind, Ident,
    Item, ItemKind, Package, Pat, PatKind, QubitInit, QubitInitKind, SpecBody, SpecDecl, Stmt,
    StmtKind, TyDef, TyDefKind, Visibility,
};

pub trait Visitor<'a>: Sized {
    fn visit_package(&mut self, package: &'a Package) {
        walk_package(self, package);
    }

    fn visit_item(&mut self, item: &'a Item) {
        walk_item(self, item);
    }

    fn visit_attr(&mut self, attr: &'a Attr) {
        walk_attr(self, attr);
    }

    fn visit_visibility(&mut self, _: &'a Visibility) {}

    fn visit_ty_def(&mut self, def: &'a TyDef) {
        walk_ty_def(self, def);
    }

    fn visit_callable_decl(&mut self, decl: &'a CallableDecl) {
        walk_callable_decl(self, decl);
    }

    fn visit_spec_decl(&mut self, decl: &'a SpecDecl) {
        walk_spec_decl(self, decl);
    }

    fn visit_functor_expr(&mut self, expr: &'a FunctorExpr) {
        walk_functor_expr(self, expr);
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

    fn visit_pat(&mut self, pat: &'a Pat) {
        walk_pat(self, pat);
    }

    fn visit_qubit_init(&mut self, init: &'a QubitInit) {
        walk_qubit_init(self, init);
    }

    fn visit_ident(&mut self, _: &'a Ident) {}
}

pub fn walk_package<'a>(vis: &mut impl Visitor<'a>, package: &'a Package) {
    package.items.values().for_each(|i| vis.visit_item(i));
    package.entry.iter().for_each(|e| vis.visit_expr(e));
}

pub fn walk_item<'a>(vis: &mut impl Visitor<'a>, item: &'a Item) {
    item.attrs.iter().for_each(|a| vis.visit_attr(a));
    item.visibility.iter().for_each(|v| vis.visit_visibility(v));
    match &item.kind {
        ItemKind::Callable(decl) => vis.visit_callable_decl(decl),
        ItemKind::Err => {}
        ItemKind::Namespace(name, _) => vis.visit_ident(name),
        ItemKind::Ty(ident, def) => {
            vis.visit_ident(ident);
            vis.visit_ty_def(def);
        }
    }
}

pub fn walk_attr<'a>(vis: &mut impl Visitor<'a>, attr: &'a Attr) {
    vis.visit_ident(&attr.name);
    vis.visit_expr(&attr.arg);
}

pub fn walk_ty_def<'a>(vis: &mut impl Visitor<'a>, def: &'a TyDef) {
    match &def.kind {
        TyDefKind::Field(name, _) => name.iter().for_each(|n| vis.visit_ident(n)),
        TyDefKind::Paren(def) => vis.visit_ty_def(def),
        TyDefKind::Tuple(defs) => defs.iter().for_each(|d| vis.visit_ty_def(d)),
    }
}

pub fn walk_callable_decl<'a>(vis: &mut impl Visitor<'a>, decl: &'a CallableDecl) {
    vis.visit_ident(&decl.name);
    decl.ty_params.iter().for_each(|p| vis.visit_ident(p));
    vis.visit_pat(&decl.input);
    decl.functors.iter().for_each(|f| vis.visit_functor_expr(f));
    match &decl.body {
        CallableBody::Block(block) => vis.visit_block(block),
        CallableBody::Specs(specs) => specs.iter().for_each(|s| vis.visit_spec_decl(s)),
    }
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
    match &expr.kind {
        FunctorExprKind::BinOp(_, lhs, rhs) => {
            vis.visit_functor_expr(lhs);
            vis.visit_functor_expr(rhs);
        }
        FunctorExprKind::Lit(_) => {}
        FunctorExprKind::Paren(expr) => vis.visit_functor_expr(expr),
    }
}

pub fn walk_block<'a>(vis: &mut impl Visitor<'a>, block: &'a Block) {
    block.stmts.iter().for_each(|s| vis.visit_stmt(s));
}

pub fn walk_stmt<'a>(vis: &mut impl Visitor<'a>, stmt: &'a Stmt) {
    match &stmt.kind {
        StmtKind::Empty => {}
        StmtKind::Expr(expr) | StmtKind::Semi(expr) => vis.visit_expr(expr),
        StmtKind::Local(_, pat, value) => {
            vis.visit_pat(pat);
            vis.visit_expr(value);
        }
        StmtKind::Qubit(_, pat, init, block) => {
            vis.visit_pat(pat);
            vis.visit_qubit_init(init);
            block.iter().for_each(|b| vis.visit_block(b));
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
        ExprKind::Field(record, _) => vis.visit_expr(record),
        ExprKind::For(pat, iter, block) => {
            vis.visit_pat(pat);
            vis.visit_expr(iter);
            vis.visit_block(block);
        }
        ExprKind::If(cond, body, otherwise) => {
            vis.visit_expr(cond);
            vis.visit_block(body);
            otherwise.iter().for_each(|e| vis.visit_expr(e));
        }
        ExprKind::Index(array, index) => {
            vis.visit_expr(array);
            vis.visit_expr(index);
        }
        ExprKind::Lambda(_, pat, expr) => {
            vis.visit_pat(pat);
            vis.visit_expr(expr);
        }
        ExprKind::Paren(expr) | ExprKind::Return(expr) | ExprKind::UnOp(_, expr) => {
            vis.visit_expr(expr);
        }
        ExprKind::Range(start, step, end) => {
            start.iter().for_each(|s| vis.visit_expr(s));
            step.iter().for_each(|s| vis.visit_expr(s));
            end.iter().for_each(|e| vis.visit_expr(e));
        }
        ExprKind::Repeat(body, until, fixup) => {
            vis.visit_block(body);
            vis.visit_expr(until);
            fixup.iter().for_each(|f| vis.visit_block(f));
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
        ExprKind::Err | ExprKind::Hole | ExprKind::Lit(_) | ExprKind::Name(_) => {}
    }
}

pub fn walk_pat<'a>(vis: &mut impl Visitor<'a>, pat: &'a Pat) {
    match &pat.kind {
        PatKind::Bind(name) => vis.visit_ident(name),
        PatKind::Discard | PatKind::Elided => {}
        PatKind::Paren(pat) => vis.visit_pat(pat),
        PatKind::Tuple(pats) => pats.iter().for_each(|p| vis.visit_pat(p)),
    }
}

pub fn walk_qubit_init<'a>(vis: &mut impl Visitor<'a>, init: &'a QubitInit) {
    match &init.kind {
        QubitInitKind::Array(len) => vis.visit_expr(len),
        QubitInitKind::Paren(init) => vis.visit_qubit_init(init),
        QubitInitKind::Single => {}
        QubitInitKind::Tuple(inits) => inits.iter().for_each(|i| vis.visit_qubit_init(i)),
    }
}
