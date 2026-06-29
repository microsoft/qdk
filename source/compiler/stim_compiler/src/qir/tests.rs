// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod collapsing_gates;
mod collapsing_gates_broadcasting;
mod control_flow;
mod generalize_pauli_product_gates;
mod noise_channels;
mod noise_channels_broadcasting;
mod pair_measurements;
mod pair_measurements_broadcasting;
mod single_qubit_gates;
mod single_qubit_gates_broadcasting;
mod two_qubit_gates;
mod two_qubit_gates_broadcasting;
mod unsupported_instructions;

use expect_test::{Expect, expect};
use qdk_simulators::noise_config::NoiseConfig;

use crate::format_stim_errors;

/// Check that a stim source compiles to the
/// expected QIR or yields the expected errors.
fn check(source: &str, expect: &Expect) {
    let mut noise = NoiseConfig::NOISELESS;
    match crate::compile(source, &mut noise) {
        Ok(qir) => {
            let actual = if noise.is_noiseless() {
                qir
            } else {
                noise.to_string() + "\n" + &qir
            };
            expect.assert_eq(&actual);
        }
        Err(errors) => {
            let errors = format_stim_errors(errors);
            expect.assert_eq(&errors);
        }
    }
}

#[test]
fn empty_src() {
    check(
        "",
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              call void @__quantum__rt__array_record_output(i64 0, ptr null)
              ret i64 0
            }

            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__initialize(ptr)
            declare void @__quantum__rt__result_record_output(ptr, ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="0" "required_num_results"="0" }
            attributes #1 = { "irreversible" }

            ; module flags

            !llvm.module.flags = !{!0, !1, !2, !3, !4, !5, !6, !7}

            !0 = !{i32 1, !"qir_major_version", i32 2}
            !1 = !{i32 7, !"qir_minor_version", i32 1}
            !2 = !{i32 1, !"dynamic_qubit_management", i1 false}
            !3 = !{i32 1, !"dynamic_result_management", i1 false}
            !4 = !{i32 5, !"int_computations", !{!"i64"}}
            !5 = !{i32 5, !"float_computations", !{!"double"}}
            !6 = !{i32 7, !"backwards_branching", i2 3}
            !7 = !{i32 1, !"arrays", i1 true}
        "#]],
    );
}
