// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::unicode_not_nfc)]

use super::{CircuitEntryPoint, Debugger, Interpreter};
use crate::{interpret::Error, target::Profile};
use expect_test::expect;
use miette::Diagnostic;
use qsc_data_structures::language_features::LanguageFeatures;
use qsc_eval::output::GenericReceiver;
use qsc_frontend::compile::SourceMap;
use qsc_passes::PackageType;

fn interpreter(code: &str, profile: Profile, trace_circuit: bool) -> Interpreter {
    let sources = SourceMap::new([("test.qs".into(), code.into())], None);
    let (std_id, store) = crate::compile::package_store_with_stdlib(profile.into());
    if trace_circuit {
        Interpreter::new_with_circuit_trace(
            sources,
            PackageType::Exe,
            profile.into(),
            LanguageFeatures::default(),
            store,
            &[(std_id, None)],
        )
    } else {
        Interpreter::new(
            sources,
            PackageType::Exe,
            profile.into(),
            LanguageFeatures::default(),
            store,
            &[(std_id, None)],
        )
    }
    .expect("interpreter creation should succeed")
}

fn circuit_err(
    code: &str,
    entry: CircuitEntryPoint,
    profile: Profile,
    simulate: bool,
) -> Vec<Error> {
    let mut interpreter = interpreter(code, profile, false);
    interpreter.set_quantum_seed(Some(2));
    interpreter
        .circuit(entry, simulate)
        .expect_err("circuit should return error")
}

fn circuit(code: &str, entry: CircuitEntryPoint, profile: Profile, simulate: bool) -> String {
    let mut interpreter = interpreter(code, profile, false);
    interpreter.set_quantum_seed(Some(2));
    interpreter
        .circuit(entry, simulate)
        .expect("circuit generation should succeed")
        .to_string()
}

#[test]
fn empty() {
    let circ = circuit(
        r#"
            namespace Test {
                @EntryPoint()
                operation Main() : Unit {
                    Message("hi");
                }
            }
        "#,
        CircuitEntryPoint::EntryPoint,
        Profile::Unrestricted,
        false,
    );

    expect![""].assert_eq(&circ);
}

