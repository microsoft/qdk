// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_data_structures::target::{Profile, TargetCapabilityFlags};
use qsc_eval::val::Value;
use qsc_partial_eval::{
    PartialEvalConfig, Program, ProgramEntry, partially_evaluate, partially_evaluate_call,
};
use qsc_rca::PackageStoreComputeProperties;
use qsc_rir::{passes::check_and_transform, rir};

pub mod v1;
pub mod v2;

/// converts the given sources to RIR using the given language features.
pub fn fir_to_rir(
    fir_store: &qsc_fir::fir::PackageStore,
    capabilities: TargetCapabilityFlags,
    compute_properties: &PackageStoreComputeProperties,
    entry: &ProgramEntry,
    partial_eval_config: PartialEvalConfig,
) -> Result<(Program, Program), qsc_partial_eval::Error> {
    let mut program = get_rir_from_compilation(
        fir_store,
        compute_properties,
        entry,
        capabilities,
        partial_eval_config,
    )?;
    let orig = program.clone();
    check_and_transform(&mut program);
    Ok((orig, program))
}

/// converts the given sources to QIR using the given language features.
pub fn fir_to_qir(
    fir_store: &qsc_fir::fir::PackageStore,
    capabilities: TargetCapabilityFlags,
    compute_properties: &PackageStoreComputeProperties,
    entry: &ProgramEntry,
) -> Result<String, qsc_partial_eval::Error> {
    let mut program = get_rir_from_compilation(
        fir_store,
        compute_properties,
        entry,
        capabilities,
        PartialEvalConfig {
            generate_debug_metadata: false,
        },
    )?;
    check_and_transform(&mut program);
    if capabilities <= Profile::AdaptiveRIF.into() {
        Ok(v1::ToQir::<String>::to_qir(&program, &program))
    } else {
        Ok(v2::ToQir::<String>::to_qir(&program, &program))
    }
}

/// converts the given callable to QIR using the given arguments and language features.
pub fn fir_to_qir_from_callable(
    fir_store: &qsc_fir::fir::PackageStore,
    capabilities: TargetCapabilityFlags,
    compute_properties: &PackageStoreComputeProperties,
    callable: qsc_fir::fir::StoreItemId,
    args: Value,
) -> Result<String, qsc_partial_eval::Error> {
    let mut program = partially_evaluate_call(
        fir_store,
        compute_properties,
        callable,
        args,
        capabilities,
        PartialEvalConfig {
            generate_debug_metadata: false,
        },
    )?;
    check_and_transform(&mut program);
    if capabilities <= Profile::AdaptiveRIF.into() {
        Ok(v1::ToQir::<String>::to_qir(&program, &program))
    } else {
        Ok(v2::ToQir::<String>::to_qir(&program, &program))
    }
}

/// converts the given callable to RIR using the given arguments and language features.
pub fn fir_to_rir_from_callable(
    fir_store: &qsc_fir::fir::PackageStore,
    capabilities: TargetCapabilityFlags,
    compute_properties: &PackageStoreComputeProperties,
    callable: qsc_fir::fir::StoreItemId,
    args: Value,
    partial_eval_config: PartialEvalConfig,
) -> Result<(Program, Program), qsc_partial_eval::Error> {
    let mut program = partially_evaluate_call(
        fir_store,
        compute_properties,
        callable,
        args,
        capabilities,
        partial_eval_config,
    )?;
    let orig = program.clone();
    check_and_transform(&mut program);
    Ok((orig, program))
}

fn get_rir_from_compilation(
    fir_store: &qsc_fir::fir::PackageStore,
    compute_properties: &PackageStoreComputeProperties,
    entry: &ProgramEntry,
    capabilities: TargetCapabilityFlags,
    partial_eval_config: PartialEvalConfig,
) -> Result<rir::Program, qsc_partial_eval::Error> {
    partially_evaluate(
        fir_store,
        compute_properties,
        entry,
        capabilities,
        partial_eval_config,
    )
}
