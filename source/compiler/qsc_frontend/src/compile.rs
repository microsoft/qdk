// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

pub mod preprocess;

use crate::{
    lower::{self, Lowerer},
    resolve::{self, GlobalScope, Locals, Names, Resolver},
    typeck::{self, Checker, Table},
};

use miette::{Diagnostic, Report};
use preprocess::TrackedName;
use qsc_ast::{
    assigner::Assigner as AstAssigner,
    ast::{self, TopLevelNode},
    mut_visit::MutVisitor,
    validate::Validator as AstValidator,
    visit::Visitor as _,
};
use qsc_data_structures::{
    error::WithSource,
    index_map::{self, IndexMap},
    language_features::LanguageFeatures,
    source::{SourceContents, SourceMap, SourceName},
    span::Span,
    target::{Profile, TargetCapabilityFlags},
};
use qsc_hir::{
    assigner::Assigner as HirAssigner,
    global::{self},
    hir::{self, PackageId},
    validate::Validator as HirValidator,
    visit::Visitor as _,
};
use std::{fmt::Debug, sync::Arc};
use thiserror::Error;

#[derive(Debug)]
pub struct CompileUnit {
    pub package: hir::Package,
    pub ast: AstPackage,
    pub assigner: HirAssigner,
    pub sources: SourceMap,
    pub errors: Vec<Error>,
    pub dropped_names: Vec<TrackedName>,
}

impl CompileUnit {
    #[must_use]
    pub fn new(package_id: PackageId) -> Self {
        Self {
            package: hir::Package::new(package_id),
            ast: Default::default(),
            assigner: Default::default(),
            sources: Default::default(),
            errors: Default::default(),
            dropped_names: Default::default(),
        }
    }

    pub fn expose(&mut self) {
        for (_item_id, item) in self.package.items.iter_mut() {
            item.visibility = hir::Visibility::Public;
        }
    }

    pub fn package_id(&self) -> PackageId {
        self.package.package_id
    }
}

#[derive(Debug, Default)]
pub struct AstPackage {
    pub package: ast::Package,
    pub tys: Table,
    pub names: Names,
    pub locals: Locals,
    pub globals: GlobalScope,
}

// the arc<str> is only `None` for the legacy stdlib, core, and an interpreter special case
pub type Dependencies = [(PackageId, Option<Arc<str>>)];

#[derive(Clone, Debug, Diagnostic, Error)]
#[diagnostic(transparent)]
#[error(transparent)]
pub struct Error(pub(super) ErrorKind);

#[derive(Clone, Debug, Diagnostic, Error)]
#[diagnostic(transparent)]
pub(super) enum ErrorKind {
    #[error("syntax error")]
    Parse(#[from] qsc_parse::Error),
    #[error("name error")]
    Resolve(#[from] resolve::Error),
    #[error("type error")]
    Type(#[from] typeck::Error),
    #[error(transparent)]
    Lower(#[from] lower::Error),
}

pub struct PackageStore {
    core: global::Table,
    units: IndexMap<PackageId, CompileUnit>,
    next_id: PackageId,
}

impl Debug for PackageStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "package store with {} units", self.units.iter().count())
    }
}

impl PackageStore {
    #[must_use]
    pub fn new(core: CompileUnit) -> Self {
        let table = global::iter_package(PackageId::CORE, &core.package).collect();
        let mut units = IndexMap::new();
        units.insert(PackageId::CORE, core);
        Self {
            core: table,
            units,
            next_id: PackageId::CORE.successor(),
        }
    }

    #[must_use]
    pub fn core(&self) -> &global::Table {
        &self.core
    }

    #[must_use]
    pub fn peek_package_id(&self) -> PackageId {
        self.next_id
    }

    pub fn insert(&mut self, unit: CompileUnit) -> PackageId {
        let id = self.next_id;
        assert_eq!(
            id,
            unit.package_id(),
            "The id of the inserted unit should match the next_id of the store."
        );
        self.next_id = id.successor();
        self.units.insert(id, unit);
        id
    }

    #[must_use]
    pub fn get(&self, id: PackageId) -> Option<&CompileUnit> {
        self.units.get(id)
    }

    #[must_use]
    pub fn iter(&self) -> Iter<'_> {
        Iter(self.units.iter())
    }

