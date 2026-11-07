// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::compiler::tests::check;
use expect_test::expect;
use indoc::indoc;

#[test]
fn peek_loss_single_qubit() {
    check(
        "PEEK_LOSS 0",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    mutable r_0 = Zero;
                    set r_0 = __quantum__qis__peek_loss__body(qs[0]);
                    [r_0]
                }
                @Measurement()
                operation __quantum__qis__peek_loss__body(arg0 : Qubit) : Result {
                    body intrinsic;
                }
            }"#]],
    );
}

#[test]
fn peek_loss_with_readout_noise_yields_expected_qir() {
    check(
        "PEEK_LOSS(0.5) 0",
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    mutable r_0 = Zero;
                    set r_0 = __quantum__qis__peek_loss__body(qs[0]);
                    __quantum__rt__readout_noise(0.5, 0.5, r_0);
                    [r_0]
                }
                @Measurement()
                operation __quantum__qis__peek_loss__body(arg0 : Qubit) : Result {
                    body intrinsic;
                }
                @NoiseIntrinsic()
                operation __quantum__rt__readout_noise(arg0 : Double, arg1 : Double, arg2 : Result) : Unit {
                    body intrinsic;
                }
            }"#]],
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
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    mutable r_0 = Zero;
                    set r_0 = __quantum__qis__peek_loss__body(qs[0]);
                    if r_0 == One {
                        X(qs[1]);
                    };
                    [r_0]
                }
                @Measurement()
                operation __quantum__qis__peek_loss__body(arg0 : Qubit) : Result {
                    body intrinsic;
                }
            }"#]],
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
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    mutable r_0 = Zero;
                    while true {
                        set r_0 = __quantum__qis__peek_loss__body(qs[0]);
                        let loss_0 = IsLossResult(r_0);
                        let parity_0 = r_0 == One;
                        let restart_0 = loss_0 or parity_0;
                        if restart_0 {
                            continue;
                        };
                        break;
                    }
                    [r_0]
                }
                @Measurement()
                operation __quantum__qis__peek_loss__body(arg0 : Qubit) : Result {
                    body intrinsic;
                }
            }"#]],
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
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[3];
                    mutable r_0 = Zero;
                    mutable r_1 = Zero;
                    mutable r_2 = Zero;
                    set r_0 = M(qs[0]);
                    set r_1 = __quantum__qis__peek_loss__body(qs[1]);
                    set r_2 = M(qs[2]);
                    [r_0, r_1, r_2]
                }
                @Measurement()
                operation __quantum__qis__peek_loss__body(arg0 : Qubit) : Result {
                    body intrinsic;
                }
            }"#]],
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
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    mutable r_0 = Zero;
                    mutable r_1 = Zero;
                    while true {
                        set r_0 = __quantum__qis__peek_loss__body(qs[0]);
                        set r_1 = M(qs[1]);
                        let loss_0 = IsLossResult(r_1) or IsLossResult(r_0);
                        let parity_0 = r_1 == One != r_0 == One;
                        let restart_0 = loss_0 or parity_0;
                        if restart_0 {
                            continue;
                        };
                        break;
                    }
                    [r_0, r_1]
                }
                @Measurement()
                operation __quantum__qis__peek_loss__body(arg0 : Qubit) : Result {
                    body intrinsic;
                }
            }"#]],
    );
}
