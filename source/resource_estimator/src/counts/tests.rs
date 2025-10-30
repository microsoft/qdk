// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::convert::Into;

use expect_test::{Expect, expect};
use indoc::indoc;
use miette::Report;
use qsc::{
    LanguageFeatures, PackageType, SourceMap, TargetCapabilityFlags,
    interpret::{GenericReceiver, Interpreter},
    target::Profile,
};

use super::LogicalCounter;

fn verify_logical_counts(source: &str, entry: Option<&str>, expect: &Expect) {
    let source_map = SourceMap::new([("test".into(), source.into())], entry.map(Into::into));
    let (std_id, store) = qsc::compile::package_store_with_stdlib(TargetCapabilityFlags::all());

    let mut interpreter = match Interpreter::new(
        source_map,
        PackageType::Exe,
        Profile::Unrestricted.into(),
        LanguageFeatures::default(),
        store,
        &[(std_id, None)],
    ) {
        Ok(interpreter) => interpreter,
        Err(err) => {
            for e in err {
                let report = Report::from(e);
                eprintln!("{report:?}");
            }
            panic!("compilation failed");
        }
    };
    let mut counter = LogicalCounter::default();
    let mut stdout = std::io::sink();
    let mut out = GenericReceiver::new(&mut stdout);

    match interpreter.eval_entry_with_sim(&mut counter, &mut out) {
        Ok(_) => {
            expect.assert_debug_eq(&counter.logical_resources());
        }
        Err(err) => {
            for e in err {
                let report = Report::from(e);
                eprintln!("{report:?}");
            }
            panic!("evaluation failed");
        }
    }
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
                num_compute_qubits: None,
                read_from_memory_count: None,
                write_to_memory_count: None,
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
                num_compute_qubits: None,
                read_from_memory_count: None,
                write_to_memory_count: None,
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
                num_compute_qubits: None,
                read_from_memory_count: None,
                write_to_memory_count: None,
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
                num_compute_qubits: None,
                read_from_memory_count: None,
                write_to_memory_count: None,
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
                num_compute_qubits: None,
                read_from_memory_count: None,
                write_to_memory_count: None,
            }
        "#]],
    );
}

#[test]
fn memory_annotations_work() {
    verify_logical_counts(
        indoc! {"
            namespace Test {
                import Std.Convert.*;
                import Std.Math.*;
                import Std.ResourceEstimation.*;

                @EntryPoint()
                operation Main() : Unit {
                    EnableMemoryComputeArchitecture(10, LeastRecentlyUsed());

                    use controls = Qubit[3];
                    use targets = Qubit[8];
                    use rotations = Qubit[8];

                    for i in 0..7 {
                        ApplyControlledOnInt(i, X, controls, targets[i]);
                    }

                    for i in 0..7 {
                        Controlled Rz([targets[i]], ((PI() / 4.0) * IntAsDouble(i), rotations[i]));
                    }

                    ResetAll(controls + targets + rotations);
                }
            }
        "},
        None,
        &expect![[r#"
            LogicalResourceCounts {
                num_qubits: 20,
                t_count: 4,
                rotation_count: 8,
                rotation_depth: 5,
                ccz_count: 16,
                ccix_count: 0,
                measurement_count: 8,
                num_compute_qubits: Some(
                    10,
                ),
                read_from_memory_count: Some(
                    28,
                ),
                write_to_memory_count: Some(
                    18,
                ),
            }
        "#]],
    );
}
