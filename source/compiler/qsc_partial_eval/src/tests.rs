// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod arrays;
mod assigns;
mod bindings;
mod branching;
mod calls;
mod classical_args;
mod debug_metadata;
mod dynamic_vars;
mod intrinsics;
mod loops;
mod misc;
mod operators;
mod output_recording;
mod qubits;
mod results;
mod returns;

use crate::{Error, PartialEvalConfig, ProgramEntry, partially_evaluate};
use expect_test::Expect;
use qsc::{PackageType, incremental::Compiler};
use qsc_data_structures::{
    language_features::LanguageFeatures,
    line_column::{Encoding, Position, Range},
    source::SourceMap,
    target::TargetCapabilityFlags,
};
use qsc_fir::fir::PackageStore;
use qsc_frontend::compile::PackageStore as HirPackageStore;
use qsc_lowerer::{Lowerer, map_fir_package_to_hir, map_hir_package_to_fir};
use qsc_rca::{Analyzer, PackageStoreComputeProperties};
use qsc_rir::{
    debug::{DbgPackageOffset, DbgScope},
    passes::check_and_transform,
    rir::{BlockId, CallableId, Program},
};

pub fn assert_block_instructions(program: &Program, block_id: BlockId, expected_insts: &Expect) {
    let block = program.get_block(block_id);
    expected_insts.assert_eq(&block.to_string());
}

/// Resolves a `DbgPackageOffset` to `(package_id-file:line:col)`.
fn fmt_dbg_offset(offset: &DbgPackageOffset, hir_store: &HirPackageStore) -> String {
    let hir_id = map_fir_package_to_hir(qsc_fir::fir::PackageId::from(offset.package_id));
    if let Some(unit) = hir_store.get(hir_id)
        && let Some(source) = unit.sources.find_by_offset(offset.offset)
    {
        let pos = Position::from_utf8_byte_offset(
            Encoding::Utf8,
            &source.contents,
            offset.offset - source.offset,
        );
        format!(
            "({}-{}:{}:{})",
            offset.package_id, source.name, pos.line, pos.column
        )
    } else {
        format!("({}-{})", offset.package_id, offset.offset)
    }
}

/// Formats the debug metadata from a program using resolved source locations.
#[allow(clippy::unwrap_used)]
fn fmt_dbg_info(program: &Program, hir_store: &HirPackageStore) -> String {
    use std::fmt::Write;
    let dbg = &program.dbg_info;
    if dbg.dbg_scopes.is_empty() && dbg.dbg_locations.is_empty() {
        return String::new();
    }
    let mut s = String::from("\ndbg_scopes:");
    for (id, (scope, _)) in dbg.dbg_scopes.iter() {
        match scope {
            DbgScope::SubProgram { name, location } => {
                write!(
                    s,
                    "\n    {} = SubProgram name={name} location={}",
                    usize::from(id),
                    fmt_dbg_offset(location, hir_store)
                )
                .unwrap();
            }
            DbgScope::LexicalBlockFile {
                discriminator,
                location,
            } => {
                write!(
                    s,
                    "\n    {} = LexicalBlockFile location={} discriminator={discriminator}",
                    usize::from(id),
                    fmt_dbg_offset(location, hir_store)
                )
                .unwrap();
            }
        }
    }
    s += "\ndbg_locations:";
    for (id, (loc, _)) in dbg.dbg_locations.iter() {
        write!(
            s,
            "\n    [{}]: scope={} location={}",
            usize::from(id),
            usize::from(loc.scope),
            fmt_dbg_offset(&loc.location, hir_store)
        )
        .unwrap();
        if let Some(inlined_at) = loc.inlined_at {
            write!(s, " inlined_at={}", usize::from(inlined_at)).unwrap();
        }
    }
    s
}

pub fn assert_blocks_with_sources(
    program: &Program,
    hir_store: &HirPackageStore,
    expected_blocks: &Expect,
) {
    let mut str = program
        .blocks
        .iter()
        .fold("Blocks:".to_string(), |acc, (id, block)| {
            acc + &format!("\nBlock {}:", id.0) + &block.to_string()
        });
    str += &fmt_dbg_info(program, hir_store);
    expected_blocks.assert_eq(&str);
}

pub fn assert_blocks(program: &Program, expected_blocks: &Expect) {
    let mut str = program
        .blocks
        .iter()
        .fold("Blocks:".to_string(), |acc, (id, block)| {
            acc + &format!("\nBlock {}:", id.0) + &block.to_string()
        });

    let dbg_info = program.dbg_info.to_string();
    if !dbg_info.is_empty() {
        str += "\n";
        str += &dbg_info;
    }
    expected_blocks.assert_eq(&str);
}

