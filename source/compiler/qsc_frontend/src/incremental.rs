// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use crate::{
    compile::{self, AstPackage, CompileUnit, Dependencies, Offsetter, PackageStore, preprocess},
    lower::Lowerer,
    resolve::{self, Resolver},
    typeck::{self, Checker},
};
use qsc_ast::{
    assigner::Assigner as AstAssigner,
    ast::{self},
    mut_visit::MutVisitor,
    validate::Validator as AstValidator,
    visit::Visitor as AstVisitor,
};
use qsc_data_structures::{
    error::WithSource, language_features::LanguageFeatures, source::SourceMap,
    target::TargetCapabilityFlags,
};
use qsc_hir::{
    assigner::Assigner as HirAssigner,
    hir::{self, PackageId},
    validate::Validator as HirValidator,
    visit::Visitor as HirVisitor,
};
use std::mem::take;

/// The frontend for an incremental compiler.
/// It is used to update a single `CompileUnit`
/// with additional sources.
pub struct Compiler {
    ast_assigner: AstAssigner,
    resolver: Resolver,
    checker: Checker,
    lowerer: Lowerer,
    capabilities: TargetCapabilityFlags,
    language_features: LanguageFeatures,
}

pub type Error = WithSource<compile::Error>;

/// The result of an incremental compilation.
/// These packages can be merged into the original
/// `CompileUnit` that was used for the incremental compilation.
#[derive(Debug)]
pub struct Increment {
    pub ast: AstPackage,
    pub hir: hir::Package,
}

impl Increment {
    pub fn clear_entry(&mut self) {
        self.hir.entry = None;
    }
}

impl Compiler {
    /// Creates a new compiler.
    pub fn new(
        store: &PackageStore,
        dependencies: &Dependencies,
        capabilities: TargetCapabilityFlags,
        language_features: LanguageFeatures,
    ) -> Self {
        let mut resolve_globals = resolve::GlobalTable::new();
        let mut typeck_globals = typeck::GlobalTable::new();
        let mut dropped_names = Vec::new();
        if let Some(unit) = store.get(PackageId::CORE) {
            resolve_globals.add_external_package(PackageId::CORE, &unit.package, store, None);
            typeck_globals.add_external_package(PackageId::CORE, &unit.package, store);
            dropped_names.extend(unit.dropped_names.iter().cloned());
        }

        for (id, alias) in dependencies {
            let unit = store
                .get(*id)
                .expect("dependency should be added to package store before compilation");
            resolve_globals.add_external_package(*id, &unit.package, store, alias.as_deref());
            typeck_globals.add_external_package(*id, &unit.package, store);
            dropped_names.extend(unit.dropped_names.iter().cloned());
        }

        // This will be the ID of the open package once the store is open.
        let package_id = store.peek_package_id();

        Self {
            ast_assigner: AstAssigner::new(),
            resolver: Resolver::with_persistent_local_scope(
                package_id,
                resolve_globals,
                dropped_names,
            ),
            checker: Checker::new(typeck_globals),
            lowerer: Lowerer::new(package_id),
            capabilities,
            language_features,
        }
    }

    /// Compiles Q# fragments.
    ///
    /// Uses the assigners and other mutable state from the passed in
    /// `CompileUnit` to guarantee uniqueness, however does not
    /// update the `CompileUnit` with the resulting AST and HIR packages.
    ///
    /// The caller can use the returned packages to perform passes,
    /// get information about the newly added items, or do other modifications.
    /// It is then the caller's responsibility to merge
    /// these packages into the current `CompileUnit`.
    ///
    /// This method calls an accumulator function with any errors returned
    /// from each of the stages (parsing, lowering), instead of failing.
    /// If the accumulator succeeds, compilation continues.
    /// If the accumulator returns an error, compilation stops and the
    /// error is returned to the caller.
    pub fn compile_fragments<F, E>(
        &mut self,
        unit: &mut CompileUnit,
        source_name: &str,
        source_contents: &str,
        accumulate_errors: F,
    ) -> Result<Increment, E>
    where
        F: FnMut(Vec<Error>) -> Result<(), E>,
    {
        let (ast, parse_errors) = Self::parse_fragments(
            &mut unit.sources,
            source_name,
            source_contents,
            self.language_features,
        );

        self.compile_fragments_internal(unit, ast, parse_errors, accumulate_errors)
    }

