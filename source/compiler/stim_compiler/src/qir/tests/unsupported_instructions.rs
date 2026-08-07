// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;

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

