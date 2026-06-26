// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::error::Error;
use crate::parser::parse;
use crate::qir::compile_to_qir;
use miette::Report;
use qdk_simulators::noise_config::NoiseConfig;
use qsc_data_structures::source::SourceMap;

mod error;
pub mod lex;
pub mod parser;
pub mod qir;

pub fn compile(
    src: &str,
    noise: &mut NoiseConfig<f64, f64>,
) -> Result<String, Vec<miette::Report>> {
    let (circuit, parser_errors) = parse(src);
    if !parser_errors.is_empty() {
        return Err(parser_errors
            .into_iter()
            .map(Error::from)
            .map(Report::new)
            .collect());
    }
    let source_map = SourceMap::new([("circuit".into(), src.into())], None);
    compile_to_qir(&circuit, source_map, noise)
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