    /// Compiles Q# AST fragments.
    ///
    /// Uses the assigners and other mutable state from the passed in
    /// `CompileUnit` to guarantee uniqueness, however does not
    /// update the `CompileUnit` with the resulting AST and HIR packages.
    ///
    /// The caller can use the returned packages to perform passes,
    /// get information about the newly added items, or do other modifications.
    /// It is then the caller's responsibility to merge
    /// these packages into the current `CompileUnit`.
    ///
    /// This method calls an accumulator function with any errors returned
    /// from each of the stages instead of failing.
    /// If the accumulator succeeds, compilation continues.
    /// If the accumulator returns an error, compilation stops and the
    /// error is returned to the caller.
    pub fn compile_ast_fragments<F, E>(
        &mut self,
        unit: &mut CompileUnit,
        source_name: &str,
        source_contents: &str,
        package: ast::Package,
        accumulate_errors: F,
    ) -> Result<Increment, E>
    where
        F: FnMut(Vec<Error>) -> Result<(), E>,
    {
        // Update the AST with source information offset from the current source map.
        let (ast, parse_errors) = Self::offset_ast_fragments(
            &mut unit.sources,
            source_name,
            source_contents,
            package,
            vec![],
        );

        self.compile_fragments_internal(unit, ast, parse_errors, accumulate_errors)
    }

    fn compile_fragments_internal<F, E>(
        &mut self,
        unit: &mut CompileUnit,
        mut ast: ast::Package,
        parse_errors: Vec<Error>,
        mut accumulate_errors: F,
    ) -> Result<Increment, E>
    where
        F: FnMut(Vec<Error>) -> Result<(), E>,
    {
        accumulate_errors(parse_errors)?;

        let (hir, errors) = self.resolve_check_lower(unit, &mut ast);

        accumulate_errors(errors)?;

        Ok(Increment {
            ast: AstPackage {
                package: ast,
                names: self.resolver.names().clone(),
                locals: self.resolver.locals().clone(),
                globals: self.resolver.globals().clone(),
                tys: self.checker.table().clone(),
            },
            hir,
        })
    }

    /// Compiles an entry expression.
    ///
    /// Uses the assigners and other mutable state from the passed in
    /// `CompileUnit` to guarantee uniqueness, however does not
    /// update the `CompileUnit` with the resulting AST and HIR packages.
    ///
    /// The caller can use the returned packages to perform passes,
    /// get information about the newly added items, or do other modifications.
    /// It is then the caller's responsibility to merge
    /// these packages into the current `CompileUnit`.
    pub fn compile_entry_expr(
        &mut self,
        unit: &mut CompileUnit,
        source_contents: &str,
    ) -> Result<Increment, Vec<Error>> {
        let (mut ast, parse_errors) =
            Self::parse_entry_expr(&mut unit.sources, source_contents, self.language_features);

        if !parse_errors.is_empty() {
            return Err(parse_errors);
        }

        let (hir, errors) = self.resolve_check_lower(unit, &mut ast);

        if !errors.is_empty() {
            return Err(errors);
        }

        Ok(Increment {
            ast: AstPackage {
                package: ast,
                names: self.resolver.names().clone(),
                locals: self.resolver.locals().clone(),
                globals: self.resolver.globals().clone(),
                tys: self.checker.table().clone(),
            },
            hir,
        })
    }

    pub fn update(&mut self, unit: &mut CompileUnit, new: Increment) {
        // Update the AST
        unit.ast.package = self.concat_ast(take(&mut unit.ast.package), new.ast.package);

        // The new `Increment` will contain the names and tys
        // from the original package as well, so just
        // replace the current tables instead of extending.
        unit.ast.names = new.ast.names;
        unit.ast.tys = new.ast.tys;
        unit.ast.locals = new.ast.locals;
        unit.ast.globals = new.ast.globals;

        // Update the HIR
        extend_hir(&mut unit.package, new.hir);
    }

    fn resolve_check_lower(
        &mut self,
        unit: &mut CompileUnit,
        ast: &mut ast::Package,
    ) -> (hir::Package, Vec<Error>) {
        let mut cond_compile = preprocess::Conditional::new(self.capabilities);
        cond_compile.visit_package(ast);

        self.ast_assigner.visit_package(ast);

        self.resolver
            .extend_dropped_names(cond_compile.into_names());
        self.resolver.bind_fragments(ast, &mut unit.assigner);
        self.resolver.resolve(&mut unit.assigner, ast);

        self.checker.check_package(self.resolver.names(), ast);
        self.checker.solve(self.resolver.names());

        let package = self.lower(&mut unit.assigner, &*ast);

        let errors = self
            .resolver
            .drain_errors()
            .map(|e| compile::Error(e.into()))
            .chain(
                self.checker
                    .drain_errors()
                    .map(|e| compile::Error(e.into())),
            )
            .chain(
                self.lowerer
                    .drain_errors()
                    .map(|e| compile::Error(e.into())),
            )
            .map(|e| WithSource::from_map(&unit.sources, e))
            .collect::<Vec<_>>();

        if !errors.is_empty() {
            self.lowerer.clear_items();
        }

        (package, errors)
    }

