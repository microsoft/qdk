// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::convert::Into;

use expect_test::{Expect, expect};
use indoc::indoc;
use qsc::{
    LanguageFeatures, PackageType, SourceMap, TargetCapabilityFlags,
    interpret::{GenericReceiver, Interpreter},
    target::Profile,
};

use super::LogicalCounter;

fn verify_logical_counts(source: &str, entry: Option<&str>, expect: &Expect) {
    let source_map = SourceMap::new([("test".into(), source.into())], entry.map(Into::into));
    let (std_id, store) = qsc::compile::package_store_with_stdlib(TargetCapabilityFlags::all());
    let mut interpreter = Interpreter::new(
        source_map,
        PackageType::Exe,
        Profile::Unrestricted.into(),
        LanguageFeatures::default(),
        store,
        &[(std_id, None)],
    )
    .expect("compilation should succeed");
    let mut counter = LogicalCounter::default();
    let mut stdout = std::io::sink();
    let mut out = GenericReceiver::new(&mut stdout);
    interpreter
        .eval_entry_with_sim(&mut counter, &mut out)
        .expect("evaluation should succeed");
    expect.assert_debug_eq(&counter.logical_resources());
}

#[test]
fn gates_are_counted() {
    verify_logical_counts(
        indoc! {"
            namespace Test {
                operation Rotate(qs: Qubit[]) : Unit {
                    for q in qs {
                        Rx(1.0, q);
                        Ry(1.0, q);
                        Rz(1.0, q);
                    }
                }

                @EntryPoint()
                operation Main() : Result[] {
                    use qs = Qubit[10];
                    within {
                        T(qs[0]);
                        CCNOT(qs[0], qs[1], qs[2]);
                    }
                    apply {
                        Rotate(qs);
                    }
                    MResetEachZ(qs)
                }
            }
        "},
        None,
        &expect![["
            LogicalResourceCounts {
                num_qubits: 10,
                t_count: 2,
                rotation_count: 30,
                rotation_depth: 5,
                ccz_count: 2,
                ccix_count: 0,
                measurement_count: 10,
            }
        "]],
    );
}

#[test]
fn estimate_caching_works() {
    verify_logical_counts(
        indoc! {r#"
            namespace Test {
                import Std.ResourceEstimation.*;

                operation Rotate(qs: Qubit[]) : Unit {
                    for q in qs {
                        Rx(1.0, q);
                        Ry(1.0, q);
                        Rz(1.0, q);
                    }
                }

                @EntryPoint()
                operation Main() : Unit {
                    use qs = Qubit[10];
                    mutable count = 0;
                    for _ in 1..10 {
                        if BeginEstimateCaching("Rotate", SingleVariant()) {
                            Rotate(qs);
                            set count += 1;
                            EndEstimateCaching();
                        }
                    }
                    for _ in 1..count {
                        T(qs[0]);
                    }
                }
            }
        "#},
        None,
        &expect![["
            LogicalResourceCounts {
                num_qubits: 10,
                t_count: 1,
                rotation_count: 300,
                rotation_depth: 30,
                ccz_count: 0,
                ccix_count: 0,
                measurement_count: 0,
            }
        "]],
    );
}

#[test]
fn estimate_repeat_works() {
    verify_logical_counts(
        indoc! {r#"
            namespace Test {
                import Std.ResourceEstimation.*;

                operation Rotate(qs: Qubit[]) : Unit {
                    for q in qs {
                        Rx(1.0, q);
                        Ry(1.0, q);
                        Rz(1.0, q);
                    }
                }

                @EntryPoint()
                operation Main() : Unit {
                    use qs = Qubit[10];
                    mutable count = 0;
                    within {
                        RepeatEstimates(10);
                    }
                    apply {
                        Rotate(qs);
                        set count += 1;
                    }
                    for _ in 1..count {
                        T(qs[0]);
                    }
                }
            }
        "#},
        None,
        &expect![[r#"
            LogicalResourceCounts {
                num_qubits: 10,
                t_count: 1,
                rotation_count: 300,
                rotation_depth: 30,
                ccz_count: 0,
                ccix_count: 0,
                measurement_count: 0,
            }
        "#]],
    );
}

#[test]
fn account_for_estimates_works() {
    verify_logical_counts(
        indoc! {"
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
            }
        "},
        None,
        &expect![["
            LogicalResourceCounts {
                num_qubits: 11,
                t_count: 2,
                rotation_count: 3,
                rotation_depth: 1,
                ccz_count: 5,
                ccix_count: 0,
                measurement_count: 6,
            }
        "]],
    );
}

#[test]
fn pauli_i_rotation_for_global_phase_is_noop() {
    verify_logical_counts(
        indoc! {"
            namespace Test {
                @EntryPoint()
                operation Main() : Unit {
                    use q = Qubit();
                    T(q);
                    R(PauliI, 1.0, q);
                }
            }
        "},
        None,
        &expect![[r#"
            LogicalResourceCounts {
                num_qubits: 1,
                t_count: 1,
                rotation_count: 0,
                rotation_depth: 0,
                ccz_count: 0,
                ccix_count: 0,
                measurement_count: 0,
            }
        "#]],
    );
}
