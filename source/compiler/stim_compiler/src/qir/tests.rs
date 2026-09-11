// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod boilerplate;
mod collapsing_gates;
mod collapsing_gates_broadcasting;
mod generalized_pauli_product_gates;
mod measurement_record_targets;
mod noise_channels;
mod noise_channels_broadcasting;
mod non_clifford_gates;
mod pair_measurements;
mod pair_measurements_broadcasting;
mod peek_loss;
mod repeat;
mod select_block;
mod single_qubit_gates;
mod single_qubit_gates_broadcasting;
mod two_qubit_gates;
mod two_qubit_gates_broadcasting;
mod unsupported_instructions;

use expect_test::Expect;
use indoc::formatdoc;
use qdk_simulators::noise_config::NoiseConfig;

use crate::format_stim_errors;

// QIR boilerplate omitted from most snapshots is covered by dedicated tests.
const BOILERPLATE_BODY_LINES: [&str; 2] = [
    "call void @__quantum__rt__initialize(ptr null)",
    "ret i64 0",
];
const BOILERPLATE_OUTPUT_CALLS: [&str; 2] = [
    "call void @__quantum__rt__array_record_output(",
    "call void @__quantum__rt__result_record_output(",
];
const BOILERPLATE_DECLARATIONS: [&str; 3] = [
    "declare void @__quantum__rt__array_record_output(i64, ptr)",
    "declare void @__quantum__rt__initialize(ptr)",
    "declare void @__quantum__rt__result_record_output(ptr, ptr)",
];

// This formats the QIR output to only include parts that aren't boilerplate,
// and to make it easier to read and compare in tests. It also includes the
// noise configuration, if noise instructions were used.
fn format_qir(qir: &str, noise: &NoiseConfig<f64, f64>) -> String {
    let (body, remainder) = qir
        .strip_prefix("define i64 @ENTRYPOINT__main() #0 {\n")
        .and_then(|qir| qir.split_once("\n}\n"))
        .expect("QIR boilerplate should contain the entry-point definition");

    let definitions = remainder
        .split_once("declare ")
        .map(|(definitions, _)| definitions.trim())
        .expect("QIR boilerplate should contain declarations");

    let required_num_qubits = extract_parameter(qir, "required_num_qubits");
    let required_num_results = extract_parameter(qir, "required_num_results");

    let body = indent_lines(body.lines().filter(|line| {
        let line = line.trim_start();
        !BOILERPLATE_BODY_LINES.contains(&line)
            && !BOILERPLATE_OUTPUT_CALLS
                .iter()
                .any(|call| line.starts_with(call))
    }));
    let body = if body.is_empty() {
        String::new()
    } else {
        format!("body:\n{body}\n\n")
    };

    let mut declarations = qir
        .lines()
        .filter(|line| line.starts_with("declare "))
        .filter(|line| !BOILERPLATE_DECLARATIONS.contains(line))
        .collect::<Vec<_>>();
    declarations.sort_unstable();
    let declarations = indent_lines(declarations);
    let declarations = if declarations.is_empty() {
        String::new()
    } else {
        format!("declarations:\n{declarations}\n\n")
    };

    let definitions = if definitions.is_empty() {
        String::new()
    } else {
        format!("definitions:\n{}\n\n", indent_lines(definitions.lines()))
    };

    let uses_noise = if qir.contains("\"qdk_noise\"") {
        "uses_noise: true\n"
    } else {
        ""
    };

    let noise_config = if noise.is_noiseless() {
        String::new()
    } else {
        noise.to_string()
    };

    formatdoc! {"
        {body}\
        {definitions}\
        {declarations}\
        required_num_qubits: {required_num_qubits}
        required_num_results: {required_num_results}
        {uses_noise}
        {noise_config}
    "}
    .trim_end()
    .to_string()
}

fn extract_parameter(qir: &str, name: &str) -> String {
    let marker = format!("\"{name}\"=\"");
    qir.split_once(&marker)
        .and_then(|(_, value)| value.split_once('"'))
        .map(|(value, _)| value.to_string())
        .unwrap_or_else(|| panic!("QIR boilerplate should contain the {name} parameter"))
}

fn indent_lines<'a>(lines: impl IntoIterator<Item = &'a str>) -> String {
    lines
        .into_iter()
        .map(|line| {
            if line.is_empty() {
                String::new()
            } else {
                format!("  {line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Check that a stim source compiles to the
/// expected formatted QIR or yields the expected errors.
fn check(source: &str, expect: &Expect) {
    let mut noise = NoiseConfig::NOISELESS;
    match crate::compile(source, &mut noise) {
        Ok(qir) => {
            let formatted_qir = format_qir(&qir, &noise);
            expect.assert_eq(&formatted_qir);
        }
        Err(errors) => {
            let errors = format_stim_errors(errors);
            expect.assert_eq(&errors);
        }
    }
}
