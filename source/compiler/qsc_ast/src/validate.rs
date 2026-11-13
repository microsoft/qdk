// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    ast::{
        Attr, Block, CallableDecl, Expr, FieldAssign, FunctorExpr, Ident, Item, Namespace, NodeId,
        Package, Pat, Path, QubitInit, SpecDecl, Stmt, Ty, TyDef,
    },
    visit::{self, Visitor},
};
use qsc_data_structures::index_map::IndexMap;
use std::fmt::Display;

#[derive(Default)]
pub struct Validator {
    ids: IndexMap<NodeId, ()>,
}

impl Validator {
    fn check(&mut self, id: NodeId, node: impl Display) {
        if id.is_default() {
            panic!("default node ID should be replaced: {node}")
        } else if self.ids.contains_key(id) {
            panic!("duplicate node ID: {node}");
        }
        self.ids.insert(id, ());
    }
}

impl Visitor<'_> for Validator {
    fn visit_package(&mut self, package: &Package) {
        self.check(package.id, package);
        visit::walk_package(self, package);
    }

    fn visit_namespace(&mut self, namespace: &Namespace) {
        self.check(namespace.id, namespace);
        visit::walk_namespace(self, namespace);
    }

    fn visit_item(&mut self, item: &Item) {
        self.check(item.id, item);
        visit::walk_item(self, item);
    }

    fn visit_attr(&mut self, attr: &Attr) {
        self.check(attr.id, attr);
        visit::walk_attr(self, attr);
    }

    fn visit_ty_def(&mut self, def: &TyDef) {
        self.check(def.id, def);
        visit::walk_ty_def(self, def);
    }

    fn visit_callable_decl(&mut self, decl: &CallableDecl) {
        self.check(decl.id, decl);
        visit::walk_callable_decl(self, decl);
    }

    fn visit_struct_decl(&mut self, decl: &'_ crate::ast::StructDecl) {
        self.check(decl.id, decl);
        visit::walk_struct_decl(self, decl);
    }

    fn visit_field_def(&mut self, def: &'_ crate::ast::FieldDef) {
        self.check(def.id, def);
        visit::walk_field_def(self, def);
    }

    fn visit_spec_decl(&mut self, decl: &SpecDecl) {
        self.check(decl.id, decl);
        visit::walk_spec_decl(self, decl);
    }

    fn visit_functor_expr(&mut self, expr: &FunctorExpr) {
        self.check(expr.id, expr);
        visit::walk_functor_expr(self, expr);
    }

    fn visit_ty(&mut self, ty: &Ty) {
        self.check(ty.id, ty);
        visit::walk_ty(self, ty);
    }

    fn visit_block(&mut self, block: &Block) {
        self.check(block.id, block);
        visit::walk_block(self, block);
    }

    fn visit_stmt(&mut self, stmt: &Stmt) {
        self.check(stmt.id, stmt);
        visit::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &Expr) {
        self.check(expr.id, expr);
        visit::walk_expr(self, expr);
    }

    fn visit_field_assign(&mut self, assign: &FieldAssign) {
        self.check(assign.id, assign);
        visit::walk_field_assign(self, assign);
    }

    fn visit_pat(&mut self, pat: &Pat) {
        self.check(pat.id, pat);
        visit::walk_pat(self, pat);
    }

    fn visit_qubit_init(&mut self, init: &QubitInit) {
        self.check(init.id, init);
        visit::walk_qubit_init(self, init);
    }

    fn visit_path(&mut self, path: &Path) {
        self.check(path.id, path);
        visit::walk_path(self, path);
    }

    fn visit_ident(&mut self, ident: &Ident) {
        self.check(ident.id, ident);
    }
}
