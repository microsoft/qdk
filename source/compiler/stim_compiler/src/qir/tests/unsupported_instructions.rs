// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;
use indoc::indoc;

#[test]
fn mpad_yields_unsupported_error() {
    let source = "MPAD 0 1";
    check(
        source,
        &expect![[r#"
        Qdk.Stim.Compiler.UnsupportedInstruction

          x unsupported instruction: MPAD
           ,----
         1 | MPAD 0 1
           : ^^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn other_annotations_are_ignored() {
    let source = indoc! {"
    DETECTOR
    OBSERVABLE_INCLUDE(0)
    QUBIT_COORDS(0, 0) 0
    SHIFT_COORDS(1, 1)
    TICK
  "};
    check(
        source,
        &expect![[r#"
            [metadata]
              required_num_qubits = 0
              required_num_results = 0"#]],
    );
}

#[test]
fn heralded_erase_yields_unsupported_error() {
    let source = "HERALDED_ERASE(0.01) 0";
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.UnsupportedInstruction

              x unsupported instruction: HERALDED_ERASE
               ,----
             1 | HERALDED_ERASE(0.01) 0
               : ^^^^^^^^^^^^^^^^^^^^^^
               `----
        "#]],
    );
}
#[test]
fn heralded_pauli_channel_1_yields_unsupported_error() {
    let source = "HERALDED_PAULI_CHANNEL_1(0, 0, 0, 0.1) 0";
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.UnsupportedInstruction

              x unsupported instruction: HERALDED_PAULI_CHANNEL_1
               ,----
             1 | HERALDED_PAULI_CHANNEL_1(0, 0, 0, 0.1) 0
               : ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn reference_to_mpad_record_does_not_panic() {
    let source = "MPAD 0\nCX rec[-1] 1";
    check(
        source,
        &expect![[r#"
        Qdk.Stim.Compiler.UnsupportedInstruction

          x unsupported instruction: MPAD
           ,-[1:1]
         1 | MPAD 0
           : ^^^^^^
         2 | CX rec[-1] 1
           `----
    "#]],
    );
}

#[test]
fn reference_to_heralded_erase_record_does_not_panic() {
    let source = "HERALDED_ERASE(0.01) 0\nCX rec[-1] 1";
    check(
        source,
        &expect![[r#"
        Qdk.Stim.Compiler.UnsupportedInstruction

          x unsupported instruction: HERALDED_ERASE
           ,-[1:1]
         1 | HERALDED_ERASE(0.01) 0
           : ^^^^^^^^^^^^^^^^^^^^^^
         2 | CX rec[-1] 1
           `----
    "#]],
    );
}

#[test]
fn reference_to_heralded_pauli_channel_1_record_does_not_panic() {
    let source = "HERALDED_PAULI_CHANNEL_1(0, 0, 0, 0.1) 0\nCX rec[-1] 1";
    check(
        source,
        &expect![[r#"
      Qdk.Stim.Compiler.UnsupportedInstruction

        x unsupported instruction: HERALDED_PAULI_CHANNEL_1
         ,-[1:1]
       1 | HERALDED_PAULI_CHANNEL_1(0, 0, 0, 0.1) 0
         : ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
       2 | CX rec[-1] 1
         `----
  "#]],
    );
}
