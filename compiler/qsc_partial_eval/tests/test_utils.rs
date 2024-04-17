// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::Expect;
use qsc::{incremental::Compiler, PackageType};
use qsc_data_structures::language_features::LanguageFeatures;
use qsc_fir::fir::PackageStore;
use qsc_frontend::compile::{PackageStore as HirPackageStore, SourceMap, TargetCapabilityFlags};
use qsc_lowerer::{map_hir_package_to_fir, Lowerer};
use qsc_partial_eval::{partially_evaluate, ProgramEntry};
use qsc_rca::{Analyzer, PackageStoreComputeProperties};
use qsc_rir::rir::{BlockId, CallableId, Program};

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

#[must_use]
pub fn compile_and_partially_evaluate(source: &str) -> Program {
    let compilation_context = CompilationContext::new(source);
    let maybe_program = partially_evaluate(
        &compilation_context.fir_store,
        &compilation_context.compute_properties,
        &compilation_context.entry,
    );
    match maybe_program {
        Ok(program) => program,
        Err(error) => panic!("partial evaluation failed: {error:?}"),
    }
}

struct CompilationContext {
    fir_store: PackageStore,
    compute_properties: PackageStoreComputeProperties,
    entry: ProgramEntry,
}

impl CompilationContext {
    fn new(source: &str) -> Self {
        let source_map = SourceMap::new([("test".into(), source.into())], Some("".into()));
        let compiler = Compiler::new(
            true,
            source_map,
            PackageType::Exe,
            TargetCapabilityFlags::all(),
            LanguageFeatures::default(),
        )
        .expect("should be able to create a new compiler");
        let package_id = map_hir_package_to_fir(compiler.source_package_id());
        let fir_store = lower_hir_package_store(compiler.package_store());
        let analyzer = Analyzer::init(&fir_store);
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
        fir_store.insert(
            map_hir_package_to_fir(id),
            lowerer.lower_package(&unit.package),
        );
    }
    fir_store
}
