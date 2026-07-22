// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::qir::tests::check;
use expect_test::expect;
use indoc::indoc;

#[test]
fn simple_select_block() {
    // should require result of M 0 == 0
    let source = indoc! {"
        SELECT {
          M 0
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
              %l_0 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 0 to ptr))
              %r_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              %restart_0 = or i1 %l_0, %r_0
              br i1 %restart_0, label %select_0, label %continue_0
            continue_0:
              call void @__quantum__rt__array_record_output(i64 1, ptr null)
              call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
              ret i64 0
            }

            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare i1 @__quantum__rt__read_loss(ptr)
            declare i1 @__quantum__rt__read_result(ptr)
            declare void @__quantum__rt__initialize(ptr)
            declare void @__quantum__qis__m__body(ptr, ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
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
fn long_select_block() {
    let source = indoc! {"
        SELECT {
          X 0
          M 0
          H 1
          X 1
          M 1
          M 2
          REQUIRE rec[-1] rec[-2] rec[-3]
        }
    "};
    check(
        source,
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              br label %select_0
            select_0:
              call void @__quantum__qis__x__body(ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__h__body(ptr inttoptr (i64 1 to ptr))
              call void @__quantum__qis__x__body(ptr inttoptr (i64 1 to ptr))
              call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
              call void @__quantum__qis__m__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
              %l_0 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 2 to ptr))
              %r_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
              %l_1 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 1 to ptr))
              %r_1 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
              %l_2 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 0 to ptr))
              %r_2 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              %loss_0 = or i1 %l_0, %l_1
              %loss_1 = or i1 %loss_0, %l_2
              %parity_0 = xor i1 %r_0, %r_1
              %parity_1 = xor i1 %parity_0, %r_2
              %restart_0 = or i1 %loss_1, %parity_1
              br i1 %restart_0, label %select_0, label %continue_0
            continue_0:
              call void @__quantum__rt__array_record_output(i64 3, ptr null)
              call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
              call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr null)
              call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr null)
              ret i64 0
            }

            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare i1 @__quantum__rt__read_result(ptr)
            declare void @__quantum__qis__h__body(ptr)
            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare void @__quantum__qis__x__body(ptr)
            declare i1 @__quantum__rt__read_loss(ptr)
            declare void @__quantum__rt__initialize(ptr)
            declare void @__quantum__qis__m__body(ptr, ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="3" }
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
fn multiple_requires_in_block() {
    let source = indoc! {"
        SELECT {
          M 0
          REQUIRE rec[-1]
          M 1
          REQUIRE rec[-1] rec[-2]
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
              call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
              %l_1 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 1 to ptr))
              %r_1 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
              %l_2 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 0 to ptr))
              %r_2 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              %loss_0 = or i1 %l_1, %l_2
              %parity_0 = xor i1 %r_1, %r_2
              %restart_1 = or i1 %loss_0, %parity_0
              br i1 %restart_1, label %select_0, label %continue_1
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

