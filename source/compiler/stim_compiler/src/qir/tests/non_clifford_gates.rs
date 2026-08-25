// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;
use indoc::indoc;

#[test]
fn t_gate_yields_expected_qir() {
    check(
        "T 0",
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              call void @__quantum__qis__t__body(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__rt__array_record_output(i64 0, ptr null)
              ret i64 0
            }

            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__qis__t__body(ptr)
            declare void @__quantum__rt__initialize(ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
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

#[test]
fn t_dag_gate_yields_expected_qir() {
    check(
        "T_DAG 0",
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              call void @__quantum__qis__t__adj(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__rt__array_record_output(i64 0, ptr null)
              ret i64 0
            }

            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__initialize(ptr)
            declare void @__quantum__qis__t__adj(ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
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

#[test]
fn t_gate_with_argument_yields_error() {
    check(
        "T(0.5) 0",
        &expect![[r#"
            Qdk.Stim.Compiler.UnsupportedArgument

              x unsupported argument in instruction: T
               ,----
             1 | T(0.5) 0
               : ^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn t_gate_with_negated_target_yields_error() {
    check(
        "T !0",
        &expect![[r#"
            Qdk.Stim.Compiler.NegatedTarget

              x target cannot be negated in instruction: T
               ,----
             1 | T !0
               :   ^^
               `----
        "#]],
    );
}

#[test]
fn t_gate_with_measurement_record_target_yields_error() {
    let source = indoc! {"
        M 0
        T rec[-1]
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.UnsupportedTarget

              x unsupported target in instruction: T
               ,-[2:3]
             1 | M 0
             2 | T rec[-1]
               :   ^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn t_gate_with_pauli_target_yields_error() {
    check(
        "T X0",
        &expect![[r#"
            Qdk.Stim.Compiler.UnsupportedTarget

              x unsupported target in instruction: T
               ,----
             1 | T X0
               :   ^^
               `----
        "#]],
    );
}

#[test]
fn tpp_single_z_yields_expected_qir() {
    // same as T 0
    check(
        "TPP Z0",
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              call void @__quantum__qis__t__body(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__rt__array_record_output(i64 0, ptr null)
              ret i64 0
            }

            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__qis__t__body(ptr)
            declare void @__quantum__rt__initialize(ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
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

#[test]
fn tpp_single_x_yields_expected_qir() {
    check(
        "TPP X0",
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__t__body(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__rt__array_record_output(i64 0, ptr null)
              ret i64 0
            }

            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__qis__t__body(ptr)
            declare void @__quantum__rt__initialize(ptr)
            declare void @__quantum__qis__h__body(ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
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

#[test]
fn tpp_single_y_yields_expected_qir() {
    check(
        "TPP Y0",
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__t__body(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__rt__array_record_output(i64 0, ptr null)
              ret i64 0
            }

            declare void @__quantum__qis__s__body(ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__qis__s__adj(ptr)
            declare void @__quantum__qis__t__body(ptr)
            declare void @__quantum__rt__initialize(ptr)
            declare void @__quantum__qis__h__body(ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
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

#[test]
fn tpp_dag_single_z_yields_expected_qir() {
    // same as T_DAG 0
    check(
        "TPP_DAG Z0",
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              call void @__quantum__qis__t__adj(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__rt__array_record_output(i64 0, ptr null)
              ret i64 0
            }

            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__initialize(ptr)
            declare void @__quantum__qis__t__adj(ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
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

#[test]
fn tpp_three_factor_product_yields_expected_qir() {
    check(
        "TPP X0*Y1*Z2",
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__s__adj(ptr inttoptr (i64 1 to ptr))
              call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
              call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__t__body(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
              call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))
              call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__rt__array_record_output(i64 0, ptr null)
              ret i64 0
            }

            declare void @__quantum__qis__cx__body(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__qis__s__body(ptr)
            declare void @__quantum__qis__s__adj(ptr)
            declare void @__quantum__qis__h__body(ptr)
            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__qis__t__body(ptr)
            declare void @__quantum__rt__initialize(ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="0" }
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

#[test]
fn tpp_negated_product_applies_inverse() {
    check(
        "TPP !Z0",
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              call void @__quantum__qis__t__adj(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__rt__array_record_output(i64 0, ptr null)
              ret i64 0
            }

            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__initialize(ptr)
            declare void @__quantum__qis__t__adj(ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
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

#[test]
fn tpp_dag_negated_product_applies_inverse() {
    check(
        "TPP_DAG !Z0",
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              call void @__quantum__qis__t__body(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__rt__array_record_output(i64 0, ptr null)
              ret i64 0
            }

            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__qis__t__body(ptr)
            declare void @__quantum__rt__initialize(ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
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

#[test]
fn tpp_negation_on_later_factor_negates_whole_product() {
    check(
        "TPP X0*!Z1",
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__t__adj(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__rt__array_record_output(i64 0, ptr null)
              ret i64 0
            }

            declare void @__quantum__qis__cx__body(ptr, ptr)
            declare void @__quantum__qis__t__adj(ptr)
            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__initialize(ptr)
            declare void @__quantum__qis__h__body(ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="0" }
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

#[test]
fn tpp_double_negation_cancels() {
    check(
        "TPP !X0*!Z1",
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__t__body(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__rt__array_record_output(i64 0, ptr null)
              ret i64 0
            }

            declare void @__quantum__qis__cx__body(ptr, ptr)
            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__qis__t__body(ptr)
            declare void @__quantum__rt__initialize(ptr)
            declare void @__quantum__qis__h__body(ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="0" }
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

#[test]
fn tpp_identity_products_are_noops() {
    let source = indoc! {"
        TPP X0*X0 !Y1*Y1
        TPP_DAG Z2*Z2 !X3*X3
    "};
    check(
        source,
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

#[test]
fn tpp_anti_hermitian_product_yields_error() {
    check(
        "TPP X0*Y0",
        &expect![[r#"
            Qdk.Stim.Compiler.AntiHermitianPauliProduct

              x Pauli product must be Hermitian
               ,----
             1 | TPP X0*Y0
               :     ^^^^^
               `----
        "#]],
    );
}

#[test]
fn tpp_with_argument_yields_error() {
    check(
        "TPP(0.5) Z0",
        &expect![[r#"
            Qdk.Stim.Compiler.UnsupportedArgument

              x unsupported argument in instruction: TPP
               ,----
             1 | TPP(0.5) Z0
               : ^^^^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn tpp_with_qubit_target_yields_error() {
    check(
        "TPP 0",
        &expect![[r#"
            Qdk.Stim.Compiler.UnsupportedTarget

              x unsupported target in instruction: TPP
               ,----
             1 | TPP 0
               :     ^
               `----
        "#]],
    );
}

#[test]
fn ch_gate_yields_expected_qir() {
    check(
        "CH 0 1",
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              call void @__quantum__qis__ry__body(double 0.7853981633974483, ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__ry__body(double -0.7853981633974483, ptr inttoptr (i64 0 to ptr))
              call void @__quantum__rt__array_record_output(i64 0, ptr null)
              ret i64 0
            }

            declare void @__quantum__qis__cx__body(ptr, ptr)
            declare void @__quantum__qis__ry__body(double, ptr)
            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__initialize(ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="0" }
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

#[test]
fn ccz_gate_yields_expected_qir() {
    check(
        "CCZ 0 1 2",
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__ccx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__rt__array_record_output(i64 0, ptr null)
              ret i64 0
            }

            declare void @__quantum__qis__ccx__body(ptr, ptr, ptr)
            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__initialize(ptr)
            declare void @__quantum__qis__h__body(ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="0" }
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

#[test]
fn ccx_gate_yields_expected_qir() {
    check(
        "CCX 0 1 2",
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              call void @__quantum__qis__ccx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr))
              call void @__quantum__rt__array_record_output(i64 0, ptr null)
              ret i64 0
            }

            declare void @__quantum__qis__ccx__body(ptr, ptr, ptr)
            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__initialize(ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="0" }
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

#[test]
fn ccz_gate_broadcasts_over_triples() {
    check(
        "CCZ 0 1 2 3 4 5",
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__ccx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__h__body(ptr inttoptr (i64 3 to ptr))
              call void @__quantum__qis__ccx__body(ptr inttoptr (i64 4 to ptr), ptr inttoptr (i64 5 to ptr), ptr inttoptr (i64 3 to ptr))
              call void @__quantum__qis__h__body(ptr inttoptr (i64 3 to ptr))
              call void @__quantum__rt__array_record_output(i64 0, ptr null)
              ret i64 0
            }

            declare void @__quantum__qis__ccx__body(ptr, ptr, ptr)
            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__initialize(ptr)
            declare void @__quantum__qis__h__body(ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="6" "required_num_results"="0" }
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

#[test]
fn ccx_gate_with_one_target_yields_error() {
    check(
        "CCX 0",
        &expect![[r#"
            Qdk.Stim.Compiler.TargetCountNotMultipleOfThree

              x instruction CCX requires a multiple of three targets
               ,----
             1 | CCX 0
               : ^^^^^
               `----
        "#]],
    );
}

#[test]
fn ccx_gate_with_two_targets_yields_error() {
    check(
        "CCX 0 1",
        &expect![[r#"
            Qdk.Stim.Compiler.TargetCountNotMultipleOfThree

              x instruction CCX requires a multiple of three targets
               ,----
             1 | CCX 0 1
               : ^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn ccx_gate_with_four_targets_yields_error() {
    check(
        "CCX 0 1 2 3",
        &expect![[r#"
            Qdk.Stim.Compiler.TargetCountNotMultipleOfThree

              x instruction CCX requires a multiple of three targets
               ,----
             1 | CCX 0 1 2 3
               : ^^^^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn ccx_gate_with_argument_yields_error() {
    check(
        "CCX(0.5) 0 1 2",
        &expect![[r#"
            Qdk.Stim.Compiler.UnsupportedArgument

              x unsupported argument in instruction: CCX
               ,----
             1 | CCX(0.5) 0 1 2
               : ^^^^^^^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn ccx_gate_with_negated_target_yields_error() {
    check(
        "CCX 0 1 !2",
        &expect![[r#"
            Qdk.Stim.Compiler.NegatedTarget

              x target cannot be negated in instruction: CCX
               ,----
             1 | CCX 0 1 !2
               :         ^^
               `----
        "#]],
    );
}

#[test]
fn ccz_gate_with_measurement_record_target_yields_error() {
    let source = indoc! {"
        M 0
        CCZ rec[-1] 1 2
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.UnsupportedTarget

              x unsupported target in instruction: CCZ
               ,-[2:5]
             1 | M 0
             2 | CCZ rec[-1] 1 2
               :     ^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn r_x_yields_expected_qir() {
    check(
        "R_X(0.25) 0",
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              call void @__quantum__qis__rx__body(double 0.7853981633974483, ptr inttoptr (i64 0 to ptr))
              call void @__quantum__rt__array_record_output(i64 0, ptr null)
              ret i64 0
            }

            declare void @__quantum__qis__rx__body(double, ptr)
            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__initialize(ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
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

#[test]
fn r_y_yields_expected_qir() {
    check(
        "R_Y(-0.375) 0",
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__ry__body(double -1.1780972450961724, ptr inttoptr (i64 0 to ptr))
          call void @__quantum__rt__array_record_output(i64 0, ptr null)
          ret i64 0
        }

        declare void @__quantum__qis__ry__body(double, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__initialize(ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
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

#[test]
fn r_z_yields_expected_qir() {
    check(
        "R_Z(123.432) 0",
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__rz__body(double 387.77306441789534, ptr inttoptr (i64 0 to ptr))
          call void @__quantum__rt__array_record_output(i64 0, ptr null)
          ret i64 0
        }

        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__rz__body(double, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
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

#[test]
fn r_x_broadcasts_over_targets() {
    check(
        "R_X(0.125) 0 1 2",
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              call void @__quantum__qis__rx__body(double 0.39269908169872414, ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__rx__body(double 0.39269908169872414, ptr inttoptr (i64 1 to ptr))
              call void @__quantum__qis__rx__body(double 0.39269908169872414, ptr inttoptr (i64 2 to ptr))
              call void @__quantum__rt__array_record_output(i64 0, ptr null)
              ret i64 0
            }

            declare void @__quantum__qis__rx__body(double, ptr)
            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__initialize(ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="0" }
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

#[test]
fn r_x_without_argument_yields_error() {
    check(
        "R_X 0",
        &expect![[r#"
            Qdk.Stim.Compiler.MissingArg

              x missing argument in instruction: R_X
               ,----
             1 | R_X 0
               : ^^^^^
               `----
        "#]],
    );
}

#[test]
fn r_x_with_two_arguments_yields_error() {
    check(
        "R_X(0.25, 0.5) 0",
        &expect![[r#"
            Qdk.Stim.Compiler.WrongArgCount

              x instruction R_X requires 1 arguments, but found 2
               ,----
             1 | R_X(0.25, 0.5) 0
               : ^^^^^^^^^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn r_x_with_negated_target_yields_error() {
    check(
        "R_X(0.25) !0",
        &expect![[r#"
            Qdk.Stim.Compiler.NegatedTarget

              x target cannot be negated in instruction: R_X
               ,----
             1 | R_X(0.25) !0
               :           ^^
               `----
        "#]],
    );
}

#[test]
fn u3_yields_expected_qir() {
    check(
        "U3(0.1, 0.2, 0.3) 0",
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              call void @__quantum__qis__rz__body(double 0.9424777960769379, ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__ry__body(double 0.3141592653589793, ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__rz__body(double 0.6283185307179586, ptr inttoptr (i64 0 to ptr))
              call void @__quantum__rt__array_record_output(i64 0, ptr null)
              ret i64 0
            }

            declare void @__quantum__qis__ry__body(double, ptr)
            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__initialize(ptr)
            declare void @__quantum__qis__rz__body(double, ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
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

#[test]
fn u_alias_yields_expected_qir() {
    check(
        "U(0.1, 0.2, 0.3) 0",
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              call void @__quantum__qis__rz__body(double 0.9424777960769379, ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__ry__body(double 0.3141592653589793, ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__rz__body(double 0.6283185307179586, ptr inttoptr (i64 0 to ptr))
              call void @__quantum__rt__array_record_output(i64 0, ptr null)
              ret i64 0
            }

            declare void @__quantum__qis__ry__body(double, ptr)
            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__initialize(ptr)
            declare void @__quantum__qis__rz__body(double, ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
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

#[test]
fn u3_without_arguments_yields_error() {
    check(
        "U3 0",
        &expect![[r#"
            Qdk.Stim.Compiler.MissingArg

              x missing argument in instruction: U3
               ,----
             1 | U3 0
               : ^^^^
               `----
        "#]],
    );
}

#[test]
fn u3_with_one_argument_yields_error() {
    check(
        "U3(0.1) 0",
        &expect![[r#"
            Qdk.Stim.Compiler.WrongArgCount

              x instruction U3 requires 3 arguments, but found 1
               ,----
             1 | U3(0.1) 0
               : ^^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn u3_with_two_arguments_yields_error() {
    check(
        "U3(0.1, 0.2) 0",
        &expect![[r#"
            Qdk.Stim.Compiler.WrongArgCount

              x instruction U3 requires 3 arguments, but found 2
               ,----
             1 | U3(0.1, 0.2) 0
               : ^^^^^^^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn u3_with_four_arguments_yields_error() {
    check(
        "U3(0.1, 0.2, 0.3, 0.4) 0",
        &expect![[r#"
            Qdk.Stim.Compiler.WrongArgCount

              x instruction U3 requires 3 arguments, but found 4
               ,----
             1 | U3(0.1, 0.2, 0.3, 0.4) 0
               : ^^^^^^^^^^^^^^^^^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn u3_with_multiple_angles_that_overflow_radians_yields_errors() {
    check(
        "U3(1e308, 0.25, -1e308) 0",
        &expect![[r#"
        Qdk.Stim.Compiler.InvalidAngle

          x angle for U3 must be finite and representable in radians; found
          | 10000000000000000000000000000000000000000000000000000000000000000000000000
          | 00000000000000000000000000000000000000000000000000000000000000000000000000
          | 00000000000000000000000000000000000000000000000000000000000000000000000000
          | 00000000000000000000000000000000000000000000000000000000000000000000000000
          | 0000000000000 half turns
           ,----
         1 | U3(1e308, 0.25, -1e308) 0
           : ^^^^^^^^^^^^^^^^^^^^^^^^^
           `----

        Qdk.Stim.Compiler.InvalidAngle

          x angle for U3 must be finite and representable in radians; found
          | -1000000000000000000000000000000000000000000000000000000000000000000000000
          | 00000000000000000000000000000000000000000000000000000000000000000000000000
          | 00000000000000000000000000000000000000000000000000000000000000000000000000
          | 00000000000000000000000000000000000000000000000000000000000000000000000000
          | 00000000000000 half turns
           ,----
         1 | U3(1e308, 0.25, -1e308) 0
           : ^^^^^^^^^^^^^^^^^^^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn r_xx_yields_expected_qir() {
    check(
        "R_XX(0.25) 0 1",
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              call void @__quantum__qis__rxx__body(double 0.7853981633974483, ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
              call void @__quantum__rt__array_record_output(i64 0, ptr null)
              ret i64 0
            }

            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__initialize(ptr)
            declare void @__quantum__qis__rxx__body(double, ptr, ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="0" }
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

#[test]
fn r_yy_yields_expected_qir() {
    check(
        "R_YY(-0.6) 0 1",
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__ryy__body(double -1.8849555921538759, ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__rt__array_record_output(i64 0, ptr null)
          ret i64 0
        }

        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__qis__ryy__body(double, ptr, ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__initialize(ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="0" }
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

#[test]
fn r_zz_yields_expected_qir() {
    check(
        "R_ZZ(0.25) 0 1",
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              call void @__quantum__qis__rzz__body(double 0.7853981633974483, ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
              call void @__quantum__rt__array_record_output(i64 0, ptr null)
              ret i64 0
            }

            declare void @__quantum__qis__rzz__body(double, ptr, ptr)
            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__initialize(ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="0" }
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

#[test]
fn r_zz_broadcasts_over_pairs() {
    check(
        "R_ZZ(0.25) 0 1 2 3",
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              call void @__quantum__qis__rzz__body(double 0.7853981633974483, ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
              call void @__quantum__qis__rzz__body(double 0.7853981633974483, ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))
              call void @__quantum__rt__array_record_output(i64 0, ptr null)
              ret i64 0
            }

            declare void @__quantum__qis__rzz__body(double, ptr, ptr)
            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__initialize(ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="0" }
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

#[test]
fn r_xx_with_odd_target_count_yields_error() {
    check(
        "R_XX(0.25) 0 1 2",
        &expect![[r#"
            Qdk.Stim.Compiler.OddTargetCount

              x instruction R_XX requires an even number of targets
               ,----
             1 | R_XX(0.25) 0 1 2
               : ^^^^^^^^^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn r_xx_without_argument_yields_error() {
    check(
        "R_XX 0 1",
        &expect![[r#"
            Qdk.Stim.Compiler.MissingArg

              x missing argument in instruction: R_XX
               ,----
             1 | R_XX 0 1
               : ^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn r_xx_with_two_arguments_yields_error() {
    check(
        "R_XX(0.25, 0.5) 0 1",
        &expect![[r#"
            Qdk.Stim.Compiler.WrongArgCount

              x instruction R_XX requires 1 arguments, but found 2
               ,----
             1 | R_XX(0.25, 0.5) 0 1
               : ^^^^^^^^^^^^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn r_pauli_single_z_yields_expected_qir() {
    // same as R_Z(0.25) 0
    check(
        "R_PAULI(0.25) Z0",
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              call void @__quantum__qis__rz__body(double 0.7853981633974483, ptr inttoptr (i64 0 to ptr))
              call void @__quantum__rt__array_record_output(i64 0, ptr null)
              ret i64 0
            }

            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__initialize(ptr)
            declare void @__quantum__qis__rz__body(double, ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
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

#[test]
fn r_pauli_single_x_yields_expected_qir() {
    check(
        "R_PAULI(0.25) X0",
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__rz__body(double 0.7853981633974483, ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__rt__array_record_output(i64 0, ptr null)
              ret i64 0
            }

            declare void @__quantum__qis__h__body(ptr)
            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__qis__rz__body(double, ptr)
            declare void @__quantum__rt__initialize(ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
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

#[test]
fn r_pauli_single_y_yields_expected_qir() {
    check(
        "R_PAULI(0.25) Y0",
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              call void @__quantum__qis__s__adj(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__rz__body(double 0.7853981633974483, ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__rt__array_record_output(i64 0, ptr null)
              ret i64 0
            }

            declare void @__quantum__qis__rz__body(double, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__qis__s__adj(ptr)
            declare void @__quantum__qis__s__body(ptr)
            declare void @__quantum__rt__initialize(ptr)
            declare void @__quantum__qis__h__body(ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
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

#[test]
fn r_pauli_mixed_basis_product_yields_expected_qir() {
    check(
        "R_PAULI(0.25) X0*Y1*Z2",
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__s__adj(ptr inttoptr (i64 1 to ptr))
              call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
              call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__rz__body(double 0.7853981633974483, ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__cx__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__cx__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
              call void @__quantum__qis__s__body(ptr inttoptr (i64 1 to ptr))
              call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__rt__array_record_output(i64 0, ptr null)
              ret i64 0
            }

            declare void @__quantum__qis__cx__body(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__qis__s__adj(ptr)
            declare void @__quantum__qis__rz__body(double, ptr)
            declare void @__quantum__qis__s__body(ptr)
            declare void @__quantum__qis__h__body(ptr)
            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__initialize(ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="0" }
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

#[test]
fn r_pauli_negated_product_negates_angle() {
    check(
        "R_PAULI(0.25) !Z0",
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              call void @__quantum__qis__rz__body(double -0.7853981633974483, ptr inttoptr (i64 0 to ptr))
              call void @__quantum__rt__array_record_output(i64 0, ptr null)
              ret i64 0
            }

            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__initialize(ptr)
            declare void @__quantum__qis__rz__body(double, ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="0" }
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

#[test]
fn r_pauli_without_argument_yields_error() {
    check(
        "R_PAULI X0",
        &expect![[r#"
        Qdk.Stim.Compiler.MissingArg

          x missing argument in instruction: R_PAULI
           ,----
         1 | R_PAULI X0
           : ^^^^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn r_pauli_with_two_arguments_yields_error() {
    check(
        "R_PAULI(0.25, 0.5) X0",
        &expect![[r#"
        Qdk.Stim.Compiler.WrongArgCount

          x instruction R_PAULI requires 1 arguments, but found 2
           ,----
         1 | R_PAULI(0.25, 0.5) X0
           : ^^^^^^^^^^^^^^^^^^^^^
           `----
    "#]],
    );
}
