// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_data_structures::target::{Profile, TargetCapabilityFlags};
use qsc_eval::val::Value;
use qsc_partial_eval::{
    PartialEvalConfig, Program, ProgramEntry, partially_evaluate, partially_evaluate_call,
};
use qsc_rca::PackageStoreComputeProperties;
use qsc_rir::{
    passes::{check_and_transform, check_and_transform_with},
    rir,
};

use crate::pass_pipeline::OptimizationLevel;
use crate::rir_optimize::{StageReport, optimize};

pub mod name;
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
    // No optimization here. This returns RIR for inspection, and the circuit
    // diagram is built from it, so cancelling a pair would show a user fewer
    // gates than they wrote.
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
    fir_to_qir_at(
        fir_store,
        capabilities,
        compute_properties,
        entry,
        OptimizationLevel::None,
    )
}

/// converts the given sources to QIR, optimizing as far as `level` allows.
///
/// # Errors
///
/// Returns the same errors as [`fir_to_qir`].
pub fn fir_to_qir_at(
    fir_store: &qsc_fir::fir::PackageStore,
    capabilities: TargetCapabilityFlags,
    compute_properties: &PackageStoreComputeProperties,
    entry: &ProgramEntry,
    level: OptimizationLevel,
) -> Result<String, qsc_partial_eval::Error> {
    fir_to_qir_reporting(fir_store, capabilities, compute_properties, entry, level)
        .map(|(qir, _)| qir)
}

/// converts the given sources to QIR, reporting what the optimization stage did.
///
/// The stage declines silently by design, so a caller that needs to know
/// whether it ran, and on what, has to be told rather than infer it from the
/// output.
///
/// # Errors
///
/// Returns the same errors as [`fir_to_qir`].
pub fn fir_to_qir_reporting(
    fir_store: &qsc_fir::fir::PackageStore,
    capabilities: TargetCapabilityFlags,
    compute_properties: &PackageStoreComputeProperties,
    entry: &ProgramEntry,
    level: OptimizationLevel,
) -> Result<(String, StageReport), qsc_partial_eval::Error> {
    let mut program = get_rir_from_compilation(
        fir_store,
        compute_properties,
        entry,
        capabilities,
        PartialEvalConfig {
            generate_debug_metadata: false,
        },
    )?;
    let mut report = StageReport::default();
    check_and_transform_with(&mut program, |program| report = optimize(program, level));
    let qir = if capabilities <= Profile::AdaptiveRIF.into() {
        v1::ToQir::<String>::to_qir(&program, &program)
    } else {
        v2::ToQir::<String>::to_qir(&program, &program)
    };
    Ok((qir, report))
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
    check_and_transform_with(&mut program, |program| {
        drop(optimize(program, OptimizationLevel::None));
    });
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
    // No optimization here, for the same reason as `fir_to_rir`.
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
