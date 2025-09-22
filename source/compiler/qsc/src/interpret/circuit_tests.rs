// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::unicode_not_nfc)]

use super::{CircuitEntryPoint, Debugger, Interpreter};
use crate::{interpret::Error, target::Profile};
use expect_test::expect;
use miette::Diagnostic;
use qsc_circuit::{Circuit, Config, GenerationMethod};
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

fn circuit(code: &str, entry: CircuitEntryPoint, config: Config) -> Result<Circuit, Vec<Error>> {
    let profile = if config.generation_method == GenerationMethod::Static {
        Profile::AdaptiveRIF
    } else {
        Profile::Unrestricted
    };
    circuit_with_profile(code, entry, config, profile)
}

fn circuit_with_profile(
    code: &str,
    entry: CircuitEntryPoint,
    config: Config,
    profile: Profile,
) -> Result<Circuit, Vec<Error>> {
    let mut interpreter = interpreter(code, profile);
    interpreter.set_quantum_seed(Some(2));
    interpreter.circuit(entry, config)
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
        Config::default(),
    )
    .expect("circuit generation should succeed");

    expect![].assert_eq(&circ.to_string());
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
        Config::default(),
    )
    .expect("circuit generation should succeed");

    expect![[r"
        q_0    ── H ──
    "]]
    .assert_eq(&circ.to_string());
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
        Config::default(),
    )
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
        Config::default(),
    )
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
        Config::default(),
    )
    .expect("circuit generation should succeed");

    expect![[r#"
        q_0    ─ Rx(1.5708) ──
    "#]]
    .assert_eq(&circ.to_string());
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
        Config::default(),
    )
    .expect("circuit generation should succeed");

    expect![[r#"
        q_0    ─ [[ ─── [Main_2] ──── X ──── X ──── X ──── X ──── X ──── X ─── ]] ──
    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn classical_for_loop_loop_detection() {
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
        Config {
            loop_detection: true,
            ..Default::default()
        },
    )
    .expect("circuit generation should succeed");

    expect![[r#"
        q_0    ─ [[ ─── [Main_2] ─── [[ ──── [X(×6)] ──── X ─── [[ ──── [X(×5)] ──── X ──── X ──── X ──── X ──── X ─── ]] ─── ]] ─── ]] ──
    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn m_base_profile() {
    let circ = circuit_with_profile(
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
        Config::default(),
        Profile::Base,
    )
    .expect("circuit generation should succeed");

    expect![[r#"
        q_0    ── H ──── M ──
                         ╘═══
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
        Config::default(),
    )
    .expect("circuit generation should succeed");

    expect![[r"
        q_0    ── H ──── M ──
                         ╘═══
    "]]
    .assert_eq(&circ.to_string());
}

#[test]
fn mresetz_default_profile() {
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
        Config::default(),
    )
    .expect("circuit generation should succeed");

    expect![[r"
        q_0    ── H ──── M ──── |0〉 ──
                         ╘════════════
    "]]
    .assert_eq(&circ.to_string());
}

#[test]
fn mresetz_base_profile() {
    let circ = circuit_with_profile(
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
        Config::default(),
        Profile::Base,
    )
    .expect("circuit generation should succeed");

    // code gen in Base turns the MResetZ into an M
    expect![[r#"
        q_0    ── H ──── M ──
                         ╘═══
    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn eval_method_result_comparison() {
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
    );

    interpreter.set_quantum_seed(Some(2));

    let circuit_err = interpreter
        .circuit(
            CircuitEntryPoint::EntryPoint,
            Config {
                generation_method: GenerationMethod::ClassicalEval,
                ..Default::default()
            },
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
            Config {
                generation_method: GenerationMethod::Simulate,
                ..Default::default()
            },
        )
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
fn result_comparison_to_literal() {
    let circ = circuit(
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
        CircuitEntryPoint::EntryPoint,
        Config::default(),
    )
    .expect("circuit generation should succeed");

    expect![[r#"
        q_0    ── H ──── M ─── [[ ──── [check (c_0 = |1〉)] ─── [[ ─── [true] ──── X ─── ]] ─── ]] ──── |0〉 ──
                         ╘═════════════════════ ● ══════════════════════ ● ══════════════════════════════════
    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn result_comparison_to_literal_zero() {
    let circ = circuit(
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
        CircuitEntryPoint::EntryPoint,
        Config::default(),
    )
    .expect("circuit generation should succeed");

    expect![[r#"
        q_0    ── H ──── M ─── [[ ──── [check (c_0 = |0〉)] ─── [[ ─── [true] ──── X ─── ]] ─── ]] ──── |0〉 ──
                         ╘═════════════════════ ● ══════════════════════ ● ══════════════════════════════════
    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn else_block_only() {
    let circ = circuit(
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
        CircuitEntryPoint::EntryPoint,
        Config::default(),
    )
    .expect("circuit generation should succeed");

    expect![[r#"
        q_0    ── H ──── M ─── [[ ──── [check (c_0 = |0〉)] ─── [[ ─── [false] ─── X ─── ]] ─── ]] ──── |0〉 ──
                         ╘═════════════════════ ● ══════════════════════ ● ══════════════════════════════════
    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn result_comparison_to_result() {
    let circ = circuit(
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
        CircuitEntryPoint::EntryPoint,
        Config::default(),
    )
    .expect("circuit generation should succeed");

    expect![[r#"
        q_0    ── H ──── M ─── [[ ───── [check (c_0c_1 = |00〉 or c_0c_1 = |11〉)] ───── [[ ─── [true] ──── X ─── ]] ─── ]] ──── |0〉 ──
                         ╘═════════════════════════════════ ● ══════════════════════════════════ ● ══════════════════════════════════
        q_1    ── H ──── M ─────────────────────────────────┼────────────────────────────────────┼──────────────────────────── |0〉 ──
                         ╘═════════════════════════════════ ● ══════════════════════════════════ ● ══════════════════════════════════
    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn loop_and_scope() {
    // TODO: something is weird with this one
    let circ = circuit(
        r"
            namespace Test {
            operation Main() : Unit {
                use qs = Qubit[2];

                PrepareSomething(qs);
                DoSomethingElse(qs);
                DoSomethingDifferent(qs);

                ResetAll(qs);
            }

            operation PrepareSomething(qs : Qubit[]) : Unit {
                for iteration in 1..10 {
                    H(qs[0]);
                    X(qs[0]);
                    CNOT(qs[0], qs[1]);
                }
            }

            operation DoSomethingElse(qs : Qubit[]) : Unit {
                for iteration in 1..10 {
                    H(qs[1]);
                    X(qs[0]);
                    X(qs[1]);
                    CNOT(qs[1], qs[0]);
                }
            }

            operation DoSomethingDifferent(qs : Qubit[]) : Unit {
                for iteration in 1..10 {
                    H(qs[0]);
                    Z(qs[0]);
                    CNOT(qs[0], qs[1]);
                }
            }
    }
    ",
        CircuitEntryPoint::Operation("Test.Main".into()),
        Config {
            loop_detection: true,
            group_scopes: true,
            ..Default::default()
        },
    )
    .expect("circuit generation should succeed");

    expect![[r#"
        q_0    ─ [[ ─── [PrepareSomething_3] ─── [[ ─── [H X X(×10)] ──── H ──── X ──── ● ─── [[ ──── [H(×9)] ──── H ──── X ──── ● ──── H ──── X ──── ● ──── H ──── X ──── ● ──── H ──── X ──── ● ──── H ──── X ──── ● ──── H ──── X ──── ● ──── H ──── X ──── ● ──── H ──── X ──── ● ──── H ──── X ──── ● ─── ]] ─── ]] ─── ]] ─── [[ ─── [DoSomethingElse_6] ── [[ ──── [H X X...(×10)] ──── X ─────────── X ─── [[ ──── [H(×9)] ──── X ─────────── X ──── X ─────────── X ──── X ─────────── X ──── X ─────────── X ──── X ─────────── X ──── X ─────────── X ──── X ─────────── X ──── X ─────────── X ──── X ─────────── X ─── ]] ─── ]] ─── ]] ─── [[ ─── [DoSomethingDifferent_9] ─── [[ ─── [H Z X(×10)] ──── H ──── Z ──── ● ─── [[ ──── [H(×9)] ──── H ──── Z ──── ● ──── H ──── Z ──── ● ──── H ──── Z ──── ● ──── H ──── Z ──── ● ──── H ──── Z ──── ● ──── H ──── Z ──── ● ──── H ──── Z ──── ● ──── H ──── Z ──── ● ──── H ──── Z ──── ● ─── ]] ─── ]] ─── ]] ──── |0〉 ──
                                  ┆                           ┆                         │                ┆                       │                    │                    │                    │                    │                    │                    │                    │                    │                                          ┆                            ┆                           │                ┆                       │                    │                    │                    │                    │                    │                    │                    │                    │                                             ┆                             ┆                         │                ┆                       │                    │                    │                    │                    │                    │                    │                    │                    │
        q_1    ─ [[ ─── [PrepareSomething_3] ─── [[ ─── [H X X(×10)] ────────────────── X ─── [[ ──── [H(×9)] ────────────────── X ────────────────── X ────────────────── X ────────────────── X ────────────────── X ────────────────── X ────────────────── X ────────────────── X ────────────────── X ─── ]] ─── ]] ─── ]] ─── [[ ─── [DoSomethingElse_6] ── [[ ──── [H X X...(×10)] ──── H ──── X ──── ● ─── [[ ──── [H(×9)] ──── H ──── X ──── ● ──── H ──── X ──── ● ──── H ──── X ──── ● ──── H ──── X ──── ● ──── H ──── X ──── ● ──── H ──── X ──── ● ──── H ──── X ──── ● ──── H ──── X ──── ● ──── H ──── X ──── ● ─── ]] ─── ]] ─── ]] ─── [[ ─── [DoSomethingDifferent_9] ─── [[ ─── [H Z X(×10)] ────────────────── X ─── [[ ──── [H(×9)] ────────────────── X ────────────────── X ────────────────── X ────────────────── X ────────────────── X ────────────────── X ────────────────── X ────────────────── X ────────────────── X ─── ]] ─── ]] ─── ]] ──── |0〉 ──
    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn result_comparison_empty_block() {
    let circ = circuit(
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
        CircuitEntryPoint::EntryPoint,
        Config::default(),
    )
    .expect("circuit generation should succeed");

    expect![[r#"
        q_0    ── H ──── M ──── |0〉 ──
                         ╘════════════
        q_1    ── H ──── M ──── |0〉 ──
                         ╘════════════
    "#]]
    .assert_eq(&circ.to_string());
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
        Config::default(),
    )
    .expect("circuit generation should succeed");

    expect![[r"
        q_0    ─ foo ─
    "]]
    .assert_eq(&circ.to_string());
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
        Config::default(),
    )
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
        Config::default(),
    )
    .expect("circuit generation should succeed");

    expect![[r#"
        q_0    ─ [[ ─── [Main_0] ──── X ─── foo(4) ─── ]] ──
    "#]]
    .assert_eq(&circ.to_string());
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
        {
            Config {
                generation_method: GenerationMethod::ClassicalEval,
                ..Default::default()
            }
        },
    )
    .expect("circuit generation should succeed");

    // This intrinsic never gets codegenned, so it's missing from the
    // circuit too.

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
fn custom_intrinsic_mixed_args_static() {
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
        Config::default(),
    )
    .expect("circuit generation should succeed");

    // This intrinsic never gets codegenned, so it's missing from the
    // circuit too.

    expect![[r#"
        q_0
        q_1
        q_2
        q_3
        q_4
        q_5
        q_6
        q_7
        q_8
        q_9
    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn custom_intrinsic_apply_idle_noise_classical_eval() {
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
        Config {
            generation_method: GenerationMethod::ClassicalEval,
            ..Default::default()
        },
    )
    .expect("circuit generation should succeed");

    // These intrinsics never get codegenned, so they're missing from the
    // circuit too.
    expect![[r#"
        q_0    ─ ApplyIdleNoise ──
    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn custom_intrinsic_apply_idle_noise_static() {
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
        Config::default(),
    )
    .expect("circuit generation should succeed");

    // These intrinsics never get codegenned, so they're missing from the
    // circuit too.
    expect![[r#"
        q_0
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
        Config::default(),
    )
    .expect("circuit generation should succeed");

    expect![[r#"
        q_0    ─ [[ ─── [Test_1] ──── H ──── ● ─── ]] ──── M ──
                            ┆                │             ╘═══
        q_1    ─ [[ ─── [Test_1] ─────────── X ─── ]] ──── M ──
                                                           ╘═══
    "#]]
    .assert_eq(&circ.to_string());
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
        Config::default(),
    )
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
        Config::default(),
    )
    .expect("circuit generation should succeed");

    expect![[r"
        q_0    ── Y ──
    "]]
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
        Config::default(),
    )
    .expect("circuit generation should succeed");

    expect![[r"
        q_0    ── H ──
    "]]
    .assert_eq(&circ.to_string());
}

#[test]
fn controlled_operation() {
    let circ_err = circuit(
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
        Config::default(),
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
        Config::default(),
    )
    .expect("circuit generation should not fail");

    expect![[r#"
        q_0    ─ [[ ─── [Test_1] ──── H ──── ● ─── ]] ──── M ──
                            ┆                │             ╘═══
        q_1    ─ [[ ─── [Test_1] ─────────── X ─── ]] ──── M ──
                                                           ╘═══
    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn operation_with_non_qubit_args() {
    let circ_err = circuit(
        r"
        namespace Test {
            @EntryPoint()
            operation Main() : Result[] { [] }

            operation Test(q1: Qubit, q2: Qubit, i: Int) : Unit {
            }

        }",
        CircuitEntryPoint::Operation("Test.Test".into()),
        Config::default(),
    )
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
        Config::default(),
    )
    .expect("circuit generation should succeed");

    expect![[r#"
        q_0    ─ [[ ─── [Main_0] ──── H ────────────────────────── ● ─── ]] ─── [[ ─── [Main_0] ──── M ────────────────────────────────────────────────────────────────────────────── ● ─── ]] ─────────
                            ┆                                      │                       ┆         ╘════════════════════════════════════════════════════════════════════════════════╪═════════════════
        q_1    ─ [[ ─── [Main_0] ──── H ──── X ─── Ry(1.0000) ──── X ─── ]] ─── [[ ─── [Main_0] ───────────────────────────────────────────────────────────────────── Rxx(1.0000) ────┼──── ]] ──── M ──
                                                                                           ┆                                                                               ┆          │             ╘═══
        q_2    ──────────────────────────────────────────────────────────────── [[ ─── [Main_0] ──── H ─── Rx(1.0000) ──── H ─── Rx(1.0000) ──── H ─── Rx(1.0000) ─────────┆──────────┼──── ]] ─────────
                                                                                           ┆                                                                               ┆          │
        q_3    ──────────────────────────────────────────────────────────────── [[ ─── [Main_0] ───────────────────────────────────────────────────────────────────── Rxx(1.0000) ─── X ─── ]] ──── M ──
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
        Config::default(),
    )
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
        Config::default(),
    )
    .expect("circuit generation should succeed");

    expect![[r#"
        q_0    ─ [[ ─── [Main_0(1.0000)] ─── Rxx(1.0000) ── Rxx(1.0000) ── ]] ──
                                ┆                 ┆              ┆
        q_1    ─ [[ ─── [Main_0(1.0000)] ─── Rxx(1.0000) ── Rxx(1.0000) ── ]] ──
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
        Config::default(),
    )
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

#[test]
fn if_else() {
    let circ = circuit(
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
        CircuitEntryPoint::EntryPoint,
        Config::default(),
    )
    .expect("circuit generation should succeed");

    expect![[r#"
        q_0    ── H ──── M ────────────────────────────────────────────────────────────────────────────────────────────────────────────────
                         ╘═════════════════════ ● ══════════════════════ ● ═════════════════════════════ ● ════════════════════════════════
        q_1    ─────────────── [[ ──── [check (c_0 = |1〉)] ─── [[ ─── [true] ──── X ─── ]] ─── [[ ─── [false] ─── Y ─── ]] ─── ]] ──── M ──
                                                                                                                                       ╘═══
    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn sequential_ifs() {
    let circ = circuit(
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
        CircuitEntryPoint::EntryPoint,
        Config::default(),
    )
    .expect("circuit generation should succeed");

    expect![[r#"
        q_0    ── H ──── M ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
                         ╘═════════════════════ ● ══════════════════════ ● ═════════════════════════════ ● ═══════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════
        q_1    ── H ──── M ─────────────────────┼────────────────────────┼───────────────────────────────┼────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
                         ╘══════════════════════╪════════════════════════╪═══════════════════════════════╪════════════════════════════════════════════ ● ══════════════════════ ● ═════════════════════════════ ● ════════════════════════════════
        q_2    ─────────────── [[ ──── [check (c_0 = |1〉)] ─── [[ ─── [true] ──── X ─── ]] ─── [[ ─── [false] ─── Z ─── ]] ─── ]] ─── [[ ──── [check (c_1 = |1〉)] ─── [[ ─── [true] ──── X ─── ]] ─── [[ ─── [false] ─── Y ─── ]] ─── ]] ──── M ──
                                                                                                                                                                                                                                              ╘═══
    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn nested_ifs() {
    let circ_err = circuit(
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
        CircuitEntryPoint::EntryPoint,
        Config::default(),
    )
    .expect_err("circuit generation should fail");

    expect![[r#"
        [
            Circuit(
                UnsupportedFeature(
                    "complex branch: true_block=BlockId(1) successor=None, false_block=BlockId(2) successor=Some(BlockId(6))",
                ),
            ),
        ]
    "#]]
    .assert_debug_eq(&circ_err);
}

#[test]
fn multiple_possible_float_values_in_unitary_arg() {
    // TODO: this doesn't show classical control
    let circ = circuit(
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
        CircuitEntryPoint::EntryPoint,
        Config::default(),
    )
    .expect("circuit generation should succeed");

    expect![[r#"
        q_0    ──────────── H ────────────── M ──
                                             ╘═══
        q_1    ─ Rx(function of: (c_0)) ──── M ──
                                             ╘═══
    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn panic_in_register_grouping() {
    let circ = circuit(
        r#"
            operation Main() : Unit {
                use qs = Qubit[3];
                for iteration in 1..10 {
                    H(qs[0]);
                    X(qs[0]);
                    CNOT(qs[0], qs[1]);
                    Message("hi");
                }
            }
        "#,
        CircuitEntryPoint::EntryPoint,
        {
            Config {
                collapse_qubit_registers: true,
                ..Default::default()
            }
        },
    )
    .expect("circuit generation should succeed");

    expect![[r#"
        q_0    ─ [[ ─── [Main_2 (q[0, 1])] ─── H (q[0]) ─── X (q[0]) ─── CX (q[0, 1]) ─── H (q[0]) ─── X (q[0]) ─── CX (q[0, 1]) ─── H (q[0]) ─── X (q[0]) ─── CX (q[0, 1]) ─── H (q[0]) ─── X (q[0]) ─── CX (q[0, 1]) ─── H (q[0]) ─── X (q[0]) ─── CX (q[0, 1]) ─── H (q[0]) ─── X (q[0]) ─── CX (q[0, 1]) ─── H (q[0]) ─── X (q[0]) ─── CX (q[0, 1]) ─── H (q[0]) ─── X (q[0]) ─── CX (q[0, 1]) ─── H (q[0]) ─── X (q[0]) ─── CX (q[0, 1]) ─── H (q[0]) ─── X (q[0]) ─── CX (q[0, 1]) ─── ]] ──
        q_2    ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
    "#]].assert_eq(&circ.to_string());
}

#[test]
fn custom_intrinsic_variable_arg() {
    let circ = circuit(
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
        CircuitEntryPoint::EntryPoint,
        Config::default(),
    )
    .expect("circuit generation should succeed");

    expect![[r#"
        q_0    ── H ──── M ─── foo(function of: (c_0)) ─
                         ╘══════════════════════════════
    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn branch_on_dynamic_double() {
    // TODO: this doesn't show classical control
    let circ = circuit(
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
        CircuitEntryPoint::EntryPoint,
        Config::default(),
    )
    .expect("circuit generation should succeed");

    expect![[r#"
        q_0    ──────────── H ────────────── M ──
                                             ╘═══
        q_1    ─ Rx(function of: (c_0)) ──── M ──
                                             ╘═══
    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn branch_on_dynamic_bool() {
    // TODO: this doesn't show classical control
    let circ = circuit(
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
        CircuitEntryPoint::EntryPoint,
        Config::default(),
    )
    .expect("circuit generation should succeed");

    expect![[r#"
        q_0    ── H ──── M ────────────────────────────────────────────────────────────────────────────────────────
                         ╘═════════════════════════ ● ══════════════════════════ ● ════════════════════════════════
        q_1    ─────────────── [[ ─── [check (function of: (c_0))] ─── [[ ─── [true] ──── X ─── ]] ─── ]] ──── M ──
                                                                                                               ╘═══
    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn teleportation() {
    let circ = circuit(
        r#"
            operation Main() : Bool {
                // Allocate `qAlice`, `qBob` qubits
                use (qAlice, qBob) = (Qubit(), Qubit());

                // Entangle `qAlice`, `qBob` qubits
                H(qAlice);
                CNOT(qAlice, qBob);

                // From now on qubits `qAlice` and `qBob` will not interact directly.

                // Allocate `qToTeleport` qubit and prepare it to be |𝜓⟩≈0.9394|0⟩−0.3429𝑖|1⟩
                use qToTeleport = Qubit();
                Rx(0.7, qToTeleport);

                // Prepare the message by entangling `qToTeleport` and `qAlice` qubits
                CNOT(qToTeleport, qAlice);
                H(qToTeleport);

                // Obtain classical measurement results b1 and b2 at Alice's site.
                let b1 = M(qToTeleport) == One;
                let b2 = M(qAlice) == One;

                // At this point classical bits b1 and b2 are "sent" to the Bob's site.

                // Decode the message by applying adjustments based on classical data b1 and b2.
                if b1 {
                    Z(qBob);
                }
                if b2 {
                    X(qBob);
                }

                // Make sure that the obtained message is |𝜓⟩≈0.9394|0⟩−0.3429𝑖|1⟩
                Rx(-0.7, qBob);
                // let correct = Std.Diagnostics.CheckZero(qBob);
                // Message($"Teleportation successful: {correct}.");

                // Reset all qubits to |0⟩ state.
                ResetAll([qAlice, qBob, qToTeleport]);

                // Return indication if the measurement of the state was correct
                // correct
                true
            }
        "#,
        CircuitEntryPoint::EntryPoint,
        Config::default(),
    )
    .expect("circuit generation should succeed");

    expect![[r#"
        q_0    ─ [[ ─── [Main_0] ──────── H ──────── ● ──── X ────────── ]] ──── M ──── |0〉 ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
                            ┆                        │      │                    ╘════════════════════════════════════════════════════════════════════════════════════════════════ ● ══════════════════════ ● ═════════════════════════════════════════════════
        q_1    ─ [[ ─── [Main_0] ─────────────────── X ─────┼─────────── ]] ─────────── [[ ───── [check (c_0 = |1〉)] ─── [[ ─── [true] ──── Z ─── ]] ─── ]] ──── [[ ───── [check (c_1 = |1〉)] ─── [[ ─── [true] ──── X ─── ]] ─── ]] ─── Rx(-0.7000) ─── |0〉 ──
                            ┆                               │                                             │                        │
        q_2    ─ [[ ─── [Main_0] ─── Rx(0.7000) ─────────── ● ──── H ─── ]] ──── M ───────────────────────┼────────────────────────┼──────────────────────────── |0〉 ──────────────────────────────────────────────────────────────────────────────────────────
                                                                                 ╘═══════════════════════ ● ══════════════════════ ● ══════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════
    "#]]
    .assert_eq(&circ.to_string());
}

#[test]
fn dot_product_phase_estimation() {
    let circ = circuit(
        DOT_PRODUCT_PHASE_ESTIMATION,
        CircuitEntryPoint::EntryPoint,
        Config::default(),
    )
    .expect("circuit generation should succeed");
    expect![[r#"
        q_0    ─ [[ ─── [StateInitialisation_16] ─── S' ──── H ─── Rz(0.2244) ──── X ─── Rz(-0.2244) ─── X ──── H ──── S ─── S' ──── H ─── Rz(0.3142) ──── X ─── Rz(-0.3142) ─── X ──── H ──── S ─── ]] ────────── [[ ─── [StateInitialisation_17] ─── S' ──── H ─── Rz(-0.3142) ─── X ─── Rz(0.3142) ──── X ──── H ──── S ─── S' ──── H ─── Rz(-0.2244) ─── X ─── Rz(0.2244) ──── X ──── H ──── S ─── ]] ──── X ─── [[ ─── [GOracle_33] ──── H ──── X ──── H ─── ]] ──── X ─── [[ ─── [StateInitialisation_16] ─── S' ──── H ─── Rz(0.2244) ──── X ─── Rz(-0.2244) ─── X ──── H ──── S ─── S' ──── H ─── Rz(0.3142) ──── X ─── Rz(-0.3142) ─── X ──── H ──── S ─── ]] ────────── [[ ─── [StateInitialisation_17] ─── S' ──── H ─── Rz(-0.3142) ─── X ─── Rz(0.3142) ──── X ──── H ──── S ─── S' ──── H ─── Rz(-0.2244) ─── X ─── Rz(0.2244) ──── X ──── H ──── S ─── ]] ──── X ─── [[ ─── [GOracle_33] ──── H ──── X ──── H ─── ]] ──── X ─── [[ ─── [StateInitialisation_16] ─── S' ──── H ─── Rz(0.2244) ──── X ─── Rz(-0.2244) ─── X ──── H ──── S ─── S' ──── H ─── Rz(0.3142) ──── X ─── Rz(-0.3142) ─── X ──── H ──── S ─── ]] ────────── [[ ─── [StateInitialisation_17] ─── S' ──── H ─── Rz(-0.3142) ─── X ─── Rz(0.3142) ──── X ──── H ──── S ─── S' ──── H ─── Rz(-0.2244) ─── X ─── Rz(0.2244) ──── X ──── H ──── S ─── ]] ──── X ─── [[ ─── [GOracle_33] ──── H ──── X ──── H ─── ]] ──── X ─── [[ ─── [StateInitialisation_16] ─── S' ──── H ─── Rz(0.2244) ──── X ─── Rz(-0.2244) ─── X ──── H ──── S ─── S' ──── H ─── Rz(0.3142) ──── X ─── Rz(-0.3142) ─── X ──── H ──── S ─── ]] ────────── [[ ─── [StateInitialisation_17] ─── S' ──── H ─── Rz(-0.3142) ─── X ─── Rz(0.3142) ──── X ──── H ──── S ─── S' ──── H ─── Rz(-0.2244) ─── X ─── Rz(0.2244) ──── X ──── H ──── S ─── ]] ──── X ─── [[ ─── [GOracle_33] ──── H ──── X ──── H ─── ]] ──── X ─── [[ ─── [StateInitialisation_16] ─── S' ──── H ─── Rz(0.2244) ──── X ─── Rz(-0.2244) ─── X ──── H ──── S ─── S' ──── H ─── Rz(0.3142) ──── X ─── Rz(-0.3142) ─── X ──── H ──── S ─── ]] ────────── [[ ─── [StateInitialisation_17] ─── S' ──── H ─── Rz(-0.3142) ─── X ─── Rz(0.3142) ──── X ──── H ──── S ─── S' ──── H ─── Rz(-0.2244) ─── X ─── Rz(0.2244) ──── X ──── H ──── S ─── ]] ──── X ─── [[ ─── [GOracle_33] ──── H ──── X ──── H ─── ]] ──── X ─── [[ ─── [StateInitialisation_16] ─── S' ──── H ─── Rz(0.2244) ──── X ─── Rz(-0.2244) ─── X ──── H ──── S ─── S' ──── H ─── Rz(0.3142) ──── X ─── Rz(-0.3142) ─── X ──── H ──── S ─── ]] ────────── [[ ─── [StateInitialisation_17] ─── S' ──── H ─── Rz(-0.3142) ─── X ─── Rz(0.3142) ──── X ──── H ──── S ─── S' ──── H ─── Rz(-0.2244) ─── X ─── Rz(0.2244) ──── X ──── H ──── S ─── ]] ──── X ─── [[ ─── [GOracle_33] ──── H ──── X ──── H ─── ]] ──── X ─── [[ ─── [StateInitialisation_16] ─── S' ──── H ─── Rz(0.2244) ──── X ─── Rz(-0.2244) ─── X ──── H ──── S ─── S' ──── H ─── Rz(0.3142) ──── X ─── Rz(-0.3142) ─── X ──── H ──── S ─── ]] ────────── [[ ─── [StateInitialisation_17] ─── S' ──── H ─── Rz(-0.3142) ─── X ─── Rz(0.3142) ──── X ──── H ──── S ─── S' ──── H ─── Rz(-0.2244) ─── X ─── Rz(0.2244) ──── X ──── H ──── S ─── ]] ──── X ─── [[ ─── [GOracle_33] ──── H ──── X ──── H ─── ]] ──── X ─── [[ ─── [StateInitialisation_16] ─── S' ──── H ─── Rz(0.2244) ──── X ─── Rz(-0.2244) ─── X ──── H ──── S ─── S' ──── H ─── Rz(0.3142) ──── X ─── Rz(-0.3142) ─── X ──── H ──── S ─── ]] ────────── [[ ─── [StateInitialisation_17] ─── S' ──── H ─── Rz(-0.3142) ─── X ─── Rz(0.3142) ──── X ──── H ──── S ─── S' ──── H ─── Rz(-0.2244) ─── X ─── Rz(0.2244) ──── X ──── H ──── S ─── ]] ──── X ─── [[ ─── [GOracle_33] ──── H ──── X ──── H ─── ]] ──── X ─── [[ ─── [StateInitialisation_16] ─── S' ──── H ─── Rz(0.2244) ──── X ─── Rz(-0.2244) ─── X ──── H ──── S ─── S' ──── H ─── Rz(0.3142) ──── X ─── Rz(-0.3142) ─── X ──── H ──── S ─── ]] ────────── [[ ─── [StateInitialisation_17] ─── S' ──── H ─── Rz(-0.3142) ─── X ─── Rz(0.3142) ──── X ──── H ──── S ─── S' ──── H ─── Rz(-0.2244) ─── X ─── Rz(0.2244) ──── X ──── H ──── S ─── ]] ──── X ─── [[ ─── [GOracle_33] ──── H ──── X ──── H ─── ]] ──── X ─── [[ ─── [StateInitialisation_16] ─── S' ──── H ─── Rz(0.2244) ──── X ─── Rz(-0.2244) ─── X ──── H ──── S ─── S' ──── H ─── Rz(0.3142) ──── X ─── Rz(-0.3142) ─── X ──── H ──── S ─── ]] ────────── [[ ─── [StateInitialisation_17] ─── S' ──── H ─── Rz(-0.3142) ─── X ─── Rz(0.3142) ──── X ──── H ──── S ─── S' ──── H ─── Rz(-0.2244) ─── X ─── Rz(0.2244) ──── X ──── H ──── S ─── ]] ──── X ─── [[ ─── [GOracle_33] ──── H ──── X ──── H ─── ]] ──── X ─── [[ ─── [StateInitialisation_16] ─── S' ──── H ─── Rz(0.2244) ──── X ─── Rz(-0.2244) ─── X ──── H ──── S ─── S' ──── H ─── Rz(0.3142) ──── X ─── Rz(-0.3142) ─── X ──── H ──── S ─── ]] ────────── [[ ─── [StateInitialisation_17] ─── S' ──── H ─── Rz(-0.3142) ─── X ─── Rz(0.3142) ──── X ──── H ──── S ─── S' ──── H ─── Rz(-0.2244) ─── X ─── Rz(0.2244) ──── X ──── H ──── S ─── ]] ──── X ─── [[ ─── [GOracle_33] ──── H ──── X ──── H ─── ]] ──── X ─── [[ ─── [StateInitialisation_16] ─── S' ──── H ─── Rz(0.2244) ──── X ─── Rz(-0.2244) ─── X ──── H ──── S ─── S' ──── H ─── Rz(0.3142) ──── X ─── Rz(-0.3142) ─── X ──── H ──── S ─── ]] ────────── [[ ─── [StateInitialisation_17] ─── S' ──── H ─── Rz(-0.3142) ─── X ─── Rz(0.3142) ──── X ──── H ──── S ─── S' ──── H ─── Rz(-0.2244) ─── X ─── Rz(0.2244) ──── X ──── H ──── S ─── ]] ──── X ─── [[ ─── [GOracle_33] ──── H ──── X ──── H ─── ]] ──── X ─── [[ ─── [StateInitialisation_16] ─── S' ──── H ─── Rz(0.2244) ──── X ─── Rz(-0.2244) ─── X ──── H ──── S ─── S' ──── H ─── Rz(0.3142) ──── X ─── Rz(-0.3142) ─── X ──── H ──── S ─── ]] ────────── [[ ─── [StateInitialisation_17] ─── S' ──── H ─── Rz(-0.3142) ─── X ─── Rz(0.3142) ──── X ──── H ──── S ─── S' ──── H ─── Rz(-0.2244) ─── X ─── Rz(0.2244) ──── X ──── H ──── S ─── ]] ──── X ─── [[ ─── [GOracle_33] ──── H ──── X ──── H ─── ]] ──── X ─── [[ ─── [StateInitialisation_16] ─── S' ──── H ─── Rz(0.2244) ──── X ─── Rz(-0.2244) ─── X ──── H ──── S ─── S' ──── H ─── Rz(0.3142) ──── X ─── Rz(-0.3142) ─── X ──── H ──── S ─── ]] ────────── [[ ─── [StateInitialisation_17] ─── S' ──── H ─── Rz(-0.3142) ─── X ─── Rz(0.3142) ──── X ──── H ──── S ─── S' ──── H ─── Rz(-0.2244) ─── X ─── Rz(0.2244) ──── X ──── H ──── S ─── ]] ──── X ─── [[ ─── [GOracle_33] ──── H ──── X ──── H ─── ]] ──── X ─── [[ ─── [StateInitialisation_16] ─── S' ──── H ─── Rz(0.2244) ──── X ─── Rz(-0.2244) ─── X ──── H ──── S ─── S' ──── H ─── Rz(0.3142) ──── X ─── Rz(-0.3142) ─── X ──── H ──── S ─── ]] ────────── [[ ─── [StateInitialisation_17] ─── S' ──── H ─── Rz(-0.3142) ─── X ─── Rz(0.3142) ──── X ──── H ──── S ─── S' ──── H ─── Rz(-0.2244) ─── X ─── Rz(0.2244) ──── X ──── H ──── S ─── ]] ──── X ─── [[ ─── [GOracle_33] ──── H ──── X ──── H ─── ]] ──── X ─── [[ ─── [StateInitialisation_16] ─── S' ──── H ─── Rz(0.2244) ──── X ─── Rz(-0.2244) ─── X ──── H ──── S ─── S' ──── H ─── Rz(0.3142) ──── X ─── Rz(-0.3142) ─── X ──── H ──── S ─── ]] ────────── [[ ─── [StateInitialisation_17] ─── S' ──── H ─── Rz(-0.3142) ─── X ─── Rz(0.3142) ──── X ──── H ──── S ─── S' ──── H ─── Rz(-0.2244) ─── X ─── Rz(0.2244) ──── X ──── H ──── S ─── ]] ──── X ─── [[ ─── [GOracle_33] ──── H ──── X ──── H ─── ]] ──── X ─── [[ ─── [StateInitialisation_16] ─── S' ──── H ─── Rz(0.2244) ──── X ─── Rz(-0.2244) ─── X ──── H ──── S ─── S' ──── H ─── Rz(0.3142) ──── X ─── Rz(-0.3142) ─── X ──── H ──── S ─── ]] ───────────────────────────────────────────────────────────────────────────────────────────────────────── [[ ─── [StateInitialisation_17] ─── S' ──── H ─── Rz(-0.3142) ─── X ─── Rz(0.3142) ──── X ──── H ──── S ─── S' ──── H ─── Rz(-0.2244) ─── X ─── Rz(0.2244) ──── X ──── H ──── S ─── ]] ──── X ─── [[ ─── [GOracle_33] ──── H ──── X ──── H ─── ]] ──── X ─── [[ ─── [StateInitialisation_16] ─── S' ──── H ─── Rz(0.2244) ──── X ─── Rz(-0.2244) ─── X ──── H ──── S ─── S' ──── H ─── Rz(0.3142) ──── X ─── Rz(-0.3142) ─── X ──── H ──── S ─── ]] ────────── [[ ─── [StateInitialisation_17] ─── S' ──── H ─── Rz(-0.3142) ─── X ─── Rz(0.3142) ──── X ──── H ──── S ─── S' ──── H ─── Rz(-0.2244) ─── X ─── Rz(0.2244) ──── X ──── H ──── S ─── ]] ──── X ─── [[ ─── [GOracle_33] ──── H ──── X ──── H ─── ]] ──── X ─── [[ ─── [StateInitialisation_16] ─── S' ──── H ─── Rz(0.2244) ──── X ─── Rz(-0.2244) ─── X ──── H ──── S ─── S' ──── H ─── Rz(0.3142) ──── X ─── Rz(-0.3142) ─── X ──── H ──── S ─── ]] ────────── [[ ─── [StateInitialisation_17] ─── S' ──── H ─── Rz(-0.3142) ─── X ─── Rz(0.3142) ──── X ──── H ──── S ─── S' ──── H ─── Rz(-0.2244) ─── X ─── Rz(0.2244) ──── X ──── H ──── S ─── ]] ──── X ─── [[ ─── [GOracle_33] ──── H ──── X ──── H ─── ]] ──── X ─── [[ ─── [StateInitialisation_16] ─── S' ──── H ─── Rz(0.2244) ──── X ─── Rz(-0.2244) ─── X ──── H ──── S ─── S' ──── H ─── Rz(0.3142) ──── X ─── Rz(-0.3142) ─── X ──── H ──── S ─── ]] ────────── [[ ─── [StateInitialisation_17] ─── S' ──── H ─── Rz(-0.3142) ─── X ─── Rz(0.3142) ──── X ──── H ──── S ─── S' ──── H ─── Rz(-0.2244) ─── X ─── Rz(0.2244) ──── X ──── H ──── S ─── ]] ──── X ─── [[ ─── [GOracle_33] ──── H ──── X ──── H ─── ]] ──── X ─── [[ ─── [StateInitialisation_16] ─── S' ──── H ─── Rz(0.2244) ──── X ─── Rz(-0.2244) ─── X ──── H ──── S ─── S' ──── H ─── Rz(0.3142) ──── X ─── Rz(-0.3142) ─── X ──── H ──── S ─── ]] ────────── [[ ─── [StateInitialisation_17] ─── S' ──── H ─── Rz(-0.3142) ─── X ─── Rz(0.3142) ──── X ──── H ──── S ─── S' ──── H ─── Rz(-0.2244) ─── X ─── Rz(0.2244) ──── X ──── H ──── S ─── ]] ──── X ─── [[ ─── [GOracle_33] ──── H ──── X ──── H ─── ]] ──── X ─── [[ ─── [StateInitialisation_16] ─── S' ──── H ─── Rz(0.2244) ──── X ─── Rz(-0.2244) ─── X ──── H ──── S ─── S' ──── H ─── Rz(0.3142) ──── X ─── Rz(-0.3142) ─── X ──── H ──── S ─── ]] ────────── [[ ─── [StateInitialisation_17] ─── S' ──── H ─── Rz(-0.3142) ─── X ─── Rz(0.3142) ──── X ──── H ──── S ─── S' ──── H ─── Rz(-0.2244) ─── X ─── Rz(0.2244) ──── X ──── H ──── S ─── ]] ──── X ─── [[ ─── [GOracle_33] ──── H ──── X ──── H ─── ]] ──── X ─── [[ ─── [StateInitialisation_16] ─── S' ──── H ─── Rz(0.2244) ──── X ─── Rz(-0.2244) ─── X ──── H ──── S ─── S' ──── H ─── Rz(0.3142) ──── X ─── Rz(-0.3142) ─── X ──── H ──── S ─── ]] ────────── [[ ─── [StateInitialisation_17] ─── S' ──── H ─── Rz(-0.3142) ─── X ─── Rz(0.3142) ──── X ──── H ──── S ─── S' ──── H ─── Rz(-0.2244) ─── X ─── Rz(0.2244) ──── X ──── H ──── S ─── ]] ──── X ─── [[ ─── [GOracle_33] ──── H ──── X ──── H ─── ]] ──── X ─── [[ ─── [StateInitialisation_16] ─── S' ──── H ─── Rz(0.2244) ──── X ─── Rz(-0.2244) ─── X ──── H ──── S ─── S' ──── H ─── Rz(0.3142) ──── X ─── Rz(-0.3142) ─── X ──── H ──── S ─── ]] ────────── [[ ─── [StateInitialisation_17] ─── S' ──── H ─── Rz(-0.3142) ─── X ─── Rz(0.3142) ──── X ──── H ──── S ─── S' ──── H ─── Rz(-0.2244) ─── X ─── Rz(0.2244) ──── X ──── H ──── S ─── ]] ──── X ─── [[ ─── [GOracle_33] ──── H ──── X ──── H ─── ]] ──── X ─── [[ ─── [StateInitialisation_16] ─── S' ──── H ─── Rz(0.2244) ──── X ─── Rz(-0.2244) ─── X ──── H ──── S ─── S' ──── H ─── Rz(0.3142) ──── X ─── Rz(-0.3142) ─── X ──── H ──── S ─── ]] ──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── [[ ─── [StateInitialisation_17] ─── S' ──── H ─── Rz(-0.3142) ─── X ─── Rz(0.3142) ──── X ──── H ──── S ─── S' ──── H ─── Rz(-0.2244) ─── X ─── Rz(0.2244) ──── X ──── H ──── S ─── ]] ──── X ─── [[ ─── [GOracle_33] ──── H ──── X ──── H ─── ]] ──── X ─── [[ ─── [StateInitialisation_16] ─── S' ──── H ─── Rz(0.2244) ──── X ─── Rz(-0.2244) ─── X ──── H ──── S ─── S' ──── H ─── Rz(0.3142) ──── X ─── Rz(-0.3142) ─── X ──── H ──── S ─── ]] ────────── [[ ─── [StateInitialisation_17] ─── S' ──── H ─── Rz(-0.3142) ─── X ─── Rz(0.3142) ──── X ──── H ──── S ─── S' ──── H ─── Rz(-0.2244) ─── X ─── Rz(0.2244) ──── X ──── H ──── S ─── ]] ──── X ─── [[ ─── [GOracle_33] ──── H ──── X ──── H ─── ]] ──── X ─── [[ ─── [StateInitialisation_16] ─── S' ──── H ─── Rz(0.2244) ──── X ─── Rz(-0.2244) ─── X ──── H ──── S ─── S' ──── H ─── Rz(0.3142) ──── X ─── Rz(-0.3142) ─── X ──── H ──── S ─── ]] ────────── [[ ─── [StateInitialisation_17] ─── S' ──── H ─── Rz(-0.3142) ─── X ─── Rz(0.3142) ──── X ──── H ──── S ─── S' ──── H ─── Rz(-0.2244) ─── X ─── Rz(0.2244) ──── X ──── H ──── S ─── ]] ──── X ─── [[ ─── [GOracle_33] ──── H ──── X ──── H ─── ]] ──── X ─── [[ ─── [StateInitialisation_16] ─── S' ──── H ─── Rz(0.2244) ──── X ─── Rz(-0.2244) ─── X ──── H ──── S ─── S' ──── H ─── Rz(0.3142) ──── X ─── Rz(-0.3142) ─── X ──── H ──── S ─── ]] ────────── [[ ─── [StateInitialisation_17] ─── S' ──── H ─── Rz(-0.3142) ─── X ─── Rz(0.3142) ──── X ──── H ──── S ─── S' ──── H ─── Rz(-0.2244) ─── X ─── Rz(0.2244) ──── X ──── H ──── S ─── ]] ──── X ─── [[ ─── [GOracle_33] ──── H ──── X ──── H ─── ]] ──── X ─── [[ ─── [StateInitialisation_16] ─── S' ──── H ─── Rz(0.2244) ──── X ─── Rz(-0.2244) ─── X ──── H ──── S ─── S' ──── H ─── Rz(0.3142) ──── X ─── Rz(-0.3142) ─── X ──── H ──── S ─── ]] ─────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── [[ ─── [StateInitialisation_17] ─── S' ──── H ─── Rz(-0.3142) ─── X ─── Rz(0.3142) ──── X ──── H ──── S ─── S' ──── H ─── Rz(-0.2244) ─── X ─── Rz(0.2244) ──── X ──── H ──── S ─── ]] ──── X ─── [[ ─── [GOracle_33] ──── H ──── X ──── H ─── ]] ──── X ─── [[ ─── [StateInitialisation_16] ─── S' ──── H ─── Rz(0.2244) ──── X ─── Rz(-0.2244) ─── X ──── H ──── S ─── S' ──── H ─── Rz(0.3142) ──── X ─── Rz(-0.3142) ─── X ──── H ──── S ─── ]] ────────── [[ ─── [StateInitialisation_17] ─── S' ──── H ─── Rz(-0.3142) ─── X ─── Rz(0.3142) ──── X ──── H ──── S ─── S' ──── H ─── Rz(-0.2244) ─── X ─── Rz(0.2244) ──── X ──── H ──── S ─── ]] ──── X ─── [[ ─── [GOracle_33] ──── H ──── X ──── H ─── ]] ──── X ─── [[ ─── [StateInitialisation_16] ─── S' ──── H ─── Rz(0.2244) ──── X ─── Rz(-0.2244) ─── X ──── H ──── S ─── S' ──── H ─── Rz(0.3142) ──── X ─── Rz(-0.3142) ─── X ──── H ──── S ─── ]] ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── [[ ─── [StateInitialisation_17] ─── S' ──── H ─── Rz(-0.3142) ─── X ─── Rz(0.3142) ──── X ──── H ──── S ─── S' ──── H ─── Rz(-0.2244) ─── X ─── Rz(0.2244) ──── X ──── H ──── S ─── ]] ──── X ─── [[ ─── [GOracle_33] ──── H ──── X ──── H ─── ]] ──── X ─── [[ ─── [StateInitialisation_16] ─── S' ──── H ─── Rz(0.2244) ──── X ─── Rz(-0.2244) ─── X ──── H ──── S ─── S' ──── H ─── Rz(0.3142) ──── X ─── Rz(-0.3142) ─── X ──── H ──── S ─── ]] ──── |0〉 ──
                                    ┆                                              │                     │                                                 │                     │                                                    ┆                                              │                     │                                                 │                     │                                              ┆                  │                                             ┆                                              │                     │                                                 │                     │                                                    ┆                                              │                     │                                                 │                     │                                              ┆                  │                                             ┆                                              │                     │                                                 │                     │                                                    ┆                                              │                     │                                                 │                     │                                              ┆                  │                                             ┆                                              │                     │                                                 │                     │                                                    ┆                                              │                     │                                                 │                     │                                              ┆                  │                                             ┆                                              │                     │                                                 │                     │                                                    ┆                                              │                     │                                                 │                     │                                              ┆                  │                                             ┆                                              │                     │                                                 │                     │                                                    ┆                                              │                     │                                                 │                     │                                              ┆                  │                                             ┆                                              │                     │                                                 │                     │                                                    ┆                                              │                     │                                                 │                     │                                              ┆                  │                                             ┆                                              │                     │                                                 │                     │                                                    ┆                                              │                     │                                                 │                     │                                              ┆                  │                                             ┆                                              │                     │                                                 │                     │                                                    ┆                                              │                     │                                                 │                     │                                              ┆                  │                                             ┆                                              │                     │                                                 │                     │                                                    ┆                                              │                     │                                                 │                     │                                              ┆                  │                                             ┆                                              │                     │                                                 │                     │                                                    ┆                                              │                     │                                                 │                     │                                              ┆                  │                                             ┆                                              │                     │                                                 │                     │                                                    ┆                                              │                     │                                                 │                     │                                              ┆                  │                                             ┆                                              │                     │                                                 │                     │                                                    ┆                                              │                     │                                                 │                     │                                              ┆                  │                                             ┆                                              │                     │                                                 │                     │                                                    ┆                                              │                     │                                                 │                     │                                              ┆                  │                                             ┆                                              │                     │                                                 │                     │                                                    ┆                                              │                     │                                                 │                     │                                              ┆                  │                                             ┆                                              │                     │                                                 │                     │                                                    ┆                                              │                     │                                                 │                     │                                              ┆                  │                                             ┆                                              │                     │                                                 │                     │                                                                                                                                                   ┆                                              │                     │                                                 │                     │                                              ┆                  │                                             ┆                                              │                     │                                                 │                     │                                                    ┆                                              │                     │                                                 │                     │                                              ┆                  │                                             ┆                                              │                     │                                                 │                     │                                                    ┆                                              │                     │                                                 │                     │                                              ┆                  │                                             ┆                                              │                     │                                                 │                     │                                                    ┆                                              │                     │                                                 │                     │                                              ┆                  │                                             ┆                                              │                     │                                                 │                     │                                                    ┆                                              │                     │                                                 │                     │                                              ┆                  │                                             ┆                                              │                     │                                                 │                     │                                                    ┆                                              │                     │                                                 │                     │                                              ┆                  │                                             ┆                                              │                     │                                                 │                     │                                                    ┆                                              │                     │                                                 │                     │                                              ┆                  │                                             ┆                                              │                     │                                                 │                     │                                                    ┆                                              │                     │                                                 │                     │                                              ┆                  │                                             ┆                                              │                     │                                                 │                     │                                                                                                                                                                                                                                  ┆                                              │                     │                                                 │                     │                                              ┆                  │                                             ┆                                              │                     │                                                 │                     │                                                    ┆                                              │                     │                                                 │                     │                                              ┆                  │                                             ┆                                              │                     │                                                 │                     │                                                    ┆                                              │                     │                                                 │                     │                                              ┆                  │                                             ┆                                              │                     │                                                 │                     │                                                    ┆                                              │                     │                                                 │                     │                                              ┆                  │                                             ┆                                              │                     │                                                 │                     │                                                                                                                                                                                                                                                                                                                 ┆                                              │                     │                                                 │                     │                                              ┆                  │                                             ┆                                              │                     │                                                 │                     │                                                    ┆                                              │                     │                                                 │                     │                                              ┆                  │                                             ┆                                              │                     │                                                 │                     │                                                                                                                                                                                                                                                                                                                                                                                                ┆                                              │                     │                                                 │                     │                                              ┆                  │                                             ┆                                              │                     │                                                 │                     │
        q_1    ─ [[ ─── [StateInitialisation_16] ──── H ────────────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── X ──── H ─── ]] ──── Z ─── [[ ─── [StateInitialisation_17] ──── H ──── X ─────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── H ────────── ]] ──── X ─── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ──── X ─── [[ ─── [StateInitialisation_16] ──── H ────────────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── X ──── H ─── ]] ──── Z ─── [[ ─── [StateInitialisation_17] ──── H ──── X ─────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── H ────────── ]] ──── X ─── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ──── X ─── [[ ─── [StateInitialisation_16] ──── H ────────────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── X ──── H ─── ]] ──── Z ─── [[ ─── [StateInitialisation_17] ──── H ──── X ─────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── H ────────── ]] ──── X ─── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ──── X ─── [[ ─── [StateInitialisation_16] ──── H ────────────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── X ──── H ─── ]] ──── Z ─── [[ ─── [StateInitialisation_17] ──── H ──── X ─────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── H ────────── ]] ──── X ─── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ──── X ─── [[ ─── [StateInitialisation_16] ──── H ────────────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── X ──── H ─── ]] ──── Z ─── [[ ─── [StateInitialisation_17] ──── H ──── X ─────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── H ────────── ]] ──── X ─── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ──── X ─── [[ ─── [StateInitialisation_16] ──── H ────────────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── X ──── H ─── ]] ──── Z ─── [[ ─── [StateInitialisation_17] ──── H ──── X ─────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── H ────────── ]] ──── X ─── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ──── X ─── [[ ─── [StateInitialisation_16] ──── H ────────────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── X ──── H ─── ]] ──── Z ─── [[ ─── [StateInitialisation_17] ──── H ──── X ─────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── H ────────── ]] ──── X ─── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ──── X ─── [[ ─── [StateInitialisation_16] ──── H ────────────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── X ──── H ─── ]] ──── Z ─── [[ ─── [StateInitialisation_17] ──── H ──── X ─────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── H ────────── ]] ──── X ─── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ──── X ─── [[ ─── [StateInitialisation_16] ──── H ────────────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── X ──── H ─── ]] ──── Z ─── [[ ─── [StateInitialisation_17] ──── H ──── X ─────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── H ────────── ]] ──── X ─── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ──── X ─── [[ ─── [StateInitialisation_16] ──── H ────────────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── X ──── H ─── ]] ──── Z ─── [[ ─── [StateInitialisation_17] ──── H ──── X ─────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── H ────────── ]] ──── X ─── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ──── X ─── [[ ─── [StateInitialisation_16] ──── H ────────────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── X ──── H ─── ]] ──── Z ─── [[ ─── [StateInitialisation_17] ──── H ──── X ─────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── H ────────── ]] ──── X ─── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ──── X ─── [[ ─── [StateInitialisation_16] ──── H ────────────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── X ──── H ─── ]] ──── Z ─── [[ ─── [StateInitialisation_17] ──── H ──── X ─────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── H ────────── ]] ──── X ─── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ──── X ─── [[ ─── [StateInitialisation_16] ──── H ────────────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── X ──── H ─── ]] ──── Z ─── [[ ─── [StateInitialisation_17] ──── H ──── X ─────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── H ────────── ]] ──── X ─── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ──── X ─── [[ ─── [StateInitialisation_16] ──── H ────────────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── X ──── H ─── ]] ──── Z ─── [[ ─── [StateInitialisation_17] ──── H ──── X ─────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── H ────────── ]] ──── X ─── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ──── X ─── [[ ─── [StateInitialisation_16] ──── H ────────────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── X ──── H ─── ]] ──── Z ─── [[ ─── [StateInitialisation_17] ──── H ──── X ─────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── H ────────── ]] ──── X ─── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ──── X ─── [[ ─── [StateInitialisation_16] ──── H ────────────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── X ──── H ─── ]] ──── Z ─── [[ ─── [StateInitialisation_17] ──── H ──── X ─────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── H ────────── ]] ──── X ─── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ──── X ─── [[ ─── [StateInitialisation_16] ──── H ────────────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── X ──── H ─── ]] ─────────────────────────────────────────────────────────────────────────────────────────────────── Z ─── [[ ─── [StateInitialisation_17] ──── H ──── X ─────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── H ────────── ]] ──── X ─── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ──── X ─── [[ ─── [StateInitialisation_16] ──── H ────────────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── X ──── H ─── ]] ──── Z ─── [[ ─── [StateInitialisation_17] ──── H ──── X ─────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── H ────────── ]] ──── X ─── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ──── X ─── [[ ─── [StateInitialisation_16] ──── H ────────────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── X ──── H ─── ]] ──── Z ─── [[ ─── [StateInitialisation_17] ──── H ──── X ─────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── H ────────── ]] ──── X ─── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ──── X ─── [[ ─── [StateInitialisation_16] ──── H ────────────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── X ──── H ─── ]] ──── Z ─── [[ ─── [StateInitialisation_17] ──── H ──── X ─────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── H ────────── ]] ──── X ─── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ──── X ─── [[ ─── [StateInitialisation_16] ──── H ────────────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── X ──── H ─── ]] ──── Z ─── [[ ─── [StateInitialisation_17] ──── H ──── X ─────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── H ────────── ]] ──── X ─── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ──── X ─── [[ ─── [StateInitialisation_16] ──── H ────────────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── X ──── H ─── ]] ──── Z ─── [[ ─── [StateInitialisation_17] ──── H ──── X ─────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── H ────────── ]] ──── X ─── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ──── X ─── [[ ─── [StateInitialisation_16] ──── H ────────────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── X ──── H ─── ]] ──── Z ─── [[ ─── [StateInitialisation_17] ──── H ──── X ─────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── H ────────── ]] ──── X ─── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ──── X ─── [[ ─── [StateInitialisation_16] ──── H ────────────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── X ──── H ─── ]] ──── Z ─── [[ ─── [StateInitialisation_17] ──── H ──── X ─────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── H ────────── ]] ──── X ─── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ──── X ─── [[ ─── [StateInitialisation_16] ──── H ────────────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── X ──── H ─── ]] ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── Z ─── [[ ─── [StateInitialisation_17] ──── H ──── X ─────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── H ────────── ]] ──── X ─── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ──── X ─── [[ ─── [StateInitialisation_16] ──── H ────────────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── X ──── H ─── ]] ──── Z ─── [[ ─── [StateInitialisation_17] ──── H ──── X ─────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── H ────────── ]] ──── X ─── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ──── X ─── [[ ─── [StateInitialisation_16] ──── H ────────────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── X ──── H ─── ]] ──── Z ─── [[ ─── [StateInitialisation_17] ──── H ──── X ─────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── H ────────── ]] ──── X ─── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ──── X ─── [[ ─── [StateInitialisation_16] ──── H ────────────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── X ──── H ─── ]] ──── Z ─── [[ ─── [StateInitialisation_17] ──── H ──── X ─────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── H ────────── ]] ──── X ─── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ──── X ─── [[ ─── [StateInitialisation_16] ──── H ────────────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── X ──── H ─── ]] ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── Z ─── [[ ─── [StateInitialisation_17] ──── H ──── X ─────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── H ────────── ]] ──── X ─── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ──── X ─── [[ ─── [StateInitialisation_16] ──── H ────────────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── X ──── H ─── ]] ──── Z ─── [[ ─── [StateInitialisation_17] ──── H ──── X ─────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── H ────────── ]] ──── X ─── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ──── X ─── [[ ─── [StateInitialisation_16] ──── H ────────────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── X ──── H ─── ]] ──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── Z ─── [[ ─── [StateInitialisation_17] ──── H ──── X ─────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── H ────────── ]] ──── X ─── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ──── X ─── [[ ─── [StateInitialisation_16] ──── H ────────────────────────── ● ─────────────────── ● ──── X ──────────────────────────────────────── ● ─────────────────── ● ──── X ──── H ─── ]] ──── |0〉 ──
                                                                                                                                                                                                             │                                                                                                                                                                                                                    ┆                  │                                                                                                                                                                                                                      │                                                                                                                                                                                                                    ┆                  │                                                                                                                                                                                                                      │                                                                                                                                                                                                                    ┆                  │                                                                                                                                                                                                                      │                                                                                                                                                                                                                    ┆                  │                                                                                                                                                                                                                      │                                                                                                                                                                                                                    ┆                  │                                                                                                                                                                                                                      │                                                                                                                                                                                                                    ┆                  │                                                                                                                                                                                                                      │                                                                                                                                                                                                                    ┆                  │                                                                                                                                                                                                                      │                                                                                                                                                                                                                    ┆                  │                                                                                                                                                                                                                      │                                                                                                                                                                                                                    ┆                  │                                                                                                                                                                                                                      │                                                                                                                                                                                                                    ┆                  │                                                                                                                                                                                                                      │                                                                                                                                                                                                                    ┆                  │                                                                                                                                                                                                                      │                                                                                                                                                                                                                    ┆                  │                                                                                                                                                                                                                      │                                                                                                                                                                                                                    ┆                  │                                                                                                                                                                                                                      │                                                                                                                                                                                                                    ┆                  │                                                                                                                                                                                                                      │                                                                                                                                                                                                                    ┆                  │                                                                                                                                                                                                                      │                                                                                                                                                                                                                    ┆                  │                                                                                                                                                                                                                                                                                                                     │                                                                                                                                                                                                                    ┆                  │                                                                                                                                                                                                                      │                                                                                                                                                                                                                    ┆                  │                                                                                                                                                                                                                      │                                                                                                                                                                                                                    ┆                  │                                                                                                                                                                                                                      │                                                                                                                                                                                                                    ┆                  │                                                                                                                                                                                                                      │                                                                                                                                                                                                                    ┆                  │                                                                                                                                                                                                                      │                                                                                                                                                                                                                    ┆                  │                                                                                                                                                                                                                      │                                                                                                                                                                                                                    ┆                  │                                                                                                                                                                                                                      │                                                                                                                                                                                                                    ┆                  │                                                                                                                                                                                                                                                                                                                                                                                                    │                                                                                                                                                                                                                    ┆                  │                                                                                                                                                                                                                      │                                                                                                                                                                                                                    ┆                  │                                                                                                                                                                                                                      │                                                                                                                                                                                                                    ┆                  │                                                                                                                                                                                                                      │                                                                                                                                                                                                                    ┆                  │                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   │                                                                                                                                                                                                                    ┆                  │                                                                                                                                                                                                                      │                                                                                                                                                                                                                    ┆                  │                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  │                                                                                                                                                                                                                    ┆                  │
        q_2    ── H ──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── ● ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── ● ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── ● ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── ● ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── ● ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── ● ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── ● ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── ● ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── ● ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── ● ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── ● ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── ● ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── ● ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── ● ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── ● ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── ● ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ──── H ──── M ──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── |0〉 ──── H ─── [[ ──── [check (c_0 = |1〉)] ─── [[ ─── [true] ─── Rz(-1.5708) ── ]] ─── ]] ──── ● ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── ● ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── ● ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── ● ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── ● ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── ● ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── ● ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── ● ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ──── H ──── M ──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── |0〉 ──── H ─── [[ ──── [check (c_0 = |1〉)] ─── [[ ─── [true] ─── Rz(-0.7854) ── ]] ─── ]] ─── [[ ──── [check (c_1 = |1〉)] ─── [[ ─── [true] ─── Rz(-1.5708) ── ]] ─── ]] ──── ● ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── ● ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── ● ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── ● ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ──── H ──── M ──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── |0〉 ──── H ─── [[ ──── [check (c_0 = |1〉)] ─── [[ ─── [true] ─── Rz(-0.3927) ── ]] ─── ]] ─── [[ ──── [check (c_1 = |1〉)] ─── [[ ─── [true] ─── Rz(-0.7854) ── ]] ─── ]] ─── [[ ──── [check (c_2 = |1〉)] ─── [[ ─── [true] ─── Rz(-1.5708) ── ]] ─── ]] ──── ● ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── ● ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ──── H ──── M ──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── |0〉 ──── H ─── [[ ──── [check (c_0 = |1〉)] ─── [[ ─── [true] ─── Rz(-0.1963) ── ]] ─── ]] ─── [[ ──── [check (c_1 = |1〉)] ─── [[ ─── [true] ─── Rz(-0.3927) ── ]] ─── ]] ─── [[ ──── [check (c_2 = |1〉)] ─── [[ ─── [true] ─── Rz(-0.7854) ── ]] ─── ]] ─── [[ ──── [check (c_3 = |1〉)] ─── [[ ─── [true] ─── Rz(-1.5708) ── ]] ─── ]] ──── ● ───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── [[ ─── [GOracle_33] ─────────── ● ────────── ]] ──── H ──── M ──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────── |0〉 ──
                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  ╘═════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════ ● ══════════════════════ ● ═════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════╪═════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════ ● ══════════════════════ ● ════════════════════════════════════════════════════╪════════════════════════╪══════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════╪═════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════ ● ══════════════════════ ● ════════════════════════════════════════════════════╪════════════════════════╪═════════════════════════════════════════════════════╪════════════════════════╪════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════╪═════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════ ● ══════════════════════ ● ════════════════════════════════════════════════════╪════════════════════════╪═════════════════════════════════════════════════════╪════════════════════════╪═════════════════════════════════════════════════════╪════════════════════════╪═════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════╪════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════
                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         ╘════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════ ● ══════════════════════ ● ═════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════╪════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════ ● ══════════════════════ ● ════════════════════════════════════════════════════╪════════════════════════╪════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════╪════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════ ● ══════════════════════ ● ════════════════════════════════════════════════════╪════════════════════════╪═════════════════════════════════════════════════════╪════════════════════════╪═════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════╪════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════
                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   ╘═══════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════ ● ══════════════════════ ● ═══════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════╪═══════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════ ● ══════════════════════ ● ════════════════════════════════════════════════════╪════════════════════════╪═════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════╪════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════
                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              ╘══════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════ ● ══════════════════════ ● ════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════╪════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════
                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         ╘════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════
    "#]].assert_eq(&circ.to_string());
}

const DOT_PRODUCT_PHASE_ESTIMATION: &str = r#"
        import Std.Math.*;
        import Std.Convert.*;

        operation Main() : (Int, Int) {
            // The angles for inner product. Inner product is meeasured for vectors
            // (cos(Θ₁/2), sin(Θ₁/2)) and (cos(Θ₂/2), sin(Θ₂/2)).
            let theta1 = PI() / 7.0;
            let theta2 = PI() / 5.0;
            // Number of iterations
            let n = 4;
            // Perform measurements
            Message("Computing inner product of vectors (cos(Θ₁/2), sin(Θ₁/2))⋅(cos(Θ₂/2), sin(Θ₂/2)) ≈ -cos(x𝝅/2ⁿ)");
            let result = PerformMeasurements(theta1, theta2, n);
            // Return result
            return (result, n);
        }

        @Config(Adaptive)
        @Config(not HigherLevelConstructs)
        @Config(not FloatingPointComputations)
        operation PerformMeasurements(theta1 : Double, theta2 : Double, n : Int) : Int {
            let measurementCount = n + 1;
            return QuantumInnerProduct(theta1, theta2, measurementCount);
        }

        @Config(HigherLevelConstructs)
        @Config(FloatingPointComputations)
        operation PerformMeasurements(theta1 : Double, theta2 : Double, n : Int) : Int {
            Message($"Θ₁={theta1}, Θ₂={theta2}.");

            // First compute quantum approximation
            let measurementCount = n + 1;
            let x = QuantumInnerProduct(theta1, theta2, measurementCount);
            let angle = PI() * IntAsDouble(x) / IntAsDouble(2^n);
            let computedInnerProduct = -Cos(angle);
            Message($"x = {x}, n = {n}.");

            // Now compute true inner product
            let trueInnterProduct = ClassicalInnerProduct(theta1, theta2);

            Message($"Computed value = {computedInnerProduct}, true value = {trueInnterProduct}");

            return x;
        }

        function ClassicalInnerProduct(theta1 : Double, theta2 : Double) : Double {
            return Cos(theta1 / 2.0) * Cos(theta2 / 2.0) + Sin(theta1 / 2.0) * Sin(theta2 / 2.0);
        }

        operation QuantumInnerProduct(theta1 : Double, theta2 : Double, iterationCount : Int) : Int {
            //Create target register
            use TargetReg = Qubit();
            //Create ancilla register
            use AncilReg = Qubit();
            //Run iterative phase estimation
            let Results = IterativePhaseEstimation(TargetReg, AncilReg, theta1, theta2, iterationCount);
            Reset(TargetReg);
            Reset(AncilReg);
            return Results;
        }

        operation IterativePhaseEstimation(
            TargetReg : Qubit,
            AncilReg : Qubit,
            theta1 : Double,
            theta2 : Double,
            Measurements : Int
        ) : Int {

            use ControlReg = Qubit();
            mutable MeasureControlReg = [Zero, size = Measurements];
            mutable bitValue = 0;
            //Apply to initialise state, this is defined by the angles theta1 and theta2
            StateInitialisation(TargetReg, AncilReg, theta1, theta2);
            for index in 0..Measurements - 1 {
                H(ControlReg);
                //Don't apply rotation on first set of oracles
                if index > 0 {
                    //Loop through previous results
                    for index2 in 0..index - 1 {
                        if MeasureControlReg[Measurements - 1 - index2] == One {
                            //Rotate control qubit dependent on previous measurements and number of measurements
                            let angle = -IntAsDouble(2^(index2)) * PI() / (2.0^IntAsDouble(index));
                            R(PauliZ, angle, ControlReg);
                        }
                    }

                }
                let powerIndex = (1 <<< (Measurements - 1 - index));
                //Apply a number of oracles equal to 2^index, where index is the number or measurements left
                for _ in 1..powerIndex {
                    Controlled GOracle([ControlReg], (TargetReg, AncilReg, theta1, theta2));
                }
                H(ControlReg);
                //Make a measurement mid circuit
                set MeasureControlReg w/= (Measurements - 1 - index) <- MResetZ(ControlReg);
                if MeasureControlReg[Measurements - 1 - index] == One {
                    //Assign bitValue based on previous measurement
                    bitValue += 2^(index);
                }
            }
            return bitValue;
        }

        /// # Summary
        /// This is state preperation operator A for encoding the 2D vector (page 7)
        operation StateInitialisation(
            TargetReg : Qubit,
            AncilReg : Qubit,
            theta1 : Double,
            theta2 : Double
        ) : Unit is Adj + Ctl {

            H(AncilReg);
            // Arbitrary controlled rotation based on theta. This is vector v.
            Controlled R([AncilReg], (PauliY, theta1, TargetReg));
            // X gate on ancilla to change from |+〉 to |-〉.
            X(AncilReg);
            // Arbitrary controlled rotation based on theta. This is vector c.
            Controlled R([AncilReg], (PauliY, theta2, TargetReg));
            X(AncilReg);
            H(AncilReg);
        }

        operation GOracle(
            TargetReg : Qubit,
            AncilReg : Qubit,
            theta1 : Double,
            theta2 : Double
        ) : Unit is Adj + Ctl {

            Z(AncilReg);
            within {
                Adjoint StateInitialisation(TargetReg, AncilReg, theta1, theta2);
                X(AncilReg);
                X(TargetReg);
            } apply {
                Controlled Z([AncilReg], TargetReg);
            }
        }

    "#;

#[test]
fn dynamics_small() {
    let circ = circuit(
        DYNAMICS_SMALL,
        CircuitEntryPoint::EntryPoint,
        Config::default(),
    )
    .expect("circuit generation should succeed");
    expect![[r#"
        q_0    ─ Rx(0.3730) ─── [[ ─── [ApplyDoubleZ_22(0.7461)] ───────────────── Rzz(0.7461) ── Rzz(0.7461) ── ]] ─── Rx(0.7461) ─── [[ ─── [ApplyDoubleZ_22(0.7461)] ───────────────── Rzz(0.7461) ── Rzz(0.7461) ── ]] ─── Rx(-0.2191) ── [[ ─── [ApplyDoubleZ_22(0.5922)] ───────────────── Rzz(0.5922) ── Rzz(0.5922) ── ]] ─── Rx(-0.2191) ── [[ ─── [ApplyDoubleZ_22(0.7461)] ───────────────── Rzz(0.7461) ── Rzz(0.7461) ── ]] ─── Rx(0.7461) ─── [[ ─── [ApplyDoubleZ_22(0.7461)] ───────────────── Rzz(0.7461) ── Rzz(0.7461) ── ]] ─── Rx(0.7461) ─── [[ ─── [ApplyDoubleZ_22(0.7461)] ───────────────── Rzz(0.7461) ── Rzz(0.7461) ── ]] ─── Rx(0.7461) ─── [[ ─── [ApplyDoubleZ_22(0.7461)] ───────────────── Rzz(0.7461) ── Rzz(0.7461) ── ]] ─── Rx(-0.2191) ── [[ ─── [ApplyDoubleZ_22(0.5922)] ───────────────── Rzz(0.5922) ── Rzz(0.5922) ── ]] ─── Rx(-0.2191) ── [[ ─── [ApplyDoubleZ_22(0.7461)] ───────────────── Rzz(0.7461) ── Rzz(0.7461) ── ]] ─── Rx(0.7461) ─── [[ ─── [ApplyDoubleZ_22(0.7461)] ───────────────── Rzz(0.7461) ── Rzz(0.7461) ── ]] ─── Rx(0.3730) ──
                                                   ┆                                    ┆              ┆                                                  ┆                                    ┆              ┆                                                  ┆                                    ┆              ┆                                                  ┆                                    ┆              ┆                                                  ┆                                    ┆              ┆                                                  ┆                                    ┆              ┆                                                  ┆                                    ┆              ┆                                                  ┆                                    ┆              ┆                                                  ┆                                    ┆              ┆                                                  ┆                                    ┆              ┆
        q_1    ─ Rx(0.3730) ─── [[ ─── [ApplyDoubleZ_22(0.7461)] ── Rzz(0.7461) ────────┆──────── Rzz(0.7461) ── ]] ─── Rx(0.7461) ─── [[ ─── [ApplyDoubleZ_22(0.7461)] ── Rzz(0.7461) ────────┆──────── Rzz(0.7461) ── ]] ─── Rx(-0.2191) ── [[ ─── [ApplyDoubleZ_22(0.5922)] ── Rzz(0.5922) ────────┆──────── Rzz(0.5922) ── ]] ─── Rx(-0.2191) ── [[ ─── [ApplyDoubleZ_22(0.7461)] ── Rzz(0.7461) ────────┆──────── Rzz(0.7461) ── ]] ─── Rx(0.7461) ─── [[ ─── [ApplyDoubleZ_22(0.7461)] ── Rzz(0.7461) ────────┆──────── Rzz(0.7461) ── ]] ─── Rx(0.7461) ─── [[ ─── [ApplyDoubleZ_22(0.7461)] ── Rzz(0.7461) ────────┆──────── Rzz(0.7461) ── ]] ─── Rx(0.7461) ─── [[ ─── [ApplyDoubleZ_22(0.7461)] ── Rzz(0.7461) ────────┆──────── Rzz(0.7461) ── ]] ─── Rx(-0.2191) ── [[ ─── [ApplyDoubleZ_22(0.5922)] ── Rzz(0.5922) ────────┆──────── Rzz(0.5922) ── ]] ─── Rx(-0.2191) ── [[ ─── [ApplyDoubleZ_22(0.7461)] ── Rzz(0.7461) ────────┆──────── Rzz(0.7461) ── ]] ─── Rx(0.7461) ─── [[ ─── [ApplyDoubleZ_22(0.7461)] ── Rzz(0.7461) ────────┆──────── Rzz(0.7461) ── ]] ─── Rx(0.3730) ──
                                                   ┆                     ┆              ┆                                                                 ┆                     ┆              ┆                                                                 ┆                     ┆              ┆                                                                 ┆                     ┆              ┆                                                                 ┆                     ┆              ┆                                                                 ┆                     ┆              ┆                                                                 ┆                     ┆              ┆                                                                 ┆                     ┆              ┆                                                                 ┆                     ┆              ┆                                                                 ┆                     ┆              ┆
        q_2    ─ Rx(0.3730) ─── [[ ─── [ApplyDoubleZ_22(0.7461)] ────────┆──────── Rzz(0.7461) ── Rzz(0.7461) ── ]] ─── Rx(0.7461) ─── [[ ─── [ApplyDoubleZ_22(0.7461)] ────────┆──────── Rzz(0.7461) ── Rzz(0.7461) ── ]] ─── Rx(-0.2191) ── [[ ─── [ApplyDoubleZ_22(0.5922)] ────────┆──────── Rzz(0.5922) ── Rzz(0.5922) ── ]] ─── Rx(-0.2191) ── [[ ─── [ApplyDoubleZ_22(0.7461)] ────────┆──────── Rzz(0.7461) ── Rzz(0.7461) ── ]] ─── Rx(0.7461) ─── [[ ─── [ApplyDoubleZ_22(0.7461)] ────────┆──────── Rzz(0.7461) ── Rzz(0.7461) ── ]] ─── Rx(0.7461) ─── [[ ─── [ApplyDoubleZ_22(0.7461)] ────────┆──────── Rzz(0.7461) ── Rzz(0.7461) ── ]] ─── Rx(0.7461) ─── [[ ─── [ApplyDoubleZ_22(0.7461)] ────────┆──────── Rzz(0.7461) ── Rzz(0.7461) ── ]] ─── Rx(-0.2191) ── [[ ─── [ApplyDoubleZ_22(0.5922)] ────────┆──────── Rzz(0.5922) ── Rzz(0.5922) ── ]] ─── Rx(-0.2191) ── [[ ─── [ApplyDoubleZ_22(0.7461)] ────────┆──────── Rzz(0.7461) ── Rzz(0.7461) ── ]] ─── Rx(0.7461) ─── [[ ─── [ApplyDoubleZ_22(0.7461)] ────────┆──────── Rzz(0.7461) ── Rzz(0.7461) ── ]] ─── Rx(0.3730) ──
                                                   ┆                     ┆                             ┆                                                  ┆                     ┆                             ┆                                                  ┆                     ┆                             ┆                                                  ┆                     ┆                             ┆                                                  ┆                     ┆                             ┆                                                  ┆                     ┆                             ┆                                                  ┆                     ┆                             ┆                                                  ┆                     ┆                             ┆                                                  ┆                     ┆                             ┆                                                  ┆                     ┆                             ┆
        q_3    ─ Rx(0.3730) ─── [[ ─── [ApplyDoubleZ_22(0.7461)] ── Rzz(0.7461) ───────────────── Rzz(0.7461) ── ]] ─── Rx(0.7461) ─── [[ ─── [ApplyDoubleZ_22(0.7461)] ── Rzz(0.7461) ───────────────── Rzz(0.7461) ── ]] ─── Rx(-0.2191) ── [[ ─── [ApplyDoubleZ_22(0.5922)] ── Rzz(0.5922) ───────────────── Rzz(0.5922) ── ]] ─── Rx(-0.2191) ── [[ ─── [ApplyDoubleZ_22(0.7461)] ── Rzz(0.7461) ───────────────── Rzz(0.7461) ── ]] ─── Rx(0.7461) ─── [[ ─── [ApplyDoubleZ_22(0.7461)] ── Rzz(0.7461) ───────────────── Rzz(0.7461) ── ]] ─── Rx(0.7461) ─── [[ ─── [ApplyDoubleZ_22(0.7461)] ── Rzz(0.7461) ───────────────── Rzz(0.7461) ── ]] ─── Rx(0.7461) ─── [[ ─── [ApplyDoubleZ_22(0.7461)] ── Rzz(0.7461) ───────────────── Rzz(0.7461) ── ]] ─── Rx(-0.2191) ── [[ ─── [ApplyDoubleZ_22(0.5922)] ── Rzz(0.5922) ───────────────── Rzz(0.5922) ── ]] ─── Rx(-0.2191) ── [[ ─── [ApplyDoubleZ_22(0.7461)] ── Rzz(0.7461) ───────────────── Rzz(0.7461) ── ]] ─── Rx(0.7461) ─── [[ ─── [ApplyDoubleZ_22(0.7461)] ── Rzz(0.7461) ───────────────── Rzz(0.7461) ── ]] ─── Rx(0.3730) ──
    "#]].assert_eq(&circ.to_string());
}

const DYNAMICS_SMALL: &str = r#"
        import Std.Math.*;
        import Std.Arrays.*;

        operation Main() : Unit {
            // n : Int, m : Int, t: Double, u : Double, tstep : Double

            let n = 2;
            let m = 2;

            let J = 1.0;
            let g = 1.0;

            let totTime = 1.0;
            let dt = 0.9;

            IsingModel2DSim(n, m, J, g, totTime, dt);
        }

        /// # Summary
        /// The function below creates a sequence containing the rotation angles that will be applied with the two operators used in the expansion of the Trotter-Suzuki formula.
        /// # Input
        /// ## p (Double) : Constant used for fourth-order formulas
        ///
        /// ## dt (Double) : Time-step used to compute rotation angles
        ///
        /// ## J (Double) : coefficient for 2-qubit interactions
        ///
        /// ## g (Double) : coefficient for transverse field
        ///
        /// # Output
        /// ## values (Double[]) : The list of rotation angles to be applies in sequence with the corresponding operators
        ///
        function SetAngleSequence(p : Double, dt : Double, J : Double, g : Double) : Double[] {

            let len1 = 3;
            let len2 = 3;
            let valLength = 2 * len1 + len2 + 1;
            mutable values = [0.0, size = valLength];

            let val1 = J * p * dt;
            let val2 = -g * p * dt;
            let val3 = J * (1.0 - 3.0 * p) * dt / 2.0;
            let val4 = g * (1.0 - 4.0 * p) * dt / 2.0;

            for i in 0..len1 {

                if (i % 2 == 0) {
                    set values w/= i <- val1;
                } else {
                    set values w/= i <- val2;
                }

            }

            for i in len1 + 1..len1 + len2 {
                if (i % 2 == 0) {
                    set values w/= i <- val3;
                } else {
                    set values w/= i <- val4;
                }
            }

            for i in len1 + len2 + 1..valLength - 1 {
                if (i % 2 == 0) {
                    set values w/= i <- val1;
                } else {
                    set values w/= i <- val2;
                }
            }
            return values;
        }

        /// # Summary
        /// Applies e^-iX(theta) on all qubits in the 2D lattice as part of simulating the transverse field in the Ising model
        /// # Input
        /// ## n (Int) : Lattice size for an n x n lattice
        ///
        /// ## qArr (Qubit[][]) : Array of qubits representing the lattice
        ///
        /// ## theta (Double) : The angle/time-step for which the unitary simulation is done.
        ///
        operation ApplyAllX(n : Int, qArr : Qubit[][], theta : Double) : Unit {
            // This applies `Rx` with an angle of `2.0 * theta` to all qubits in `qs`
            // using partial application
            for row in 0..n - 1 {
                ApplyToEach(Rx(2.0 * theta, _), qArr[row]);
            }
        }

        /// # Summary
        /// Applies e^-iP(theta) where P = Z o Z as part of the repulsion terms.
        /// # Input
        /// ## n, m (Int, Int) : Lattice sizes for an n x m lattice
        ///
        /// ## qArr (Qubit[]) : Array of qubits representing the lattice
        ///
        /// ## theta (Double) : The angle/time-step for which unitary simulation is done.
        ///
        /// ## dir (Bool) : Direction is true for vertical direction.
        ///
        /// ## grp (Bool) : Group is true for odd starting indices
        ///
        operation ApplyDoubleZ(n : Int, m : Int, qArr : Qubit[][], theta : Double, dir : Bool, grp : Bool) : Unit {
            let start = grp ? 1 | 0;    // Choose either odd or even indices based on group number
            let P_op = [PauliZ, PauliZ];
            let c_end = dir ? m - 1 | m - 2;
            let r_end = dir ? m - 2 | m - 1;

            for row in 0..r_end {
                for col in start..2..c_end {
                    // Iterate through even or odd columns based on `grp`

                    let row2 = dir ? row + 1 | row;
                    let col2 = dir ? col | col + 1;

                    Exp(P_op, theta, [qArr[row][col], qArr[row2][col2]]);
                }
            }
        }

        /// # Summary
        /// The main function that takes in various parameters and calls the operations needed to simulate fourth order Trotterizatiuon of the Ising Hamiltonian for a given time-step
        /// # Input
        /// ## N1, N2 (Int, Int) : Lattice sizes for an N1 x N2 lattice
        ///
        /// ## J (Double) : coefficient for 2-qubit interactions
        ///
        /// ## g (Double) : coefficient for transverse field
        ///
        /// ## totTime (Double) : The total time-step for which unitary simulation is done.
        ///
        /// ## dt (Double) : The time the simulation is done for each timestep
        ///
        operation IsingModel2DSim(N1 : Int, N2 : Int, J : Double, g : Double, totTime : Double, dt : Double) : Unit {

            use qs = Qubit[N1 * N2];
            let qubitArray = Chunks(N2, qs); // qubits are re-arranged to be in an N1 x N2 array

            let p = 1.0 / (4.0 - 4.0^(1.0 / 3.0));
            let t = Ceiling(totTime / dt);

            let seqLen = 10 * t + 1;

            let angSeq = SetAngleSequence(p, dt, J, g);

            for i in 0..seqLen - 1 {
                let theta = (i == 0 or i == seqLen - 1) ? J * p * dt / 2.0 | angSeq[i % 10];

                // for even indexes
                if i % 2 == 0 {
                    ApplyAllX(N1, qubitArray, theta);
                } else {
                    // iterate through all possible combinations for `dir` and `grp`.
                    for (dir, grp) in [(true, true), (true, false), (false, true), (false, false)] {
                        ApplyDoubleZ(N1, N2, qubitArray, theta, dir, grp);
                    }
                }
            }
        }

    "#;

/// Tests that invoke circuit generation throught the debugger.
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
