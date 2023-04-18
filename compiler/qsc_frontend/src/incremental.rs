// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    compile::{PackageId, PackageStore},
    lower::Lowerer,
    parse,
    resolve::{self, GlobalTable, Res, Resolutions, Resolver},
};
use miette::Diagnostic;
use qsc_ast::{
    assigner::Assigner,
    ast::{self, ItemKind, NodeId},
    mut_visit::MutVisitor,
    visit::Visitor as AstVisitor,
};
use qsc_data_structures::index_map::IndexMap;
use qsc_hir::{hir, visit::Visitor as HirVisitor};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Clone, Debug, Diagnostic, Error)]
#[diagnostic(transparent)]
#[error(transparent)]
pub struct Error(ErrorKind);

#[derive(Clone, Debug, Diagnostic, Error)]
#[diagnostic(transparent)]
#[error(transparent)]
enum ErrorKind {
    Parse(parse::Error),
    Resolve(resolve::Error),
}

pub enum Fragment {
    Stmt(hir::Stmt),
    Callable(hir::CallableDecl),
    Error(Vec<Error>),
}

pub struct Compiler<'a> {
    assigner: Assigner,
    resolver: Resolver<'a>,
    resolutions: Resolutions<hir::NodeId>,
    scope: HashMap<&'a str, NodeId>,
    lowerer: Lowerer,
}

impl<'a> Compiler<'a> {
    pub fn new(store: &'a PackageStore, dependencies: impl IntoIterator<Item = PackageId>) -> Self {
        let mut globals = GlobalTable::new();
        for dependency in dependencies {
            let unit = store
                .get(dependency)
                .expect("dependency should be added to package store before compilation");
            globals.set_package(dependency);
            HirVisitor::visit_package(&mut globals, &unit.package);
        }

        Self {
            assigner: Assigner::new(),
            resolver: globals.into_resolver(),
            resolutions: IndexMap::new(),
            scope: HashMap::new(),
            lowerer: Lowerer::new(),
        }
    }

    #[must_use]
    pub fn resolutions(&self) -> &Resolutions<hir::NodeId> {
        &self.resolutions
    }

    /// Compile a single string as either a callable declaration or a statement into a `Fragment`.
    /// # Errors
    /// This will Err if the fragment cannot be compiled due to parsing or symbol resolution errors.
    pub fn compile_fragment(&mut self, source: impl AsRef<str>) -> Vec<Fragment> {
        let (item, errors) = parse::item(source.as_ref());
        match item.kind {
            ItemKind::Callable(decl) if errors.is_empty() => {
                return vec![self.compile_callable_decl(decl)];
            }
            _ => {}
        }

        let (stmts, errors) = parse::stmts(source.as_ref());
        if !errors.is_empty() {
            return vec![Fragment::Error(
                errors
                    .into_iter()
                    .map(|e| Error(ErrorKind::Parse(e)))
                    .collect(),
            )];
        }

        let mut fragments = Vec::new();
        for stmt in stmts {
            fragments.push(self.compile_stmt(stmt));
            if matches!(fragments.last(), Some(Fragment::Error(_))) {
                break;
            }
        }
        fragments
    }

    fn compile_callable_decl(&mut self, mut decl: ast::CallableDecl) -> Fragment {
        self.assigner.visit_callable_decl(&mut decl);
        let decl = Box::leak(Box::new(decl));
        self.resolver.with_scope(&mut self.scope, |resolver| {
            resolver.add_global_callable(decl);
            AstVisitor::visit_callable_decl(resolver, decl);
        });

        let errors: Vec<_> = self
            .resolver
            .drain_errors()
            .map(|e| Error(ErrorKind::Resolve(e)))
            .collect();

        let decl = self.lowerer.lower_callable_decl(decl);
        self.lower_resolutions();
        if errors.is_empty() {
            Fragment::Callable(decl)
        } else {
            Fragment::Error(errors)
        }
    }

    fn compile_stmt(&mut self, mut stmt: ast::Stmt) -> Fragment {
        self.assigner.visit_stmt(&mut stmt);
        let stmt = Box::leak(Box::new(stmt));
        self.resolver.with_scope(&mut self.scope, |resolver| {
            resolver.visit_stmt(stmt);
        });

        let errors: Vec<_> = self
            .resolver
            .drain_errors()
            .map(|e| Error(ErrorKind::Resolve(e)))
            .collect();

        let stmt = self.lowerer.lower_stmt(stmt);
        self.lower_resolutions();
        if errors.is_empty() {
            Fragment::Stmt(stmt)
        } else {
            Fragment::Error(errors)
        }
    }

    fn lower_resolutions(&mut self) {
        for (id, res) in self.resolver.drain_resolutions() {
            let Some(id) = self.lowerer.get_id(id) else { continue; };
            let res = match res {
                Res::Internal(node) => Res::Internal(
                    self.lowerer
                        .get_id(node)
                        .expect("lowered node should not resolve to deleted node"),
                ),
                Res::External(package, node) => Res::External(package, node),
            };
            self.resolutions.insert(id, res);
        }
    }
}
