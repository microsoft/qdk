// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qdk_simulators::noise_config::NoiseConfig;

pub mod lex;
pub mod parser;
pub mod qir;

pub fn compile(src: &str, noise: &mut NoiseConfig<f64, f64>) -> String {
    let circuit = parser::parse(src);
    qir::compile_to_qir(&circuit, noise)
}
