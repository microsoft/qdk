// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::unicode_not_nfc)]

use super::{CircuitEntryPoint, Debugger, Interpreter};
use crate::target::Profile;
use expect_test::expect;
use miette::Diagnostic;
use qsc_data_structures::language_features::LanguageFeatures;
use qsc_eval::output::GenericReceiver;
use qsc_frontend::compile::SourceMap;
use qsc_passes::PackageType;

fn interpreter(code: &str, profile: Profile) -> Interpreter {
    let sources = SourceMap::new([("test.qs".into(), code.into())], None);
    let (std_id, store) = crate::compile::package_store_with_stdlib(profile.into());
    Interpreter::new(
        sources,
        PackageType::Exe,
        profile.into(),
        LanguageFeatures::default(),
        store,
        &[(std_id, None)],
    )
    .expect("interpreter creation should succeed")
}

fn interpreter_with_circuit_trace(code: &str, profile: Profile) -> Interpreter {
    let sources = SourceMap::new([("test.qs".into(), code.into())], None);
    let (std_id, store) = crate::compile::package_store_with_stdlib(profile.into());
    Interpreter::with_circuit_trace(
        sources,
        PackageType::Exe,
        profile.into(),
        LanguageFeatures::default(),
        store,
        &[(std_id, None)],
    )
    .expect("interpreter creation should succeed")
}

#[test]
fn empty() {
    let mut interpreter = interpreter(
        r#"
            namespace Test {
                @EntryPoint()
                operation Main() : Unit {
                    Message("hi");
                }
            }
        "#,
        Profile::Unrestricted,
    );

    let circ = interpreter
        .circuit(CircuitEntryPoint::EntryPoint, false)
        .expect("circuit generation should succeed");

    expect![].assert_eq(&circ.to_string());
}

