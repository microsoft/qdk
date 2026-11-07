// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::compiler::tests::check;
use expect_test::expect;
use indoc::indoc;

#[test]
fn cx_with_rec_control_yields_expected_qir() {
    let source = indoc! {"
        M 0
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
                    set r_0 = M(qs[0]);
                    if r_0 == One {
                        X(qs[1]);
                    };
                    [r_0]
                }
            }"#]],
    );
}

#[test]
fn cx_with_older_rec_control_yields_expected_qir() {
    let source = indoc! {"
        M 0
        M 1
        CX rec[-2] 2
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
                    set r_0 = M(qs[0]);
                    set r_1 = M(qs[1]);
                    if r_0 == One {
                        X(qs[2]);
                    };
                    [r_0, r_1]
                }
            }"#]],
    );
}

#[test]
fn cy_with_rec_control_yields_expected_qir() {
    let source = indoc! {"
        M 0
        CY rec[-1] 1
    "};
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    mutable r_0 = Zero;
                    set r_0 = M(qs[0]);
                    if r_0 == One {
                        Y(qs[1]);
                    };
                    [r_0]
                }
            }"#]],
    );
}

#[test]
fn cz_with_rec_on_first_target_yields_expected_qir() {
    let source = indoc! {"
        M 0
        CZ rec[-1] 1
    "};
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    mutable r_0 = Zero;
                    set r_0 = M(qs[0]);
                    if r_0 == One {
                        Z(qs[1]);
                    };
                    [r_0]
                }
            }"#]],
    );
}

#[test]
fn cz_with_rec_on_second_target_yields_expected_qir() {
    let source = indoc! {"
        M 0
        CZ 1 rec[-1]
    "};
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    mutable r_0 = Zero;
                    set r_0 = M(qs[0]);
                    if r_0 == One {
                        Z(qs[1]);
                    };
                    [r_0]
                }
            }"#]],
    );
}

#[test]
fn xcz_with_rec_on_second_target_yields_expected_qir() {
    let source = indoc! {"
        M 0
        XCZ 1 rec[-1]
    "};
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    mutable r_0 = Zero;
                    set r_0 = M(qs[0]);
                    if r_0 == One {
                        X(qs[1]);
                    };
                    [r_0]
                }
            }"#]],
    );
}

#[test]
fn ycz_with_rec_on_second_target_yields_expected_qir() {
    let source = indoc! {"
        M 0
        YCZ 1 rec[-1]
    "};
    check(
        source,
        &expect![[r#"
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    mutable r_0 = Zero;
                    set r_0 = M(qs[0]);
                    if r_0 == One {
                        Y(qs[1]);
                    };
                    [r_0]
                }
            }"#]],
    );
}

#[test]
fn cx_with_rec_control_crossing_select_boundary() {
    let source = indoc! {"
        M 0
        SELECT {
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
                    set r_0 = M(qs[0]);
                    while true {
                        if r_0 == One {
                            X(qs[1]);
                        };
                        break;
                    }
                    [r_0]
                }
            }"#]],
    );
}

#[test]
fn top_level_classical_control_reaches_into_select() {
    let source = indoc! {"
        SELECT {
          M 0
        }
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
                    while true {
                        set r_0 = M(qs[0]);
                        break;
                    }
                    if r_0 == One {
                        X(qs[1]);
                    };
                    [r_0]
                }
            }"#]],
    );
}