    /// Creates a new `Package` by combining two packages.
    /// The two packages should not contain any conflicting `NodeId`s.
    /// Entry expressions are ignored.
    #[must_use]
    fn concat_ast(&mut self, mut left: ast::Package, right: ast::Package) -> ast::Package {
        let mut nodes = Vec::with_capacity(left.nodes.len() + right.nodes.len());
        nodes.extend(left.nodes.into_vec());
        nodes.extend(right.nodes.into_vec());
        left.id = self.ast_assigner.next_id();
        left.nodes = nodes.into_boxed_slice();

        AstValidator::default().visit_package(&left);
        left
    }

    fn parse_entry_expr(
        sources: &mut SourceMap,
        source_contents: &str,
        language_features: LanguageFeatures,
    ) -> (ast::Package, Vec<Error>) {
        let offset = sources.push("<entry>".into(), source_contents.into());

        let (mut expr, errors) = qsc_parse::expr(source_contents, language_features);

        let mut offsetter = Offsetter(offset);
        offsetter.visit_expr(&mut expr);

        let package = ast::Package {
            id: ast::NodeId::default(),
            nodes: Box::default(),
            entry: Some(expr),
        };

        (package, with_source(errors, sources, offset))
    }

    fn parse_fragments(
        sources: &mut SourceMap,
        source_name: &str,
        source_contents: &str,
        features: LanguageFeatures,
    ) -> (ast::Package, Vec<Error>) {
        let offset = sources.push(source_name.into(), source_contents.into());
        let (mut top_level_nodes, errors) = qsc_parse::top_level_nodes(source_contents, features);
        let mut offsetter = Offsetter(offset);
        for node in &mut top_level_nodes {
            match node {
                ast::TopLevelNode::Namespace(ns) => offsetter.visit_namespace(ns),
                ast::TopLevelNode::Stmt(stmt) => offsetter.visit_stmt(stmt),
            }
        }
        let package = ast::Package {
            id: ast::NodeId::default(),
            nodes: top_level_nodes.into_boxed_slice(),
            entry: None,
        };
        (package, with_source(errors, sources, offset))
    }

    /// offset all top level nodes based on the source input
    /// and return the updated package and errors
    fn offset_ast_fragments(
        sources: &mut SourceMap,
        source_name: &str,
        source_contents: &str,
        mut package: ast::Package,
        errors: Vec<qsc_parse::Error>,
    ) -> (ast::Package, Vec<Error>) {
        let offset = sources.push(source_name.into(), source_contents.into());

        let mut offsetter = Offsetter(offset);
        for node in &mut *package.nodes {
            match node {
                ast::TopLevelNode::Namespace(ns) => offsetter.visit_namespace(ns),
                ast::TopLevelNode::Stmt(stmt) => offsetter.visit_stmt(stmt),
            }
        }

        (package, with_source(errors, sources, offset))
    }

    fn lower(&mut self, hir_assigner: &mut HirAssigner, package: &ast::Package) -> hir::Package {
        self.lowerer
            .with(hir_assigner, self.resolver.names(), self.checker.table())
            .lower_package(package)
    }
}

/// Extends the `Package` with the contents of another `Package`.
/// `other` should not contain any `LocalItemId`s
/// that conflict with the current `Package`.
/// The entry expression from `other` will be ignored.
fn extend_hir(this: &mut hir::Package, mut other: hir::Package) {
    for (k, v) in other.items.drain() {
        this.items.insert(k, v);
    }

    this.stmts.extend(other.stmts);

    HirValidator::default().visit_package(this);
}

fn with_source(
    errors: Vec<qsc_parse::Error>,
    sources: &SourceMap,
    offset: u32,
) -> Vec<WithSource<compile::Error>> {
    errors
        .into_iter()
        .map(|e| {
            WithSource::from_map(
                sources,
                compile::Error(compile::ErrorKind::Parse(e.with_offset(offset))),
            )
        })
        .collect()
}
