// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::unicode_not_nfc)]

use super::CircuitEntryPoint;
use crate::{
    interpret::{
        CircuitGenerationMethod,
        circuit_tests::{circuit_with_options_success, default_test_tracer_config},
    },
    target::Profile,
};
use expect_test::expect;
use indoc::indoc;

fn circuit_static(code: &str) -> String {
    circuit_with_options_success(
        code,
        Profile::AdaptiveRIF,
        CircuitEntryPoint::EntryPoint,
        CircuitGenerationMethod::Static,
        default_test_tracer_config(),
    )
    .display_with_groups()
    .to_string()
}

#[test]
fn result_comparison_to_literal() {
    let circ = circuit_static(indoc! {r"
            operation Main() : Result[] {
                use q1 = Qubit();
                H(q1);
                let r1 = M(q1);
                if (r1 == One) {
                    X(q1);
                }
                Reset(q1);
                [r1]
            }
        "});

    expect![[r#"
        q_0    ─ Main[1] ─
                    ╘═════

        [1] Main:
            q_0    ─ H@test.qs:2:4 ── M@test.qs:3:13 ──── if: c_0 = |1〉@test.qs:4:4[2] ───── |0〉@test.qs:7:4 ──
                                             ╘═════════════════════════ ● ═════════════════════════════════════

        [2] if: c_0 = |1〉:
            q_0    ─ X@test.qs:5:8 ─

    "#]]
    .assert_eq(&circ);
}

#[test]
fn result_comparison_to_literal_zero() {
    let circ = circuit_static(indoc! {r"
            operation Main() : Result[] {
                use q1 = Qubit();
                H(q1);
                let r1 = M(q1);
                if (r1 == Zero) {
                    X(q1);
                }
                Reset(q1);
                [r1]
            }
        "});

    expect![[r#"
        q_0    ─ Main[1] ─
                    ╘═════

        [1] Main:
            q_0    ─ H@test.qs:2:4 ── M@test.qs:3:13 ──── if: c_0 = |0〉@test.qs:4:4[2] ───── |0〉@test.qs:7:4 ──
                                             ╘═════════════════════════ ● ═════════════════════════════════════

        [2] if: c_0 = |0〉:
            q_0    ─ X@test.qs:5:8 ─

    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn else_block_only() {
    let circ = circuit_static(indoc! {r"
            operation Main() : Result[] {
                use q1 = Qubit();
                H(q1);
                let r1 = M(q1);
                if (r1 == Zero) {
                } else {
                    X(q1);
                }
                Reset(q1);
                [r1]
            }
        "});

    expect![[r#"
        q_0    ─ Main[1] ─
                    ╘═════

        [1] Main:
            q_0    ─ H@test.qs:2:4 ── M@test.qs:3:13 ──── if: c_0 = |1〉@test.qs:4:4[2] ───── |0〉@test.qs:8:4 ──
                                             ╘═════════════════════════ ● ═════════════════════════════════════

        [2] if: c_0 = |1〉:
            q_0    ─ X@test.qs:6:8 ─

    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn result_comparison_to_result() {
    let circ = circuit_static(indoc! {r"
            operation Main() : Result[] {
                use q1 = Qubit();
                use q2 = Qubit();
                H(q1);
                H(q2);
                let r1 = M(q1);
                let r2 = M(q2);
                if (r1 == r2) {
                    X(q1);
                }
                ResetAll([q1, q2]);
                [r1, r2]
            }
        "});

    expect![[r#"
        q_0    ─ Main[1] ─
                    ╘═════
        q_1    ─ Main[1] ─
                    ╘═════

        [1] Main:
            q_0    ─ H@test.qs:3:4 ── M@test.qs:5:13 ───── if: c_0c_1 = |00〉 or c_0c_1 = |11〉@test.qs:7:4[2] ───── |0〉@test.qs:10:4 ───
                                             ╘════════════════════════════════════ ● ══════════════════════════════════════════════════
            q_1    ─ H@test.qs:4:4 ── M@test.qs:6:13 ──────────────────────────────┼────────────────────────────── |0〉@test.qs:10:4 ───
                                             ╘════════════════════════════════════ ● ══════════════════════════════════════════════════

        [2] if: c_0c_1 = |00〉 or c_0c_1 = |11〉:
            q_0    ─ X@test.qs:8:8 ─

            q_1    ─────────────────

    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn result_comparison_empty_block() {
    let circ = circuit_static(indoc! {r"
            operation Main() : Int {
                use q1 = Qubit();
                use q2 = Qubit();
                H(q1);
                H(q2);
                let r1 = M(q1);
                let r2 = M(q2);
                mutable i = 4;
                if (r1 == r2) {
                    set i = 5;
                }
                ResetAll([q1, q2]);
                return i;
            }
        "});

    expect![[r#"
        q_0    ─ Main[1] ─
                    ╘═════
        q_1    ─ Main[1] ─
                    ╘═════

        [1] Main:
            q_0    ─ H@test.qs:3:4 ── M@test.qs:5:13 ──── |0〉@test.qs:11:4 ───
                                             ╘════════════════════════════════
            q_1    ─ H@test.qs:4:4 ── M@test.qs:6:13 ──── |0〉@test.qs:11:4 ───
                                             ╘════════════════════════════════
    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn if_else() {
    let circ = circuit_static(indoc! {r"
            operation Main() : Result[] {
                use q0 = Qubit();
                use q1 = Qubit();
                H(q0);
                let r = M(q0);
                if r == One {
                    X(q1);
                } else {
                    Y(q1);
                }
                let r1 = M(q1);
                [r, r1]
            }
        "});

    expect![[r#"
        q_0    ─ Main[1] ─
                    ╘═════
        q_1    ─ Main[1] ─
                    ╘═════

        [1] Main:
            q_0    ─ H@test.qs:3:4 ── M@test.qs:4:12 ───────────────────────────────────────────────────────────────────────────────────────────
                                             ╘═════════════════════════ ● ════════════════════════════════ ● ═══════════════════════════════════
            q_1    ────────────────────────────────────── if: c_0 = |1〉@test.qs:5:4[2] ───── if: c_0 = |0〉@test.qs:5:4[3] ──── M@test.qs:10:13 ─
                                                                                                                                      ╘═════════

        [2] if: c_0 = |1〉:
            q_0    ─────────────────

            q_1    ─ X@test.qs:6:8 ─


        [3] if: c_0 = |0〉:
            q_0    ─────────────────

            q_1    ─ Y@test.qs:8:8 ─

    "#]]
    .assert_eq(&circ);
}

#[test]
fn sequential_ifs() {
    let circ = circuit_static(indoc! {r"
            operation Main() : Result[] {
                use q0 = Qubit();
                use q1 = Qubit();
                use q2 = Qubit();
                H(q0);
                H(q1);
                let r0 = M(q0);
                let r1 = M(q1);
                if r0 == One {
                    X(q2);
                } else {
                    Z(q2);
                }
                if r1 == One {
                    X(q2);
                } else {
                    Y(q2);
                }
                let r2 = M(q2);
                [r0, r1, r2]
            }
        "});

    expect![[r#"
        q_0    ─ Main[1] ─
                    ╘═════
        q_1    ─ Main[1] ─
                    ╘═════
        q_2    ─ Main[1] ─
                    ╘═════

        [1] Main:
            q_0    ─ H@test.qs:4:4 ── M@test.qs:6:13 ─────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
                                             ╘═════════════════════════ ● ════════════════════════════════ ● ═════════════════════════════════════════════════════════════════════════════════════════════════════════
            q_1    ─ H@test.qs:5:4 ── M@test.qs:7:13 ───────────────────┼──────────────────────────────────┼──────────────────────────────────────────────────────────────────────────────────────────────────────────
                                             ╘══════════════════════════╪══════════════════════════════════╪═════════════════════════════════ ● ════════════════════════════════ ● ═══════════════════════════════════
            q_2    ────────────────────────────────────── if: c_0 = |1〉@test.qs:8:4[2] ───── if: c_0 = |0〉@test.qs:8:4[3] ───── if: c_1 = |1〉@test.qs:13:4[4] ──── if: c_1 = |0〉@test.qs:13:4[5] ─── M@test.qs:18:13 ─
                                                                                                                                                                                                            ╘═════════

        [2] if: c_0 = |1〉:
            q_0    ─────────────────

            q_1    ─────────────────

            q_2    ─ X@test.qs:9:8 ─


        [3] if: c_0 = |0〉:
            q_0    ───────────────────

            q_1    ───────────────────

            q_2    ─ Z@test.qs:11:8 ──


        [4] if: c_1 = |1〉:
            q_0    ───────────────────

            q_1    ───────────────────

            q_2    ─ X@test.qs:14:8 ──


        [5] if: c_1 = |0〉:
            q_0    ───────────────────

            q_1    ───────────────────

            q_2    ─ Y@test.qs:16:8 ──

    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn nested_ifs() {
    let circ = circuit_static(indoc! {r"
            operation Main() : Result[] {
                use q0 = Qubit();
                use q1 = Qubit();
                use q2 = Qubit();
                H(q0);
                H(q1);
                let r0 = M(q0);
                let r1 = M(q1);
                if r0 == One {
                    if r1 == One {
                        X(q2);
                    } else {
                        Y(q2);
                    }
                } else {
                    Z(q2);
                }
                let r2 = M(q2);
                [r0, r1, r2]
            }
        "});

    expect![[r#"
        q_0    ─ Main[1] ─
                    ╘═════
        q_1    ─ Main[1] ─
                    ╘═════
        q_2    ─ Main[1] ─
                    ╘═════

        [1] Main:
            q_0    ─ H@test.qs:4:4 ── M@test.qs:6:13 ───────────────────────────────────────────────────────────────────────────────────────────
                                             ╘═════════════════════════ ● ════════════════════════════════ ● ═══════════════════════════════════
            q_1    ─ H@test.qs:5:4 ── M@test.qs:7:13 ───────────────────┼──────────────────────────────────┼────────────────────────────────────
                                             ╘══════════════════════════╪══════════════════════════════════╪════════════════════════════════════
            q_2    ────────────────────────────────────── if: c_0 = |1〉@test.qs:8:4[2] ───── if: c_0 = |0〉@test.qs:8:4[3] ──── M@test.qs:17:13 ─
                                                                                                                                      ╘═════════

        [2] if: c_0 = |1〉:
            q_0    ──────────────────────────────────────────────────────────────────────
                   ════════════════ ● ════════════════════════════════ ● ════════════════
            q_1    ─────────────────┼──────────────────────────────────┼─────────────────
                   ════════════════ ● ════════════════════════════════ ● ════════════════
            q_2    ── if: c_1 = |1〉@test.qs:9:8[4] ───── if: c_1 = |0〉@test.qs:9:8[5] ───


        [3] if: c_0 = |0〉:
            q_0    ───────────────────

            q_1    ───────────────────

            q_2    ─ Z@test.qs:15:8 ──


        [4] if: c_1 = |1〉:
            q_0    ───────────────────

            q_1    ───────────────────

            q_2    ─ X@test.qs:10:12 ─


        [5] if: c_1 = |0〉:
            q_0    ───────────────────

            q_1    ───────────────────

            q_2    ─ Y@test.qs:12:12 ─

    "#]]
    .assert_eq(&circ);
}

#[test]
fn variable_double_in_unitary_arg() {
    let circ = circuit_static(indoc! {r"
            operation Main() : Result[] {
                use q0 = Qubit();
                use q1 = Qubit();
                H(q0);
                let r = M(q0);
                mutable theta = 1.0;
                if r == One {
                    set theta = 2.0;
                };
                Rx(theta, q1);
                let r1 = M(q1);
                [r, r1]
            }
        "});

    expect![[r#"
        q_0    ─ Main[1] ─
                    ╘═════
        q_1    ─ Main[1] ─
                    ╘═════

        [1] Main:
            q_0    ─ H@test.qs:3:4 ── M@test.qs:4:12 ──────────────────────────────────────────────────
                                             ╘══════════════════════ ● ════════════════════════════════
            q_1    ───────────────────────────────────── using: c_0@test.qs:9:4[2] ── M@test.qs:10:13 ─
                                                                                             ╘═════════

        [2] using: c_0:
            q_0    ───────────────────────────────────────────────────────────

            q_1    ─ Rx(f(c_0))@qsharp-library-source:Std/Intrinsic.qs:510:8 ─

    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn custom_intrinsic_variable_arg() {
    let circ = circuit_static(indoc! {r"
        operation foo(q: Qubit, x: Int): Unit {
            body intrinsic;
        }

        @EntryPoint()
        operation Main() : Unit {
            use q = Qubit();
            mutable x = 4;
            H(q);
            if (M(q) == One) {
                set x = 5;
            }
            foo(q, x);
        }
        "});

    expect![[r#"
        q_0    ─ Main[1] ─
                    ╘═════

        [1] Main:
            q_0    ─ H@test.qs:8:4 ── M@test.qs:9:8 ── using: c_0@test.qs:12:4[2] ──
                                            ╘══════════════════════ ● ══════════════

        [2] using: c_0:
            q_0    ─ foo(f(c_0))@test.qs:12:4 ──

    "#]]
    .assert_eq(&circ);
}

#[test]
fn branch_on_dynamic_double() {
    let circ = circuit_static(indoc! {r"
            operation Main() : Result[] {
                use q0 = Qubit();
                use q1 = Qubit();
                H(q0);
                let r = M(q0);
                mutable theta = 1.0;
                if r == One {
                    set theta = 2.0;
                };
                if theta > 1.5 {
                    set theta = 3.0;
                } else {
                    set theta = 4.0;
                }
                Rx(theta, q1);
                let r1 = M(q1);
                [r, r1]
            }
        "});

    expect![[r#"
        q_0    ─ Main[1] ─
                    ╘═════
        q_1    ─ Main[1] ─
                    ╘═════

        [1] Main:
            q_0    ─ H@test.qs:3:4 ── M@test.qs:4:12 ────────────────────────────────────────────────────
                                             ╘═══════════════════════ ● ═════════════════════════════════
            q_1    ───────────────────────────────────── using: c_0@test.qs:14:4[2] ─── M@test.qs:15:13 ─
                                                                                               ╘═════════

        [2] using: c_0:
            q_0    ───────────────────────────────────────────────────────────

            q_1    ─ Rx(f(c_0))@qsharp-library-source:Std/Intrinsic.qs:510:8 ─

    "#]]
    .assert_eq(&circ);
}

#[test]
fn branch_on_dynamic_bool() {
    let circ = circuit_static(indoc! {r"
            operation Main() : Result[] {
                use q0 = Qubit();
                use q1 = Qubit();
                H(q0);
                let r = M(q0);
                mutable cond = true;
                if r == One {
                    set cond = false;
                };
                if cond {
                    set cond = false;
                } else {
                    set cond = true;
                }
                if cond {
                    X(q1);
                }
                let r1 = M(q1);
                [r, r1]
            }
        "});

    expect![[r#"
        q_0    ─ Main[1] ─
                    ╘═════
        q_1    ─ Main[1] ─
                    ╘═════

        [1] Main:
            q_0    ─ H@test.qs:3:4 ── M@test.qs:4:12 ────────────────────────────────────────────────────
                                             ╘═══════════════════════ ● ═════════════════════════════════
            q_1    ───────────────────────────────────── if: f(c_0)@test.qs:14:4[2] ─── M@test.qs:17:13 ─
                                                                                               ╘═════════

        [2] if: f(c_0):
            q_0    ───────────────────

            q_1    ─ X@test.qs:15:8 ──

    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn nested_callables_in_branch() {
    let circ = circuit_static(indoc! {r"
            operation Main() : Unit {
                use q = Qubit();
                Foo(q);
                use q1 = Qubit();
                H(q1);
                if (M(q1) == One) {
                    Foo(q);
                }
            }
            operation Foo(q: Qubit) : Unit {
                Bar(q);
            }
            operation Bar(q: Qubit) : Unit {
                X(q);
                Y(q);
            }
        "});

    expect![[r#"
        q_0    ─ Main[1] ─
                    ┆
        q_1    ─ Main[1] ─
                    ╘═════

        [1] Main:
            q_0    ─ [ [Foo@test.qs:2:4] ── [ [Bar@test.qs:10:4] ─── X@test.qs:13:4 ─── Y@test.qs:14:4 ──── ] ──── ] ───────────────────── if: c_0 = |1〉@test.qs:5:4[2] ───
                                                                                                                                                         │
            q_1    ──── H@test.qs:4:4 ────────────────────────────────────────────────────────────────────────────────── M@test.qs:5:8 ──────────────────┼─────────────────
                                                                                                                               ╘════════════════════════ ● ════════════════

        [2] if: c_0 = |1〉:
            q_0    ─ [ [Foo@test.qs:6:8] ── [ [Bar@test.qs:10:4] ─── X@test.qs:13:4 ─── Y@test.qs:14:4 ──── ] ──── ] ──
            q_1    ────────────────────────────────────────────────────────────────────────────────────────────────────

    "#]]
    .assert_eq(&circ.to_string());
}
