// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::sync::Arc;
use std::vec;

use qsc_frontend::compile::PackageStore;
use qsc_frontend::error::WithSource;
use qsc_hir::hir::PackageId;
use qsc_passes::PackageType;
use qsc_qasm::{compiler::parse_and_compile_to_qsharp_ast_with_config, io::SourceResolver};

pub use qsc_qasm::{
    CompilerConfig, OperationSignature, OutputSemantics, ProgramType, QasmCompileUnit,
    QubitSemantics,
};

pub mod io {
    pub use qsc_qasm::io::*;
}

pub mod parser {
    pub use qsc_qasm::parser::*;
}

pub mod semantic {
    pub use qsc_qasm::semantic::*;
}

pub mod error {
    pub use qsc_qasm::Error;
    pub use qsc_qasm::ErrorKind;
}

pub mod completion {
    pub use qsc_qasm::parser::completion::*;
}

pub mod compiler {
    pub use qsc_qasm::compiler::*;
}

pub mod stdlib {
    pub use qsc_qasm::stdlib::*;
}

use crate::compile::package_store_with_stdlib;

pub struct CompileRawQasmResult(
    pub PackageStore,
    pub PackageId,
    pub Vec<(PackageId, Option<std::sync::Arc<str>>)>,
    pub Option<OperationSignature>,
    pub Vec<crate::compile::Error>,
);

#[must_use]
pub fn compile_openqasm(unit: QasmCompileUnit, package_type: PackageType) -> CompileRawQasmResult {
    let (source_map, openqasm_errors, package, sig, profile) = unit.into_tuple();

    let (stdid, mut store) = package_store_with_stdlib(profile.into());
    let dependencies = vec![(PackageId::CORE, None), (stdid, None)];

    let (mut unit, compile_errors) = crate::compile::compile_ast(
        &store,
        &dependencies,
        package,
        source_map.clone(),
        package_type,
        profile.into(),
    );
    unit.expose();
    let source_package_id = store.insert(unit);

    // We allow the best effort compilation, but for errors we only
    // want to provide OpenQASM OR Q# errors. Otherwise we get confusing
    // error reporting and duplicate errors (like undeclared identifier and
    // type errors)
    let surfaced_errors = if openqasm_errors.is_empty() {
        // we have no OpenQASM errors, surface the Q# compilation errors
        compile_errors
    } else {
        // We have OpenQASM errors, convert them to the same type as as the Q#
        // compilation errors
        let mut compile_errors = Vec::with_capacity(openqasm_errors.len());
        for error in openqasm_errors {
            let err = WithSource::from_map(
                &source_map,
                crate::compile::ErrorKind::OpenQasm(error.into_error()),
            );
            compile_errors.push(err);
        }
        compile_errors
    };

    CompileRawQasmResult(store, source_package_id, dependencies, sig, surfaced_errors)
}

#[must_use]
pub fn parse_and_compile_raw_qasm<R: SourceResolver, S: Into<Arc<str>>>(
    source: S,
    path: S,
    resolver: Option<&mut R>,
    package_type: PackageType,
) -> CompileRawQasmResult {
    let config = CompilerConfig::new(
        QubitSemantics::Qiskit,
        OutputSemantics::OpenQasm,
        ProgramType::File,
        Some("program".into()),
        None,
    );
    parse_and_compile_with_config(source, path, resolver, config, package_type)
}

#[must_use]
pub fn parse_and_compile_with_config<R: SourceResolver, S: Into<Arc<str>>>(
    source: S,
    path: S,
    resolver: Option<&mut R>,
    config: CompilerConfig,
    package_type: PackageType,
) -> CompileRawQasmResult {
    let unit = parse_and_compile_to_qsharp_ast_with_config(source, path, resolver, config);
    compile_openqasm(unit, package_type)
}

#[must_use]
pub fn parse_raw_qasm_as_fragments<R: SourceResolver, S: Into<Arc<str>>>(
    source: S,
    path: S,
    resolver: Option<&mut R>,
) -> QasmCompileUnit {
    let config = CompilerConfig::new(
        QubitSemantics::Qiskit,
        OutputSemantics::OpenQasm,
        ProgramType::Fragments,
        None,
        None,
    );
    parse_and_compile_to_qsharp_ast_with_config(source, path, resolver, config)
}

#[must_use]
pub fn parse_raw_qasm_as_operation<
    R: SourceResolver,
    S: Into<Arc<str>>,
    N: Into<Arc<str>>,
    P: Into<Arc<str>>,
>(
    source: S,
    name: N,
    path: P,
    resolver: Option<&mut R>,
) -> QasmCompileUnit {
    let config = CompilerConfig::new(
        QubitSemantics::Qiskit,
        OutputSemantics::OpenQasm,
        ProgramType::Operation,
        Some(name.into()),
        None,
    );
    parse_and_compile_to_qsharp_ast_with_config(source, path, resolver, config)
}
