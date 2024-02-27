// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use miette::{Diagnostic, Report};
use qsc_data_structures::language_features::LanguageFeatures;
use qsc_frontend::{
    compile::{CompileUnit, PackageStore, RuntimeCapabilityFlags, SourceMap},
    error::WithSource,
};
use qsc_hir::hir::PackageId;
use qsc_passes::{run_core_passes, run_default_passes, PackageType};
use thiserror::Error;

pub type Error = WithSource<ErrorKind>;

#[derive(Clone, Debug, Diagnostic, Error)]
#[diagnostic(transparent)]
#[error(transparent)]
pub enum ErrorKind {
    Frontend(#[from] qsc_frontend::compile::Error),
    Pass(#[from] qsc_passes::Error),
}

#[must_use]
pub fn compile(
    store: &PackageStore,
    dependencies: &[PackageId],
    sources: SourceMap,
    package_type: PackageType,
    capabilities: RuntimeCapabilityFlags,
    language_features: LanguageFeatures,
) -> (CompileUnit, Vec<Error>) {
    let mut unit = qsc_frontend::compile::compile(
        store,
        dependencies,
        sources,
        capabilities,
        language_features,
    );
    let mut errors = Vec::new();
    for error in unit.errors.drain(..) {
        errors.push(WithSource::from_map(&unit.sources, error.into()));
    }

    if errors.is_empty() {
        for error in run_default_passes(store.core(), &mut unit, package_type, capabilities) {
            errors.push(WithSource::from_map(&unit.sources, error.into()));
        }
    }

    (unit, errors)
}

/// Compiles the core library.
///
/// # Panics
///
/// Panics if the core library does not compile without errors.
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
pub fn std(store: &PackageStore, capabilities: RuntimeCapabilityFlags) -> CompileUnit {
    let mut unit = qsc_frontend::compile::std(store, capabilities);
    let pass_errors = run_default_passes(store.core(), &mut unit, PackageType::Lib, capabilities);
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