    /// "Opens" the package store. This inserts an empty
    /// package into the store, which will be considered
    /// the open package and which can be incrementally updated.
    #[must_use]
    pub fn open(mut self) -> OpenPackageStore {
        let id = self.next_id;
        self.next_id = id.successor();
        self.units.insert(id, CompileUnit::new(id));

        OpenPackageStore {
            store: self,
            open: id,
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.units.is_empty()
    }
}

impl<'a> IntoIterator for &'a PackageStore {
    type IntoIter = Iter<'a>;
    type Item = (qsc_hir::hir::PackageId, &'a CompileUnit);
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// A package store that contains one mutable `CompileUnit`.
pub struct OpenPackageStore {
    store: PackageStore,
    open: PackageId,
}

impl OpenPackageStore {
    /// Returns a reference to the underlying, immutable,
    /// package store.
    #[must_use]
    pub fn package_store(&self) -> &PackageStore {
        &self.store
    }

    /// Returns the ID of the open package.
    #[must_use]
    pub fn open_package_id(&self) -> PackageId {
        self.open
    }

    /// Returns a mutable reference to the open package,
    /// along with a reference to the core library that can be used
    /// to perform passes.
    #[must_use]
    pub fn get_open_mut(&mut self) -> (&global::Table, &mut CompileUnit) {
        let id = self.open;

        (
            &self.store.core,
            self.store
                .units
                .get_mut(id)
                .expect("open package id should exist in store"),
        )
    }

    /// Consumes the `OpenPackageStore` and returns a `PackageStore`
    /// along with the id of the formerly open package.
    #[must_use]
    pub fn into_package_store(self) -> (PackageStore, PackageId) {
        (self.store, self.open)
    }
}

pub struct Iter<'a>(index_map::Iter<'a, PackageId, CompileUnit>);

impl<'a> Iterator for Iter<'a> {
    type Item = (PackageId, &'a CompileUnit);

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

impl DoubleEndedIterator for Iter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.0.next_back()
    }
}

pub(super) struct Offsetter(pub(super) u32);

impl MutVisitor for Offsetter {
    fn visit_span(&mut self, span: &mut Span) {
        span.lo += self.0;
        span.hi += self.0;
    }
}

#[must_use]
pub fn compile(
    store: &PackageStore,
    dependencies: &Dependencies,
    sources: SourceMap,
    capabilities: TargetCapabilityFlags,
    language_features: LanguageFeatures,
) -> CompileUnit {
    let (ast_package, parse_errors) = parse_all(&sources, language_features);

    compile_ast(
        store,
        dependencies,
        ast_package,
        sources,
        capabilities,
        parse_errors,
    )
}

#[allow(clippy::module_name_repetitions)]
pub fn compile_ast(
    store: &PackageStore,
    dependencies: &Dependencies,
    mut ast_package: ast::Package,
    sources: SourceMap,
    capabilities: TargetCapabilityFlags,
    parse_errors: Vec<qsc_parse::Error>,
) -> CompileUnit {
    let mut cond_compile = preprocess::Conditional::new(capabilities);
    cond_compile.visit_package(&mut ast_package);
    let dropped_names = cond_compile.into_names();

    let mut remove_spans = preprocess::RemoveCircuitSpans::new(&sources);
    remove_spans.visit_package(&mut ast_package);

    let mut ast_assigner = AstAssigner::new();
    ast_assigner.visit_package(&mut ast_package);
    AstValidator::default().visit_package(&ast_package);
    let mut hir_assigner = HirAssigner::new();
    let ResolveResult {
        names,
        locals,
        globals,
        errors: name_errors,
    } = resolve_all(
        store,
        dependencies,
        &mut hir_assigner,
        &ast_package,
        dropped_names.clone(),
    );
    let (tys, ty_errors) = typeck_all(store, dependencies, &ast_package, &names);
    let package_id = store.peek_package_id();
    let mut lowerer = Lowerer::new(package_id);
    let package = lowerer
        .with(&mut hir_assigner, &names, &tys)
        .lower_package(&ast_package);
    HirValidator::default().visit_package(&package);
    let lower_errors = lowerer.drain_errors();

    let errors = parse_errors
        .into_iter()
        .map(Into::into)
        .chain(name_errors.into_iter().map(Into::into))
        .chain(ty_errors.into_iter().map(Into::into))
        .chain(lower_errors.into_iter().map(Into::into))
        .map(Error)
        .collect();

    CompileUnit {
        package,
        ast: AstPackage {
            package: ast_package,
            tys,
            names,
            locals,
            globals,
        },
        assigner: hir_assigner,
        sources,
        errors,
        dropped_names,
    }
}

/// Compiles the core library.
///
/// # Panics
///
/// Panics if the core library does not compile without errors.
#[must_use]
pub fn core() -> CompileUnit {
    let store = PackageStore {
        core: global::Table::default(),
        units: IndexMap::new(),
        next_id: PackageId::CORE,
    };

    let core: Vec<(SourceName, SourceContents)> = library::CORE_LIB
        .iter()
        .map(|(name, contents)| ((*name).into(), (*contents).into()))
        .collect();
    let sources = SourceMap::new(core, None);

    let mut unit = compile(
        &store,
        &[],
        sources,
        TargetCapabilityFlags::empty(),
        LanguageFeatures::default(),
    );
    assert_no_errors(&unit.sources, &mut unit.errors);
    unit
}

/// Compiles the standard library.
///
/// # Panics
///
/// Panics if the standard library does not compile without errors.
#[must_use]
pub fn std(store: &PackageStore, capabilities: TargetCapabilityFlags) -> CompileUnit {
    let std: Vec<(SourceName, SourceContents)> = library::STD_LIB
        .iter()
        .map(|(name, contents)| ((*name).into(), (*contents).into()))
        .collect();
    let sources = SourceMap::new(std, None);

    let mut unit = compile(
        store,
        &[(PackageId::CORE, None)],
        sources,
        capabilities,
        LanguageFeatures::default(),
    );
    assert_no_errors(&unit.sources, &mut unit.errors);
    unit
}

#[must_use]
pub fn parse_all(
    sources: &SourceMap,
    features: LanguageFeatures,
) -> (ast::Package, Vec<qsc_parse::Error>) {
    let mut namespaces = Vec::new();
    let mut errors = Vec::new();
    for source in sources.relative_sources() {
        let (source_namespaces, source_errors) =
            qsc_parse::namespaces(&source.contents, Some(&source.name), features);
        for mut namespace in source_namespaces {
            Offsetter(source.offset).visit_namespace(&mut namespace);
            namespaces.push(TopLevelNode::Namespace(namespace));
        }

        append_parse_errors(&mut errors, source.offset, source_errors);
    }

    let entry = sources
        .entry()
        .as_ref()
        .filter(|source| !source.contents.is_empty())
        .map(|source| {
            let (mut entry, entry_errors) = qsc_parse::expr(&source.contents, features);
            Offsetter(source.offset).visit_expr(&mut entry);
            append_parse_errors(&mut errors, source.offset, entry_errors);
            entry
        });

    let package = ast::Package {
        id: ast::NodeId::default(),
        nodes: namespaces.into_boxed_slice(),
        entry,
    };

    (package, errors)
}

#[must_use]
pub fn get_target_profile_from_entry_point(
    sources: &[(Arc<str>, Arc<str>)],
) -> Option<(Profile, Span)> {
    let (ast_package, parse_errors) = parse_all(
        &SourceMap::new(sources.iter().cloned(), None),
        LanguageFeatures::default(),
    );

    if !parse_errors.is_empty() {
        return None;
    }

    let mut check = preprocess::DetectEntryPointProfile::new();
    check.visit_package(&ast_package);
    check.profile
}

pub(crate) struct ResolveResult {
    pub names: Names,
    pub locals: Locals,
    pub globals: GlobalScope,
    pub errors: Vec<resolve::Error>,
}

fn resolve_all(
    store: &PackageStore,
    dependencies: &Dependencies,
    assigner: &mut HirAssigner,
    package: &ast::Package,
    mut dropped_names: Vec<TrackedName>,
) -> ResolveResult {
    let mut globals = resolve::GlobalTable::new();
    let mut errors = Vec::new();
    if let Some(unit) = store.get(PackageId::CORE) {
        globals.add_external_package(PackageId::CORE, &unit.package, store, None);
        dropped_names.extend(unit.dropped_names.iter().cloned());
    }

    for (id, alias) in dependencies {
        let unit = store
            .get(*id)
            .expect("dependency should be in package store before compilation");
        globals.add_external_package(*id, &unit.package, store, alias.as_deref());
        dropped_names.extend(unit.dropped_names.iter().cloned());
    }

    // bind all declarations in the package, but don't resolve imports/exports yet
    let package_id = store.peek_package_id();
    errors.extend(globals.add_local_package(assigner, package, package_id));
    let mut resolver = Resolver::new(package_id, globals, dropped_names);

    // resolve all symbols, binding imports/export names as they're resolved
    resolver.resolve(assigner, package);
    let (names, globals, locals, mut resolver_errors) = resolver.into_result();

    errors.append(&mut resolver_errors);

    ResolveResult {
        names,
        locals,
        globals,
        errors,
    }
}

fn typeck_all(
    store: &PackageStore,
    dependencies: &Dependencies,
    package: &ast::Package,
    names: &Names,
) -> (typeck::Table, Vec<typeck::Error>) {
    let mut globals = typeck::GlobalTable::new();
    if let Some(unit) = store.get(PackageId::CORE) {
        globals.add_external_package(PackageId::CORE, &unit.package, store);
    }

    for (id, _alias) in dependencies {
        let unit = store
            .get(*id)
            .expect("dependency should be added to package store before compilation");
        // we can ignore the dependency alias here, because the
        // typechecker doesn't do any name resolution -- it only operates on item ids.
        // because of this, the typechecker doesn't actually need to care about visibility
        // or the names of items at all.
        globals.add_external_package(*id, &unit.package, store);
    }

    let mut checker = Checker::new(globals);
    checker.check_package(names, package);
    checker.into_table()
}

fn append_parse_errors(
    errors: &mut Vec<qsc_parse::Error>,
    offset: u32,
    other: Vec<qsc_parse::Error>,
) {
    for error in other {
        errors.push(error.with_offset(offset));
    }
}

fn assert_no_errors(sources: &SourceMap, errors: &mut Vec<Error>) {
    if !errors.is_empty() {
        for error in errors.drain(..) {
            eprintln!("{:?}", Report::new(WithSource::from_map(sources, error)));
        }

        panic!("could not compile package");
    }
}
