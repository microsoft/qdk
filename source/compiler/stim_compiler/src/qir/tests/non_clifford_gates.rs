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
        "R_Y(0.25) 0",
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              call void @__quantum__qis__ry__body(double 0.7853981633974483, ptr inttoptr (i64 0 to ptr))
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
        "R_Z(0.25) 0",
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
fn r_x_with_angle_that_overflows_radians_yields_error() {
    check(
        "R_X(1e308) 0",
        &expect![[r#"
            Qdk.Stim.Compiler.InvalidAngle

              x angle for R_X must be finite and representable in radians; found
              | 10000000000000000000000000000000000000000000000000000000000000000000000000
              | 00000000000000000000000000000000000000000000000000000000000000000000000000
              | 00000000000000000000000000000000000000000000000000000000000000000000000000
              | 00000000000000000000000000000000000000000000000000000000000000000000000000
              | 0000000000000 half turns
               ,----
             1 | R_X(1e308) 0
               : ^^^^^^^^^^^^
               `----
        "#]],
    );
}
