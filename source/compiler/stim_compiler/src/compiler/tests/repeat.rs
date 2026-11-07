// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::compiler::tests::check;
use expect_test::expect;
use indoc::indoc;

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
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    H(qs[0]);
                    []
                }
            }"#]],
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
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    H(qs[0]);
                    H(qs[0]);
                    H(qs[0]);
                    []
                }
            }"#]],
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
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    []
                }
            }"#]],
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
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    H(qs[0]);
                    S(qs[0]);
                    X(qs[0]);
                    H(qs[0]);
                    S(qs[0]);
                    X(qs[0]);
                    []
                }
            }"#]],
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
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    mutable r_0 = Zero;
                    mutable r_1 = Zero;
                    mutable r_2 = Zero;
                    set r_0 = M(qs[0]);
                    set r_1 = M(qs[0]);
                    set r_2 = M(qs[0]);
                    [r_0, r_1, r_2]
                }
            }"#]],
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
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    mutable r_0 = Zero;
                    mutable r_1 = Zero;
                    mutable r_2 = Zero;
                    set r_0 = M(qs[0]);
                    if r_0 == One {
                        X(qs[1]);
                    };
                    set r_1 = M(qs[0]);
                    if r_1 == One {
                        X(qs[1]);
                    };
                    set r_2 = M(qs[0]);
                    if r_2 == One {
                        X(qs[1]);
                    };
                    [r_0, r_1, r_2]
                }
            }"#]],
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
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    mutable r_0 = Zero;
                    mutable r_1 = Zero;
                    H(qs[0]);
                    set r_0 = M(qs[0]);
                    H(qs[0]);
                    set r_1 = M(qs[0]);
                    if r_0 == One {
                        X(qs[0]);
                    };
                    [r_0, r_1]
                }
            }"#]],
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
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    X(qs[0]);
                    X(qs[0]);
                    X(qs[0]);
                    X(qs[0]);
                    X(qs[0]);
                    X(qs[0]);
                    []
                }
            }"#]],
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
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    mutable r_0 = Zero;
                    mutable r_1 = Zero;
                    mutable r_2 = Zero;
                    mutable r_3 = Zero;
                    set r_0 = M(qs[0]);
                    set r_1 = M(qs[0]);
                    set r_2 = M(qs[0]);
                    set r_3 = M(qs[0]);
                    [r_0, r_1, r_2, r_3]
                }
            }"#]],
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
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    mutable r_0 = Zero;
                    mutable r_1 = Zero;
                    while true {
                        set r_0 = M(qs[0]);
                        let loss_0 = IsLossResult(r_0);
                        let parity_0 = r_0 == One;
                        let restart_0 = loss_0 or parity_0;
                        if restart_0 {
                            continue;
                        };
                        break;
                    }
                    while true {
                        set r_1 = M(qs[0]);
                        let loss_1 = IsLossResult(r_1);
                        let parity_1 = r_1 == One;
                        let restart_1 = loss_1 or parity_1;
                        if restart_1 {
                            continue;
                        };
                        break;
                    }
                    [r_0, r_1]
                }
            }"#]],
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
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    mutable r_0 = Zero;
                    mutable r_1 = Zero;
                    mutable r_2 = Zero;
                    while true {
                        set r_0 = M(qs[0]);
                        set r_1 = M(qs[0]);
                        set r_2 = M(qs[0]);
                        let loss_0 = IsLossResult(r_2);
                        let parity_0 = r_2 == One;
                        let restart_0 = loss_0 or parity_0;
                        if restart_0 {
                            continue;
                        };
                        break;
                    }
                    [r_0, r_1, r_2]
                }
            }"#]],
    );
}
