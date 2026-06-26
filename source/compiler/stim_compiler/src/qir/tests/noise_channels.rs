// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;

#[test]
fn e_yields_expected_qir() {
    let source = "E(0.01) X0";
    check(
        source,
        &expect![[r#"
            NoiseConfig:
            intrinsics:
                0: NoiseTable:
                    qubits: 1
                    X: 0.01


            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              call void @noise_intrinsic_0(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__rt__array_record_output(i64 0, ptr null)
              ret i64 0
            }

            declare void @noise_intrinsic_0(ptr) #2
            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__initialize(ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
            attributes #1 = { "irreversible" }

            ; module flags

            attributes #2 = { "qdk_noise" }

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

#[test]
fn correlated_error_yields_expected_qir() {
    let source = "CORRELATED_ERROR(0.01) X0";
    check(
        source,
        &expect![[r#"
            NoiseConfig:
            intrinsics:
                0: NoiseTable:
                    qubits: 1
                    X: 0.01


            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              call void @noise_intrinsic_0(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__rt__array_record_output(i64 0, ptr null)
              ret i64 0
            }

            declare void @noise_intrinsic_0(ptr) #2
            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__initialize(ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
            attributes #1 = { "irreversible" }

            ; module flags

            attributes #2 = { "qdk_noise" }

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

#[test]
fn else_correlated_error_with_preceding_correlated_error_yields_expected_qir() {
    let source = "
CORRELATED_ERROR(0.01) X0
ELSE_CORRELATED_ERROR(0.02) Z0
";
    check(
        source,
        &expect![[r#"
            NoiseConfig:
            intrinsics:
                0: NoiseTable:
                    qubits: 1
                    X: 0.01
                    Z: 0.0198


            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              call void @noise_intrinsic_0(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__rt__array_record_output(i64 0, ptr null)
              ret i64 0
            }

            declare void @noise_intrinsic_0(ptr) #2
            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__initialize(ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
            attributes #1 = { "irreversible" }

            ; module flags

            attributes #2 = { "qdk_noise" }

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

#[test]
fn correlated_error_chain_with_common_qubit_yields_expected_qir() {
    let source = "
CORRELATED_ERROR(0.01) X0
ELSE_CORRELATED_ERROR(0.02) Z0 L1
ELSE_CORRELATED_ERROR(0.03) X0 Z1 Y2
";
    check(
        source,
        &expect![[r#"
            NoiseConfig:
            intrinsics:
                0: NoiseTable:
                    qubits: 3
                    XII: 0.01
                    ZLI: 0.0198
                    XZY: 0.029105999999999996


            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              call void @noise_intrinsic_0(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr))
              call void @__quantum__rt__array_record_output(i64 0, ptr null)
              ret i64 0
            }

            declare void @noise_intrinsic_0(ptr, ptr, ptr) #2
            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__initialize(ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="0" }
            attributes #1 = { "irreversible" }

            ; module flags

            attributes #2 = { "qdk_noise" }

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

#[test]
fn correlated_error_chain_with_disjoint_qubits_yields_expected_qir() {
    let source = "
CORRELATED_ERROR(0.01) X0
ELSE_CORRELATED_ERROR(0.02) Z1 L2
ELSE_CORRELATED_ERROR(0.03) Y3 Z4
";
    check(
        source,
        &expect![[r#"
            NoiseConfig:
            intrinsics:
                0: NoiseTable:
                    qubits: 5
                    XIIII: 0.01
                    IZLII: 0.0198
                    IIIYZ: 0.029105999999999996


            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              call void @noise_intrinsic_0(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 4 to ptr))
              call void @__quantum__rt__array_record_output(i64 0, ptr null)
              ret i64 0
            }

            declare void @noise_intrinsic_0(ptr, ptr, ptr, ptr, ptr) #2
            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__initialize(ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="5" "required_num_results"="0" }
            attributes #1 = { "irreversible" }

            ; module flags

            attributes #2 = { "qdk_noise" }

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

#[test]
fn else_correlated_error_with_preceding_else_correlated_error_yields_expected_qir() {
    let source = "
CORRELATED_ERROR(0.01) X0
ELSE_CORRELATED_ERROR(0.02) Y0
ELSE_CORRELATED_ERROR(0.03) Z0
";
    check(
        source,
        &expect![[r#"
            NoiseConfig:
            intrinsics:
                0: NoiseTable:
                    qubits: 1
                    X: 0.01
                    Y: 0.0198
                    Z: 0.029105999999999996


            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              call void @noise_intrinsic_0(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__rt__array_record_output(i64 0, ptr null)
              ret i64 0
            }

            declare void @noise_intrinsic_0(ptr) #2
            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__initialize(ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
            attributes #1 = { "irreversible" }

            ; module flags

            attributes #2 = { "qdk_noise" }

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

#[test]
fn else_correlated_error_by_itself_yields_error() {
    let source = "ELSE_CORRELATED_ERROR(0.02) X0";
    check(
        source,
        &expect![[r#"
            NoiseConfig:
            intrinsics:
                0: NoiseTable:
                    qubits: 1
                    X: 0.02


            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              call void @noise_intrinsic_0(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__rt__array_record_output(i64 0, ptr null)
              ret i64 0
            }

            declare void @noise_intrinsic_0(ptr) #2
            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__initialize(ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
            attributes #1 = { "irreversible" }

            ; module flags

            attributes #2 = { "qdk_noise" }

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

#[test]
fn else_correlated_error_without_preceding_correlated_error_yields_error() {
    let source = "
CORRELATED_ERROR(0.01) X0
I 0
ELSE_CORRELATED_ERROR(0.02) X0
";
    check(
        source,
        &expect![[r#"
            NoiseConfig:
            intrinsics:
                0: NoiseTable:
                    qubits: 1
                    X: 0.01

                1: NoiseTable:
                    qubits: 1
                    X: 0.02


            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              call void @noise_intrinsic_0(ptr inttoptr (i64 0 to ptr))
              call void @noise_intrinsic_1(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__rt__array_record_output(i64 0, ptr null)
              ret i64 0
            }

            declare void @noise_intrinsic_1(ptr) #2
            declare void @noise_intrinsic_0(ptr) #2
            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__initialize(ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
            attributes #1 = { "irreversible" }

            ; module flags

            attributes #2 = { "qdk_noise" }

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

#[test]
fn else_correlated_error_without_preceding_else_correlated_error_yields_error() {
    let source = "
CORRELATED_ERROR(0.01) X0
ELSE_CORRELATED_ERROR(0.02) Y0
I 0
ELSE_CORRELATED_ERROR(0.02) Z0
";
    check(
        source,
        &expect![[r#"
            NoiseConfig:
            intrinsics:
                0: NoiseTable:
                    qubits: 1
                    X: 0.01
                    Y: 0.0198

                1: NoiseTable:
                    qubits: 1
                    Z: 0.02


            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              call void @noise_intrinsic_0(ptr inttoptr (i64 0 to ptr))
              call void @noise_intrinsic_1(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__rt__array_record_output(i64 0, ptr null)
              ret i64 0
            }

            declare void @noise_intrinsic_1(ptr) #2
            declare void @noise_intrinsic_0(ptr) #2
            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__initialize(ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
            attributes #1 = { "irreversible" }

            ; module flags

            attributes #2 = { "qdk_noise" }

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

#[test]
fn depolarize1_yields_expected_qir() {
    let source = "DEPOLARIZE1(0.01) 0";
    check(
        source,
        &expect![[r#"
        NoiseConfig:
        intrinsics:
            0: NoiseTable:
                qubits: 1
                X: 0.0033333333333333335
                Y: 0.0033222222222222225
                Z: 0.0033111481481481486


        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @noise_intrinsic_0(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__rt__array_record_output(i64 0, ptr null)
          ret i64 0
        }

        declare void @noise_intrinsic_0(ptr) #2
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__initialize(ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
        attributes #1 = { "irreversible" }

        ; module flags

        attributes #2 = { "qdk_noise" }

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

#[test]
fn depolarize1_without_probability_yields_error() {
    let source = "DEPOLARIZE1 0";
    check(source, &expect![[""]]);
}

#[test]
fn depolarize2_yields_expected_qir() {
    let source = "DEPOLARIZE2(0.01) 0 1";
    check(
        source,
        &expect![[r#"
        NoiseConfig:
        intrinsics:
            0: NoiseTable:
                qubits: 2
                IX: 0.0006666666666666666
                IY: 0.0006662222222222221
                IZ: 0.000665778074074074
                XI: 0.0006653342220246913
                XX: 0.0006648906658766748
                XY: 0.000664447405432757
                XZ: 0.0006640044404958018
                YI: 0.0006635617708688046
                YX: 0.000663119396354892
                YY: 0.0006626773167573221
                YZ: 0.0006622355318794839
                ZI: 0.0006617940415248975
                ZX: 0.0006613528454972143
                ZY: 0.000660911943600216
                ZZ: 0.0006604713356378159


        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @noise_intrinsic_0(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__rt__array_record_output(i64 0, ptr null)
          ret i64 0
        }

        declare void @noise_intrinsic_0(ptr, ptr) #2
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__initialize(ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="0" }
        attributes #1 = { "irreversible" }

        ; module flags

        attributes #2 = { "qdk_noise" }

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

#[test]
fn depolarize2_without_probability_yields_error() {
    let source = "DEPOLARIZE2 0 1 2 3";
    check(source, &expect![[""]]);
}

#[test]
fn depolarize2_with_odd_number_of_targets_yields_error() {
    let source = "DEPOLARIZE2 0";
    check(source, &expect![[""]]);
}

#[test]
#[ignore = "unsupported instruction"]
fn heralded_erase_yields_expected_qir() {
    let source = "HERALDED_ERASE(0.01) 0";
    check(source, &expect![[""]]);
}

#[test]
#[ignore = "unsupported instruction"]
fn heralded_pauli_channel_1_yields_expected_qir() {
    let source = "HERALDED_PAULI_CHANNEL_1(0, 0, 0, 0.1) 0";
    check(source, &expect![[""]]);
}

#[test]
#[ignore = "unsupported instruction"]
fn i_error_yields_expected_qir() {
    let source = "
# does nothing
I_ERROR 0

# does nothing with probability 0.1, else does nothing
I_ERROR(0.1) 0

# doesn't require a probability argument
I_ERROR[LEAKAGE_NOISE_FOR_AN_ADVANCED_SIMULATOR:0.1] 0

# checks for you that the disjoint probabilities in the arguments are legal
I_ERROR[MULTIPLE_NOISE_MECHANISMS](0.1, 0.2) 0
";
    check(source, &expect![[""]]);
}

#[test]
#[ignore = "unsupported instruction"]
fn ii_error_yields_expected_qir() {
    let source = "
# does nothing
II_ERROR 0 1

# does nothing with probability 0.1, else does nothing
II_ERROR(0.1) 0 1

# checks for you that the targets are two-qubit pairs
II_ERROR[TWO_QUBIT_LEAKAGE_NOISE_FOR_AN_ADVANCED_SIMULATOR:0.1] 0 2 4 6

# checks for you that the disjoint probabilities in the arguments are legal
II_ERROR[MULTIPLE_TWO_QUBIT_NOISE_MECHANISMS](0.1, 0.2) 0 2 4 6
";
    check(source, &expect![[""]]);
}

#[test]
#[ignore = "unsupported instruction"]
fn ii_error_with_odd_number_of_targets_yields_expected_qir() {
    let source = "II_ERROR 0";
    check(source, &expect![[""]]);
}

#[test]
#[ignore = "unsupported instruction"]
fn pauli_channel_1_yields_expected_qir() {
    let source = "PAULI_CHANNEL_1(0.1, 0.2, 0.3) 0";
    check(source, &expect![[""]]);
}

#[test]
#[ignore = "unsupported instruction"]
fn pauli_channel_2_yields_expected_qir() {
    let source = "PAULI_CHANNEL_2(0,0,0, 0,0.1,0,0, 0,0,0,0.2, 0,0,0,0) 0 1";
    check(source, &expect![[""]]);
}

#[test]
#[ignore = "unsupported instruction"]
fn pauli_channel_2_with_odd_number_of_targets_yields_expected_qir() {
    let source = "PAULI_CHANNEL_2(0,0,0, 0,0.1,0,0, 0,0,0,0.2, 0,0,0,0) 0";
    check(source, &expect![[""]]);
}

#[test]
fn x_error_yields_expected_qir() {
    let source = "X_ERROR(0.01) 0";
    check(
        source,
        &expect![[r#"
            NoiseConfig:
            intrinsics:
                0: NoiseTable:
                    qubits: 1
                    X: 0.01


            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              call void @noise_intrinsic_0(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__rt__array_record_output(i64 0, ptr null)
              ret i64 0
            }

            declare void @noise_intrinsic_0(ptr) #2
            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__initialize(ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
            attributes #1 = { "irreversible" }

            ; module flags

            attributes #2 = { "qdk_noise" }

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

#[test]
fn y_error_yields_expected_qir() {
    let source = "Y_ERROR(0.01) 0";
    check(
        source,
        &expect![[r#"
            NoiseConfig:
            intrinsics:
                0: NoiseTable:
                    qubits: 1
                    Y: 0.01


            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              call void @noise_intrinsic_0(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__rt__array_record_output(i64 0, ptr null)
              ret i64 0
            }

            declare void @noise_intrinsic_0(ptr) #2
            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__initialize(ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
            attributes #1 = { "irreversible" }

            ; module flags

            attributes #2 = { "qdk_noise" }

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

#[test]
fn z_error_yields_expected_qir() {
    let source = "Z_ERROR(0.01) 0";
    check(
        source,
        &expect![[r#"
            NoiseConfig:
            intrinsics:
                0: NoiseTable:
                    qubits: 1
                    Z: 0.01


            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              call void @noise_intrinsic_0(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__rt__array_record_output(i64 0, ptr null)
              ret i64 0
            }

            declare void @noise_intrinsic_0(ptr) #2
            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__initialize(ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
            attributes #1 = { "irreversible" }

            ; module flags

            attributes #2 = { "qdk_noise" }

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

#[test]
fn loss_error_yields_expected_qir() {
    let source = "LOSS_ERROR(0.01) 0";
    check(
        source,
        &expect![[r#"
            NoiseConfig:
            intrinsics:
                0: NoiseTable:
                    qubits: 1
                    L: 0.01


            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              call void @noise_intrinsic_0(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__rt__array_record_output(i64 0, ptr null)
              ret i64 0
            }

            declare void @noise_intrinsic_0(ptr) #2
            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__initialize(ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
            attributes #1 = { "irreversible" }

            ; module flags

            attributes #2 = { "qdk_noise" }

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
