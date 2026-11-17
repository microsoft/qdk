// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::{Expect, expect};
use qdk_simulators::noise_config::NoiseConfig;

use crate::format_stim_errors;

fn check(source: &str, expect: &Expect) {
    let mut noise = NoiseConfig::NOISELESS;
    match crate::compile(source, &mut noise) {
        Ok(qir) => expect.assert_eq(&qir),
        Err(errors) => {
            let errors = format_stim_errors(errors);
            expect.assert_eq(&errors);
        }
    }
}

// Check that the QIR boilerplate is generally correct
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

// Checks the result_record_output calls are correctly generated
#[test]
fn measured_qir_has_expected_boilerplate() {
    check(
        "M 0 1",
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__rt__array_record_output(i64 2, ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr null)
          ret i64 0
        }

        declare void @__quantum__qis__m__body(ptr, ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="2" }
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

// Checks that the qdk_noise attribute is correctly generated whenever there is noise.
#[test]
fn noisy_qir_has_expected_boilerplate() {
    check(
        "M(0.1) 0",
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          call void @__quantum__rt__readout_noise(double 0.1, double 0.1, ptr inttoptr (i64 0 to ptr))
          call void @__quantum__rt__array_record_output(i64 1, ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
          ret i64 0
        }

        declare void @__quantum__qis__m__body(ptr, ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__rt__readout_noise(double, double, ptr) #2
        declare void @__quantum__rt__result_record_output(ptr, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
        attributes #1 = { "irreversible" }
        attributes #2 = { "qdk_noise" }

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