#[test]
fn one_gate() {
    let circ = circuit(
        r"
            namespace Test {
                @EntryPoint()
                operation Main() : Unit {
                    use q = Qubit();
                    H(q);
                }
            }
        ",
        CircuitEntryPoint::EntryPoint,
        Profile::Unrestricted,
        false,
    );

    expect![[r#"
        q_0    ── H ──
    "#]]
    .assert_eq(&circ);
}

#[test]
fn measure_same_qubit_twice() {
    let circ = circuit(
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
        CircuitEntryPoint::EntryPoint,
        Profile::Unrestricted,
        false,
    );

    expect![[r#"
        q_0    ── H ──── M ──── M ──
                         ╘══════╪═══
                                ╘═══
    "#]]
    .assert_eq(&circ);
}

#[test]
fn toffoli() {
    let circ = circuit(
        r"
            namespace Test {
                @EntryPoint()
                operation Main() : Unit {
                    use q = Qubit[3];
                    CCNOT(q[0], q[1], q[2]);
                }
            }
        ",
        CircuitEntryPoint::EntryPoint,
        Profile::Unrestricted,
        false,
    );

    expect![[r#"
        q_0    ── ● ──
        q_1    ── ● ──
        q_2    ── X ──
    "#]]
    .assert_eq(&circ);
}

#[test]
fn rotation_gate() {
    let circ = circuit(
        r"
            namespace Test {
                @EntryPoint()
                operation Main() : Unit {
                    use q = Qubit();
                    Rx(Microsoft.Quantum.Math.PI()/2.0, q);
                }
            }
        ",
        CircuitEntryPoint::EntryPoint,
        Profile::Unrestricted,
        false,
    );

    expect![[r#"
        q_0    ─ Rx(1.5708) ──
    "#]]
    .assert_eq(&circ);
}

#[test]
fn classical_for_loop() {
    let circ = circuit(
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
        CircuitEntryPoint::EntryPoint,
        Profile::Unrestricted,
        false,
    );

    expect![[r#"
        q_0    ── X ──── X ──── X ──── X ──── X ──── X ──
    "#]]
    .assert_eq(&circ);
}

#[test]
fn m_base_profile() {
    let circ = circuit(
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
        CircuitEntryPoint::EntryPoint,
        Profile::Base,
        false,
    );

    expect![[r#"
        q_0    ── H ──── M ──
                         ╘═══
    "#]]
    .assert_eq(&circ);
}

#[test]
fn m_unrestricted_profile() {
    let circ = circuit(
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
        CircuitEntryPoint::EntryPoint,
        Profile::Unrestricted,
        false,
    );

    expect![[r#"
        q_0    ── H ──── M ──
                         ╘═══
    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn mresetz_unrestricted_profile() {
    let circ = circuit(
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
        CircuitEntryPoint::EntryPoint,
        Profile::Unrestricted,
        false,
    );

    expect![[r#"
        q_0    ── H ──── M ──── |0〉 ──
                         ╘════════════
    "#]]
    .assert_eq(&circ);
}

#[test]
fn mresetz_base_profile() {
    let circ = circuit(
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
        CircuitEntryPoint::EntryPoint,
        Profile::Base,
        false,
    );

    expect![[r#"
        q_0    ── H ──── M ──── |0〉 ──
                         ╘════════════
    "#]]
    .assert_eq(&circ);
}

#[test]
fn unrestricted_profile_result_comparison() {
    let mut interpreter = interpreter(
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
        true, // trace_circuit
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
    expect![[r#"
        q_0    ── H ──── M ───── X ───── |0〉 ──
                         ╘═════════════════════
        q_1    ── H ──── M ──── |0〉 ───────────
                         ╘═════════════════════
    "#]]
    .assert_eq(&circuit.to_string());
}

#[test]
fn custom_intrinsic() {
    let circ = circuit(
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
        CircuitEntryPoint::EntryPoint,
        Profile::Unrestricted,
        false,
    );

    expect![[r#"
        q_0    ─ foo ─
    "#]]
    .assert_eq(&circ);
}

#[test]
fn custom_intrinsic_classical_arg() {
    let circ = circuit(
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
        CircuitEntryPoint::EntryPoint,
        Profile::Unrestricted,
        false,
    );

    // A custom intrinsic that doesn't take qubits just doesn't
    // show up on the circuit.
    expect![[r#"
        q_0    ── X ──
    "#]]
    .assert_eq(&circ);
}

#[test]
fn custom_intrinsic_one_classical_arg() {
    let circ = circuit(
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
        CircuitEntryPoint::EntryPoint,
        Profile::Unrestricted,
        false,
    );

    expect![[r#"
        q_0    ── X ─── foo(4) ──
    "#]]
    .assert_eq(&circ);
}

#[test]
fn custom_intrinsic_mixed_args_classical_eval() {
    let circ = circuit(
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
        CircuitEntryPoint::EntryPoint,
        Profile::Unrestricted,
        false,
    );

    expect![[r#"
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
    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn custom_intrinsic_apply_idle_noise() {
    let circ = circuit(
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
        CircuitEntryPoint::EntryPoint,
        Profile::Unrestricted,
        false,
    );

    // ConfigurePauliNoise has no qubit arguments so it shouldn't show up.
    // ApplyIdleNoise is a quantum operation so it shows up.
    expect![[r#"
        q_0    ─ ApplyIdleNoise ──
    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn operation_with_qubits() {
    let circ = circuit(
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
        CircuitEntryPoint::Operation("Test.Test".into()),
        Profile::Unrestricted,
        false,
    );

    expect![[r#"
        q_0    ── H ──── ● ──── M ──
                         │      ╘═══
        q_1    ───────── X ──── M ──
                                ╘═══
    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn operation_with_qubits_base_profile() {
    let circ = circuit(
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
        CircuitEntryPoint::Operation("Test.Test".into()),
        Profile::Base,
        false,
    );
    expect![[r#"
        q_0    ── H ──── ● ──── M ──
                         │      ╘═══
        q_1    ───────── X ──── M ──
                                ╘═══
    "#]]
    .assert_eq(&circ);
}

#[test]
fn operation_with_qubit_arrays() {
    let circ = circuit(
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
        CircuitEntryPoint::Operation("Test.Test".into()),
        Profile::Unrestricted,
        false,
    );

    expect![[r#"
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
    "#]]
    .assert_eq(&circ);
}

#[test]
fn adjoint_operation() {
    let circ = circuit(
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
        CircuitEntryPoint::Operation("Adjoint Test.Foo".into()),
        Profile::Unrestricted,
        false,
    );

    expect![[r#"
        q_0    ── Y ──
    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn lambda() {
    let circ = circuit(
        r"
        namespace Test {
            @EntryPoint()
            operation Main() : Result[] { [] }
        }",
        CircuitEntryPoint::Operation("q => H(q)".into()),
        Profile::Unrestricted,
        false,
    );

    expect![[r#"
        q_0    ── H ──
    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn controlled_operation() {
    let circ_err = circuit_err(
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
        CircuitEntryPoint::Operation("Controlled Test.SWAP".into()),
        Profile::Unrestricted,
        false,
    );

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
    let circ = circuit(
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
        CircuitEntryPoint::Operation("Test.Test".into()),
        Profile::Unrestricted,
        false,
    );

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
    let circ_err = circuit_err(
        r"
        namespace Test {
            @EntryPoint()
            operation Main() : Result[] { [] }

            operation Test(q1: Qubit, q2: Qubit, i: Int) : Unit {
            }

        }",
        CircuitEntryPoint::Operation("Test.Test".into()),
        Profile::Unrestricted,
        false,
    );

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
    let circ = circuit(
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
        CircuitEntryPoint::EntryPoint,
        Profile::Unrestricted,
        false,
    );

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
    let circ = circuit(
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
        CircuitEntryPoint::EntryPoint,
        Profile::Unrestricted,
        false,
    );

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
    let circ = circuit(
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
        CircuitEntryPoint::EntryPoint,
        Profile::Unrestricted,
        false,
    );

    expect![[r#"
        q_0    ─ Rxx(1.0000) ── Rxx(1.0000) ─
                      ┆              ┆
        q_1    ─ Rxx(1.0000) ── Rxx(1.0000) ─
    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn operation_with_subsequent_qubits_no_added_rows() {
    let circ = circuit(
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
        CircuitEntryPoint::EntryPoint,
        Profile::Unrestricted,
        false,
    );

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
        let mut debugger = Debugger::new_with_circuit_trace(
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
