// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;
use indoc::indoc;

#[test]
fn correlated_error_yields_expected_qir() {
    let source = "CORRELATED_ERROR(0.01) X0";
    check(
        source,
        &expect![[r#"
            [entry_point]
                call void @noise_intrinsic_0(ptr inttoptr (i64 0 to ptr))

            [declarations]
              declare void @noise_intrinsic_0(ptr) #2

            [metadata]
              required_num_qubits = 1
              required_num_results = 0
              uses_noise = true

            NoiseConfig:
            intrinsics:
                0: NoiseTable:
                    qubits: 1
                    X: 0.01"#]],
    );
}

#[test]
fn correlated_error_chain_with_input_probabilities_summing_above_one_is_valid() {
    // The resulting mutually exclusive probabilities sum to at most 1
    // when each input probability is in [0, 1].
    let source = indoc! {"
        CORRELATED_ERROR(0.5) X0
        ELSE_CORRELATED_ERROR(0.5) Y0
        ELSE_CORRELATED_ERROR(0.5) Z0
    "};
    check(
        source,
        &expect![[r#"
            [entry_point]
                call void @noise_intrinsic_0(ptr inttoptr (i64 0 to ptr))

            [declarations]
              declare void @noise_intrinsic_0(ptr) #2

            [metadata]
              required_num_qubits = 1
              required_num_results = 0
              uses_noise = true

            NoiseConfig:
            intrinsics:
                0: NoiseTable:
                    qubits: 1
                    X: 0.5
                    Y: 0.25
                    Z: 0.125"#]],
    );
}

#[test]
fn correlated_error_chain_with_common_qubit_yields_expected_qir() {
    let source = indoc! {"
        CORRELATED_ERROR(0.01) X0
        ELSE_CORRELATED_ERROR(0.02) Z0 L1
        ELSE_CORRELATED_ERROR(0.03) X0 Z1 Y2
    "};
    check(
        source,
        &expect![[r#"
            [entry_point]
                call void @noise_intrinsic_0(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr))

            [declarations]
              declare void @noise_intrinsic_0(ptr, ptr, ptr) #2

            [metadata]
              required_num_qubits = 3
              required_num_results = 0
              uses_noise = true

            NoiseConfig:
            intrinsics:
                0: NoiseTable:
                    qubits: 3
                    XII: 0.01
                    ZLI: 0.0198
                    XZY: 0.029105999999999996"#]],
    );
}

#[test]
fn correlated_error_chain_with_disjoint_qubits_yields_expected_qir() {
    let source = indoc! {"
        CORRELATED_ERROR(0.01) X0
        ELSE_CORRELATED_ERROR(0.02) Z1 L2
        ELSE_CORRELATED_ERROR(0.03) Y3 Z4
    "};
    check(
        source,
        &expect![[r#"
            [entry_point]
                call void @noise_intrinsic_0(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr), ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr), ptr inttoptr (i64 4 to ptr))

            [declarations]
              declare void @noise_intrinsic_0(ptr, ptr, ptr, ptr, ptr) #2

            [metadata]
              required_num_qubits = 5
              required_num_results = 0
              uses_noise = true

            NoiseConfig:
            intrinsics:
                0: NoiseTable:
                    qubits: 5
                    XIIII: 0.01
                    IZLII: 0.0198
                    IIIYZ: 0.029105999999999996"#]],
    );
}

#[test]
fn else_correlated_error_with_preceding_else_correlated_error_yields_expected_qir() {
    let source = indoc! {"
        CORRELATED_ERROR(0.01) X0
        ELSE_CORRELATED_ERROR(0.02) Y0
        ELSE_CORRELATED_ERROR(0.03) Z0
    "};
    check(
        source,
        &expect![[r#"
            [entry_point]
                call void @noise_intrinsic_0(ptr inttoptr (i64 0 to ptr))

            [declarations]
              declare void @noise_intrinsic_0(ptr) #2

            [metadata]
              required_num_qubits = 1
              required_num_results = 0
              uses_noise = true

            NoiseConfig:
            intrinsics:
                0: NoiseTable:
                    qubits: 1
                    X: 0.01
                    Y: 0.0198
                    Z: 0.029105999999999996"#]],
    );
}

#[test]
fn else_correlated_error_by_itself_yields_error() {
    let source = "ELSE_CORRELATED_ERROR(0.02) X0";
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.OrphanedElseCorrelatedError

              x else_correlated_error must be preceded by a correlated_error or
              | else_correlated_error instruction
               ,----
             1 | ELSE_CORRELATED_ERROR(0.02) X0
               : ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn else_correlated_error_without_preceding_correlated_error_yields_error() {
    let source = indoc! {"
        CORRELATED_ERROR(0.01) X0
        I 0
        ELSE_CORRELATED_ERROR(0.02) X0
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.OrphanedElseCorrelatedError

              x else_correlated_error must be preceded by a correlated_error or
              | else_correlated_error instruction
               ,-[3:1]
             2 | I 0
             3 | ELSE_CORRELATED_ERROR(0.02) X0
               : ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn else_correlated_error_without_preceding_else_correlated_error_yields_error() {
    let source = indoc! {"
        CORRELATED_ERROR(0.01) X0
        ELSE_CORRELATED_ERROR(0.02) Y0
        I 0
        ELSE_CORRELATED_ERROR(0.02) Z0
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.OrphanedElseCorrelatedError

              x else_correlated_error must be preceded by a correlated_error or
              | else_correlated_error instruction
               ,-[4:1]
             3 | I 0
             4 | ELSE_CORRELATED_ERROR(0.02) Z0
               : ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn correlated_error_chain_does_not_continue_into_select_block() {
    let source = indoc! {"
        CORRELATED_ERROR(0.01) X0
        SELECT {
            ELSE_CORRELATED_ERROR(0.02) Y0
        }
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.OrphanedElseCorrelatedError

              x else_correlated_error must be preceded by a correlated_error or
              | else_correlated_error instruction
               ,-[3:5]
             2 | SELECT {
             3 |     ELSE_CORRELATED_ERROR(0.02) Y0
               :     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
             4 | }
               `----
        "#]],
    );
}

#[test]
fn correlated_error_chain_does_not_continue_out_of_select_block() {
    let source = indoc! {"
        SELECT {
            CORRELATED_ERROR(0.01) X0
        }
        ELSE_CORRELATED_ERROR(0.02) Y0
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.OrphanedElseCorrelatedError

              x else_correlated_error must be preceded by a correlated_error or
              | else_correlated_error instruction
               ,-[4:1]
             3 | }
             4 | ELSE_CORRELATED_ERROR(0.02) Y0
               : ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn correlated_error_chain_does_not_continue_into_repeat_block() {
    let source = indoc! {"
        CORRELATED_ERROR(0.01) X0
        REPEAT 2 {
            ELSE_CORRELATED_ERROR(0.02) Y0
        }
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.OrphanedElseCorrelatedError

              x else_correlated_error must be preceded by a correlated_error or
              | else_correlated_error instruction
               ,-[3:5]
             2 | REPEAT 2 {
             3 |     ELSE_CORRELATED_ERROR(0.02) Y0
               :     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
             4 | }
               `----
        "#]],
    );
}

#[test]
fn correlated_error_chain_does_not_continue_out_of_repeat_block() {
    let source = indoc! {"
        REPEAT 2 {
            CORRELATED_ERROR(0.01) X0
        }
        ELSE_CORRELATED_ERROR(0.02) Y0
    "};
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.OrphanedElseCorrelatedError

              x else_correlated_error must be preceded by a correlated_error or
              | else_correlated_error instruction
               ,-[4:1]
             3 | }
             4 | ELSE_CORRELATED_ERROR(0.02) Y0
               : ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn depolarize1_yields_expected_qir() {
    let source = "DEPOLARIZE1(0.01) 0";
    check(
        source,
        &expect![[r#"
            [entry_point]
                call void @noise_intrinsic_0(ptr inttoptr (i64 0 to ptr))

            [declarations]
              declare void @noise_intrinsic_0(ptr) #2

            [metadata]
              required_num_qubits = 1
              required_num_results = 0
              uses_noise = true

            NoiseConfig:
            intrinsics:
                0: NoiseTable:
                    qubits: 1
                    X: 0.0033333333333333335
                    Y: 0.0033333333333333335
                    Z: 0.0033333333333333335"#]],
    );
}

#[test]
fn depolarize2_yields_expected_qir() {
    let source = "DEPOLARIZE2(0.01) 0 1";
    check(
        source,
        &expect![[r#"
            [entry_point]
                call void @noise_intrinsic_0(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))

            [declarations]
              declare void @noise_intrinsic_0(ptr, ptr) #2

            [metadata]
              required_num_qubits = 2
              required_num_results = 0
              uses_noise = true

            NoiseConfig:
            intrinsics:
                0: NoiseTable:
                    qubits: 2
                    IX: 0.0006666666666666666
                    IY: 0.0006666666666666666
                    IZ: 0.0006666666666666666
                    XI: 0.0006666666666666666
                    XX: 0.0006666666666666666
                    XY: 0.0006666666666666666
                    XZ: 0.0006666666666666666
                    YI: 0.0006666666666666666
                    YX: 0.0006666666666666666
                    YY: 0.0006666666666666666
                    YZ: 0.0006666666666666666
                    ZI: 0.0006666666666666666
                    ZX: 0.0006666666666666666
                    ZY: 0.0006666666666666666
                    ZZ: 0.0006666666666666666"#]],
    );
}

#[test]
fn pauli_channel_1_yields_expected_qir() {
    let source = "PAULI_CHANNEL_1(0.1, 0.2, 0.3) 0";
    check(
        source,
        &expect![[r#"
            [entry_point]
                call void @noise_intrinsic_0(ptr inttoptr (i64 0 to ptr))

            [declarations]
              declare void @noise_intrinsic_0(ptr) #2

            [metadata]
              required_num_qubits = 1
              required_num_results = 0
              uses_noise = true

            NoiseConfig:
            intrinsics:
                0: NoiseTable:
                    qubits: 1
                    X: 0.1
                    Y: 0.2
                    Z: 0.3"#]],
    );
}

#[test]
fn pauli_channel_2_yields_expected_qir() {
    let source = "PAULI_CHANNEL_2(0,0,0, 0,0.1,0,0, 0,0,0,0.2, 0,0,0,0) 0 1";
    check(
        source,
        &expect![[r#"
            [entry_point]
                call void @noise_intrinsic_0(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))

            [declarations]
              declare void @noise_intrinsic_0(ptr, ptr) #2

            [metadata]
              required_num_qubits = 2
              required_num_results = 0
              uses_noise = true

            NoiseConfig:
            intrinsics:
                0: NoiseTable:
                    qubits: 2
                    IX: 0
                    IY: 0
                    IZ: 0
                    XI: 0
                    XX: 0.1
                    XY: 0
                    XZ: 0
                    YI: 0
                    YX: 0
                    YY: 0
                    YZ: 0.2
                    ZI: 0
                    ZX: 0
                    ZY: 0
                    ZZ: 0"#]],
    );
}

#[test]
fn x_error_yields_expected_qir() {
    let source = "X_ERROR(0.01) 0";
    check(
        source,
        &expect![[r#"
            [entry_point]
                call void @noise_intrinsic_0(ptr inttoptr (i64 0 to ptr))

            [declarations]
              declare void @noise_intrinsic_0(ptr) #2

            [metadata]
              required_num_qubits = 1
              required_num_results = 0
              uses_noise = true

            NoiseConfig:
            intrinsics:
                0: NoiseTable:
                    qubits: 1
                    X: 0.01"#]],
    );
}

#[test]
fn y_error_yields_expected_qir() {
    let source = "Y_ERROR(0.01) 0";
    check(
        source,
        &expect![[r#"
            [entry_point]
                call void @noise_intrinsic_0(ptr inttoptr (i64 0 to ptr))

            [declarations]
              declare void @noise_intrinsic_0(ptr) #2

            [metadata]
              required_num_qubits = 1
              required_num_results = 0
              uses_noise = true

            NoiseConfig:
            intrinsics:
                0: NoiseTable:
                    qubits: 1
                    Y: 0.01"#]],
    );
}

#[test]
fn z_error_yields_expected_qir() {
    let source = "Z_ERROR(0.01) 0";
    check(
        source,
        &expect![[r#"
            [entry_point]
                call void @noise_intrinsic_0(ptr inttoptr (i64 0 to ptr))

            [declarations]
              declare void @noise_intrinsic_0(ptr) #2

            [metadata]
              required_num_qubits = 1
              required_num_results = 0
              uses_noise = true

            NoiseConfig:
            intrinsics:
                0: NoiseTable:
                    qubits: 1
                    Z: 0.01"#]],
    );
}

#[test]
fn loss_error_yields_expected_qir() {
    let source = "LOSS_ERROR(0.01) 0";
    check(
        source,
        &expect![[r#"
            [entry_point]
                call void @noise_intrinsic_0(ptr inttoptr (i64 0 to ptr))

            [declarations]
              declare void @noise_intrinsic_0(ptr) #2

            [metadata]
              required_num_qubits = 1
              required_num_results = 0
              uses_noise = true

            NoiseConfig:
            intrinsics:
                0: NoiseTable:
                    qubits: 1
                    L: 0.01"#]],
    );
}

#[test]
fn noise_intrinsics_are_memoized() {
    let source = "
X_ERROR(0.01) 0
X_ERROR(0.02) 1
X_ERROR(0.01) 2
";
    check(
        source,
        &expect![[r#"
            [entry_point]
                call void @noise_intrinsic_0(ptr inttoptr (i64 0 to ptr))
                call void @noise_intrinsic_1(ptr inttoptr (i64 1 to ptr))
                call void @noise_intrinsic_0(ptr inttoptr (i64 2 to ptr))

            [declarations]
              declare void @noise_intrinsic_0(ptr) #2
              declare void @noise_intrinsic_1(ptr) #2

            [metadata]
              required_num_qubits = 3
              required_num_results = 0
              uses_noise = true

            NoiseConfig:
            intrinsics:
                0: NoiseTable:
                    qubits: 1
                    X: 0.01

                1: NoiseTable:
                    qubits: 1
                    X: 0.02"#]],
    );
}

#[test]
fn correlated_error_chains_with_same_shape_are_memoized() {
    let source = indoc! {"
            CORRELATED_ERROR(0.01) X0 Z1
            ELSE_CORRELATED_ERROR(0.02) Y0 Y1
            CORRELATED_ERROR(0.01) X2 Z3
            ELSE_CORRELATED_ERROR(0.02) Y2 Y3
        "};
    check(
        source,
        &expect![[r#"
            [entry_point]
                call void @noise_intrinsic_0(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @noise_intrinsic_0(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))

            [declarations]
              declare void @noise_intrinsic_0(ptr, ptr) #2

            [metadata]
              required_num_qubits = 4
              required_num_results = 0
              uses_noise = true

            NoiseConfig:
            intrinsics:
                0: NoiseTable:
                    qubits: 2
                    XZ: 0.01
                    YY: 0.0198"#]],
    );
}
