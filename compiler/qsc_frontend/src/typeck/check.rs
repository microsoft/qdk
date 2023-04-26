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
use qsc_ast::ast;
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

    pub(crate) fn add_local_package(&mut self, resolutions: &Resolutions, package: &ast::Package) {
        for namespace in &package.namespaces {
            for item in &namespace.items {
                if let ast::ItemKind::Callable(decl) = &item.kind {
                    let (ty, errors) = convert::ast_callable_ty(resolutions, decl);
                    let Some(&Res::Item(item)) = resolutions.get(decl.name.id) else {
                        panic!("callable should have item ID");
                    };
                    self.globals.insert(item, ty);
                    for MissingTyError(span) in errors {
                        self.errors.push(Error(ErrorKind::MissingItemTy(span)));
                    }
                }
            }
        }
    }

    pub(crate) fn add_external_package(&mut self, id: PackageId, package: &hir::Package) {
        for item in package.items.values() {
            if let hir::ItemKind::Callable(decl) = &item.kind {
                let item_id = ItemId {
                    package: Some(id),
                    item: item.id,
                };
                self.globals.insert(item_id, convert::hir_callable_ty(decl));
            }
        }
    }

    pub(crate) fn into_checker(self) -> Checker {
        Checker {
            globals: self.globals,
            tys: Tys::new(),
            errors: self.errors,
        }
    }
}

pub(crate) struct Checker {
    globals: HashMap<ItemId, Ty>,
    tys: Tys,
    errors: Vec<Error>,
}

impl Checker {
    pub(crate) fn tys(&self) -> &Tys {
        &self.tys
    }

    pub(crate) fn into_tys(self) -> (Tys, Vec<Error>) {
        (self.tys, self.errors)
    }

    pub(crate) fn drain_errors(&mut self) -> vec::Drain<Error> {
        self.errors.drain(..)
    }

    pub(crate) fn add_global_callable(
        &mut self,
        resolutions: &Resolutions,
        decl: &ast::CallableDecl,
    ) {
        let (ty, errors) = convert::ast_callable_ty(resolutions, decl);
        let Some(&Res::Item(item)) = resolutions.get(decl.name.id) else {
            panic!("callable should have item ID");
        };
        self.globals.insert(item, ty);
        for MissingTyError(span) in errors {
            self.errors.push(Error(ErrorKind::MissingItemTy(span)));
        }
    }

    pub(crate) fn check_package(&mut self, resolutions: &Resolutions, package: &ast::Package) {
        for namespace in &package.namespaces {
            for item in &namespace.items {
                if let ast::ItemKind::Callable(decl) = &item.kind {
                    self.check_callable_decl(resolutions, decl);
                }
            }
        }

        if let Some(entry) = &package.entry {
            self.errors.append(&mut rules::expr(
                resolutions,
                &self.globals,
                &mut self.tys,
                entry,
            ));
        }
    }

    pub(crate) fn check_callable_decl(
        &mut self,
        resolutions: &Resolutions,
        decl: &ast::CallableDecl,
    ) {
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

    pub(crate) fn check_stmt(&mut self, resolutions: &Resolutions, stmt: &ast::Stmt) {
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
