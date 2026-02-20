// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::unicode_not_nfc)]

use super::{CircuitEntryPoint, Interpreter};
use crate::{
    interpret::{CircuitGenerationMethod, Error},
    target::Profile,
};
use expect_test::expect;
use qsc_circuit::{Circuit, TracerConfig};
use qsc_data_structures::{language_features::LanguageFeatures, source::SourceMap};
use qsc_passes::PackageType;

fn interpreter(code: &str, package_type: PackageType, profile: Profile) -> Interpreter {
    let sources = SourceMap::new([("test.qs".into(), code.into())], None);
    let (std_id, store) = crate::compile::package_store_with_stdlib(profile.into());
    Interpreter::new(
        sources,
        package_type,
        profile.into(),
        LanguageFeatures::default(),
        store,
        &[(std_id, None)],
    )
    .expect("interpreter creation should succeed")
}

fn circuit_static(code: &str) -> String {
    circuit_with_options_success(
        code,
        Profile::AdaptiveRIF,
        CircuitEntryPoint::EntryPoint,
        CircuitGenerationMethod::Static,
        TracerConfig {
            source_locations: false,
            ..default_test_tracer_config()
        },
    )
    .display_with_groups()
    .to_string()
}

fn circuit_with_options_success(
    code: &str,
    profile: Profile,
    entry: CircuitEntryPoint,
    method: CircuitGenerationMethod,
    config: TracerConfig,
) -> Circuit {
    circuit_with_options(code, profile, entry, method, config)
        .expect("circuit generation should succeed")
}

fn circuit_with_options(
    code: &str,
    profile: Profile,
    entry: CircuitEntryPoint,
    method: CircuitGenerationMethod,
    config: TracerConfig,
) -> Result<Circuit, Vec<Error>> {
    let mut interpreter = interpreter(code, PackageType::Exe, profile);
    interpreter.set_quantum_seed(Some(2));
    interpreter.circuit(entry, method, config)
}

fn default_test_tracer_config() -> TracerConfig {
    TracerConfig {
        max_operations: TracerConfig::DEFAULT_MAX_OPERATIONS,
        source_locations: true,
        group_by_scope: true,
        prune_classical_qubits: false,
    }
}

