// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use miette::{Diagnostic, Report};
use qsc_data_structures::{
    error::WithSource, language_features::LanguageFeatures, source::SourceMap, span::Span,
    target::TargetCapabilityFlags,
};
pub use qsc_frontend::compile::Dependencies;
use qsc_frontend::compile::{CompileUnit, PackageStore};
pub use qsc_frontend::typeck::{TyInfo, TyInfoKind};
use qsc_passes::{PackageType, PassContext, run_core_passes, run_default_passes};
use thiserror::Error;

pub type Error = WithSource<ErrorKind>;

/// Attaches a FIR transform diagnostic to the source map owned by the package
/// that produced its labels.
#[must_use]
pub fn attach_fir_transform_source(
    store: &PackageStore,
    diagnostic: qsc_fir_transforms::PipelineError,
) -> WithSource<qsc_fir_transforms::PipelineError> {
    let owner = diagnostic.owner();
    let package_id = qsc_lowerer::map_fir_package_to_hir(owner);
    let unit = store.get(package_id).unwrap_or_else(|| {
        panic!(
            "FIR transform diagnostic owner {owner:?} maps to HIR package {package_id:?}, \
             which must exist in the package store before source attachment"
        )
    });
    WithSource::from_map(&unit.sources, diagnostic)
}

#[derive(Clone, Debug, Diagnostic, Error)]
#[error(transparent)]
/// `ErrorKind` represents the different kinds of errors that can occur in the compiler.
/// Each variant of the enum corresponds to a different stage of the compilation process.
pub enum ErrorKind {
    /// `Frontend` variant represents errors that occur during the frontend stage of the compiler.
    /// These errors are typically related to syntax and semantic checks.
    #[diagnostic(transparent)]
    Frontend(#[from] qsc_frontend::compile::Error),

    /// `Pass` variant represents errors that occur during the `qsc_passes` stage of the compiler.
    /// These errors are typically related to optimization, transformation, code generation, passes,
    /// and static analysis passes.
    #[diagnostic(transparent)]
    Pass(#[from] qsc_passes::Error),

    /// Errors from FIR-level transforms (return unification, defunctionalization,
    /// monomorphization) that run before capability checking.
    #[diagnostic(transparent)]
    FirTransform(#[from] qsc_fir_transforms::PipelineError),

    /// `Lint` variant represents lints generated during the linting stage. These diagnostics are
    /// typically emitted from the language server and happens after all other compilation passes.
    #[diagnostic(transparent)]
    Lint(#[from] qsc_linter::Lint),

    #[error("Cycle in dependency graph")]
    /// `DependencyCycle` occurs when there is a cycle in the dependency graph.
    DependencyCycle,

    #[error("{0}")]
    /// `CircuitParse` variant represents errors that occur while parsing circuit files.
    CircuitParse(String),

    /// `OpenQASM` compilation errors.
    #[diagnostic(transparent)]
    OpenQasm(#[from] crate::openqasm::error::Error),

    #[error(
        "The @EntryPoint attribute with a profile argument is not allowed in a Q# project (with qsharp.json). Please specify the profile in qsharp.json instead."
    )]
    EntryPointProfileInProject(#[label] Span),
}

/// Compiles a package from its AST representation.
#[must_use]
#[allow(clippy::module_name_repetitions)]
pub fn compile_ast(
    store: &PackageStore,
    dependencies: &Dependencies,
    ast_package: qsc_ast::ast::Package,
    sources: SourceMap,
    package_type: PackageType,
    capabilities: TargetCapabilityFlags,
) -> (CompileUnit, Vec<Error>) {
    let unit = qsc_frontend::compile::compile_ast(
        store,
        dependencies,
        ast_package,
        sources,
        capabilities,
        vec![],
    );
    process_compile_unit(store, package_type, unit)
}

/// Compiles a package from its source representation.
#[must_use]
pub fn compile(
    store: &PackageStore,
    dependencies: &Dependencies,
    sources: SourceMap,
    package_type: PackageType,
    capabilities: TargetCapabilityFlags,
    language_features: LanguageFeatures,
) -> (CompileUnit, Vec<Error>) {
    let unit = qsc_frontend::compile::compile(
        store,
        dependencies,
        sources,
        capabilities,
        language_features,
    );
    process_compile_unit(store, package_type, unit)
}

#[must_use]
pub fn compile_with_pass_context(
    store: &PackageStore,
    dependencies: &Dependencies,
    sources: SourceMap,
    package_type: PackageType,
    capabilities: TargetCapabilityFlags,
    language_features: LanguageFeatures,
    pass_context: &mut PassContext,
) -> (CompileUnit, Vec<Error>) {
    let unit = qsc_frontend::compile::compile(
        store,
        dependencies,
        sources,
        capabilities,
        language_features,
    );
    process_compile_unit_with_pass_context(store, package_type, unit, pass_context)
}

#[must_use]
#[allow(clippy::module_name_repetitions)]
fn process_compile_unit(
    store: &PackageStore,
    package_type: PackageType,
    unit: CompileUnit,
) -> (CompileUnit, Vec<Error>) {
    let mut pass_context = PassContext::default();
    process_compile_unit_with_pass_context(store, package_type, unit, &mut pass_context)
}

#[allow(clippy::module_name_repetitions)]
fn process_compile_unit_with_pass_context(
    store: &PackageStore,
    package_type: PackageType,
    mut unit: CompileUnit,
    pass_context: &mut PassContext,
) -> (CompileUnit, Vec<Error>) {
    let mut errors = Vec::new();
    for error in unit.errors.drain(..) {
        errors.push(WithSource::from_map(&unit.sources, error.into()));
    }

    if errors.is_empty() {
        for error in pass_context.run_default_passes(
            &mut unit.package,
            &mut unit.assigner,
            store.core(),
            package_type,
        ) {
            errors.push(WithSource::from_map(&unit.sources, error.into()));
        }
    }

    (unit, errors)
}

#[must_use]
pub fn package_store_with_stdlib(
    capabilities: TargetCapabilityFlags,
) -> (qsc_hir::hir::PackageId, PackageStore) {
    let mut store = PackageStore::new(core());
    let std_id = store.insert(std(&store, capabilities));
    (std_id, store)
}

/// Compiles the core library.
///
/// # Panics
///
/// Panics if the core library compiles with errors.
#[must_use]
pub fn core() -> CompileUnit {
    let mut unit = qsc_frontend::compile::core();
    let pass_errors = run_core_passes(&mut unit);
    if pass_errors.is_empty() {
        unit
    } else {
        for error in pass_errors {
            let report = Report::new(WithSource::from_map(&unit.sources, error));
            eprintln!("{report:?}");
        }

        panic!("could not compile core library")
    }
}

/// Compiles the standard library.
///
/// # Panics
///
/// Panics if the standard library does not compile without errors.
#[must_use]
pub fn std(store: &PackageStore, capabilities: TargetCapabilityFlags) -> CompileUnit {
    let mut unit = qsc_frontend::compile::std(store, capabilities);
    let pass_errors = run_default_passes(store.core(), &mut unit, PackageType::Lib);
    if pass_errors.is_empty() {
        unit
    } else {
        for error in pass_errors {
            let report = Report::new(WithSource::from_map(&unit.sources, error));
            eprintln!("{report:?}");
        }

        panic!("could not compile standard library")
    }
}
