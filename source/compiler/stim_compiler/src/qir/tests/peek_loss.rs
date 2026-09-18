// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::qir::tests::check;
use expect_test::expect;
use indoc::indoc;

#[test]
fn peek_loss_single_qubit() {
    check(
        "PEEK_LOSS 0",
        &expect![[r#"
            body:
                call void @__quantum__qis__peek_loss__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__peek_loss__body(ptr, ptr)

            required_num_qubits: 1
            required_num_results: 1"#]],
    );
}

#[test]
fn peek_loss_broadcasts_over_multiple_qubits() {
    check(
        "PEEK_LOSS 0 1 2",
        &expect![[r#"
            body:
                call void @__quantum__qis__peek_loss__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__peek_loss__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__peek_loss__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))

            declarations:
              declare void @__quantum__qis__peek_loss__body(ptr, ptr)

            required_num_qubits: 3
            required_num_results: 3"#]],
    );
}

#[test]
fn peek_loss_with_readout_noise_yields_expected_qir() {
    check(
        "PEEK_LOSS(0.5) 0",
        &expect![[r#"
            body:
                call void @__quantum__qis__peek_loss__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__rt__readout_noise(double 0.5, double 0.5, ptr inttoptr (i64 0 to ptr))

            declarations:
              declare void @__quantum__qis__peek_loss__body(ptr, ptr)
              declare void @__quantum__rt__readout_noise(double, double, ptr) #2

            required_num_qubits: 1
            required_num_results: 1
            uses_noise: true"#]],
    );
}

#[test]
fn peek_loss_with_invalid_readout_noise_yields_error() {
    check(
        "PEEK_LOSS(1.1) 0",
        &expect![[r#"
            Qdk.Stim.Compiler.InvalidProbability

              x probability for PEEK_LOSS must be between 0 and 1; found 1.1
               ,----
             1 | PEEK_LOSS(1.1) 0
               :           ^^^
               `----
        "#]],
    );
    check(
        "PEEK_LOSS(-0.1) 0",
        &expect![[r#"
            Qdk.Stim.Compiler.InvalidProbability

              x probability for PEEK_LOSS must be between 0 and 1; found -0.1
               ,----
             1 | PEEK_LOSS(-0.1) 0
               :           ^^^^
               `----
        "#]],
    );
}

#[test]
fn peek_loss_with_readout_noise_in_radians_yields_error() {
    check(
        "PEEK_LOSS(0.1rad) 0",
        &expect![[r#"
        Qdk.Stim.Compiler.UnexpectedRadians

          x argument for PEEK_LOSS cannot be specified in radians
           ,----
         1 | PEEK_LOSS(0.1rad) 0
           :           ^^^^^^
           `----
    "#]],
    );
}

#[test]
fn peek_loss_with_negative_readout_noise_in_radians_yields_errors() {
    check(
        "PEEK_LOSS(-0.1rad) 0",
        &expect![[r#"
            Qdk.Stim.Compiler.UnexpectedRadians

              x argument for PEEK_LOSS cannot be specified in radians
               ,----
             1 | PEEK_LOSS(-0.1rad) 0
               :           ^^^^^^^
               `----

            Qdk.Stim.Compiler.InvalidProbability

              x probability for PEEK_LOSS must be between 0 and 1; found -0.1
               ,----
             1 | PEEK_LOSS(-0.1rad) 0
               :           ^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn peek_loss_with_negated_target_yields_error() {
    check(
        "PEEK_LOSS !0",
        &expect![[r#"
            Qdk.Stim.Compiler.NegatedTarget

              x target cannot be negated in instruction: PEEK_LOSS
               ,----
             1 | PEEK_LOSS !0
               :           ^^
               `----
        "#]],
    );
}

#[test]
fn peek_loss_referenced_by_classical_control() {
    let source = indoc! {"
        PEEK_LOSS 0
        CX rec[-1] 1
    "};
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__peek_loss__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @classical_control_cx(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))

            definitions:
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

            declarations:
              declare i1 @__quantum__rt__read_result(ptr)
              declare void @__quantum__qis__peek_loss__body(ptr, ptr)
              declare void @__quantum__qis__x__body(ptr)

            required_num_qubits: 2
            required_num_results: 1"#]],
    );
}

#[test]
fn peek_loss_referenced_by_notleaked_in_select_block_yields_error() {
    let source = indoc! {"
        SELECT {
          PEEK_LOSS 0
          NOTLEAKED rec[-1]
        }
    "};
    check(
        source,
        &expect![[r#"
        Qdk.Stim.Compiler.NotLeakedOnPeekLoss

          x NOTLEAKED cannot reference a record produced by PEEK_LOSS
           ,-[3:13]
         2 |   PEEK_LOSS 0
         3 |   NOTLEAKED rec[-1]
           :             ^^^^^^^
         4 | }
           `----
    "#]],
    );
}

#[test]
fn peek_loss_referenced_by_require_in_select_block() {
    let source = indoc! {"
        SELECT {
          PEEK_LOSS 0
          REQUIRE rec[-1]
        }
    "};
    check(
        source,
        &expect![[r#"
            body:
                br label %select_0
              select_0:
                call void @__quantum__qis__peek_loss__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                %l_0 = call i1 @__quantum__rt__read_loss(ptr inttoptr (i64 0 to ptr))
                %r_0 = call i1 @__quantum__rt__read_result(ptr inttoptr (i64 0 to ptr))
                %restart_0 = or i1 %l_0, %r_0
                br i1 %restart_0, label %select_0, label %continue_0
              continue_0:

            declarations:
              declare i1 @__quantum__rt__read_loss(ptr)
              declare i1 @__quantum__rt__read_result(ptr)
              declare void @__quantum__qis__peek_loss__body(ptr, ptr)

            required_num_qubits: 1
            required_num_results: 1"#]],
    );
}

#[test]
fn peek_loss_interleaved_with_measurements() {
    let source = indoc! {"
        M 0
        PEEK_LOSS 1
        M 2
    "};
    check(
        source,
        &expect![[r#"
            body:
                call void @__quantum__qis__m__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
                call void @__quantum__qis__peek_loss__body(ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @__quantum__qis__m__body(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 2 to ptr))

            declarations:
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__peek_loss__body(ptr, ptr)

            required_num_qubits: 3
            required_num_results: 3"#]],
    );
}

#[test]
fn notleaked_errors_on_peek_mixed_with_measurement() {
    let source = indoc! {"
        SELECT {
          PEEK_LOSS 0
          M 1
          NOTLEAKED rec[-1] rec[-2]
        }
    "};
    check(
        source,
        &expect![[r#"
        Qdk.Stim.Compiler.NotLeakedOnPeekLoss

          x NOTLEAKED cannot reference a record produced by PEEK_LOSS
           ,-[4:21]
         3 |   M 1
         4 |   NOTLEAKED rec[-1] rec[-2]
           :                     ^^^^^^^
         5 | }
           `----
    "#]],
    );
}

#[test]
fn require_allows_peek_record_mixed_with_measurement() {
    let source = indoc! {"
        SELECT {
          PEEK_LOSS 0
          M 1
          REQUIRE rec[-1] rec[-2]
        }
    "};
    check(
        source,
        &expect![[r#"
            body:
                br label %select_0
              select_0:
                call void @__quantum__qis__peek_loss__body(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 0 to ptr))
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

            declarations:
              declare i1 @__quantum__rt__read_loss(ptr)
              declare i1 @__quantum__rt__read_result(ptr)
              declare void @__quantum__qis__m__body(ptr, ptr)
              declare void @__quantum__qis__peek_loss__body(ptr, ptr)

            required_num_qubits: 2
            required_num_results: 2"#]],
    );
}
