// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::sync::Arc;

use crate::error::Error;
use crate::parser::parse;
use miette::Report;
use qdk_simulators::noise_config::NoiseConfig;
use qsc::ast::Package;
use qsc::compile::package_store_with_stdlib;
use qsc::hir::PackageId;
use qsc::target::Profile;
use qsc::{SourceMap, TargetCapabilityFlags};

mod ast_builder;
pub mod compiler;
mod error;
pub mod lex;
pub mod parser;
pub mod semantic;

/// Compiles a Stim circuit to a Q# AST, whose entry point `Main` returns the measurement records.
///
/// The noise channels used by the circuit are added to `noise` as noise intrinsics, which the
/// generated Q# calls as `noise_intrinsic_{id}`. Errors are reported with the source attached.
pub fn compile_to_qsharp_ast(
    src: &str,
    noise: &mut NoiseConfig<f64, f64>,
) -> Result<Package, Vec<Report>> {
    // Create an Arc of `source` so that each error can reference
    // the source without making copies of a potentially large string.
    let src_ref = Arc::new(src.to_string());
    let (ast, parser_errors) = parse(src);
    if !parser_errors.is_empty() {
        return Err(parser_errors
            .into_iter()
            .map(Error::from)
            .map(|e| Report::new(e).with_source_code(src_ref.clone()))
            .collect());
    }
    let (semantic_ast, semantic_errors) = semantic::lower(ast);
    if !semantic_errors.is_empty() {
        return Err(semantic_errors
            .into_iter()
            .map(Error::from)
            .map(|e| Report::new(e).with_source_code(src_ref.clone()))
            .collect());
    }
    compiler::compile(&semantic_ast, noise).map_err(|errors| {
        errors
            .into_iter()
            .map(Error::from)
            .map(|e| Report::new(e).with_source_code(src_ref.clone()))
            .collect()
    })
}

/// Compiles a Stim circuit to Q# source code. See [`compile_to_qsharp_ast`].
pub fn compile_to_qsharp(
    src: &str,
    noise: &mut NoiseConfig<f64, f64>,
) -> Result<String, Vec<Report>> {
    let package = compile_to_qsharp_ast(src, noise)?;
    Ok(qsc::codegen::qsharp::write_package_string(&package))
}

/// Compiles a Stim circuit to QIR for the Adaptive profile, by compiling the Q# AST from
/// [`compile_to_qsharp_ast`] against the standard library.
pub fn compile_to_qir(src: &str, noise: &mut NoiseConfig<f64, f64>) -> Result<String, Vec<Report>> {
    let package = compile_to_qsharp_ast(src, noise)?;
    let capabilities = TargetCapabilityFlags::from(Profile::Adaptive);
    let (std_id, mut store) = package_store_with_stdlib(capabilities);
    let dependencies = [(PackageId::CORE, None), (std_id, None)];
    let sources = SourceMap::new([("".into(), src.into())], None);
    qsc::codegen::qir::get_qir_from_ast(&mut store, &dependencies, package, sources, capabilities)
        .map_err(|errors| errors.into_iter().map(Report::new).collect())
}

/// Formats a list of Stim errors into a single string.
pub fn format_stim_errors(errors: Vec<miette::Report>) -> String {
    errors
        .iter()
        .map(|e| format!("{e:?}"))
        .collect::<Vec<_>>()
        .join("\n")
}
