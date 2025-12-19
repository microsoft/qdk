// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    PackageStore, TargetCapabilityFlags,
    compile::{self, Error, ErrorKind, package_store_with_stdlib},
    hir::PackageId,
};
use qsc_circuit::circuit_to_qsharp::circuits_to_qsharp;
use qsc_data_structures::{
    error::WithSource, language_features::LanguageFeatures, source::SourceMap, target::Profile,
};
use qsc_passes::PackageType;
use qsc_project::PackageGraphSources;
use rustc_hash::FxHashMap;
use std::sync::Arc;

#[cfg(test)]
mod tests;

/// A program that is ready to be built -- dependencies have all been built, and the user code is ready.
#[derive(Debug)]
pub struct BuildableProgram {
    pub store: PackageStore,
    pub user_code: qsc_project::PackageInfo,
    pub user_code_dependencies: Vec<(PackageId, Option<Arc<str>>)>,
    pub dependency_errors: Vec<compile::Error>,
    pub capabilities: TargetCapabilityFlags,
}

impl BuildableProgram {
    /// Compiles all dependencies, populates the `PackageStore`, and prepares a `BuildableProgram` for the user code.
    #[must_use]
    pub fn new(
        capabilities: TargetCapabilityFlags,
        package_graph_sources: PackageGraphSources,
    ) -> Self {
        prepare_package_store(capabilities, package_graph_sources)
    }
}

/// Converts circuit files to Q# source code.
fn convert_circuit_sources(
    sources: Vec<(Arc<str>, Arc<str>)>,
    errors: &mut Vec<Error>,
) -> Vec<(Arc<str>, Arc<str>)> {
    let mut processed_sources: Vec<(Arc<str>, Arc<str>)> = Vec::new();

    for (name, content) in sources {
        let name_path = std::path::Path::new(name.as_ref());

        // Check if the file extension is "qsc"
        if name_path.extension().and_then(|ext| ext.to_str()) == Some("qsc") {
            let file_stem = name_path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or(name.as_ref());

            // Convert circuit files to Q# source code
            match circuits_to_qsharp(file_stem, content.as_ref()) {
                Ok(transformed_content) => {
                    processed_sources.push((name, Arc::from(transformed_content)));
                }
                Err(error_message) => {
                    errors.push(Error::from_map(
                        &SourceMap::default(),
                        ErrorKind::CircuitParse(error_message),
                    ));
                }
            }
        } else {
            // Leave other files unchanged
            processed_sources.push((name, content));
        }
    }

    processed_sources
}

/// Given a `PackageGraphSources`, checks if the entry profile is set in the source code.
/// If it is, returns the profile; otherwise, returns `None`.
/// This will also report an error if the entry profile is set in a project file.
#[must_use]
pub fn get_target_profile_from_entry_point(
    package_graph_sources: &PackageGraphSources,
    errors: &mut Vec<WithSource<ErrorKind>>,
) -> Option<Profile> {
    let sources = package_graph_sources.root.sources.clone();

    let filtered_sources: Vec<_> = sources
        .into_iter()
        .filter(|(name, _)| {
            std::path::Path::new(name.as_ref())
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("qs"))
        })
        .collect();

    // Check if the entry profile is set in the source code.
    let target_profile =
        qsc_frontend::compile::get_target_profile_from_entry_point(&filtered_sources);

    if let Some((profile, span)) = target_profile {
        // If the entry profile is set, we need to ensure that the user code is compiled with it.
        if package_graph_sources.has_manifest {
            errors.push(Error::from_map(
                &SourceMap::new(filtered_sources, None),
                ErrorKind::EntryPointProfileInProject(span),
            ));
            None
        } else {
            Some(profile)
        }
    } else {
        None
    }
}

/// Given a program config, prepare the package store by compiling all dependencies in the correct order and inserting them.
#[must_use]
pub fn prepare_package_store(
    mut capabilities: TargetCapabilityFlags,
    package_graph_sources: PackageGraphSources,
) -> BuildableProgram {
    let mut dependency_errors = Vec::new();

    let target_profile =
        get_target_profile_from_entry_point(&package_graph_sources, &mut dependency_errors);

    // If the entry profile is set, we need to ensure that the user code is compiled with it.
    if let Some(profile) = target_profile {
        capabilities = profile.into();
    }

    let (std_id, mut package_store) = package_store_with_stdlib(capabilities);

    let mut canonical_package_identifier_to_package_id_mapping = FxHashMap::default();

    let (ordered_packages, user_code) = package_graph_sources.compilation_order();

    let ordered_packages = if let Ok(o) = ordered_packages {
        o
    } else {
        // If there was a cycle in the dependencies, we treat the compilation as if
        // there were no dependencies, and report the error upwards
        dependency_errors.push(Error::from_map(
            &SourceMap::default(),
            ErrorKind::DependencyCycle,
        ));
        vec![]
    };

    for (package_name, package_to_compile) in ordered_packages {
        let sources = convert_circuit_sources(package_to_compile.sources, &mut dependency_errors);

        let source_map = SourceMap::new(sources, None);
        let dependencies = package_to_compile
            .dependencies
            .iter()
            .filter_map(|(alias, key)| {
                canonical_package_identifier_to_package_id_mapping
                    .get(key)
                    .copied()
                    .map(|pkg| (alias.clone(), pkg))
            })
            .collect::<FxHashMap<_, _>>();
        let dependencies = dependencies
            .iter()
            .map(|(alias, b)| (*b, Some(alias.clone())))
            .chain(std::iter::once((std_id, None)))
            .collect::<Vec<_>>();
        let (compile_unit, mut this_errors) = compile::compile(
            &package_store,
            &dependencies[..],
            source_map,
            PackageType::Lib,
            capabilities,
            LanguageFeatures::from_iter(package_to_compile.language_features),
        );

        let package_id = package_store.insert(compile_unit);
        if !this_errors.is_empty() {
            dependency_errors.append(&mut this_errors);
        }

        canonical_package_identifier_to_package_id_mapping.insert(package_name, package_id);
    }

    // Convert circuit files in user code to generated Q#
    let converted_user_code_sources =
        convert_circuit_sources(user_code.sources, &mut dependency_errors);
    let user_code = qsc_project::PackageInfo {
        sources: converted_user_code_sources,
        language_features: user_code.language_features,
        dependencies: user_code.dependencies,
        package_type: user_code.package_type,
    };

    let user_code_dependencies = user_code
        .dependencies
        .iter()
        .filter_map(|(alias, key)| {
            canonical_package_identifier_to_package_id_mapping
                .get(key)
                .copied()
                .map(|pkg| (pkg, Some(alias.clone())))
        })
        .chain(std::iter::once((std_id, None)))
        .collect::<Vec<_>>();

    BuildableProgram {
        store: package_store,
        dependency_errors,
        user_code,
        user_code_dependencies,
        capabilities,
    }
}