#[test]
fn result_comparison_to_literal() {
    let circ = circuit_static(
        r"
            namespace Test {
                import Std.Measurement.*;
                @EntryPoint()
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
            }
        ",
    );

    expect![[r#"
        q_0    ─ Main[1] ─
                    ╘═════

        [1] Main:
            q_0    ── H ──── M ──── if: c_0 = |1〉[2] ───── |0〉 ──
                             ╘═════════════ ● ═══════════════════

        [2] if: c_0 = |1〉:
            q_0    ── X ──

    "#]]
    .assert_eq(&circ);
}

#[test]
fn result_comparison_to_literal_zero() {
    let circ = circuit_static(
        r"
            namespace Test {
                import Std.Measurement.*;
                @EntryPoint()
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
            }
        ",
    );

    expect![[r#"
        q_0    ─ Main[1] ─
                    ╘═════

        [1] Main:
            q_0    ── H ──── M ──── if: c_0 = |0〉[2] ───── |0〉 ──
                             ╘═════════════ ● ═══════════════════

        [2] if: c_0 = |0〉:
            q_0    ── X ──

    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn else_block_only() {
    let circ = circuit_static(
        r"
            namespace Test {
                import Std.Measurement.*;
                @EntryPoint()
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
            }
        ",
    );

    expect![[r#"
        q_0    ─ Main[1] ─
                    ╘═════

        [1] Main:
            q_0    ── H ──── M ──── if: c_0 = |1〉[2] ───── |0〉 ──
                             ╘═════════════ ● ═══════════════════

        [2] if: c_0 = |1〉:
            q_0    ── X ──

    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn result_comparison_to_result() {
    let circ = circuit_static(
        r"
            namespace Test {
                import Std.Measurement.*;
                @EntryPoint()
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
            }
        ",
    );

    expect![[r#"
        q_0    ─ Main[1] ─
                    ╘═════
        q_1    ─ Main[1] ─
                    ╘═════

        [1] Main:
            q_0    ── H ──── M ───── if: c_0c_1 = |00〉 or c_0c_1 = |11〉[2] ───── |0〉 ──
                             ╘════════════════════════ ● ══════════════════════════════
            q_1    ── H ──── M ────────────────────────┼──────────────────────── |0〉 ──
                             ╘════════════════════════ ● ══════════════════════════════

        [2] if: c_0c_1 = |00〉 or c_0c_1 = |11〉:
            q_0    ── X ──

            q_1    ───────

    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn result_comparison_empty_block() {
    let circ = circuit_static(
        r"
            namespace Test {
                import Std.Measurement.*;
                @EntryPoint()
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
            }
        ",
    );

    expect![[r#"
        q_0    ─ Main[1] ─
                    ╘═════
        q_1    ─ Main[1] ─
                    ╘═════

        [1] Main:
            q_0    ── H ──── M ──── |0〉 ──
                             ╘════════════
            q_1    ── H ──── M ──── |0〉 ──
                             ╘════════════
    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn if_else() {
    let circ = circuit_static(
        r"
            namespace Test {
                import Std.Measurement.*;

                @EntryPoint()
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
            }
        ",
    );

    expect![[r#"
        q_0    ─ Main[1] ─
                    ╘═════
        q_1    ─ Main[1] ─
                    ╘═════

        [1] Main:
            q_0    ── H ──── M ───────────────────────────────────────────────────────
                             ╘═════════════ ● ════════════════════ ● ═════════════════
            q_1    ──────────────── if: c_0 = |1〉[2] ───── if: c_0 = |0〉[3] ───── M ──
                                                                                  ╘═══

        [2] if: c_0 = |1〉:
            q_0    ───────

            q_1    ── X ──


        [3] if: c_0 = |0〉:
            q_0    ───────

            q_1    ── Y ──

    "#]]
    .assert_eq(&circ);
}

#[test]
fn sequential_ifs() {
    let circ = circuit_static(
        r"
            namespace Test {
                import Std.Measurement.*;

                @EntryPoint()
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
            }
        ",
    );

    expect![[r#"
        q_0    ─ Main[1] ─
                    ╘═════
        q_1    ─ Main[1] ─
                    ╘═════
        q_2    ─ Main[1] ─
                    ╘═════

        [1] Main:
            q_0    ── H ──── M ─────────────────────────────────────────────────────────────────────────────────────────────────────
                             ╘═════════════ ● ════════════════════ ● ═══════════════════════════════════════════════════════════════
            q_1    ── H ──── M ─────────────┼──────────────────────┼────────────────────────────────────────────────────────────────
                             ╘══════════════╪══════════════════════╪═════════════════════ ● ════════════════════ ● ═════════════════
            q_2    ──────────────── if: c_0 = |1〉[2] ───── if: c_0 = |0〉[3] ───── if: c_1 = |1〉[4] ───── if: c_1 = |0〉[5] ───── M ──
                                                                                                                                ╘═══

        [2] if: c_0 = |1〉:
            q_0    ───────

            q_1    ───────

            q_2    ── X ──


        [3] if: c_0 = |0〉:
            q_0    ───────

            q_1    ───────

            q_2    ── Z ──


        [4] if: c_1 = |1〉:
            q_0    ───────

            q_1    ───────

            q_2    ── X ──


        [5] if: c_1 = |0〉:
            q_0    ───────

            q_1    ───────

            q_2    ── Y ──

    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn nested_ifs() {
    let circ = circuit_static(
        r"
            namespace Test {
                import Std.Measurement.*;

                @EntryPoint()
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
            }
        ",
    );

    expect![[r#"
        q_0    ─ Main[1] ─
                    ╘═════
        q_1    ─ Main[1] ─
                    ╘═════
        q_2    ─ Main[1] ─
                    ╘═════

        [1] Main:
            q_0    ── H ──── M ───────────────────────────────────────────────────────
                             ╘═════════════ ● ════════════════════ ● ═════════════════
            q_1    ── H ──── M ─────────────┼──────────────────────┼──────────────────
                             ╘══════════════╪══════════════════════╪══════════════════
            q_2    ──────────────── if: c_0 = |1〉[2] ───── if: c_0 = |0〉[3] ───── M ──
                                                                                  ╘═══

        [2] if: c_0 = |1〉:
            q_0    ──────────────────────────────────────────────
                   ══════════ ● ════════════════════ ● ══════════
            q_1    ───────────┼──────────────────────┼───────────
                   ══════════ ● ════════════════════ ● ══════════
            q_2    ── if: c_1 = |1〉[4] ───── if: c_1 = |0〉[5] ───


        [3] if: c_0 = |0〉:
            q_0    ───────

            q_1    ───────

            q_2    ── Z ──


        [4] if: c_1 = |1〉:
            q_0    ───────

            q_1    ───────

            q_2    ── X ──


        [5] if: c_1 = |0〉:
            q_0    ───────

            q_1    ───────

            q_2    ── Y ──

    "#]]
    .assert_eq(&circ);
}

#[test]
fn variable_double_in_unitary_arg() {
    let circ = circuit_static(
        r"
            namespace Test {
                import Std.Measurement.*;

                @EntryPoint()
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
            }
        ",
    );

    expect![[r#"
        q_0    ─ Main[1] ─
                    ╘═════
        q_1    ─ Main[1] ─
                    ╘═════

        [1] Main:
            q_0    ── H ──── M ──────────────────────────
                             ╘══════════ ● ══════════════
            q_1    ─────────────── using: c_0[2] ─── M ──
                                                     ╘═══

        [2] using: c_0:
            q_0    ───────────────

            q_1    ─ Rx(f(c_0)) ──

    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn custom_intrinsic_variable_arg() {
    let circ = circuit_static(
        r"
        namespace Test {
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
        }
        ",
    );

    expect![[r#"
        q_0    ─ Main[1] ─
                    ╘═════

        [1] Main:
            q_0    ── H ──── M ─── using: c_0[2] ─
                             ╘══════════ ● ═══════

        [2] using: c_0:
            q_0    ─ foo(f(c_0)) ─

    "#]]
    .assert_eq(&circ);
}

#[test]
fn branch_on_dynamic_double() {
    let circ = circuit_static(
        r"
            namespace Test {
                import Std.Measurement.*;

                @EntryPoint()
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
            }
        ",
    );

    expect![[r#"
        q_0    ─ Main[1] ─
                    ╘═════
        q_1    ─ Main[1] ─
                    ╘═════

        [1] Main:
            q_0    ── H ──── M ──────────────────────────
                             ╘══════════ ● ══════════════
            q_1    ─────────────── using: c_0[2] ─── M ──
                                                     ╘═══

        [2] using: c_0:
            q_0    ───────────────

            q_1    ─ Rx(f(c_0)) ──

    "#]]
    .assert_eq(&circ);
}

#[test]
fn branch_on_dynamic_bool() {
    let circ = circuit_static(
        r"
            namespace Test {
                import Std.Measurement.*;

                @EntryPoint()
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
            }
        ",
    );

    expect![[r#"
        q_0    ─ Main[1] ─
                    ╘═════
        q_1    ─ Main[1] ─
                    ╘═════

        [1] Main:
            q_0    ── H ──── M ──────────────────────────
                             ╘══════════ ● ══════════════
            q_1    ─────────────── if: f(c_0)[2] ─── M ──
                                                     ╘═══

        [2] if: f(c_0):
            q_0    ───────

            q_1    ── X ──

    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn nested_callables_in_branch() {
    let circ = circuit_static(
        r"
            namespace Test {
                @EntryPoint()
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
            }
        ",
    );

    expect![[r#"
        q_0    ─ Main[1] ─
                    ┆
        q_1    ─ Main[1] ─
                    ╘═════

        [1] Main:
            q_0    ─ [ [Foo] ── [ [Bar] ─── X ──── Y ──── ] ──── ] ─────────── if: c_0 = |1〉[2] ───
                                                                                       │
            q_1    ──── H ───────────────────────────────────────────── M ─────────────┼───────────
                                                                        ╘═════════════ ● ══════════

        [2] if: c_0 = |1〉:
            q_0    ─ [ [Foo] ── [ [Bar] ─── X ──── Y ──── ] ──── ] ──
            q_1    ──────────────────────────────────────────────────

    "#]]
    .assert_eq(&circ.to_string());
}
