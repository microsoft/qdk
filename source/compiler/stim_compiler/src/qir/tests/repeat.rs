// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::qir::tests::check;
use expect_test::expect;
use indoc::indoc;

#[test]
fn repeat_zero_times_yields_error() {
    let source = indoc! {"
        REPEAT 0 {
          X 0
        }
    "};
    check(
        source,
        &expect![[r#"
        Qdk.Stim.Compiler.ZeroRepeatCount

          x a REPEAT count of zero is not supported
           ,-[1:8]
         1 | REPEAT 0 {
           :        ^
         2 |   X 0
           `----
    "#]],
    );
}

#[test]
fn repeat_with_tag() {
    let source = indoc! {"
        REPEAT[my_tag] 3 {
          X 0
        }
    "};
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__rt__array_record_output(i64 0, ptr null)
          ret i64 0
        }

        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__qis__x__body(ptr)
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
fn repeat_with_args_yields_error() {
    let source = indoc! {"
        REPEAT(0.1) 3 {
          X 0
        }
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.UnsupportedArgument

              x unsupported argument in instruction: REPEAT
               ,-[1:1]
             1 | REPEAT(0.1) 3 {
               : ^^^^^^^^^^^^^
             2 |   X 0
               `----
        "#]],
    );
}

#[test]
fn repeat_with_multiple_targets_yields_error() {
    let source = indoc! {"
        REPEAT 3 2 1 {
          X 0
        }
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.UnsupportedTarget

              x unsupported target in instruction: REPEAT
               ,-[1:10]
             1 | REPEAT 3 2 1 {
               :          ^
             2 |   X 0
               `----
        "#]],
    );
}

#[test]
fn repeat_with_no_targets_yields_error() {
    let source = indoc! {"
        REPEAT {
          X 0
        }
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.MissingTarget

              x missing target in instruction: REPEAT
               ,-[1:1]
             1 | REPEAT {
               : ^^^^^^
             2 |   X 0
               `----
        "#]],
    );
}

#[test]
fn repeat_with_negated_count_yields_error() {
    let source = indoc! {"
        REPEAT !3 {
          X 0
        }
    "};
    check(
        source,
        &expect![[r#"
        Qdk.Stim.Compiler.UnsupportedTarget

          x unsupported target in instruction: REPEAT
           ,-[1:8]
         1 | REPEAT !3 {
           :        ^^
         2 |   X 0
           `----
    "#]],
    );
}

#[test]
fn repeat_with_pauli_target_yields_error() {
    let source = indoc! {"
        REPEAT X0 {
          X 0
        }
    "};
    check(
        source,
        &expect![[r#"
        Qdk.Stim.Compiler.UnsupportedTarget

          x unsupported target in instruction: REPEAT
           ,-[1:8]
         1 | REPEAT X0 {
           :        ^^
         2 |   X 0
           `----
    "#]],
    );
}

#[test]
fn repeat_with_measurement_record_target_yields_error() {
    let source = indoc! {"
        REPEAT rec[-1] {
          X 0
        }
    "};
    check(
        source,
        &expect![[r#"
        Qdk.Stim.Compiler.UnsupportedTarget

          x unsupported target in instruction: REPEAT
           ,-[1:8]
         1 | REPEAT rec[-1] {
           :        ^^^^^^^
         2 |   X 0
           `----
    "#]],
    );
}

#[test]
fn repeat_single_iteration() {
    let source = indoc! {"
        REPEAT 1 {
          H 0
        }
    "};
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__rt__array_record_output(i64 0, ptr null)
          ret i64 0
        }

        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
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
fn repeat_three_iterations() {
    let source = indoc! {"
        REPEAT 3 {
          H 0
        }
    "};
    check(
        source,
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__rt__array_record_output(i64 0, ptr null)
              ret i64 0
            }

            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
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
fn repeat_empty_body_yields_no_operations() {
    let source = indoc! {"
        REPEAT 4 {
        }
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
fn repeat_multiple_gates() {
    let source = indoc! {"
        REPEAT 2 {
          H 0
          S 0
          X 0
        }
    "};
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__s__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__rt__array_record_output(i64 0, ptr null)
          ret i64 0
        }

        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__qis__x__body(ptr)
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
fn repeat_broadcast() {
    let source = indoc! {"
        REPEAT 2 {
          X 0 1 2
        }
    "};
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__x__body(ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__x__body(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__x__body(ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__x__body(ptr inttoptr (i64 2 to ptr))
          call void @__quantum__rt__array_record_output(i64 0, ptr null)
          ret i64 0
        }

        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__qis__x__body(ptr)
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
fn repeat_with_measurement() {
    let source = indoc! {"
        REPEAT 3 {
          M 0
        }
    "};
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 2 to ptr))
          call void @__quantum__rt__array_record_output(i64 3, ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr null)
          ret i64 0
        }

        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__m__body(ptr, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="3" }
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
fn repeat_with_classically_controlled_gate() {
    let source = indoc! {"
        REPEAT 3 {
          M 0
          CX rec[-1] 1
        }
    "};
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          call void @classical_control_cx(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @classical_control_cx(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 2 to ptr))
          call void @classical_control_cx(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__rt__array_record_output(i64 3, ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr null)
          ret i64 0
        }

        define void @classical_control_cx(ptr %result, ptr %qubit) {
        block_cx_entry:
          %result_val = call i1 @__quantum__rt__read_result(ptr %result)
          br i1 %result_val, label %block_cx_apply, label %block_cx_exit
        block_cx_apply:
          call void @__quantum__qis__x__body(ptr %qubit)
          br label %block_cx_exit
        block_cx_exit:
          ret void
        }

        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__qis__x__body(ptr)
        declare i1 @__quantum__rt__read_result(ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__m__body(ptr, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="3" }
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
fn repeat_with_classically_controlled_gate_after_loop() {
    // this shouldn't yield an error because each measurement iteration is recorded as a separate result,
    // so rec[-2] is valid after the loop
    let source = indoc! {"
        REPEAT 2 {
          H 0
          M 0
        }
        CX rec[-2] 0
    "};
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__h__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @classical_control_cx(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          call void @__quantum__rt__array_record_output(i64 2, ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr null)
          ret i64 0
        }

        define void @classical_control_cx(ptr %result, ptr %qubit) {
        block_cx_entry:
          %result_val = call i1 @__quantum__rt__read_result(ptr %result)
          br i1 %result_val, label %block_cx_apply, label %block_cx_exit
        block_cx_apply:
          call void @__quantum__qis__x__body(ptr %qubit)
          br label %block_cx_exit
        block_cx_exit:
          ret void
        }

        declare void @__quantum__qis__h__body(ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__qis__x__body(ptr)
        declare i1 @__quantum__rt__read_result(ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__m__body(ptr, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="2" }
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
fn nested_repeat() {
    let source = indoc! {"
        REPEAT 2 {
          REPEAT 3 {
            X 0
          }
        }
    "};
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__rt__array_record_output(i64 0, ptr null)
          ret i64 0
        }

        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__qis__x__body(ptr)
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
fn nested_repeat_with_measurement() {
    let source = indoc! {"
        REPEAT 2 {
          REPEAT 2 {
            M 0
          }
        }
    "};
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 3 to ptr))
          call void @__quantum__rt__array_record_output(i64 4, ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 3 to ptr), ptr null)
          ret i64 0
        }

        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__m__body(ptr, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="4" }
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
fn sequential_repeats() {
    let source = indoc! {"
        REPEAT 2 {
          X 0
        }
        REPEAT 3 {
          Z 0
        }
    "};
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__z__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__z__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__z__body(ptr inttoptr (i64 0 to ptr))
          call void @__quantum__rt__array_record_output(i64 0, ptr null)
          ret i64 0
        }

        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare void @__quantum__qis__x__body(ptr)
        declare void @__quantum__qis__z__body(ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)

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
fn select_inside_repeat() {
    let source = indoc! {"
        REPEAT 2 {
          SELECT {
            M 0
            REQUIRE rec[-1]
          }
        }
    "};
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          br label %select_0
        select_0:
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          %l_0 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 0 to ptr))
          %r_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          %restart_0 = or i1 %l_0, %r_0
          br i1 %restart_0, label %select_0, label %continue_0
        continue_0:
          br label %select_1
        select_1:
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
          %l_1 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 1 to ptr))
          %r_1 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
          %restart_1 = or i1 %l_1, %r_1
          br i1 %restart_1, label %select_1, label %continue_1
        continue_1:
          call void @__quantum__rt__array_record_output(i64 2, ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr null)
          ret i64 0
        }

        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare i1 @__quantum__rt__read_loss(ptr)
        declare i1 @__quantum__rt__read_result(ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__m__body(ptr, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="2" }
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
fn repeat_inside_select() {
    let source = indoc! {"
        SELECT {
          REPEAT 3 {
            M 0
          }
          REQUIRE rec[-1]
        }
    "};
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          br label %select_0
        select_0:
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 2 to ptr))
          %l_0 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 2 to ptr))
          %r_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
          %restart_0 = or i1 %l_0, %r_0
          br i1 %restart_0, label %select_0, label %continue_0
        continue_0:
          call void @__quantum__rt__array_record_output(i64 3, ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr null)
          ret i64 0
        }

        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare i1 @__quantum__rt__read_loss(ptr)
        declare i1 @__quantum__rt__read_result(ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__m__body(ptr, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="3" }
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
fn require_inside_bare_repeat_yields_error() {
    let source = indoc! {"
        REPEAT 2 {
          M 0
          REQUIRE rec[-1]
        }
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.InstructionOutsideSelectBlock

              x REQUIRE must appear inside a SELECT block
               ,-[3:3]
             2 |   M 0
             3 |   REQUIRE rec[-1]
               :   ^^^^^^^^^^^^^^^
             4 | }
               `----
        "#]],
    );
}
