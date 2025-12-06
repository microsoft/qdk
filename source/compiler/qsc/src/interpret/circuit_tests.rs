// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::unicode_not_nfc)]

use super::{CircuitEntryPoint, Debugger, Interpreter};
use crate::{
    interpret::{CircuitGenerationMethod, Error},
    target::Profile,
};
use expect_test::expect;
use miette::Diagnostic;
use qsc_circuit::{Circuit, TracerConfig};
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
        Default::default(),
    )
    .expect("interpreter creation should succeed")
}

#[allow(clippy::needless_pass_by_value)]
fn circuit(code: &str, entry: CircuitEntryPoint) -> String {
    circuit_with_options(
        code,
        Profile::Unrestricted,
        entry,
        CircuitGenerationMethod::ClassicalEval,
        Default::default(),
    )
    .expect("circuit generation should succeed")
    .to_string()
}

fn circuit_err(
    code: &str,
    entry: CircuitEntryPoint,
    method: CircuitGenerationMethod,
    tracer_config: TracerConfig,
) -> Vec<Error> {
    circuit_with_options(code, Profile::Unrestricted, entry, method, tracer_config)
        .expect_err("circuit generation should fail")
}

fn circuit_with_options(
    code: &str,
    profile: Profile,
    entry: CircuitEntryPoint,
    method: CircuitGenerationMethod,
    config: TracerConfig,
) -> Result<Circuit, Vec<Error>> {
    let mut interpreter = interpreter(code, profile);
    interpreter.set_quantum_seed(Some(2));
    interpreter.circuit(entry, method, config)
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
    );

    expect![[r#"
        q_0@test.qs:4:20 ─ H@test.qs:5:20 ──
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
    );

    expect![[r#"
        q_0@test.qs:4:20 ─ H@test.qs:5:20 ─── M@test.qs:6:29 ─── M@test.qs:7:29 ──
                                                     ╘══════════════════╪═════════
                                                                        ╘═════════
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
    );

    expect![[r#"
        q_0@test.qs:4:20 ──────── ● ────────
        q_1@test.qs:4:20 ──────── ● ────────
        q_2@test.qs:4:20 ─ X@test.qs:5:20 ──
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
    );

    expect![[r#"
        q_0@test.qs:4:20 ─ Rx(1.5708)@test.qs:5:20 ─
    "#]]
    .assert_eq(&circ);
}

#[test]
fn grouping_nested_callables() {
    let circ = circuit_with_options(
        r"
            namespace Test {
                @EntryPoint()
                operation Main() : Unit {
                    use q = Qubit();
                    for i in 0..5 {
                        Foo(q);
                    }
                    MResetZ(q);
                }

                operation Foo(q: Qubit) : Unit {
                    H(q);
                }
            }
        ",
        Profile::Unrestricted,
        CircuitEntryPoint::EntryPoint,
        CircuitGenerationMethod::ClassicalEval,
        TracerConfig {
            max_operations: usize::MAX,
            source_locations: false,
            group_by_scope: true,
            prune_classical_qubits: false,
        },
    )
    .expect("circuit generation should succeed");

    expect![[r#"
        q_0    ─ [ [Main] ─── [ [Foo] ─── H ──── H ──── H ──── H ──── H ──── H ──── ] ──── M ──── |0〉 ──── ] ──
                     [                                                                     ╘══════════════ ] ══
    "#]]
    .assert_eq(&circ.display_with_groups().to_string());
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
    );

    expect![[r#"
        q_0@test.qs:4:20 ─ X@test.qs:6:24 ─── X@test.qs:6:24 ─── X@test.qs:6:24 ─── X@test.qs:6:24 ─── X@test.qs:6:24 ─── X@test.qs:6:24 ──
    "#]]
    .assert_eq(&circ);
}

#[test]
fn m_base_profile() {
    let circ = circuit_with_options(
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
        CircuitEntryPoint::EntryPoint,
        CircuitGenerationMethod::ClassicalEval,
        Default::default(),
    )
    .expect("circuit generation should succeed");

    expect![[r#"
        q_0@test.qs:5:20 ─ H@test.qs:6:20 ─── M@test.qs:7:21 ──
                                                     ╘═════════
    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn m_default_profile() {
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
    );

    expect![[r#"
        q_0@test.qs:5:20 ─ H@test.qs:6:20 ─── M@test.qs:7:21 ──
                                                     ╘═════════
    "#]]
    .assert_eq(&circ);
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
    );

    expect![[r#"
        q_0@test.qs:5:20 ─ H@test.qs:6:20 ─── M@test.qs:7:21 ──── |0〉@test.qs:7:21 ───
                                                     ╘════════════════════════════════
    "#]]
    .assert_eq(&circ);
}

#[test]
fn mresetz_base_profile() {
    let circ = circuit_with_options(
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
        CircuitEntryPoint::EntryPoint,
        CircuitGenerationMethod::ClassicalEval,
        Default::default(),
    )
    .expect("circuit generation should succeed");

    // code gen in Base turns the MResetZ into an M
    expect![[r#"
        q_0@test.qs:5:20 ─ H@test.qs:6:20 ─── M@test.qs:7:21 ──── |0〉@test.qs:7:21 ───
                                                     ╘════════════════════════════════
    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn qubit_relabel() {
    let circ = circuit(
        "
        namespace Test {
            operation Main() : Unit {
                use (q1, q2) = (Qubit(), Qubit());
                H(q1);
                CNOT(q1, q2);
                Relabel([q1, q2], [q2, q1]);
                H(q1);
                CNOT(q1, q2);
                MResetZ(q1);
                MResetZ(q2);
            }
        }
    ",
        CircuitEntryPoint::EntryPoint,
    );

    expect![[r#"
        q_0@test.qs:3:32 ─ H@test.qs:4:16 ────────── ● ──────────────────────────── X@test.qs:8:16 ─── M@test.qs:10:16 ─── |0〉@test.qs:10:16 ──
                                                     │                                     │                  ╘════════════════════════════════
        q_1@test.qs:3:41 ──────────────────── X@test.qs:5:16 ─── H@test.qs:7:16 ────────── ● ───────── M@test.qs:9:16 ──── |0〉@test.qs:9:16 ───
                                                                                                              ╘════════════════════════════════
    "#]]
    .assert_eq(&circ);
}

#[test]
fn qubit_reuse() {
    let circ = circuit(
        "
        namespace Test {
            operation Main() : Unit {
                {
                    use q1 = Qubit();
                    X(q1);
                    MResetZ(q1);
                }
                {
                    use q2 = Qubit();
                    Y(q2);
                    MResetZ(q2);
                }
            }
        }
    ",
        CircuitEntryPoint::EntryPoint,
    );

    expect![[r#"
        q_0@test.qs:4:20, test.qs:9:20 ─ X@test.qs:5:20 ─── M@test.qs:6:20 ──── |0〉@test.qs:6:20 ──── Y@test.qs:10:20 ── M@test.qs:11:20 ─── |0〉@test.qs:11:20 ──
                                                                   ╘════════════════════════════════════════════════════════════╪════════════════════════════════
                                                                                                                                ╘════════════════════════════════
    "#]]
    .assert_eq(&circ);
}

#[test]
fn qubit_reuse_no_measurements() {
    let circ = circuit(
        "
        namespace Test {
            operation Main() : Unit {
                {
                    use q1 = Qubit();
                    X(q1);
                    Reset(q1);
                }
                {
                    use q2 = Qubit();
                    Y(q2);
                    Reset(q2);
                }
            }
        }
    ",
        CircuitEntryPoint::EntryPoint,
    );

    expect![[r#"
        q_0@test.qs:4:20, test.qs:9:20 ─ X@test.qs:5:20 ──── |0〉@test.qs:6:20 ──── Y@test.qs:10:20 ─── |0〉@test.qs:11:20 ──
    "#]]
    .assert_eq(&circ);
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
        .circuit(
            CircuitEntryPoint::EntryPoint,
            CircuitGenerationMethod::ClassicalEval,
            Default::default(),
        )
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
        .circuit(
            CircuitEntryPoint::EntryPoint,
            CircuitGenerationMethod::Simulate,
            Default::default(),
        )
        .expect("circuit generation should succeed");

    expect![[r#"
        q_0@test.qs:5:20 ─ H@test.qs:7:20 ─── M@test.qs:9:29 ───── X@test.qs:12:24 ───── |0〉@test.qs:14:20 ──
                                                     ╘═══════════════════════════════════════════════════════
        q_1@test.qs:6:20 ─ H@test.qs:8:20 ─── M@test.qs:10:29 ─── |0〉@test.qs:14:20 ─────────────────────────
                                                     ╘═══════════════════════════════════════════════════════
    "#]]
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
        q_0@test.qs:5:20 ─ H@test.qs:7:20 ─── M@test.qs:9:29 ───── X@test.qs:12:24 ───── |0〉@test.qs:14:20 ──
                                                     ╘═══════════════════════════════════════════════════════
        q_1@test.qs:6:20 ─ H@test.qs:8:20 ─── M@test.qs:10:29 ─── |0〉@test.qs:14:20 ─────────────────────────
                                                     ╘═══════════════════════════════════════════════════════
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
    );

    expect![[r#"
        q_0@test.qs:8:12 ─ foo@test.qs:9:12 ──
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
    );

    // A custom intrinsic that doesn't take qubits just doesn't
    // show up on the circuit.
    expect![[r#"
        q_0@test.qs:8:12 ─ X@test.qs:9:12 ──
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
    );

    expect![[r#"
        q_0@test.qs:8:12 ─ X@test.qs:9:12 ─── foo(4)@test.qs:10:12 ──
    "#]]
    .assert_eq(&circ);
}

#[test]
fn custom_intrinsic_mixed_args() {
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
    );

    expect![[r#"
        q_0@test.qs:6:12 ─ AccountForEstimatesInternal([(0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (5, 6)], 1)@test.qs:7:12 ─
                                                                         ┆
        q_1@test.qs:6:12 ─ AccountForEstimatesInternal([(0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (5, 6)], 1)@test.qs:7:12 ─
                                                                         ┆
        q_2@test.qs:6:12 ─ AccountForEstimatesInternal([(0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (5, 6)], 1)@test.qs:7:12 ─
                                                                         ┆
        q_3@test.qs:6:12 ─ AccountForEstimatesInternal([(0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (5, 6)], 1)@test.qs:7:12 ─
                                                                         ┆
        q_4@test.qs:6:12 ─ AccountForEstimatesInternal([(0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (5, 6)], 1)@test.qs:7:12 ─
                                                                         ┆
        q_5@test.qs:6:12 ─ AccountForEstimatesInternal([(0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (5, 6)], 1)@test.qs:7:12 ─
                                                                         ┆
        q_6@test.qs:6:12 ─ AccountForEstimatesInternal([(0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (5, 6)], 1)@test.qs:7:12 ─
                                                                         ┆
        q_7@test.qs:6:12 ─ AccountForEstimatesInternal([(0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (5, 6)], 1)@test.qs:7:12 ─
                                                                         ┆
        q_8@test.qs:6:12 ─ AccountForEstimatesInternal([(0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (5, 6)], 1)@test.qs:7:12 ─
                                                                         ┆
        q_9@test.qs:6:12 ─ AccountForEstimatesInternal([(0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (5, 6)], 1)@test.qs:7:12 ─
    "#]]
    .assert_eq(&circ);
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
    );

    expect![[r#"
        q_0@test.qs:6:12 ─ ApplyIdleNoise@test.qs:7:12 ─
    "#]]
    .assert_eq(&circ);
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
    );

    expect![[r#"
        q_0@test.qs:5:27 ─ H@test.qs:6:16 ────────── ● ───────── M@test.qs:8:17 ──
                                                     │                  ╘═════════
        q_1@test.qs:5:38 ──────────────────── X@test.qs:7:16 ─── M@test.qs:8:24 ──
                                                                        ╘═════════
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
    );

    expect![[r#"
        q_0@test.qs:6:27   ─ H@test.qs:8:20 ─── M@test.qs:23:16 ─
                                                       ╘═════════
        q_1@test.qs:6:27   ─ H@test.qs:8:20 ─── M@test.qs:23:16 ─
                                                       ╘═════════
        q_2@test.qs:6:40   ─ X@test.qs:12:24 ────────────────────
        q_3@test.qs:6:40   ─ X@test.qs:12:24 ────────────────────
        q_4@test.qs:6:40   ─ X@test.qs:12:24 ────────────────────
        q_5@test.qs:6:40   ─ X@test.qs:12:24 ────────────────────
        q_6@test.qs:6:55   ─ Y@test.qs:18:28 ────────────────────
        q_7@test.qs:6:55   ─ Y@test.qs:18:28 ────────────────────
        q_8@test.qs:6:55   ─ Y@test.qs:18:28 ────────────────────
        q_9@test.qs:6:55   ─ Y@test.qs:18:28 ────────────────────
        q_10@test.qs:6:55  ─ Y@test.qs:18:28 ────────────────────
        q_11@test.qs:6:55  ─ Y@test.qs:18:28 ────────────────────
        q_12@test.qs:6:55  ─ Y@test.qs:18:28 ────────────────────
        q_13@test.qs:6:55  ─ Y@test.qs:18:28 ────────────────────
        q_14@test.qs:6:72  ─ X@test.qs:22:16 ────────────────────
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
    );

    expect![[r#"
        q_0@test.qs:5:27 ─ Y@test.qs:13:20 ─
    "#]]
    .assert_eq(&circ);
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
    );

    expect![[r#"
        q_0@line_0:0:0 ─ H@<entry>:2:18 ──
    "#]]
    .assert_eq(&circ);
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
        CircuitGenerationMethod::ClassicalEval,
        Default::default(),
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
    );

    expect![[r#"
        q_0@test.qs:5:36 ─ H@test.qs:6:16 ────────── ● ───────── M@test.qs:8:17 ──
                                                     │                  ╘═════════
        q_1@test.qs:5:47 ──────────────────── X@test.qs:7:16 ─── M@test.qs:8:24 ──
                                                                        ╘═════════
    "#]]
    .assert_eq(&circ);
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
        CircuitGenerationMethod::ClassicalEval,
        Default::default(),
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
    );

    expect![[r#"
        q_0@test.qs:6:20   ─ H@test.qs:9:20 ───────────────────────────────────────────────────────────────────────── ● ────────────── M@test.qs:14:20 ─────────────────────────────────────────────────────────────────── ● ───────────────────────────
                                                                                                                      │                       ╘════════════════════════════════════════════════════════════════════════════╪════════════════════════════
        q_1@test.qs:7:20   ─ H@test.qs:10:20 ─────── X@test.qs:11:20 ─────── Ry(1.0000)@test.qs:12:20 ──────── X@test.qs:13:20 ─────────────────────────────────────────────────────── Rxx(1.0000)@test.qs:27:20 ──────────┼────────── M@test.qs:31:21 ─
                                                                                                                                                                                                   ┆                       │                  ╘═════════
        q_2@test.qs:16:20  ─ H@test.qs:18:20 ── Rx(1.0000)@test.qs:19:20 ──────── H@test.qs:20:20 ─────── Rx(1.0000)@test.qs:21:20 ─── H@test.qs:22:20 ── Rx(1.0000)@test.qs:23:20 ────────────────┆───────────────────────┼────────────────────────────
        q_3@test.qs:25:20  ─────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── Rxx(1.0000)@test.qs:27:20 ── X@test.qs:29:20 ── M@test.qs:31:28 ─
                                                                                                                                                                                                                                              ╘═════════
    "#]]
    .assert_eq(&circ);
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
    );

    expect![[r#"
        q_0@test.qs:6:20   ─ Rxx(1.0000)@test.qs:8:20 ──
                                         ┆
        q_1@test.qs:7:20   ─ Rxx(1.0000)@test.qs:8:20 ──
        q_2@test.qs:10:20  ─ Rxx(1.0000)@test.qs:12:20 ─
                                         ┆
        q_3@test.qs:11:20  ─ Rxx(1.0000)@test.qs:12:20 ─
    "#]]
    .assert_eq(&circ);
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
    );

    expect![[r#"
        q_0@test.qs:6:20 ─ Rxx(1.0000)@test.qs:8:20 ─── Rxx(1.0000)@test.qs:9:20 ──
                                       ┆                            ┆
        q_1@test.qs:7:20 ─ Rxx(1.0000)@test.qs:8:20 ─── Rxx(1.0000)@test.qs:9:20 ──
    "#]]
    .assert_eq(&circ);
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
    );

    expect![[r#"
        q_0@test.qs:6:20   ─ Rxx(1.0000)@test.qs:8:20 ─── M@test.qs:14:21 ─
                                         ┆                       ╘═════════
        q_1@test.qs:7:20   ─ Rxx(1.0000)@test.qs:8:20 ─────────────────────
        q_2@test.qs:10:20  ─ Rxx(1.0000)@test.qs:12:20 ── M@test.qs:14:28 ─
                                         ┆                       ╘═════════
        q_3@test.qs:11:20  ─ Rxx(1.0000)@test.qs:12:20 ────────────────────
    "#]]
    .assert_eq(&circ);
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
            q_0@test.qs:5:24
            step:
            q_0@test.qs:5:24 ─ H@test.qs:6:24 ──
            step:
            q_0@test.qs:5:24 ─ H@test.qs:6:24 ─── M@test.qs:7:32 ──
                                                         ╘═════════
            step:
            q_0@test.qs:5:24 ─ H@test.qs:6:24 ─── M@test.qs:7:32 ──── |0〉@test.qs:8:24 ───
                                                         ╘════════════════════════════════
            step:
            q_0@test.qs:5:24 ─ H@test.qs:6:24 ─── M@test.qs:7:32 ──── |0〉@test.qs:8:24 ───
                                                         ╘════════════════════════════════
            step:
            q_0@test.qs:5:24 ─ H@test.qs:6:24 ─── M@test.qs:7:32 ──── |0〉@test.qs:8:24 ───
                                                         ╘════════════════════════════════
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
            q_0@test.qs:5:24
            step:
            q_0@test.qs:5:24 ─ H@test.qs:6:24 ──
            step:
            q_0@test.qs:5:24 ─ H@test.qs:6:24 ─── M@test.qs:7:32 ──
                                                         ╘═════════
            step:
            q_0@test.qs:5:24 ─ H@test.qs:6:24 ─── M@test.qs:7:32 ──── |0〉@test.qs:8:24 ───
                                                         ╘════════════════════════════════
            step:
            q_0@test.qs:5:24 ─ H@test.qs:6:24 ─── M@test.qs:7:32 ──── |0〉@test.qs:8:24 ───
                                                         ╘════════════════════════════════
            step:
            q_0@test.qs:5:24 ─ H@test.qs:6:24 ─── M@test.qs:7:32 ──── |0〉@test.qs:8:24 ───
                                                         ╘════════════════════════════════
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
            q_0@test.qs:5:24
            step:
            q_0@test.qs:5:24 ─ H@test.qs:6:24 ──
            step:
            q_0@test.qs:5:24 ─ H@test.qs:6:24 ─── M@test.qs:7:32 ──
                                                         ╘═════════
            step:
            q_0@test.qs:5:24 ─ H@test.qs:6:24 ─── M@test.qs:7:32 ──
                                                         ╘═════════
            step:
            q_0@test.qs:5:24 ─ H@test.qs:6:24 ─── M@test.qs:7:32 ─── X@test.qs:9:28 ──
                                                         ╘════════════════════════════
            step:
            q_0@test.qs:5:24 ─ H@test.qs:6:24 ─── M@test.qs:7:32 ─── X@test.qs:9:28 ──
                                                         ╘════════════════════════════
            step:
            q_0@test.qs:5:24 ─ H@test.qs:6:24 ─── M@test.qs:7:32 ─── X@test.qs:9:28 ──
                                                         ╘════════════════════════════
            step:
            q_0@test.qs:5:24 ─ H@test.qs:6:24 ─── M@test.qs:7:32 ─── X@test.qs:9:28 ──
                                                         ╘════════════════════════════
        "#]]
        .assert_eq(&circs);
    }
}
