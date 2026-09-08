// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::check;
use expect_test::expect;
use indoc::indoc;

#[test]
fn depolarize1_yields_expected_qir() {
    let source = "DEPOLARIZE1(0.01) 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @noise_intrinsic_0(ptr inttoptr (i64 0 to ptr))
                call void @noise_intrinsic_0(ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @noise_intrinsic_0(ptr) #2

            required_num_qubits: 2
            required_num_results: 0
            uses_noise: true

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
fn depolarize1_without_probability_yields_error() {
    let source = "DEPOLARIZE1 0 1";
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.MissingArg

              x missing argument in instruction: DEPOLARIZE1
               ,----
             1 | DEPOLARIZE1 0 1
               : ^^^^^^^^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn depolarize2_yields_expected_qir() {
    let source = "DEPOLARIZE2(0.01) 0 1 2 3";
    check(
        source,
        &expect![[r#"
            body:
                call void @noise_intrinsic_0(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @noise_intrinsic_0(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))

            declarations:
              declare void @noise_intrinsic_0(ptr, ptr) #2

            required_num_qubits: 4
            required_num_results: 0
            uses_noise: true

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
fn depolarize2_without_probability_yields_error() {
    let source = "DEPOLARIZE2 0 1 2 3";
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.MissingArg

              x missing argument in instruction: DEPOLARIZE2
               ,----
             1 | DEPOLARIZE2 0 1 2 3
               : ^^^^^^^^^^^^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn depolarize2_with_odd_number_of_targets_yields_error() {
    let source = "DEPOLARIZE2(0.01) 0";
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.OddTargetCount

              x instruction DEPOLARIZE2 requires an even number of targets
               ,----
             1 | DEPOLARIZE2(0.01) 0
               : ^^^^^^^^^^^^^^^^^^^
               `----
        "#]],
    );
}

#[test]
#[ignore = "unsupported instruction"]
fn heralded_erase_yields_error() {
    let source = "HERALDED_ERASE(0.01) 0 1";
    check(source, &expect![[""]]);
}

#[test]
#[ignore = "unsupported instruction"]
fn heralded_pauli_channel_1_yields_error() {
    let source = "HERALDED_PAULI_CHANNEL_1(0, 0, 0, 0.1) 0 1";
    check(source, &expect![[""]]);
}

#[test]
fn i_error_yields_expected_qir() {
    let source = indoc! {"
        # does nothing
        I_ERROR 0 1

        # does nothing with probability 0.1, else does nothing
        I_ERROR(0.1) 0 1

        # doesn't require a probability argument
        I_ERROR[LEAKAGE_NOISE_FOR_AN_ADVANCED_SIMULATOR:0.1] 0 2 4

        # checks for you that the disjoint probabilities in the arguments are legal
        I_ERROR[MULTIPLE_NOISE_MECHANISMS](0.1, 0.2) 0 2 4
    "};
    check(
        source,
        &expect![[r#"
            required_num_qubits: 0
            required_num_results: 0"#]],
    );
}

#[test]
fn ii_error_yields_expected_qir() {
    let source = indoc! {"
        # does nothing
        II_ERROR 0 1

        # does nothing with probability 0.1, else does nothing
        II_ERROR(0.1) 0 1

        # checks for you that the targets are two-qubit pairs
        II_ERROR[TWO_QUBIT_LEAKAGE_NOISE_FOR_AN_ADVANCED_SIMULATOR:0.1] 0 2 4 6

        # checks for you that the disjoint probabilities in the arguments are legal
        II_ERROR[MULTIPLE_TWO_QUBIT_NOISE_MECHANISMS](0.1, 0.2) 0 2 4 6
    "};
    check(
        source,
        &expect![[r#"
            required_num_qubits: 0
            required_num_results: 0"#]],
    );
}

#[test]
fn ii_error_with_odd_number_of_targets_yields_error() {
    let source = "II_ERROR 0";
    check(
        source,
        &expect![[r#"
        Qdk.Stim.Compiler.OddTargetCount

          x instruction II_ERROR requires an even number of targets
           ,----
         1 | II_ERROR 0
           : ^^^^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn pauli_channel_1_yields_expected_qir() {
    let source = "PAULI_CHANNEL_1(0.1, 0.2, 0.3) 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @noise_intrinsic_0(ptr inttoptr (i64 0 to ptr))
                call void @noise_intrinsic_0(ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @noise_intrinsic_0(ptr) #2

            required_num_qubits: 2
            required_num_results: 0
            uses_noise: true

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
fn pauli_channel_1_with_wrong_number_of_args_yields_error() {
    let source = "PAULI_CHANNEL_1(0.1, 0.2) 0 1";
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.TooFewArgs

              x too few arguments for instruction PAULI_CHANNEL_1; expected 3, found 2
               ,----
             1 | PAULI_CHANNEL_1(0.1, 0.2) 0 1
               :                 ^^^^^^^^
               `----
        "#]],
    );
}

#[test]
fn pauli_channel_2_yields_expected_qir() {
    let source = "PAULI_CHANNEL_2(0,0,0, 0,0.1,0,0, 0,0,0,0.2, 0,0,0,0) 0 1 2 3";
    check(
        source,
        &expect![[r#"
            body:
                call void @noise_intrinsic_0(ptr inttoptr (i64 0 to ptr), ptr inttoptr (i64 1 to ptr))
                call void @noise_intrinsic_0(ptr inttoptr (i64 2 to ptr), ptr inttoptr (i64 3 to ptr))

            declarations:
              declare void @noise_intrinsic_0(ptr, ptr) #2

            required_num_qubits: 4
            required_num_results: 0
            uses_noise: true

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
fn pauli_channel_2_with_odd_number_of_targets_yields_error() {
    let source = "PAULI_CHANNEL_2(0,0,0, 0,0.1,0,0, 0,0,0,0.2, 0,0,0,0) 0 1 2";
    check(
        source,
        &expect![[r#"
        Qdk.Stim.Compiler.OddTargetCount

          x instruction PAULI_CHANNEL_2 requires an even number of targets
           ,----
         1 | PAULI_CHANNEL_2(0,0,0, 0,0.1,0,0, 0,0,0,0.2, 0,0,0,0) 0 1 2
           : ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
           `----
    "#]],
    );
}

#[test]
fn pauli_channel_2_with_wrong_number_of_args_yields_error() {
    let source = "PAULI_CHANNEL_2(0.1) 0 1 2 3";
    check(
        source,
        &expect![[r#"
            Qdk.Stim.Compiler.TooFewArgs

              x too few arguments for instruction PAULI_CHANNEL_2; expected 15, found 1
               ,----
             1 | PAULI_CHANNEL_2(0.1) 0 1 2 3
               :                 ^^^
               `----
        "#]],
    );
}

#[test]
fn x_error_yields_expected_qir() {
    let source = "X_ERROR(0.01) 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @noise_intrinsic_0(ptr inttoptr (i64 0 to ptr))
                call void @noise_intrinsic_0(ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @noise_intrinsic_0(ptr) #2

            required_num_qubits: 2
            required_num_results: 0
            uses_noise: true

            NoiseConfig:
            intrinsics:
                0: NoiseTable:
                    qubits: 1
                    X: 0.01"#]],
    );
}

#[test]
fn y_error_yields_expected_qir() {
    let source = "Y_ERROR(0.01) 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @noise_intrinsic_0(ptr inttoptr (i64 0 to ptr))
                call void @noise_intrinsic_0(ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @noise_intrinsic_0(ptr) #2

            required_num_qubits: 2
            required_num_results: 0
            uses_noise: true

            NoiseConfig:
            intrinsics:
                0: NoiseTable:
                    qubits: 1
                    Y: 0.01"#]],
    );
}

#[test]
fn z_error_yields_expected_qir() {
    let source = "Z_ERROR(0.01) 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @noise_intrinsic_0(ptr inttoptr (i64 0 to ptr))
                call void @noise_intrinsic_0(ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @noise_intrinsic_0(ptr) #2

            required_num_qubits: 2
            required_num_results: 0
            uses_noise: true

            NoiseConfig:
            intrinsics:
                0: NoiseTable:
                    qubits: 1
                    Z: 0.01"#]],
    );
}

#[test]
fn loss_error_yields_expected_qir() {
    let source = "LOSS_ERROR(0.01) 0 1";
    check(
        source,
        &expect![[r#"
            body:
                call void @noise_intrinsic_0(ptr inttoptr (i64 0 to ptr))
                call void @noise_intrinsic_0(ptr inttoptr (i64 1 to ptr))

            declarations:
              declare void @noise_intrinsic_0(ptr) #2

            required_num_qubits: 2
            required_num_results: 0
            uses_noise: true

            NoiseConfig:
            intrinsics:
                0: NoiseTable:
                    qubits: 1
                    L: 0.01"#]],
    );
}
