// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{
    rules::{self, SpecImpl},
    Error, ErrorKind, Tys,
};
use crate::{
    resolve::{Res, Resolutions},
    typeck::convert::{self, MissingTyError},
};
use qsc_ast::{
    ast,
    visit::{self, Visitor},
};
use qsc_hir::hir::{self, ItemId, PackageId, Ty};
use std::{collections::HashMap, vec};

pub(crate) struct GlobalTable {
    globals: HashMap<ItemId, Ty>,
    errors: Vec<Error>,
}

impl GlobalTable {
    pub(crate) fn new() -> Self {
        Self {
            globals: HashMap::new(),
            errors: Vec::new(),
        }
    }

    pub(crate) fn add_external_package(&mut self, id: PackageId, package: &hir::Package) {
        for item in package.items.values() {
            let item_id = ItemId {
                package: Some(id),
                item: item.id,
            };

            match &item.kind {
                hir::ItemKind::Callable(decl) => {
                    self.globals.insert(item_id, convert::hir_callable_ty(decl));
                }
                hir::ItemKind::Namespace(..) => {}
                hir::ItemKind::Ty(_, def) => {
                    self.globals.insert(
                        item_id,
                        convert::ty_cons_ty(item_id, convert::hir_ty_def_ty(def)),
                    );
                }
            }
        }
    }
}

pub(crate) struct Checker {
    globals: HashMap<ItemId, Ty>,
    tys: Tys,
    errors: Vec<Error>,
}

impl Checker {
    pub(crate) fn new(globals: GlobalTable) -> Self {
        Checker {
            globals: globals.globals,
            tys: Tys::new(),
            errors: globals.errors,
        }
    }

    pub(crate) fn tys(&self) -> &Tys {
        &self.tys
    }

    pub(crate) fn into_tys(self) -> (Tys, Vec<Error>) {
        (self.tys, self.errors)
    }

    pub(crate) fn drain_errors(&mut self) -> vec::Drain<Error> {
        self.errors.drain(..)
    }

    pub(crate) fn check_package(&mut self, resolutions: &Resolutions, package: &ast::Package) {
        ItemCollector::new(resolutions, &mut self.globals, &mut self.errors).visit_package(package);
        ItemChecker::new(self, resolutions).visit_package(package);

        if let Some(entry) = &package.entry {
            self.errors.append(&mut rules::expr(
                resolutions,
                &self.globals,
                &mut self.tys,
                entry,
            ));
        }
    }

    pub(crate) fn check_namespace(
        &mut self,
        resolutions: &Resolutions,
        namespace: &ast::Namespace,
    ) {
        ItemCollector::new(resolutions, &mut self.globals, &mut self.errors)
            .visit_namespace(namespace);
        ItemChecker::new(self, resolutions).visit_namespace(namespace);
    }

    fn check_callable_decl(&mut self, resolutions: &Resolutions, decl: &ast::CallableDecl) {
        self.tys
            .insert(decl.name.id, convert::ast_callable_ty(resolutions, decl).0);
        self.check_callable_signature(resolutions, decl);

        let output = convert::ty_from_ast(resolutions, &decl.output).0;
        match &decl.body {
            ast::CallableBody::Block(block) => self.check_spec(
                resolutions,
                SpecImpl {
                    spec: ast::Spec::Body,
                    callable_input: &decl.input,
                    spec_input: None,
                    output: &output,
                    block,
                },
            ),
            ast::CallableBody::Specs(specs) => {
                for spec in specs {
                    if let ast::SpecBody::Impl(input, block) = &spec.body {
                        self.check_spec(
                            resolutions,
                            SpecImpl {
                                spec: spec.spec,
                                callable_input: &decl.input,
                                spec_input: Some(input),
                                output: &output,
                                block,
                            },
                        );
                    }
                }
            }
        }
    }

    fn check_callable_signature(&mut self, resolutions: &Resolutions, decl: &ast::CallableDecl) {
        if !convert::ast_callable_functors(decl).is_empty() {
            let output = convert::ty_from_ast(resolutions, &decl.output).0;
            match &output {
                Ty::Tuple(items) if items.is_empty() => {}
                _ => self.errors.push(Error(ErrorKind::TypeMismatch(
                    Ty::UNIT,
                    output,
                    decl.output.span,
                ))),
            }
        }
    }

    fn check_spec(&mut self, resolutions: &Resolutions, spec: SpecImpl) {
        self.errors.append(&mut rules::spec(
            resolutions,
            &self.globals,
            &mut self.tys,
            spec,
        ));
    }

    pub(crate) fn check_stmt_fragment(&mut self, resolutions: &Resolutions, stmt: &ast::Stmt) {
        ItemCollector::new(resolutions, &mut self.globals, &mut self.errors).visit_stmt(stmt);
        ItemChecker::new(self, resolutions).visit_stmt(stmt);

        // TODO: Normally, all statements in a specialization are type checked in the same inference
        // context. However, during incremental compilation, each statement is type checked with a
        // new inference context. This can cause issues if inference variables aren't fully solved
        // for within each statement. Either those variables should cause an error, or the
        // incremental compiler should be able to persist the inference context across statements.
        // https://github.com/microsoft/qsharp/issues/205
        self.errors.append(&mut rules::stmt(
            resolutions,
            &self.globals,
            &mut self.tys,
            stmt,
        ));
    }
}

struct ItemCollector<'a> {
    resolutions: &'a Resolutions,
    globals: &'a mut HashMap<ItemId, Ty>,
    errors: &'a mut Vec<Error>,
}

impl<'a> ItemCollector<'a> {
    fn new(
        resolutions: &'a Resolutions,
        globals: &'a mut HashMap<ItemId, Ty>,
        errors: &'a mut Vec<Error>,
    ) -> Self {
        Self {
            resolutions,
            globals,
            errors,
        }
    }
}

impl Visitor<'_> for ItemCollector<'_> {
    fn visit_item(&mut self, item: &ast::Item) {
        match &item.kind {
            ast::ItemKind::Callable(decl) => {
                let Some(&Res::Item(item)) = self.resolutions.get(decl.name.id) else {
                    panic!("callable should have item ID");
                };

                let (ty, errors) = convert::ast_callable_ty(self.resolutions, decl);
                for MissingTyError(span) in errors {
                    self.errors.push(Error(ErrorKind::MissingItemTy(span)));
                }

                self.globals.insert(item, ty);
            }
            ast::ItemKind::Ty(name, def) => {
                let Some(&Res::Item(item)) = self.resolutions.get(name.id) else {
                    panic!("type should have item ID");
                };

                let (ty, errors) = convert::ast_ty_def_ty(self.resolutions, def);
                for MissingTyError(span) in errors {
                    self.errors.push(Error(ErrorKind::MissingItemTy(span)));
                }

                self.globals.insert(item, convert::ty_cons_ty(item, ty));
            }
            _ => {}
        }

        visit::walk_item(self, item);
    }
}

struct ItemChecker<'a> {
    checker: &'a mut Checker,
    resolutions: &'a Resolutions,
}

impl<'a> ItemChecker<'a> {
    fn new(checker: &'a mut Checker, resolutions: &'a Resolutions) -> Self {
        Self {
            checker,
            resolutions,
        }
    }
}

impl Visitor<'_> for ItemChecker<'_> {
    fn visit_callable_decl(&mut self, decl: &ast::CallableDecl) {
        self.checker.check_callable_decl(self.resolutions, decl);
        visit::walk_callable_decl(self, decl);
    }
}