#[test]
fn multiple_targets_in_require() {
    let source = indoc! {"
      SELECT {
        M 0
        M 1
        M 2
        M 3
        REQUIRE rec[-1] rec[-2] rec[-3] rec[-4]
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
              call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
              call void @__quantum__qis__m__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
              call void @__quantum__qis__m__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 3 to ptr))
              %l_0 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 3 to ptr))
              %r_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 3 to ptr))
              %l_1 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 2 to ptr))
              %r_1 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
              %l_2 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 1 to ptr))
              %r_2 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
              %l_3 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 0 to ptr))
              %r_3 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              %loss_0 = or i1 %l_0, %l_1
              %loss_1 = or i1 %loss_0, %l_2
              %loss_2 = or i1 %loss_1, %l_3
              %parity_0 = xor i1 %r_0, %r_1
              %parity_1 = xor i1 %parity_0, %r_2
              %parity_2 = xor i1 %parity_1, %r_3
              %restart_0 = or i1 %loss_2, %parity_2
              br i1 %restart_0, label %select_0, label %continue_0
            continue_0:
              call void @__quantum__rt__array_record_output(i64 4, ptr null)
              call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
              call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr null)
              call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr null)
              call void @__quantum__rt__result_record_output(ptr inttoptr (i64 3 to ptr), ptr null)
              ret i64 0
            }

            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare i1 @__quantum__rt__read_loss(ptr)
            declare i1 @__quantum__rt__read_result(ptr)
            declare void @__quantum__rt__initialize(ptr)
            declare void @__quantum__qis__m__body(ptr, ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="4" }
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
fn select_block_no_require() {
    // should compile to a QIR that works as if there was no select
    let source = indoc! {"
        SELECT {
          M 0
          M 1
          M 2
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
              call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
              call void @__quantum__qis__m__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
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

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="3" }
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
fn empty_select_block() {
    let source = indoc! {"
        SELECT {
        }
    "};
    check(
        source,
        &expect![[r#"
            define i64 @ENTRYPOINT__main() #0 {
              call void @__quantum__rt__initialize(ptr null)
              br label %select_0
            select_0:
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
fn select_block_with_args_yields_error() {
    let source = indoc! {"
        SELECT(0.5) {
          M 0
          REQUIRE rec[-1]
        }
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.UnsupportedArgument

              x unsupported argument in instruction: SELECT
               ,-[1:1]
             1 | SELECT(0.5) {
               : ^^^^^^^^^^^
             2 |   M 0
               `----
        "#]],
    );
}

#[test]
fn select_block_with_targets_yields_error() {
    let source = indoc! {"
        SELECT 0 1 {
          M 0
          REQUIRE rec[-1]
        }
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.UnsupportedTarget

              x unsupported target in instruction: SELECT
               ,-[1:8]
             1 | SELECT 0 1 {
               :        ^
             2 |   M 0
               `----
        "#]],
    );
}

#[test]
fn select_block_with_tag() {
    let source = indoc! {"
        SELECT[some_tag] {
          M 0
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
              %l_0 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 0 to ptr))
              %r_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              %restart_0 = or i1 %l_0, %r_0
              br i1 %restart_0, label %select_0, label %continue_0
            continue_0:
              call void @__quantum__rt__array_record_output(i64 1, ptr null)
              call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
              ret i64 0
            }

            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare i1 @__quantum__rt__read_loss(ptr)
            declare i1 @__quantum__rt__read_result(ptr)
            declare void @__quantum__rt__initialize(ptr)
            declare void @__quantum__qis__m__body(ptr, ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
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
fn require_with_negated_target() {
    let source = indoc! {"
        SELECT {
          M 0
          REQUIRE !rec[-1]
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
              %n_0 = xor i1 %r_0, true
              %restart_0 = or i1 %l_0, %n_0
              br i1 %restart_0, label %select_0, label %continue_0
            continue_0:
              call void @__quantum__rt__array_record_output(i64 1, ptr null)
              call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
              ret i64 0
            }

            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare i1 @__quantum__rt__read_loss(ptr)
            declare i1 @__quantum__rt__read_result(ptr)
            declare void @__quantum__rt__initialize(ptr)
            declare void @__quantum__qis__m__body(ptr, ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
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
fn require_with_integer_target_yields_error() {
    let source = indoc! {"
        SELECT {
          M 0
          REQUIRE 0
        }
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.UnsupportedTarget

              x unsupported target in instruction: REQUIRE
               ,-[3:11]
             2 |   M 0
             3 |   REQUIRE 0
               :           ^
             4 | }
               `----
        "#]],
    );
}

#[test]
fn require_with_pauli_target_yields_error() {
    let source = indoc! {"
        SELECT {
          M 0
          REQUIRE X0
        }
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.UnsupportedTarget

              x unsupported target in instruction: REQUIRE
               ,-[3:11]
             2 |   M 0
             3 |   REQUIRE X0
               :           ^^
             4 | }
               `----
        "#]],
    );
}

#[test]
fn require_with_no_targets_yields_error() {
    let source = indoc! {"
        SELECT {
          M 0
          REQUIRE
        }
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.MissingTarget

              x missing target in instruction: REQUIRE
               ,-[3:3]
             2 |   M 0
             3 |   REQUIRE
               :   ^^^^^^^
             4 | }
               `----
        "#]],
    );
}

#[test]
fn require_no_select_block_yields_error() {
    let source = "REQUIRE rec[-1]";
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.InstructionOutsideSelectBlock

              x REQUIRE must appear inside a SELECT block
               ,----
             1 | REQUIRE rec[-1]
               : ^^^^^^^^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn require_before_measurement_yields_error() {
    let source = indoc! {"
        M 0
        SELECT {
          REQUIRE rec[-1]
          M 1
        }
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.AllMeasurementRecordsOutOfScope

              x all measurement records referenced by REQUIRE are out of scope
               ,-[3:3]
             2 | SELECT {
             3 |   REQUIRE rec[-1]
               :   ^^^^^^^^^^^^^^^
             4 |   M 1
               `----
        "#]],
    );
}

#[test]
fn rec_index_out_of_bounds() {
    let source = indoc! {"
        SELECT {
          M 0
          REQUIRE rec[-2]
        }
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.MeasurementRecordOutOfBounds

              x measurement record is out of bounds
               ,-[3:11]
             2 |   M 0
             3 |   REQUIRE rec[-2]
               :           ^^^^^^^
             4 | }
               `----
        "#]],
    );
}

#[test]
fn all_measurement_records_out_of_scope() {
    let source = indoc! {"
        M 0
        SELECT {
          M 1
          REQUIRE rec[-2]
        }
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.AllMeasurementRecordsOutOfScope

              x all measurement records referenced by REQUIRE are out of scope
               ,-[4:3]
             3 |   M 1
             4 |   REQUIRE rec[-2]
               :   ^^^^^^^^^^^^^^^
             5 | }
               `----
        "#]],
    );
}

#[test]
fn require_with_at_least_one_record_in_scope() {
    let source = indoc! {"
        M 0
        SELECT {
          M 1
          REQUIRE rec[-1] rec[-2]
        }
    "};
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          br label %select_0
        select_0:
          call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
          %l_0 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 1 to ptr))
          %r_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
          %l_1 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 0 to ptr))
          %r_1 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          %loss_0 = or i1 %l_0, %l_1
          %parity_0 = xor i1 %r_0, %r_1
          %restart_0 = or i1 %loss_0, %parity_0
          br i1 %restart_0, label %select_0, label %continue_0
        continue_0:
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

#[test]
fn reset_does_not_count_as_measurement() {
    // R does not produce a measurement record, so rec[-1] should be out of bounds.
    let source = indoc! {"
        SELECT {
          R 0
          REQUIRE rec[-1]
        }
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.MeasurementRecordOutOfBounds

              x measurement record is out of bounds
               ,-[3:11]
             2 |   R 0
             3 |   REQUIRE rec[-1]
               :           ^^^^^^^
             4 | }
               `----
        "#]],
    );
}

#[test]
fn measure_reset_counts_as_measurement() {
    // MR produces a measurement record.
    let source = indoc! {"
        SELECT {
          MR 0
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
              call void @__quantum__qis__mresetz__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              %l_0 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 0 to ptr))
              %r_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              %restart_0 = or i1 %l_0, %r_0
              br i1 %restart_0, label %select_0, label %continue_0
            continue_0:
              call void @__quantum__rt__array_record_output(i64 1, ptr null)
              call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
              ret i64 0
            }

            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare i1 @__quantum__rt__read_loss(ptr)
            declare i1 @__quantum__rt__read_result(ptr)
            declare void @__quantum__rt__initialize(ptr)
            declare void @__quantum__qis__mresetz__body(ptr, ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
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
fn pair_measurement_record_in_select() {
    let source = indoc! {"
        SELECT {
          MZZ 0 1
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
              call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
              call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 0 to ptr))
              call void @__quantum__qis__cx__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
              %l_0 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 0 to ptr))
              %r_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              %restart_0 = or i1 %l_0, %r_0
              br i1 %restart_0, label %select_0, label %continue_0
            continue_0:
              call void @__quantum__rt__array_record_output(i64 1, ptr null)
              call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
              ret i64 0
            }

            declare void @__quantum__qis__cx__body(ptr, ptr)
            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare i1 @__quantum__rt__read_loss(ptr)
            declare i1 @__quantum__rt__read_result(ptr)
            declare void @__quantum__rt__initialize(ptr)
            declare void @__quantum__qis__m__body(ptr, ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="2" "required_num_results"="1" }
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
fn pair_measurement_record_in_select_out_of_bounds() {
    // A two-qubit measurement produces a single measurement record.
    // So this should not be valid
    let source = indoc! {"
        SELECT {
          MZZ 0 1
          REQUIRE rec[-1] rec[-2]
        }
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.MeasurementRecordOutOfBounds

              x measurement record is out of bounds
               ,-[3:19]
             2 |   MZZ 0 1
             3 |   REQUIRE rec[-1] rec[-2]
               :                   ^^^^^^^
             4 | }
               `----
        "#]],
    );
}

#[test]
fn nested_select_blocks() {
    let source = indoc! {"
        SELECT {
          SELECT {
            M 0
            REQUIRE rec[-1]
          }
          M 1
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
              br label %select_1
            select_1:
              call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              %l_0 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 0 to ptr))
              %r_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              %restart_0 = or i1 %l_0, %r_0
              br i1 %restart_0, label %select_1, label %continue_0
            continue_0:
              call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
              %l_1 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 1 to ptr))
              %r_1 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
              %restart_1 = or i1 %l_1, %r_1
              br i1 %restart_1, label %select_0, label %continue_1
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

#[test]
fn deeply_nested_select_blocks() {
    let source = indoc! {"
        SELECT {
          SELECT {
            SELECT {
              M 0
              REQUIRE rec[-1]
            }
            M 1
            REQUIRE rec[-1]
          }
          M 2
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
              br label %select_1
            select_1:
              br label %select_2
            select_2:
              call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              %l_0 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 0 to ptr))
              %r_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              %restart_0 = or i1 %l_0, %r_0
              br i1 %restart_0, label %select_2, label %continue_0
            continue_0:
              call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
              %l_1 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 1 to ptr))
              %r_1 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
              %restart_1 = or i1 %l_1, %r_1
              br i1 %restart_1, label %select_1, label %continue_1
            continue_1:
              call void @__quantum__qis__m__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
              %l_2 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 2 to ptr))
              %r_2 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
              %restart_2 = or i1 %l_2, %r_2
              br i1 %restart_2, label %select_0, label %continue_2
            continue_2:
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

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="3" }
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
fn outer_select_reaches_into_inner_select() {
    let source = indoc! {"
        SELECT {
          SELECT {
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
              br label %select_1
            select_1:
              call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              %l_0 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 0 to ptr))
              %r_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              %restart_0 = or i1 %l_0, %r_0
              br i1 %restart_0, label %select_0, label %continue_0
            continue_0:
              call void @__quantum__rt__array_record_output(i64 1, ptr null)
              call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
              ret i64 0
            }

            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare i1 @__quantum__rt__read_loss(ptr)
            declare i1 @__quantum__rt__read_result(ptr)
            declare void @__quantum__rt__initialize(ptr)
            declare void @__quantum__qis__m__body(ptr, ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
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
fn outer_select_reaches_into_deeply_nested_inner_select() {
    let source = indoc! {"
        SELECT {
          SELECT {
            SELECT {
              M 0
            }
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
              br label %select_1
            select_1:
              br label %select_2
            select_2:
              call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
              %l_0 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 0 to ptr))
              %r_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              %restart_0 = or i1 %l_0, %r_0
              br i1 %restart_0, label %select_0, label %continue_0
            continue_0:
              call void @__quantum__rt__array_record_output(i64 1, ptr null)
              call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
              ret i64 0
            }

            declare void @__quantum__rt__array_record_output(i64, ptr)
            declare void @__quantum__rt__result_record_output(ptr, ptr)
            declare i1 @__quantum__rt__read_loss(ptr)
            declare i1 @__quantum__rt__read_result(ptr)
            declare void @__quantum__rt__initialize(ptr)
            declare void @__quantum__qis__m__body(ptr, ptr)

            attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
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
fn all_inner_selects_recs_out_of_scope() {
    let source = indoc! {"
        SELECT {
          M 0
          SELECT {
            REQUIRE rec[-1]
          }
        }
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.AllMeasurementRecordsOutOfScope

              x all measurement records referenced by REQUIRE are out of scope
               ,-[4:5]
             3 |   SELECT {
             4 |     REQUIRE rec[-1]
               :     ^^^^^^^^^^^^^^^
             5 |   }
               `----
        "#]],
    );
}

#[test]
fn sibling_select_blocks() {
    let source = indoc! {"
        SELECT {
          M 0
          REQUIRE rec[-1]
        }
        SELECT {
          M 1
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
              %l_0 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 0 to ptr))
              %r_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
              %restart_0 = or i1 %l_0, %r_0
              br i1 %restart_0, label %select_0, label %continue_0
            continue_0:
              br label %select_1
            select_1:
              call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
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

#[test]
fn all_sibling_select_block_recs_out_of_scope() {
    let source = indoc! {"
        SELECT {
          M 0
        }
        SELECT {
          M 1
          REQUIRE rec[-2]
        }
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.AllMeasurementRecordsOutOfScope

              x all measurement records referenced by REQUIRE are out of scope
               ,-[6:3]
             5 |   M 1
             6 |   REQUIRE rec[-2]
               :   ^^^^^^^^^^^^^^^
             7 | }
               `----
        "#]],
    );
}

#[test]
fn require_with_multiple_records_out_of_scope() {
    let source = indoc! {"
        SELECT {
          M 0
          M 1
        }
        SELECT {
          M 2
          REQUIRE rec[-2] rec[-3]
        }
    "};
    check(
        source,
        &expect![[r#"
        Qdk.Stim.Compiler.AllMeasurementRecordsOutOfScope

          x all measurement records referenced by REQUIRE are out of scope
           ,-[7:3]
         6 |   M 2
         7 |   REQUIRE rec[-2] rec[-3]
           :   ^^^^^^^^^^^^^^^^^^^^^^^
         8 | }
           `----
    "#]],
    );
}

#[test]
fn blockless_select_yields_error() {
    let source = indoc! {"
        X 0
        SELECT
        M 0
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.InstructionWithoutBlock

              x SELECT instruction must start a block
               ,-[2:1]
             1 | X 0
             2 | SELECT
               : ^^^^^^
             3 | M 0
               `----
        "#]],
    );
}

#[test]
fn require_with_args_yields_error() {
    let source = indoc! {"
        SELECT {
          M 0
          REQUIRE(0.5) rec[-1]
        }
    "};
    check(
        source,
        &expect![[r#"
        Qdk.Stim.Compiler.UnsupportedArgument

          x unsupported argument in instruction: REQUIRE
           ,-[3:3]
         2 |   M 0
         3 |   REQUIRE(0.5) rec[-1]
           :   ^^^^^^^^^^^^^^^^^^^^
         4 | }
           `----
    "#]],
    );
}

#[test]
fn simple_notleaked() {
    let source = indoc! {"
        SELECT {
          M 0
          NOTLEAKED rec[-1]
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
          br i1 %l_0, label %select_0, label %continue_0
        continue_0:
          call void @__quantum__rt__array_record_output(i64 1, ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
          ret i64 0
        }

        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare i1 @__quantum__rt__read_loss(ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__m__body(ptr, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="1" "required_num_results"="1" }
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
fn multiple_targets_in_notleaked() {
    let source = indoc! {"
        SELECT {
          M 0
          M 1
          M 2
          M 3
          NOTLEAKED rec[-1] rec[-2] rec[-3] rec[-4]
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
          call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__m__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
          call void @__quantum__qis__m__body(ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 3 to ptr))
          %l_0 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 3 to ptr))
          %l_1 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 2 to ptr))
          %l_2 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 1 to ptr))
          %l_3 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 0 to ptr))
          %loss_0 = or i1 %l_0, %l_1
          %loss_1 = or i1 %loss_0, %l_2
          %loss_2 = or i1 %loss_1, %l_3
          br i1 %loss_2, label %select_0, label %continue_0
        continue_0:
          call void @__quantum__rt__array_record_output(i64 4, ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 2 to ptr), ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 3 to ptr), ptr null)
          ret i64 0
        }

        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare i1 @__quantum__rt__read_loss(ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__m__body(ptr, ptr)

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="4" "required_num_results"="4" }
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
fn multiple_notleakeds_in_block() {
    let source = indoc! {"
        SELECT {
          M 0
          NOTLEAKED rec[-1]
          M 1
          NOTLEAKED rec[-1] rec[-2]
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
          br i1 %l_0, label %select_0, label %continue_0
        continue_0:
          call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
          %l_1 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 1 to ptr))
          %l_2 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 0 to ptr))
          %loss_0 = or i1 %l_1, %l_2
          br i1 %loss_0, label %select_0, label %continue_1
        continue_1:
          call void @__quantum__rt__array_record_output(i64 2, ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr null)
          ret i64 0
        }

        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare i1 @__quantum__rt__read_loss(ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__m__body(ptr, ptr)

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

#[test]
fn notleaked_with_negated_target_yields_error() {
    let source = indoc! {"
        SELECT {
          M 0
          NOTLEAKED !rec[-1]
        }
    "};
    check(
        source,
        &expect![[r#"
        Qdk.Stim.Compiler.NegatedTarget

          x target cannot be negated in instruction: NOTLEAKED
           ,-[3:3]
         2 |   M 0
         3 |   NOTLEAKED !rec[-1]
           :   ^^^^^^^^^^^^^^^^^^
         4 | }
           `----
    "#]],
    );
}

#[test]
fn notleaked_with_at_least_one_record_in_scope() {
    let source = indoc! {"
        M 0
        SELECT {
          M 1
          NOTLEAKED rec[-1] rec[-2]
        }
    "};
    check(
        source,
        &expect![[r#"
        define i64 @ENTRYPOINT__main() #0 {
          call void @__quantum__rt__initialize(ptr null)
          call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
          br label %select_0
        select_0:
          call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
          %l_0 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 1 to ptr))
          %l_1 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 0 to ptr))
          %loss_0 = or i1 %l_0, %l_1
          br i1 %loss_0, label %select_0, label %continue_0
        continue_0:
          call void @__quantum__rt__array_record_output(i64 2, ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 0 to ptr), ptr null)
          call void @__quantum__rt__result_record_output(ptr inttoptr (i64 1 to ptr), ptr null)
          ret i64 0
        }

        declare void @__quantum__rt__result_record_output(ptr, ptr)
        declare i1 @__quantum__rt__read_loss(ptr)
        declare void @__quantum__rt__array_record_output(i64, ptr)
        declare void @__quantum__rt__initialize(ptr)
        declare void @__quantum__qis__m__body(ptr, ptr)

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

#[test]
fn notleaked_with_integer_target_yields_error() {
    let source = indoc! {"
        SELECT {
          M 0
          NOTLEAKED 0
        }
    "};
    check(
        source,
        &expect![[r#"
        Qdk.Stim.Compiler.UnsupportedTarget

          x unsupported target in instruction: NOTLEAKED
           ,-[3:13]
         2 |   M 0
         3 |   NOTLEAKED 0
           :             ^
         4 | }
           `----
    "#]],
    );
}

#[test]
fn notleaked_with_pauli_target_yields_error() {
    let source = indoc! {"
        SELECT {
          M 0
          NOTLEAKED X0
        }
    "};
    check(
        source,
        &expect![[r#"
        Qdk.Stim.Compiler.UnsupportedTarget

          x unsupported target in instruction: NOTLEAKED
           ,-[3:13]
         2 |   M 0
         3 |   NOTLEAKED X0
           :             ^^
         4 | }
           `----
    "#]],
    );
}

#[test]
fn notleaked_with_no_targets_yields_error() {
    let source = indoc! {"
        SELECT {
          M 0
          NOTLEAKED
        }
    "};
    check(
        source,
        &expect![[r#"
        Qdk.Stim.Compiler.MissingTarget

          x missing target in instruction: NOTLEAKED
           ,-[3:3]
         2 |   M 0
         3 |   NOTLEAKED
           :   ^^^^^^^^^
         4 | }
           `----
    "#]],
    );
}

#[test]
fn notleaked_no_select_block_yields_error() {
    let source = "NOTLEAKED rec[-1]";
    check(
        source,
        &expect![[r#"
        Qdk.Stim.Compiler.InstructionOutsideSelectBlock

          x NOTLEAKED must appear inside a SELECT block
           ,----
         1 | NOTLEAKED rec[-1]
           : ^^^^^^^^^^^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn notleaked_rec_index_out_of_bounds() {
    let source = indoc! {"
        SELECT {
          M 0
          NOTLEAKED rec[-2]
        }
    "};
    check(
        source,
        &expect![[r#"
        Qdk.Stim.Compiler.MeasurementRecordOutOfBounds

          x measurement record is out of bounds
           ,-[3:13]
         2 |   M 0
         3 |   NOTLEAKED rec[-2]
           :             ^^^^^^^
         4 | }
           `----
    "#]],
    );
}

#[test]
fn notleaked_all_measurement_records_out_of_scope() {
    let source = indoc! {"
        M 0
        SELECT {
          M 1
          NOTLEAKED rec[-2]
        }
    "};
    check(
        source,
        &expect![[r#"
        Qdk.Stim.Compiler.AllMeasurementRecordsOutOfScope

          x all measurement records referenced by NOTLEAKED are out of scope
           ,-[4:3]
         3 |   M 1
         4 |   NOTLEAKED rec[-2]
           :   ^^^^^^^^^^^^^^^^^
         5 | }
           `----
    "#]],
    );
}

#[test]
fn notleaked_with_all_multiple_records_out_of_scope() {
    let source = indoc! {"
        M 0
        M 1
        SELECT {
          M 2
          NOTLEAKED rec[-2] rec[-3]
        }
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.AllMeasurementRecordsOutOfScope

              x all measurement records referenced by NOTLEAKED are out of scope
               ,-[5:3]
             4 |   M 2
             5 |   NOTLEAKED rec[-2] rec[-3]
               :   ^^^^^^^^^^^^^^^^^^^^^^^^^
             6 | }
               `----
        "#]],
    );
}

#[test]
fn notleaked_with_args_yields_error() {
    let source = indoc! {"
        SELECT {
          M 0
          NOTLEAKED(0.5) rec[-1]
        }
    "};
    check(
        source,
        &expect![[r#"
        Qdk.Stim.Compiler.UnsupportedArgument

          x unsupported argument in instruction: NOTLEAKED
           ,-[3:3]
         2 |   M 0
         3 |   NOTLEAKED(0.5) rec[-1]
           :   ^^^^^^^^^^^^^^^^^^^^^^
         4 | }
           `----
    "#]],
    );
}

#[test]
fn require_and_notleaked_in_block() {
    let source = indoc! {"
        SELECT {
          M 0
          M 1
          REQUIRE rec[-1]
          NOTLEAKED rec[-2]
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
          call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
          %l_0 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 1 to ptr))
          %r_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
          %restart_0 = or i1 %l_0, %r_0
          br i1 %restart_0, label %select_0, label %continue_0
        continue_0:
          %l_1 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 0 to ptr))
          br i1 %l_1, label %select_0, label %continue_1
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

#[test]
fn require_and_notleaked_with_multiple_targets() {
    let source = indoc! {"
        SELECT {
          M 0
          M 1
          M 2
          NOTLEAKED rec[-1] rec[-2]
          REQUIRE rec[-1] rec[-2] rec[-3]
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
          call void @__quantum__qis__m__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
          call void @__quantum__qis__m__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))
          %l_0 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 2 to ptr))
          %l_1 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 1 to ptr))
          %loss_0 = or i1 %l_0, %l_1
          br i1 %loss_0, label %select_0, label %continue_0
        continue_0:
          %l_2 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 2 to ptr))
          %r_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 2 to ptr))
          %l_3 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 1 to ptr))
          %r_1 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 1 to ptr))
          %l_4 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 0 to ptr))
          %r_2 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
          %loss_1 = or i1 %l_2, %l_3
          %loss_2 = or i1 %loss_1, %l_4
          %parity_0 = xor i1 %r_0, %r_1
          %parity_1 = xor i1 %parity_0, %r_2
          %restart_0 = or i1 %loss_2, %parity_1
          br i1 %restart_0, label %select_0, label %continue_1
        continue_1:
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

        attributes #0 = { "entry_point" "output_labeling_schema" "qir_profiles"="adaptive_profile" "required_num_qubits"="3" "required_num_results"="3" }
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