#[test]
fn one_gate() {
    let mut interpreter = interpreter(
        r"
            namespace Test {
                @EntryPoint()
                operation Main() : Unit {
                    use q = Qubit();
                    H(q);
                }
            }
        ",
        Profile::Unrestricted,
    );

    let circ = interpreter
        .circuit(CircuitEntryPoint::EntryPoint, false)
        .expect("circuit generation should succeed");

    expect![[r"
        q_0    ── H ──
    "]]
    .assert_eq(&circ.to_string());
}

#[test]
fn measure_same_qubit_twice() {
    let mut interpreter = interpreter(
        r"
            namespace Test {
                @EntryPoint()
                operation Main() : Result[] {
                    use q = Qubit();
                    H(q);
                    let r1 = M(q);
                    let r2 = M(q);
                    [r1, r2]
                }
            }
        ",
        Profile::Unrestricted,
    );

    let circ = interpreter
        .circuit(CircuitEntryPoint::EntryPoint, false)
        .expect("circuit generation should succeed");

    expect![["
        q_0    ── H ──── M ──── M ──
                         ╘══════╪═══
                                ╘═══
    "]]
    .assert_eq(&circ.to_string());
}

#[test]
fn toffoli() {
    let mut interpreter = interpreter(
        r"
            namespace Test {
                @EntryPoint()
                operation Main() : Unit {
                    use q = Qubit[3];
                    CCNOT(q[0], q[1], q[2]);
                }
            }
        ",
        Profile::Unrestricted,
    );

    let circ = interpreter
        .circuit(CircuitEntryPoint::EntryPoint, false)
        .expect("circuit generation should succeed");

    expect![[r"
        q_0    ── ● ──
        q_1    ── ● ──
        q_2    ── X ──
    "]]
    .assert_eq(&circ.to_string());
}

#[test]
fn rotation_gate() {
    let mut interpreter = interpreter(
        r"
            namespace Test {
                @EntryPoint()
                operation Main() : Unit {
                    use q = Qubit();
                    Rx(Microsoft.Quantum.Math.PI()/2.0, q);
                }
            }
        ",
        Profile::Unrestricted,
    );

    let circ = interpreter
        .circuit(CircuitEntryPoint::EntryPoint, false)
        .expect("circuit generation should succeed");

    expect![[r#"
        q_0    ─ Rx(1.5708) ──
    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn classical_for_loop() {
    let mut interpreter = interpreter(
        r"
            namespace Test {
                @EntryPoint()
                operation Main() : Unit {
                    use q = Qubit();
                    for i in 0..5 {
                        X(q);
                    }
                }
            }
        ",
        Profile::Unrestricted,
    );

    let circ = interpreter
        .circuit(CircuitEntryPoint::EntryPoint, false)
        .expect("circuit generation should succeed");

    expect![[r"
        q_0    ── X ──── X ──── X ──── X ──── X ──── X ──
    "]]
    .assert_eq(&circ.to_string());
}

#[test]
fn m_base_profile() {
    let mut interpreter = interpreter(
        r"
            namespace Test {
                import Std.Measurement.*;
                @EntryPoint()
                operation Main() : Result[] {
                    use q = Qubit();
                    H(q);
                    [M(q)]
                }
            }
        ",
        Profile::Base,
    );

    let circ = interpreter
        .circuit(CircuitEntryPoint::EntryPoint, false)
        .expect("circuit generation should succeed");

    expect![[r#"
        q_0    ── H ──── M ──
                         ╘═══
    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn m_unrestricted_profile() {
    let mut interpreter = interpreter(
        r"
            namespace Test {
                import Std.Measurement.*;
                @EntryPoint()
                operation Main() : Result[] {
                    use q = Qubit();
                    H(q);
                    [M(q)]
                }
            }
        ",
        Profile::Unrestricted,
    );

    let circ = interpreter
        .circuit(CircuitEntryPoint::EntryPoint, false)
        .expect("circuit generation should succeed");

    expect![[r"
        q_0    ── H ──── M ──
                         ╘═══
    "]]
    .assert_eq(&circ.to_string());
}

#[test]
fn mresetz_unrestricted_profile() {
    let mut interpreter = interpreter(
        r"
            namespace Test {
                import Std.Measurement.*;
                @EntryPoint()
                operation Main() : Result[] {
                    use q = Qubit();
                    H(q);
                    [MResetZ(q)]
                }
            }
        ",
        Profile::Unrestricted,
    );

    let circ = interpreter
        .circuit(CircuitEntryPoint::EntryPoint, false)
        .expect("circuit generation should succeed");

    expect![[r"
        q_0    ── H ──── M ──── |0〉 ──
                         ╘════════════
    "]]
    .assert_eq(&circ.to_string());
}

#[test]
fn mresetz_base_profile() {
    let mut interpreter = interpreter(
        r"
            namespace Test {
                import Std.Measurement.*;
                @EntryPoint()
                operation Main() : Result[] {
                    use q = Qubit();
                    H(q);
                    [MResetZ(q)]
                }
            }
        ",
        Profile::Base,
    );

    let circ = interpreter
        .circuit(CircuitEntryPoint::EntryPoint, false)
        .expect("circuit generation should succeed");

    expect![[r#"
        q_0    ── H ──── M ──── |0〉 ──
                         ╘════════════
    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn unrestricted_profile_result_comparison() {
    let mut interpreter = interpreter_with_circuit_trace(
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
        Profile::Unrestricted,
    );

    interpreter.set_quantum_seed(Some(2));

    let circuit_err = interpreter
        .circuit(CircuitEntryPoint::EntryPoint, false)
        .expect_err("circuit should return error")
        .pop()
        .expect("error should exist");

    expect!["Qsc.Eval.ResultComparisonUnsupported"].assert_eq(
        &circuit_err
            .code()
            .expect("error code should exist")
            .to_string(),
    );

    let circuit = interpreter.get_circuit();
    expect![""].assert_eq(&circuit.to_string());

    let mut out = std::io::sink();
    let mut r = GenericReceiver::new(&mut out);

    // Result comparisons are okay when tracing
    // circuit with the simulator.
    let circ = interpreter
        .circuit(CircuitEntryPoint::EntryPoint, true)
        .expect("circuit generation should succeed");

    expect![[r"
        q_0    ── H ──── M ───── X ───── |0〉 ──
                         ╘═════════════════════
        q_1    ── H ──── M ──── |0〉 ───────────
                         ╘═════════════════════
    "]]
    .assert_eq(&circ.to_string());

    // Result comparisons are also okay if calling
    // get_circuit() after incremental evaluation,
    // because we're using the current simulator
    // state.
    interpreter
        .eval_fragments(&mut r, "Test.Main();")
        .expect("eval should succeed");

    let circuit = interpreter.get_circuit();
    expect![[r"
        q_0    ── H ──── M ───── X ───── |0〉 ──
                         ╘═════════════════════
        q_1    ── H ──── M ──── |0〉 ───────────
                         ╘═════════════════════
    "]]
    .assert_eq(&circuit.to_string());
}

#[test]
fn custom_intrinsic() {
    let mut interpreter = interpreter(
        r"
    namespace Test {
        operation foo(q: Qubit): Unit {
            body intrinsic;
        }

        @EntryPoint()
        operation Main() : Unit {
            use q = Qubit();
            foo(q);
        }
    }",
        Profile::Unrestricted,
    );

    let circ = interpreter
        .circuit(CircuitEntryPoint::EntryPoint, false)
        .expect("circuit generation should succeed");

    expect![[r"
        q_0    ─ foo ─
    "]]
    .assert_eq(&circ.to_string());
}

#[test]
fn custom_intrinsic_classical_arg() {
    let mut interpreter = interpreter(
        r"
    namespace Test {
        operation foo(n: Int): Unit {
            body intrinsic;
        }

        @EntryPoint()
        operation Main() : Unit {
            use q = Qubit();
            X(q);
            foo(4);
        }
    }",
        Profile::Unrestricted,
    );

    let circ = interpreter
        .circuit(CircuitEntryPoint::EntryPoint, false)
        .expect("circuit generation should succeed");

    // A custom intrinsic that doesn't take qubits just doesn't
    // show up on the circuit.
    expect![[r"
        q_0    ── X ──
    "]]
    .assert_eq(&circ.to_string());
}

#[test]
fn custom_intrinsic_one_classical_arg() {
    let mut interpreter = interpreter(
        r"
    namespace Test {
        operation foo(n: Int, q: Qubit): Unit {
            body intrinsic;
        }

        @EntryPoint()
        operation Main() : Unit {
            use q = Qubit();
            X(q);
            foo(4, q);
        }
    }",
        Profile::Unrestricted,
    );

    let circ = interpreter
        .circuit(CircuitEntryPoint::EntryPoint, false)
        .expect("circuit generation should succeed");

    expect![[r"
        q_0    ── X ─── foo(4) ──
    "]]
    .assert_eq(&circ.to_string());
}

#[test]
fn custom_intrinsic_mixed_args() {
    let mut interpreter = interpreter(
        r"
    namespace Test {
        import Std.ResourceEstimation.*;

        @EntryPoint()
        operation Main() : Unit {
            use qs = Qubit[10];
            AccountForEstimates(
                [
                    AuxQubitCount(1),
                    TCount(2),
                    RotationCount(3),
                    RotationDepth(4),
                    CczCount(5),
                    MeasurementCount(6),
                ],
                PSSPCLayout(),
                qs);
        }
    }",
        Profile::Unrestricted,
    );

    let circ = interpreter
        .circuit(CircuitEntryPoint::EntryPoint, false)
        .expect("circuit generation should succeed");

    expect![[r"
        q_0    ─ AccountForEstimatesInternal([(0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (5, 6)], 1) ──
                                                         ┆
        q_1    ─ AccountForEstimatesInternal([(0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (5, 6)], 1) ──
                                                         ┆
        q_2    ─ AccountForEstimatesInternal([(0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (5, 6)], 1) ──
                                                         ┆
        q_3    ─ AccountForEstimatesInternal([(0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (5, 6)], 1) ──
                                                         ┆
        q_4    ─ AccountForEstimatesInternal([(0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (5, 6)], 1) ──
                                                         ┆
        q_5    ─ AccountForEstimatesInternal([(0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (5, 6)], 1) ──
                                                         ┆
        q_6    ─ AccountForEstimatesInternal([(0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (5, 6)], 1) ──
                                                         ┆
        q_7    ─ AccountForEstimatesInternal([(0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (5, 6)], 1) ──
                                                         ┆
        q_8    ─ AccountForEstimatesInternal([(0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (5, 6)], 1) ──
                                                         ┆
        q_9    ─ AccountForEstimatesInternal([(0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (5, 6)], 1) ──
    "]]
    .assert_eq(&circ.to_string());

    assert_eq!(circ.component_grid.len(), 1);
    assert_eq!(circ.component_grid[0].components.len(), 1);
}

#[test]
fn custom_intrinsic_apply_idle_noise() {
    let mut interpreter = interpreter(
        r"
    namespace Test {
        import Std.Diagnostics.*;
        @EntryPoint()
        operation Main() : Unit {
            ConfigurePauliNoise(BitFlipNoise(1.0));
            use q = Qubit();
            ApplyIdleNoise(q);
        }
    }",
        Profile::Unrestricted,
    );

    let circ = interpreter
        .circuit(CircuitEntryPoint::EntryPoint, false)
        .expect("circuit generation should succeed");

    // ConfigurePauliNoise has no qubit arguments so it shouldn't show up.
    // ApplyIdleNoise is a quantum operation so it shows up.
    expect![[r#"
        q_0    ─ ApplyIdleNoise ──
    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn operation_with_qubits() {
    let mut interpreter = interpreter(
        r"
        namespace Test {
            @EntryPoint()
            operation Main() : Result[] { [] }

            operation Test(q1: Qubit, q2: Qubit) : Result[] {
                H(q1);
                CNOT(q1, q2);
                [M(q1), M(q2)]
            }

        }",
        Profile::Unrestricted,
    );

    let circ = interpreter
        .circuit(CircuitEntryPoint::Operation("Test.Test".into()), false)
        .expect("circuit generation should succeed");

    expect![[r"
        q_0    ── H ──── ● ──── M ──
                         │      ╘═══
        q_1    ───────── X ──── M ──
                                ╘═══
    "]]
    .assert_eq(&circ.to_string());
}

#[test]
fn operation_with_qubits_base_profile() {
    let mut interpreter = interpreter(
        r"
        namespace Test {
            @EntryPoint()
            operation Main() : Result[] { [] }

            operation Test(q1: Qubit, q2: Qubit) : Result[] {
                H(q1);
                CNOT(q1, q2);
                [M(q1), M(q2)]
            }

        }",
        Profile::Base,
    );

    let circ = interpreter
        .circuit(CircuitEntryPoint::Operation("Test.Test".into()), false)
        .expect("circuit generation should succeed");

    expect![[r#"
        q_0    ── H ──── ● ──── M ──
                         │      ╘═══
        q_1    ───────── X ──── M ──
                                ╘═══
    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn operation_with_qubit_arrays() {
    let mut interpreter = interpreter(
        r"
        namespace Test {
            @EntryPoint()
            operation Main() : Result[] { [] }

            import Std.Measurement.*;
            operation Test(q1: Qubit[], q2: Qubit[][], q3: Qubit[][][], q: Qubit) : Result[] {
                for q in q1 {
                    H(q);
                }
                for qs in q2 {
                    for q in qs {
                        X(q);
                    }
                }
                for qss in q3 {
                    for qs in qss {
                        for q in qs {
                            Y(q);
                        }
                    }
                }
                X(q);
                MeasureEachZ(q1)
            }
        }",
        Profile::Unrestricted,
    );

    let circ = interpreter
        .circuit(CircuitEntryPoint::Operation("Test.Test".into()), false)
        .expect("circuit generation should succeed");

    expect![[r"
        q_0    ── H ──── M ──
                         ╘═══
        q_1    ── H ──── M ──
                         ╘═══
        q_2    ── X ─────────
        q_3    ── X ─────────
        q_4    ── X ─────────
        q_5    ── X ─────────
        q_6    ── Y ─────────
        q_7    ── Y ─────────
        q_8    ── Y ─────────
        q_9    ── Y ─────────
        q_10   ── Y ─────────
        q_11   ── Y ─────────
        q_12   ── Y ─────────
        q_13   ── Y ─────────
        q_14   ── X ─────────
    "]]
    .assert_eq(&circ.to_string());
}

#[test]
fn adjoint_operation() {
    let mut interpreter = interpreter(
        r"
        namespace Test {
            @EntryPoint()
            operation Main() : Result[] { [] }

            operation Foo (q : Qubit) : Unit
                is Adj + Ctl {

                body (...) {
                    X(q);
                }

                adjoint (...) {
                    Y(q);
                }

                controlled (cs, ...) {
                }
            }

        }",
        Profile::Unrestricted,
    );

    let circ = interpreter
        .circuit(
            CircuitEntryPoint::Operation("Adjoint Test.Foo".into()),
            false,
        )
        .expect("circuit generation should succeed");

    expect![[r"
        q_0    ── Y ──
    "]]
    .assert_eq(&circ.to_string());
}

#[test]
fn lambda() {
    let mut interpreter = interpreter(
        r"
        namespace Test {
            @EntryPoint()
            operation Main() : Result[] { [] }
        }",
        Profile::Unrestricted,
    );

    let circ = interpreter
        .circuit(CircuitEntryPoint::Operation("q => H(q)".into()), false)
        .expect("circuit generation should succeed");

    expect![[r"
        q_0    ── H ──
    "]]
    .assert_eq(&circ.to_string());
}

#[test]
fn controlled_operation() {
    let mut interpreter = interpreter(
        r"
        namespace Test {
            @EntryPoint()
            operation Main() : Result[] { [] }

            operation SWAP (q1 : Qubit, q2 : Qubit) : Unit
                is Adj + Ctl {

                body (...) {
                    CNOT(q1, q2);
                    CNOT(q2, q1);
                    CNOT(q1, q2);
                }

                adjoint (...) {
                    SWAP(q1, q2);
                }

                controlled (cs, ...) {
                    CNOT(q1, q2);
                    Controlled CNOT(cs, (q2, q1));
                    CNOT(q1, q2);
                }
            }

        }",
        Profile::Unrestricted,
    );

    let circ_err = interpreter
        .circuit(
            CircuitEntryPoint::Operation("Controlled Test.SWAP".into()),
            false,
        )
        .expect_err("circuit generation should fail");

    // Controlled operations are not supported at the moment.
    // We don't generate an accurate call signature with the tuple arguments.
    expect![[r"
        [
            Circuit(
                ControlledUnsupported,
            ),
        ]
    "]]
    .assert_debug_eq(&circ_err);
}

#[test]
fn internal_operation() {
    let mut interpreter = interpreter(
        r"
        namespace Test {
            @EntryPoint()
            operation Main() : Result[] { [] }

            internal operation Test(q1: Qubit, q2: Qubit) : Result[] {
                H(q1);
                CNOT(q1, q2);
                [M(q1), M(q2)]
            }
        }",
        Profile::Unrestricted,
    );

    let circ = interpreter
        .circuit(CircuitEntryPoint::Operation("Test.Test".into()), false)
        .expect("circuit generation should not fail");

    expect![[r#"
        q_0    ── H ──── ● ──── M ──
                         │      ╘═══
        q_1    ───────── X ──── M ──
                                ╘═══
    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn operation_with_non_qubit_args() {
    let mut interpreter = interpreter(
        r"
        namespace Test {
            @EntryPoint()
            operation Main() : Result[] { [] }

            operation Test(q1: Qubit, q2: Qubit, i: Int) : Unit {
            }

        }",
        Profile::Unrestricted,
    );

    let circ_err = interpreter
        .circuit(CircuitEntryPoint::Operation("Test.Test".into()), false)
        .expect_err("circuit generation should fail");

    expect![[r"
        [
            Circuit(
                NoQubitParameters,
            ),
        ]
    "]]
    .assert_debug_eq(&circ_err);
}

#[test]
fn operation_with_long_gates_properly_aligned() {
    let mut interpreter = interpreter(
        r"
            namespace Test {
                import Std.Measurement.*;

                @EntryPoint()
                operation Main() : Result[] {
                    use q0 = Qubit();
                    use q1 = Qubit();

                    H(q0);
                    H(q1);
                    X(q1);
                    Ry(1.0, q1);
                    CNOT(q0, q1);
                    M(q0);

                    use q2 = Qubit();

                    H(q2);
                    Rx(1.0, q2);
                    H(q2);
                    Rx(1.0, q2);
                    H(q2);
                    Rx(1.0, q2);

                    use q3 = Qubit();

                    Rxx(1.0, q1, q3);

                    CNOT(q0, q3);

                    [M(q1), M(q3)]
                }
            }
        ",
        Profile::Unrestricted,
    );

    let circ = interpreter
        .circuit(CircuitEntryPoint::EntryPoint, false)
        .expect("circuit generation should succeed");

    expect![[r#"
        q_0    ── H ────────────────────────────────────── ● ──────── M ────────────────────────────────── ● ─────────
                                                           │          ╘════════════════════════════════════╪══════════
        q_1    ── H ──────── X ─────── Ry(1.0000) ──────── X ───────────────────────────── Rxx(1.0000) ────┼───── M ──
                                                                                                ┆          │      ╘═══
        q_2    ── H ─── Rx(1.0000) ──────── H ─────── Rx(1.0000) ──── H ─── Rx(1.0000) ─────────┆──────────┼──────────
        q_3    ─────────────────────────────────────────────────────────────────────────── Rxx(1.0000) ─── X ──── M ──
                                                                                                                  ╘═══
    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn operation_with_subsequent_qubits_gets_horizontal_lines() {
    let mut interpreter = interpreter(
        r"
            namespace Test {
                import Std.Measurement.*;

                @EntryPoint()
                operation Main() : Unit {
                    use q0 = Qubit();
                    use q1 = Qubit();
                    Rxx(1.0, q0, q1);

                    use q2 = Qubit();
                    use q3 = Qubit();
                    Rxx(1.0, q2, q3);
                }
            }
        ",
        Profile::Unrestricted,
    );

    let circ = interpreter
        .circuit(CircuitEntryPoint::EntryPoint, false)
        .expect("circuit generation should succeed");

    expect![[r#"
        q_0    ─ Rxx(1.0000) ─
                      ┆
        q_1    ─ Rxx(1.0000) ─
        q_2    ─ Rxx(1.0000) ─
                      ┆
        q_3    ─ Rxx(1.0000) ─
    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn operation_with_subsequent_qubits_no_double_rows() {
    let mut interpreter = interpreter(
        r"
            namespace Test {
                import Std.Measurement.*;

                @EntryPoint()
                operation Main() : Unit {
                    use q0 = Qubit();
                    use q1 = Qubit();
                    Rxx(1.0, q0, q1);
                    Rxx(1.0, q0, q1);
                }
            }
        ",
        Profile::Unrestricted,
    );

    let circ = interpreter
        .circuit(CircuitEntryPoint::EntryPoint, false)
        .expect("circuit generation should succeed");

    expect![[r#"
        q_0    ─ Rxx(1.0000) ── Rxx(1.0000) ─
                      ┆              ┆
        q_1    ─ Rxx(1.0000) ── Rxx(1.0000) ─
    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn operation_with_subsequent_qubits_no_added_rows() {
    let mut interpreter = interpreter(
        r"
            namespace Test {
                import Std.Measurement.*;

                @EntryPoint()
                operation Main() : Result[] {
                    use q0 = Qubit();
                    use q1 = Qubit();
                    Rxx(1.0, q0, q1);

                    use q2 = Qubit();
                    use q3 = Qubit();
                    Rxx(1.0, q2, q3);

                    [M(q0), M(q2)]
                }
            }
        ",
        Profile::Unrestricted,
    );

    let circ = interpreter
        .circuit(CircuitEntryPoint::EntryPoint, false)
        .expect("circuit generation should succeed");

    expect![[r#"
        q_0    ─ Rxx(1.0000) ─── M ──
                      ┆          ╘═══
        q_1    ─ Rxx(1.0000) ────────
        q_2    ─ Rxx(1.0000) ─── M ──
                      ┆          ╘═══
        q_3    ─ Rxx(1.0000) ────────
    "#]]
    .assert_eq(&circ.to_string());
}

/// Tests that invoke circuit generation through the debugger.
mod debugger_stepping {
    use super::Debugger;
    use crate::target::Profile;
    use expect_test::expect;
    use qsc_data_structures::language_features::LanguageFeatures;
    use qsc_data_structures::line_column::Encoding;
    use qsc_eval::{StepAction, StepResult, output::GenericReceiver};
    use qsc_frontend::compile::SourceMap;
    use std::fmt::Write;

    /// Steps through the code in the debugger and collects the
    /// circuit representation at each step.
    fn generate_circuit_steps(code: &str, profile: Profile) -> String {
        let sources = SourceMap::new([("test.qs".into(), code.into())], None);
        let (std_id, store) = crate::compile::package_store_with_stdlib(profile.into());
        let mut debugger = Debugger::new(
            sources,
            profile.into(),
            Encoding::Utf8,
            LanguageFeatures::default(),
            store,
            &[(std_id, None)],
        )
        .expect("debugger creation should succeed");

        debugger.interpreter.set_quantum_seed(Some(2));

        let mut out = std::io::sink();
        let mut r = GenericReceiver::new(&mut out);

        let mut circs = String::new();
        let mut result = debugger
            .eval_step(&mut r, &[], StepAction::In)
            .expect("step should succeed");

        write!(&mut circs, "step:\n{}", debugger.circuit()).expect("write should succeed");
        while !matches!(result, StepResult::Return(_)) {
            result = debugger
                .eval_step(&mut r, &[], StepAction::Next)
                .expect("step should succeed");

            write!(&mut circs, "step:\n{}", debugger.circuit()).expect("write should succeed");
        }
        circs
    }

    #[test]
    fn base_profile() {
        let circs = generate_circuit_steps(
            r"
                namespace Test {
                    import Std.Measurement.*;
                    @EntryPoint()
                    operation Main() : Result[] {
                        use q = Qubit();
                        H(q);
                        let r = M(q);
                        Reset(q);
                        [r]
                    }
                }
            ",
            Profile::Base,
        );

        expect![[r#"
            step:
            step:
            q_0
            step:
            q_0    ── H ──
            step:
            q_0    ── H ──── M ──
                             ╘═══
            step:
            q_0    ── H ──── M ──── |0〉 ──
                             ╘════════════
            step:
            q_0    ── H ──── M ──── |0〉 ──
                             ╘════════════
            step:
            q_0    ── H ──── M ──── |0〉 ──
                             ╘════════════
        "#]]
        .assert_eq(&circs);
    }

    #[test]
    fn unrestricted_profile() {
        let circs = generate_circuit_steps(
            r"
                namespace Test {
                    import Std.Measurement.*;
                    @EntryPoint()
                    operation Main() : Result[] {
                        use q = Qubit();
                        H(q);
                        let r = M(q);
                        Reset(q);
                        [r]
                    }
                }
            ",
            Profile::Unrestricted,
        );

        expect![[r#"
            step:
            step:
            q_0
            step:
            q_0    ── H ──
            step:
            q_0    ── H ──── M ──
                             ╘═══
            step:
            q_0    ── H ──── M ──── |0〉 ──
                             ╘════════════
            step:
            q_0    ── H ──── M ──── |0〉 ──
                             ╘════════════
            step:
            q_0    ── H ──── M ──── |0〉 ──
                             ╘════════════
        "#]]
        .assert_eq(&circs);
    }

    #[test]
    fn unrestricted_profile_result_comparison() {
        let circs = generate_circuit_steps(
            r"
                namespace Test {
                    import Std.Measurement.*;
                    @EntryPoint()
                    operation Main() : Result[] {
                        use q = Qubit();
                        H(q);
                        let r = M(q);
                        if (r == One) {
                            X(q);
                        }
                        [r]
                    }
                }
            ",
            Profile::Unrestricted,
        );

        // We set the random seed in the test to account for
        // the nondeterministic output. Since the debugger is running
        // the real simulator, the circuit is going to vary from run to run
        // depending on measurement outcomes.
        expect![[r#"
            step:
            step:
            q_0
            step:
            q_0    ── H ──
            step:
            q_0    ── H ──── M ──
                             ╘═══
            step:
            q_0    ── H ──── M ──
                             ╘═══
            step:
            q_0    ── H ──── M ──── X ──
                             ╘══════════
            step:
            q_0    ── H ──── M ──── X ──
                             ╘══════════
            step:
            q_0    ── H ──── M ──── X ──
                             ╘══════════
            step:
            q_0    ── H ──── M ──── X ──
                             ╘══════════
        "#]]
        .assert_eq(&circs);
    }
}
