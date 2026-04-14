// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_data_structures::{
    span::Span,
    target::{Profile, TargetCapabilityFlags},
};
use qsc_eval::{PackageSpan, val::Value};
use qsc_llvm::qir::QirProfile;
use qsc_lowerer::map_fir_package_to_hir;
use qsc_partial_eval::{
    PartialEvalConfig, Program, ProgramEntry, partially_evaluate, partially_evaluate_call,
};
use qsc_rca::PackageStoreComputeProperties;
use qsc_rir::{passes::check_and_transform, rir};

mod common;

/// converts the given sources to RIR using the given language features.
pub fn fir_to_rir(
    fir_store: &qsc_fir::fir::PackageStore,
    capabilities: TargetCapabilityFlags,
    compute_properties: Option<PackageStoreComputeProperties>,
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
    compute_properties: Option<PackageStoreComputeProperties>,
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
    let module = build_module(&program, capabilities);
    #[cfg(debug_assertions)]
    {
        let ir_errors = qsc_llvm::validate_ir(&module);
        assert!(
            ir_errors.is_empty(),
            "codegen produced invalid IR in fir_to_qir: {ir_errors:?}"
        );
    }
    Ok(qsc_llvm::write_module_to_string(&module))
}

/// converts the given sources to QIR bitcode using the given language features.
pub fn fir_to_qir_bitcode(
    fir_store: &qsc_fir::fir::PackageStore,
    capabilities: TargetCapabilityFlags,
    compute_properties: Option<PackageStoreComputeProperties>,
    entry: &ProgramEntry,
) -> Result<Vec<u8>, qsc_partial_eval::Error> {
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
    let module = build_module(&program, capabilities);
    #[cfg(debug_assertions)]
    {
        let ir_errors = qsc_llvm::validate_ir(&module);
        assert!(
            ir_errors.is_empty(),
            "codegen produced invalid IR in fir_to_qir_bitcode: {ir_errors:?}"
        );
    }
    qsc_llvm::try_write_bitcode(&module).map_err(|error| bitcode_write_error(entry, &error))
}

fn bitcode_write_error(
    entry: &ProgramEntry,
    error: &qsc_llvm::WriteError,
) -> qsc_partial_eval::Error {
    qsc_partial_eval::Error::Unexpected(
        format!("QIR bitcode emission failed: {error}"),
        PackageSpan {
            package: map_fir_package_to_hir(entry.expr.package),
            span: Span::default(),
        },
    )
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

    let mut program = partially_evaluate_call(
        fir_store,
        &compute_properties,
        callable,
        args,
        capabilities,
        PartialEvalConfig {
            generate_debug_metadata: false,
        },
    )?;
    check_and_transform(&mut program);
    let module = build_module(&program, capabilities);
    #[cfg(debug_assertions)]
    {
        let ir_errors = qsc_llvm::validate_ir(&module);
        assert!(
            ir_errors.is_empty(),
            "codegen produced invalid IR in fir_to_qir_from_callable: {ir_errors:?}"
        );
    }
    Ok(qsc_llvm::write_module_to_string(&module))
}

/// converts the given callable to RIR using the given arguments and language features.
pub fn fir_to_rir_from_callable(
    fir_store: &qsc_fir::fir::PackageStore,
    capabilities: TargetCapabilityFlags,
    compute_properties: Option<PackageStoreComputeProperties>,
    callable: qsc_fir::fir::StoreItemId,
    args: Value,
    partial_eval_config: PartialEvalConfig,
) -> Result<(Program, Program), qsc_partial_eval::Error> {
    let compute_properties = compute_properties.unwrap_or_else(|| {
        let analyzer = qsc_rca::Analyzer::init(fir_store, capabilities);
        analyzer.analyze_all()
    });

    let mut program = partially_evaluate_call(
        fir_store,
        &compute_properties,
        callable,
        args,
        capabilities,
        partial_eval_config,
    )?;
    let orig = program.clone();
    check_and_transform(&mut program);
    Ok((orig, program))
}

fn build_module(
    program: &rir::Program,
    capabilities: TargetCapabilityFlags,
) -> qsc_llvm::model::Module {
    let profile = if capabilities <= Profile::AdaptiveRIF.into() {
        if program.config.is_base() {
            QirProfile::BaseV1
        } else {
            QirProfile::AdaptiveV1
        }
    } else {
        QirProfile::AdaptiveV2
    };
    common::build_qir_module(program, profile)
}

fn get_rir_from_compilation(
    fir_store: &qsc_fir::fir::PackageStore,
    compute_properties: Option<PackageStoreComputeProperties>,
    entry: &ProgramEntry,
    capabilities: TargetCapabilityFlags,
    partial_eval_config: PartialEvalConfig,
) -> Result<rir::Program, qsc_partial_eval::Error> {
    let compute_properties = compute_properties.unwrap_or_else(|| {
        let analyzer = qsc_rca::Analyzer::init(fir_store, capabilities);
        analyzer.analyze_all()
    });

    partially_evaluate(
        fir_store,
        &compute_properties,
        entry,
        capabilities,
        partial_eval_config,
    )
}
