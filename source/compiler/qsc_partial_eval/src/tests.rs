// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod arrays;
mod assigns;
mod bindings;
mod branching;
mod calls;
mod classical_args;
mod dynamic_vars;
mod intrinsics;
mod loops;
mod misc;
mod operators;
mod output_recording;
mod qubits;
mod results;
mod returns;

use crate::{Error, ProgramEntry, partially_evaluate};
use expect_test::Expect;
use qsc::{PackageType, incremental::Compiler};
use qsc_data_structures::{
    language_features::LanguageFeatures, source::SourceMap, target::TargetCapabilityFlags,
};
use qsc_fir::fir::PackageStore;
use qsc_frontend::compile::PackageStore as HirPackageStore;
use qsc_lowerer::{Lowerer, map_hir_package_to_fir};
use qsc_rca::{Analyzer, PackageStoreComputeProperties};
use qsc_rir::{
    passes::check_and_transform,
    rir::{BlockId, CallableId, Program},
};

pub fn assert_block_instructions(program: &Program, block_id: BlockId, expected_insts: &Expect) {
    let block = program.get_block(block_id);
    expected_insts.assert_eq(&block.to_string());
}

pub fn assert_blocks(program: &Program, expected_blocks: &Expect) {
    let all_blocks = program
        .blocks
        .iter()
        .fold("Blocks:".to_string(), |acc, (id, block)| {
            acc + &format!("\nBlock {}:", id.0) + &block.to_string()
        });
    expected_blocks.assert_eq(&all_blocks);
}

pub fn assert_callable(program: &Program, callable_id: CallableId, expected_callable: &Expect) {
    let actual_callable = program.get_callable(callable_id);
    expected_callable.assert_eq(&actual_callable.to_string());
}

pub fn assert_error(error: &Error, expected_error: &Expect) {
    expected_error.assert_eq(format!("{error:?}").as_str());
}

#[must_use]
pub fn get_partial_evaluation_error(source: &str) -> Error {
    let capabilities = TargetCapabilityFlags::Adaptive
        | TargetCapabilityFlags::IntegerComputations
        | TargetCapabilityFlags::FloatingPointComputations;
    let maybe_program = compile_and_partially_evaluate(source, capabilities);
    match maybe_program {
        Ok(_) => panic!("partial evaluation succeeded"),
        Err(error) => error,
    }
}

#[must_use]
pub fn get_partial_evaluation_error_with_capabilities(
    source: &str,
    capabilities: TargetCapabilityFlags,
) -> Error {
    let maybe_program = compile_and_partially_evaluate(source, capabilities);
    match maybe_program {
        Ok(_) => panic!("partial evaluation succeeded"),
        Err(error) => error,
    }
}

#[must_use]
pub fn get_rir_program(source: &str) -> Program {
    let capabilities = TargetCapabilityFlags::Adaptive
        | TargetCapabilityFlags::IntegerComputations
        | TargetCapabilityFlags::FloatingPointComputations;
    let maybe_program = compile_and_partially_evaluate(source, capabilities);
    match maybe_program {
        Ok(program) => {
            // Verify the program can go through transformations.
            check_and_transform(&mut program.clone());
            program
        }
        Err(error) => panic!("partial evaluation failed: {error:?}"),
    }
}

#[must_use]
pub fn get_rir_program_with_capabilities(
    source: &str,
    capabilities: TargetCapabilityFlags,
) -> Program {
    let maybe_program = compile_and_partially_evaluate(source, capabilities);
    match maybe_program {
        Ok(program) => program,
        Err(error) => panic!("partial evaluation failed: {error:?}"),
    }
}

fn compile_and_partially_evaluate(
    source: &str,
    capabilities: TargetCapabilityFlags,
) -> Result<Program, Error> {
    let compilation_context = CompilationContext::new(source, capabilities);
    partially_evaluate(
        &compilation_context.fir_store,
        &compilation_context.compute_properties,
        &compilation_context.entry,
        capabilities,
    )
}

struct CompilationContext {
    fir_store: PackageStore,
    compute_properties: PackageStoreComputeProperties,
    entry: ProgramEntry,
}

impl CompilationContext {
    fn new(source: &str, capabilities: TargetCapabilityFlags) -> Self {
        let source_map = SourceMap::new([("test".into(), source.into())], Some("".into()));
        let (std_id, store) = qsc::compile::package_store_with_stdlib(capabilities);
        let compiler = Compiler::new(
            source_map,
            PackageType::Exe,
            capabilities,
            LanguageFeatures::default(),
            store,
            &[(std_id, None)],
        )
        .expect("should be able to create a new compiler");
        let package_id = map_hir_package_to_fir(compiler.source_package_id());
        let fir_store = lower_hir_package_store(compiler.package_store());
        let analyzer = Analyzer::init(&fir_store, capabilities);
        let compute_properties = analyzer.analyze_all();
        let package = fir_store.get(package_id);
        let entry = ProgramEntry {
            exec_graph: package.entry_exec_graph.clone(),
            expr: (
                package_id,
                package
                    .entry
                    .expect("package must have an entry expression"),
            )
                .into(),
        };

        Self {
            fir_store,
            compute_properties,
            entry,
        }
    }
}

fn lower_hir_package_store(hir_package_store: &HirPackageStore) -> PackageStore {
    let mut fir_store = PackageStore::new();
    for (id, unit) in hir_package_store {
        let mut lowerer = Lowerer::new();
        let lowered_package = lowerer.lower_package(&unit.package, &fir_store);
        fir_store.insert(map_hir_package_to_fir(id), lowered_package);
    }
    fir_store
}
