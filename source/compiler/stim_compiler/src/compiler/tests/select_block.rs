// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::compiler::tests::check;
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
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    mutable r_0 = Zero;
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
                    [r_0]
                }
            }"#]],
    );
}

#[test]
fn require_with_negated_record_yields_expected_qsharp() {
    let source = indoc! {"
      SELECT {
        M 0 1
        REQUIRE !rec[-1] rec[-2]
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
                        set r_0 = M(qs[0]);
                        set r_1 = M(qs[1]);
                        let loss_0 = IsLossResult(r_1) or IsLossResult(r_0);
                        let parity_0 = r_1 == Zero != r_0 == One;
                        let restart_0 = loss_0 or parity_0;
                        if restart_0 {
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
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[3];
                    mutable r_0 = Zero;
                    mutable r_1 = Zero;
                    mutable r_2 = Zero;
                    while true {
                        X(qs[0]);
                        set r_0 = M(qs[0]);
                        H(qs[1]);
                        X(qs[1]);
                        set r_1 = M(qs[1]);
                        set r_2 = M(qs[2]);
                        let loss_0 = IsLossResult(r_2) or IsLossResult(r_1) or IsLossResult(r_0);
                        let parity_0 = r_2 == One != r_1 == One != r_0 == One;
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
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
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
                        set r_1 = M(qs[1]);
                        let loss_1 = IsLossResult(r_1) or IsLossResult(r_0);
                        let parity_1 = r_1 == One != r_0 == One;
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
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[4];
                    mutable r_0 = Zero;
                    mutable r_1 = Zero;
                    mutable r_2 = Zero;
                    mutable r_3 = Zero;
                    while true {
                        set r_0 = M(qs[0]);
                        set r_1 = M(qs[1]);
                        set r_2 = M(qs[2]);
                        set r_3 = M(qs[3]);
                        let loss_0 = IsLossResult(r_3) or IsLossResult(r_2) or IsLossResult(r_1) or IsLossResult(r_0);
                        let parity_0 = r_3 == One != r_2 == One != r_1 == One != r_0 == One;
                        let restart_0 = loss_0 or parity_0;
                        if restart_0 {
                            continue;
                        };
                        break;
                    }
                    [r_0, r_1, r_2, r_3]
                }
            }"#]],
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
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[3];
                    let r_0 = M(qs[0]);
                    let r_1 = M(qs[1]);
                    let r_2 = M(qs[2]);
                    [r_0, r_1, r_2]
                }
            }"#]],
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
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    []
                }
            }"#]],
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
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    mutable r_0 = Zero;
                    mutable r_1 = Zero;
                    while true {
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
                        set r_1 = M(qs[1]);
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
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[3];
                    mutable r_0 = Zero;
                    mutable r_1 = Zero;
                    mutable r_2 = Zero;
                    while true {
                        while true {
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
                            set r_1 = M(qs[1]);
                            let loss_1 = IsLossResult(r_1);
                            let parity_1 = r_1 == One;
                            let restart_1 = loss_1 or parity_1;
                            if restart_1 {
                                continue;
                            };
                            break;
                        }
                        set r_2 = M(qs[2]);
                        let loss_2 = IsLossResult(r_2);
                        let parity_2 = r_2 == One;
                        let restart_2 = loss_2 or parity_2;
                        if restart_2 {
                            continue;
                        };
                        break;
                    }
                    [r_0, r_1, r_2]
                }
            }"#]],
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
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    mutable r_0 = Zero;
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
                    [r_0]
                }
            }"#]],
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
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    mutable r_0 = Zero;
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
                    [r_0]
                }
            }"#]],
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
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
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
                        set r_1 = M(qs[1]);
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
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[1];
                    mutable r_0 = Zero;
                    while true {
                        set r_0 = M(qs[0]);
                        let loss_0 = IsLossResult(r_0);
                        if loss_0 {
                            continue;
                        };
                        break;
                    }
                    [r_0]
                }
            }"#]],
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
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[4];
                    mutable r_0 = Zero;
                    mutable r_1 = Zero;
                    mutable r_2 = Zero;
                    mutable r_3 = Zero;
                    while true {
                        set r_0 = M(qs[0]);
                        set r_1 = M(qs[1]);
                        set r_2 = M(qs[2]);
                        set r_3 = M(qs[3]);
                        let loss_0 = IsLossResult(r_3) or IsLossResult(r_2) or IsLossResult(r_1) or IsLossResult(r_0);
                        if loss_0 {
                            continue;
                        };
                        break;
                    }
                    [r_0, r_1, r_2, r_3]
                }
            }"#]],
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
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    mutable r_0 = Zero;
                    mutable r_1 = Zero;
                    while true {
                        set r_0 = M(qs[0]);
                        let loss_0 = IsLossResult(r_0);
                        if loss_0 {
                            continue;
                        };
                        set r_1 = M(qs[1]);
                        let loss_1 = IsLossResult(r_1) or IsLossResult(r_0);
                        if loss_1 {
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
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    mutable r_0 = Zero;
                    mutable r_1 = Zero;
                    while true {
                        set r_0 = M(qs[0]);
                        set r_1 = M(qs[1]);
                        let loss_0 = IsLossResult(r_1);
                        let parity_0 = r_1 == One;
                        let restart_0 = loss_0 or parity_0;
                        if restart_0 {
                            continue;
                        };
                        let loss_1 = IsLossResult(r_0);
                        if loss_1 {
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
            namespace Stim {
                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[3];
                    mutable r_0 = Zero;
                    mutable r_1 = Zero;
                    mutable r_2 = Zero;
                    while true {
                        set r_0 = M(qs[0]);
                        set r_1 = M(qs[1]);
                        set r_2 = M(qs[2]);
                        let loss_0 = IsLossResult(r_2) or IsLossResult(r_1);
                        if loss_0 {
                            continue;
                        };
                        let loss_1 = IsLossResult(r_2) or IsLossResult(r_1) or IsLossResult(r_0);
                        let parity_0 = r_2 == One != r_1 == One != r_0 == One;
                        let restart_0 = loss_1 or parity_0;
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

#[test]
fn records_after_restartable_select_are_not_hoisted() {
    let source = indoc! {"
        M 0
        SELECT {
          M 1
          REQUIRE rec[-1]
        }
        M 2
    "};
    check(
        source,
        &expect![[r#"
        namespace Stim {
            @EntryPoint()
            operation Main() : Result[] {
                use qs = Qubit[3];
                mutable r_1 = Zero;
                let r_0 = M(qs[0]);
                while true {
                    set r_1 = M(qs[1]);
                    let loss_0 = IsLossResult(r_1);
                    let parity_0 = r_1 == One;
                    let restart_0 = loss_0 or parity_0;
                    if restart_0 {
                        continue;
                    };
                    break;
                }
                let r_2 = M(qs[2]);
                [r_0, r_1, r_2]
            }
        }"#]],
    );
}
