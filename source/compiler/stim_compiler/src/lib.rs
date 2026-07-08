// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::sync::Arc;

use crate::error::Error;
use crate::parser::parse;
use crate::qir::compile_to_qir;
use miette::Report;
use qdk_simulators::noise_config::NoiseConfig;

mod error;
pub mod lex;
pub mod parser;
pub mod qir;

pub fn compile(
    src: &str,
    noise: &mut NoiseConfig<f64, f64>,
) -> Result<String, Vec<miette::Report>> {
    // Create an Arc of `source` so that each error can reference
    // the source without making copies of a potentially large string.
    let src_ref = Arc::new(src.to_string());
    let (circuit, parser_errors) = parse(src);
    if !parser_errors.is_empty() {
        return Err(parser_errors
            .into_iter()
            .map(Error::from)
            .map(|e| Report::new(e).with_source_code(src_ref.clone()))
            .collect());
    }
    compile_to_qir(&circuit, noise).map_err(|errors| {
        errors
            .into_iter()
            .map(Error::from)
            .map(|e| Report::new(e).with_source_code(src_ref.clone()))
            .collect()
    })
}

/// Formats a list of Stim errors into a single string.
pub fn format_stim_errors(errors: Vec<miette::Report>) -> String {
    errors
        .iter()
        .map(|e| format!("{e:?}"))
        .collect::<Vec<_>>()
        .join("\n")
}
