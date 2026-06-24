// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::error::Error;
use crate::parser::parse;
use crate::qir::compile_to_qir;
use miette::Report;
use qdk_simulators::noise_config::NoiseConfig;

mod error;
pub mod lex;
pub mod parser;
pub mod qir;

pub fn compile(src: &str, noise: &mut NoiseConfig<f64, f64>) -> Result<String, Vec<Report>> {
    let (circuit, parser_errors) = parse(src);
    if !parser_errors.is_empty() {
        return Err(parser_errors
            .into_iter()
            .map(Error::from)
            .map(Report::new)
            .collect());
    }
    compile_to_qir(&circuit, noise).map_err(|errors| {
        errors
            .into_iter()
            .map(Error::from)
            .map(Report::new)
            .collect()
    })
}
