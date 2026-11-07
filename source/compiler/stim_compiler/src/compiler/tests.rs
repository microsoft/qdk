// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod boilerplate;
mod collapsing_gates;
mod generalized_pauli_product_gates;
mod measurement_record_targets;
mod noise_channels;
mod non_clifford_gates;
mod pair_measurements;
mod peek_loss;
mod repeat;
mod select_block;
mod single_qubit_gates;
mod two_qubit_gates;
mod unsupported_instructions;

use expect_test::Expect;
use qdk_simulators::noise_config::NoiseConfig;

use crate::format_stim_errors;

/// Check that a stim source compiles to the expected Q# or yields the expected errors.
/// If the circuit registers noise, the resulting noise configuration is appended.
/// Successful compilations must also compile to QIR, which checks that the generated Q# is valid.
fn check(source: &str, expect: &Expect) {
    let mut noise = NoiseConfig::NOISELESS;
    match crate::compile_to_qsharp(source, &mut noise) {
        Ok(qsharp) => {
            let mut qir_noise = NoiseConfig::NOISELESS;
            if let Err(errors) = crate::compile_to_qir(source, &mut qir_noise) {
                panic!(
                    "generated Q# should compile to QIR:\n{}",
                    format_stim_errors(errors)
                );
            }
            let mut actual = qsharp.trim_end().to_string();
            if !noise.is_noiseless() {
                actual = format!("{actual}\n\n{}", noise.to_string().trim_end());
            }
            expect.assert_eq(&actual);
        }
        Err(errors) => {
            let errors = format_stim_errors(errors);
            expect.assert_eq(&errors);
        }
    }
}
