// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use crate::{
    compile::{preprocess, CompileUnit, Offsetter, PackageStore, TargetProfile},
    lower::{self, Lowerer},
    resolve::{self, Resolver},
    typeck::{self, Checker},
};
use miette::Diagnostic;
use qsc_ast::{assigner::Assigner as AstAssigner, ast, mut_visit::MutVisitor, visit::Visitor};
use qsc_hir::{
    assigner::Assigner as HirAssigner,
    hir::{self, PackageId},
};
use thiserror::Error;

#[derive(Clone, Debug, Diagnostic, Error)]
#[diagnostic(transparent)]
#[error(transparent)]
pub struct Error(ErrorKind);

#[derive(Clone, Debug, Diagnostic, Error)]
#[diagnostic(transparent)]
enum ErrorKind {
    #[error("syntax error")]
    Parse(#[from] qsc_parse::Error),
    #[error("name error")]
    Resolve(#[from] resolve::Error),
    #[error("type error")]
    Type(#[from] typeck::Error),
    #[error(transparent)]
    Lower(#[from] lower::Error),
}

#[derive(Debug)]
pub enum Fragment {
    Stmt(hir::Stmt),
    Item(hir::Item),
}

pub struct Compiler {
    ast_assigner: AstAssigner,
    resolver: Resolver,
    checker: Checker,
    lowerer: Lowerer,
    target: TargetProfile,
}

impl Compiler {
    /// # Panics
    ///
    /// This function will panic if compiler state is invalid or in out-of-memory conditions.
    pub fn new(
        store: &PackageStore,
        dependencies: impl IntoIterator<Item = PackageId>,
        target: TargetProfile,
    ) -> Self {
        let mut resolve_globals = resolve::GlobalTable::new();
        let mut typeck_globals = typeck::GlobalTable::new();
        let mut dropped_names = Vec::new();
        if let Some(unit) = store.get(PackageId::CORE) {
            resolve_globals.add_external_package(PackageId::CORE, &unit.package);
            typeck_globals.add_external_package(PackageId::CORE, &unit.package);
            dropped_names.extend(unit.dropped_names.iter().cloned());
        }

        for id in dependencies {
            let unit = store
                .get(id)
                .expect("dependency should be added to package store before compilation");
            resolve_globals.add_external_package(id, &unit.package);
            typeck_globals.add_external_package(id, &unit.package);
            dropped_names.extend(unit.dropped_names.iter().cloned());
        }

        Self {
            ast_assigner: AstAssigner::new(),
            resolver: Resolver::with_persistent_local_scope(resolve_globals, dropped_names),
            checker: Checker::new(typeck_globals),
            lowerer: Lowerer::new(),
            target,
        }
    }

    /// Compile a string with a single fragment of Q# code that is an expression.
    /// # Errors
    /// Returns a vector of errors if the input fails compilation.
    pub fn compile_expr(
        &mut self,
        unit: &mut CompileUnit,
        source_name: &str,
        source_contents: &str,
    ) -> Result<Vec<Fragment>, Vec<Error>> {
        let fragments = self.compile(unit, source_name, source_contents, |s| {
            let (expr, errors) = qsc_parse::expr(s);
            if !errors.is_empty() {
                return (Vec::new(), errors);
            }

            let fragment = qsc_parse::Fragment::Stmt(Box::new(ast::Stmt {
                id: ast::NodeId::default(),
                span: expr.span,
                kind: Box::new(ast::StmtKind::Expr(expr)),
            }));

            (vec![fragment], errors)
        })?;

        Ok(fragments)
    }

    /// Compile a string with one or more fragments of Q# code.
    /// # Errors
    /// Returns a vector of errors if any of the input fails compilation.
    pub fn compile_fragments(
        &mut self,
        unit: &mut CompileUnit,
        source_name: &str,
        source_contents: &str,
    ) -> Result<Vec<Fragment>, Vec<Error>> {
        self.compile(unit, source_name, source_contents, qsc_parse::fragments)
    }

    fn compile<F>(
        &mut self,
        unit: &mut CompileUnit,
        source_name: &str,
        source_contents: &str,
        parse: F,
    ) -> Result<Vec<Fragment>, Vec<Error>>
    where
        F: Fn(&str) -> (Vec<qsc_parse::Fragment>, Vec<qsc_parse::Error>),
    {
        // Append the line to the source map with the appropriate offset
        let offset = unit
            .sources
            .push(source_name.into(), source_contents.into());

        let (mut fragments, errors) = parse(source_contents);
        if !errors.is_empty() {
            return Err(errors
                .into_iter()
                .map(|e| Error(ErrorKind::Parse(e.with_offset(offset))))
                .collect());
        }

        let mut offsetter = Offsetter(offset);
        for fragment in &mut fragments {
            match fragment {
                qsc_parse::Fragment::Namespace(namespace) => offsetter.visit_namespace(namespace),
                qsc_parse::Fragment::Stmt(stmt) => offsetter.visit_stmt(stmt),
            }
        }

        let mut cond_compile = preprocess::Conditional::new(self.target);
        for fragment in &mut fragments {
            match fragment {
                qsc_parse::Fragment::Namespace(namespace) => {
                    cond_compile.visit_namespace(namespace);
                }
                qsc_parse::Fragment::Stmt(stmt) => {
                    cond_compile.visit_stmt(stmt);
                }
            }
        }
        self.resolver
            .extend_dropped_names(cond_compile.into_names());

        // Namespaces must be processed before top-level statements, so sort the fragments.
        // Note that stable sorting is used here to preserve the order of top-level statements.
        fragments.sort_by_key(|f| match f {
            qsc_parse::Fragment::Namespace(_) => 0,
            qsc_parse::Fragment::Stmt(_) => 1,
        });

        self.assign_ast_ids(&mut fragments);

        self.bind_items(&mut unit.assigner, &fragments);

        self.resolve(&mut unit.assigner, &fragments);

        self.collect_items(&fragments);

        self.type_check(&fragments);

        let fragments = fragments
            .into_iter()
            .flat_map(|f| self.lower_fragment(&mut unit.assigner, f))
            .collect();

        let errors = self.drain_errors();
        if errors.is_empty() {
            Ok(fragments)
        } else {
            self.lowerer.clear_items();
            Err(errors)
        }
    }

    fn type_check(&mut self, fragments: &Vec<qsc_parse::Fragment>) {
        for fragment in fragments {
            match fragment {
                qsc_parse::Fragment::Namespace(namespace) => self
                    .checker
                    .check_namespace_fragment(self.resolver.names(), namespace),
                qsc_parse::Fragment::Stmt(stmt) => self
                    .checker
                    .check_stmt_fragment(self.resolver.names(), stmt),
            }
        }

        self.checker.solve(self.resolver.names());
    }

    fn collect_items(&mut self, fragments: &Vec<qsc_parse::Fragment>) {
        for fragment in fragments {
            match fragment {
                qsc_parse::Fragment::Namespace(namespace) => self
                    .checker
                    .collect_namespace_items(self.resolver.names(), namespace),
                qsc_parse::Fragment::Stmt(stmt) => {
                    self.checker.collect_stmt_items(self.resolver.names(), stmt);
                }
            }
        }
    }

    fn resolve(&mut self, hir_assigner: &mut HirAssigner, fragments: &Vec<qsc_parse::Fragment>) {
        for fragment in fragments {
            match fragment {
                qsc_parse::Fragment::Namespace(namespace) => {
                    self.resolver.with(hir_assigner).visit_namespace(namespace);
                }
                qsc_parse::Fragment::Stmt(stmt) => {
                    self.resolver.with(hir_assigner).visit_stmt(stmt);
                }
            }
        }
    }

    fn bind_items(&mut self, hir_assigner: &mut HirAssigner, fragments: &Vec<qsc_parse::Fragment>) {
        for fragment in fragments {
            match fragment {
                qsc_parse::Fragment::Namespace(namespace) => {
                    self.resolver.bind_namespace_items(hir_assigner, namespace);
                }
                qsc_parse::Fragment::Stmt(stmt) => {
                    if let ast::StmtKind::Item(item) = stmt.kind.as_ref() {
                        self.resolver.bind_local_item(hir_assigner, item);
                    }
                }
            }
        }
    }

    fn assign_ast_ids(&mut self, fragments: &mut Vec<qsc_parse::Fragment>) {
        for fragment in fragments {
            match fragment {
                qsc_parse::Fragment::Namespace(namespace) => {
                    self.ast_assigner.visit_namespace(namespace);
                }
                qsc_parse::Fragment::Stmt(stmt) => self.ast_assigner.visit_stmt(stmt),
            }
        }
    }

    fn lower_fragment(
        &mut self,
        hir_assigner: &mut HirAssigner,
        fragment: qsc_parse::Fragment,
    ) -> Vec<Fragment> {
        let fragment = match fragment {
            qsc_parse::Fragment::Namespace(namespace) => {
                self.lower_namespace(hir_assigner, &namespace);
                None
            }
            qsc_parse::Fragment::Stmt(stmt) => self.lower_stmt(hir_assigner, &stmt),
        };

        self.lowerer
            .drain_items()
            .map(Fragment::Item)
            .chain(fragment)
            .collect()
    }

    fn lower_namespace(&mut self, hir_assigner: &mut HirAssigner, namespace: &ast::Namespace) {
        self.lowerer
            .with(hir_assigner, self.resolver.names(), self.checker.table())
            .lower_namespace(namespace);
    }

    fn lower_stmt(&mut self, hir_assigner: &mut HirAssigner, stmt: &ast::Stmt) -> Option<Fragment> {
        self.lowerer
            .with(hir_assigner, self.resolver.names(), self.checker.table())
            .lower_stmt(stmt)
            .map(Fragment::Stmt)
    }

    fn drain_errors(&mut self) -> Vec<Error> {
        self.resolver
            .drain_errors()
            .map(|e| Error(e.into()))
            .chain(self.checker.drain_errors().map(|e| Error(e.into())))
            .chain(self.lowerer.drain_errors().map(|e| Error(e.into())))
            .collect()
    }
}
