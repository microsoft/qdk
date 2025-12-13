// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod v1;
mod v2;

use qsc_data_structures::target::TargetCapabilityFlags;
use qsc_eval::val::Value;
use qsc_lowerer::map_hir_package_to_fir;
use qsc_partial_eval::{ProgramEntry, partially_evaluate, partially_evaluate_call};
use qsc_rca::PackageStoreComputeProperties;
use qsc_rir::{
    passes::check_and_transform,
    rir::{self, Program},
};

fn lower_store(package_store: &qsc_frontend::compile::PackageStore) -> qsc_fir::fir::PackageStore {
    let mut fir_store = qsc_fir::fir::PackageStore::new();
    for (id, unit) in package_store {
        let package = qsc_lowerer::Lowerer::new().lower_package(&unit.package, &fir_store);
        fir_store.insert(map_hir_package_to_fir(id), package);
    }
    fir_store
}

/// converts the given sources to QIR using the given language features.
pub fn hir_to_qir(
    package_store: &qsc_frontend::compile::PackageStore,
    capabilities: TargetCapabilityFlags,
    compute_properties: Option<PackageStoreComputeProperties>,
    entry: &ProgramEntry,
) -> Result<String, qsc_partial_eval::Error> {
    let fir_store = lower_store(package_store);
    fir_to_qir(&fir_store, capabilities, compute_properties, entry)
}

/// converts the given sources to RIR using the given language features.
pub fn fir_to_rir(
    fir_store: &qsc_fir::fir::PackageStore,
    capabilities: TargetCapabilityFlags,
    compute_properties: Option<PackageStoreComputeProperties>,
    entry: &ProgramEntry,
) -> Result<(Program, Program), qsc_partial_eval::Error> {
    let mut program = get_rir_from_compilation(fir_store, compute_properties, entry, capabilities)?;
    let orig = program.clone();
    check_and_transform(&mut program);
    Ok((orig, program))
}

/// converts the given sources to QIR using the given language features.
pub fn fir_to_qir(
    fir_store: &qsc_fir::fir::PackageStore,
    capabilities: TargetCapabilityFlags,
    compute_properties: Option<PackageStoreComputeProperties>,
    entry: &ProgramEntry,
) -> Result<String, qsc_partial_eval::Error> {
    let mut program = get_rir_from_compilation(fir_store, compute_properties, entry, capabilities)?;
    check_and_transform(&mut program);
    if capabilities.is_advanced() {
        Ok(v2::ToQir::<String>::to_qir(&program, &program))
    } else {
        Ok(v1::ToQir::<String>::to_qir(&program, &program))
    }
}

/// converts the given callable to QIR using the given arguments and language features.
pub fn fir_to_qir_from_callable(
    fir_store: &qsc_fir::fir::PackageStore,
    capabilities: TargetCapabilityFlags,
    compute_properties: Option<PackageStoreComputeProperties>,
    callable: qsc_fir::fir::StoreItemId,
    args: Value,
) -> Result<String, qsc_partial_eval::Error> {
    let compute_properties = compute_properties.unwrap_or_else(|| {
        let analyzer = qsc_rca::Analyzer::init(fir_store, capabilities);
        analyzer.analyze_all()
    });

    let mut program =
        partially_evaluate_call(fir_store, &compute_properties, callable, args, capabilities)?;
    check_and_transform(&mut program);
    if capabilities.is_advanced() {
        Ok(v2::ToQir::<String>::to_qir(&program, &program))
    } else {
        Ok(v1::ToQir::<String>::to_qir(&program, &program))
    }
}

fn get_rir_from_compilation(
    fir_store: &qsc_fir::fir::PackageStore,
    compute_properties: Option<PackageStoreComputeProperties>,
    entry: &ProgramEntry,
    capabilities: TargetCapabilityFlags,
) -> Result<rir::Program, qsc_partial_eval::Error> {
    let compute_properties = compute_properties.unwrap_or_else(|| {
        let analyzer = qsc_rca::Analyzer::init(fir_store, capabilities);
        analyzer.analyze_all()
    });

    partially_evaluate(fir_store, &compute_properties, entry, capabilities)
}