pub fn assert_callable(program: &Program, callable_id: CallableId, expected_callable: &Expect) {
    let actual_callable = program.get_callable(callable_id);
    expected_callable.assert_eq(&actual_callable.to_string());
}

pub fn assert_error(error: &Error, expected_error: &Expect) {
    expected_error.assert_eq(format!("{error:?}").as_str());
}

/// Like `assert_error` but resolves `PackageSpan` and `Span` byte offsets
/// to file:line:col, making baselines resilient to library source changes.
pub fn assert_error_with_sources(
    error: &Error,
    hir_store: &HirPackageStore,
    expected_error: &Expect,
) {
    expected_error.assert_eq(&fmt_error_with_sources(error, hir_store));
}

/// Formats an error by resolving package spans to source:range.
fn fmt_error_with_sources(error: &Error, hir_store: &HirPackageStore) -> String {
    match error {
        Error::CapabilityError(cap_err) => fmt_capability_error_with_sources(cap_err, hir_store),
        Error::UnexpectedDynamicValue(ps) => {
            format!(
                "UnexpectedDynamicValue({})",
                fmt_package_span(ps, hir_store)
            )
        }
        // Fall through to Debug for other variants
        other => format!("{other:?}"),
    }
}

/// Formats a `PackageSpan` with resolved source location.
fn fmt_package_span(ps: &qsc_eval::PackageSpan, hir_store: &HirPackageStore) -> String {
    let hir_id = map_fir_package_to_hir(qsc_fir::fir::PackageId::from(Into::<usize>::into(
        ps.package,
    )));
    if let Some(unit) = hir_store.get(hir_id)
        && let Some(source) = unit.sources.find_by_offset(ps.span.lo)
    {
        let range = Range::from_span(Encoding::Utf8, &source.contents, &(ps.span - source.offset));
        format!(
            "PackageSpan {{ package: PackageId({}), source: {}, range: {}:{}-{}:{} }}",
            Into::<usize>::into(ps.package),
            source.name,
            range.start.line,
            range.start.column,
            range.end.line,
            range.end.column,
        )
    } else {
        format!("{ps:?}")
    }
}

/// Formats a `CapabilityError` with its bare `Span` resolved.
fn fmt_capability_error_with_sources(
    cap_err: &qsc_rca::errors::Error,
    hir_store: &HirPackageStore,
) -> String {
    // All CapabilityError variants contain a single bare Span.
    // Extract the variant name and span using Debug, then resolve.
    let dbg = format!("{cap_err:?}");
    // The Debug format is e.g. "UseOfDynamicQubit(Span { lo: 67160, hi: 67173 })"
    let variant_end = dbg.find('(').unwrap_or(dbg.len());
    let variant_name = &dbg[..variant_end];

    // Extract the span from the error using miette::Diagnostic labels
    if let Some(label) = miette::Diagnostic::labels(cap_err).and_then(|mut labels| labels.next()) {
        let lo = u32::try_from(label.offset()).expect("offset fits in u32");
        let hi = lo + u32::try_from(label.len()).expect("len fits in u32");
        let span = qsc_data_structures::span::Span { lo, hi };

        // Search all packages for the one containing this offset
        for (_hir_id, unit) in hir_store {
            if let Some(source) = unit.sources.find_by_offset(lo) {
                let local_offset = lo - source.offset;
                if (local_offset as usize) > source.contents.len() {
                    continue;
                }
                let range =
                    Range::from_span(Encoding::Utf8, &source.contents, &(span - source.offset));
                return format!(
                    "CapabilityError({variant_name}({}: {}:{}-{}:{}))",
                    source.name,
                    range.start.line,
                    range.start.column,
                    range.end.line,
                    range.end.column,
                );
            }
        }
    }

    // Fallback
    format!("CapabilityError({dbg})")
}

#[must_use]
pub fn get_partial_evaluation_error(source: &str) -> Error {
    let maybe_program = compile_and_partially_evaluate(
        source,
        TargetCapabilityFlags::all(),
        PartialEvalConfig {
            generate_debug_metadata: false,
        },
    );
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
    let maybe_program = compile_and_partially_evaluate(
        source,
        capabilities,
        PartialEvalConfig {
            generate_debug_metadata: false,
        },
    );
    match maybe_program {
        Ok(_) => panic!("partial evaluation succeeded"),
        Err(error) => error,
    }
}

#[must_use]
pub fn get_rir_program(source: &str) -> Program {
    let maybe_program = compile_and_partially_evaluate(
        source,
        TargetCapabilityFlags::all(),
        PartialEvalConfig {
            generate_debug_metadata: false,
        },
    );
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
pub fn get_rir_program_with_dbg_metadata(source: &str) -> (Program, HirPackageStore) {
    let compilation_context = CompilationContext::new(source, TargetCapabilityFlags::all());
    let maybe_program = partially_evaluate(
        &compilation_context.fir_store,
        &compilation_context.compute_properties,
        &compilation_context.entry,
        TargetCapabilityFlags::all(),
        PartialEvalConfig {
            generate_debug_metadata: true,
        },
    );
    match maybe_program {
        Ok(program) => {
            // Verify the program can go through transformations.
            check_and_transform(&mut program.clone());
            validate(&program);
            (program, compilation_context.hir_store)
        }
        Err(error) => panic!("partial evaluation failed: {error:?}"),
    }
}

#[must_use]
pub fn get_partial_evaluation_error_with_sources(source: &str) -> (Error, HirPackageStore) {
    let compilation_context = CompilationContext::new(source, TargetCapabilityFlags::all());
    let maybe_program = partially_evaluate(
        &compilation_context.fir_store,
        &compilation_context.compute_properties,
        &compilation_context.entry,
        TargetCapabilityFlags::all(),
        PartialEvalConfig {
            generate_debug_metadata: false,
        },
    );
    match maybe_program {
        Ok(_) => panic!("partial evaluation succeeded"),
        Err(error) => (error, compilation_context.hir_store),
    }
}

fn validate(program: &Program) {
    let mut dbg_scopes = program.dbg_info.dbg_scopes.clone();
    let mut dbg_locations = program.dbg_info.dbg_locations.clone();

    // All scope and inlined_at references should be to existing dbg scopes and locations.
    for (dbg_location, _) in dbg_locations.values() {
        assert!(dbg_scopes.contains_key(dbg_location.scope));
        if let Some(inlined_at) = dbg_location.inlined_at {
            assert!(dbg_locations.contains_key(inlined_at));
        }
    }

    // All dbg location references in instructions should be to existing dbg locations.
    for instruction in program.blocks.iter().flat_map(|(_, block)| &block.0) {
        if let Some(dbg_location) = instruction.metadata().map(|metadata| metadata.dbg_location) {
            assert!(dbg_locations.contains_key(dbg_location));
        }
    }

    // Ensure all entries are referenced by removing referenced scopes/locations from the lists and then checking if any remain at the end.
    for instruction in program.blocks.iter().flat_map(|(_, block)| &block.0) {
        if let Some(dbg_location) = instruction.metadata().map(|metadata| metadata.dbg_location) {
            let mut to_remove = vec![dbg_location];
            let mut next = dbg_locations.get(dbg_location);

            while let Some(entry) = next {
                // remove referenced scope
                dbg_scopes.remove(entry.0.scope);

                if let Some(inlined_at) = entry.0.inlined_at {
                    // collect referenced dbg locations
                    next = dbg_locations.get(inlined_at);
                    to_remove.push(inlined_at);
                } else {
                    break;
                }
            }
            for id in to_remove {
                dbg_locations.remove(id);
            }
        }
    }

    assert!(
        dbg_locations.is_empty(),
        "unreferenced entry in dbg locations"
    );

    assert!(dbg_scopes.is_empty(), "unreferenced entry in dbg scopes");
}

#[must_use]
pub fn get_rir_program_with_capabilities(
    source: &str,
    capabilities: TargetCapabilityFlags,
) -> Program {
    let maybe_program = compile_and_partially_evaluate(
        source,
        capabilities,
        PartialEvalConfig {
            generate_debug_metadata: false,
        },
    );
    match maybe_program {
        Ok(program) => program,
        Err(error) => panic!("partial evaluation failed: {error:?}"),
    }
}

fn compile_and_partially_evaluate(
    source: &str,
    capabilities: TargetCapabilityFlags,
    config: PartialEvalConfig,
) -> Result<Program, Error> {
    let compilation_context = CompilationContext::new(source, capabilities);
    partially_evaluate(
        &compilation_context.fir_store,
        &compilation_context.compute_properties,
        &compilation_context.entry,
        capabilities,
        config,
    )
}

struct CompilationContext {
    fir_store: PackageStore,
    hir_store: HirPackageStore,
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
        let (hir_store, _) = compiler.into_package_store();
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
            hir_store,
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
