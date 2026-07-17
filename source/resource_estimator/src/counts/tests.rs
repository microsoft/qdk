// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::convert::Into;

use crate::system::LogicalResourceCounts;
use expect_test::{Expect, expect};
use indoc::indoc;
use miette::Report;
use qsc::{
    LanguageFeatures, PackageType, SourceMap, TargetCapabilityFlags,
    interpret::{GenericReceiver, Interpreter, PackageGlobal, Value},
    target::Profile,
};

use crate::logical_counts_call;

use super::LogicalCounter;

fn run_logical_counts_result(
    source: &str,
    entry: Option<&str>,
) -> Result<LogicalResourceCounts, String> {
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
            let mut messages = Vec::new();
            for e in err {
                let report = Report::from(e);
                messages.push(format!("{report:?}"));
            }
            return Err(format!("compilation failed:\n{}", messages.join("\n")));
        }
    };
    let mut counter = LogicalCounter::default();
    let mut stdout = std::io::sink();
    let mut out = GenericReceiver::new(&mut stdout);

    match interpreter.eval_entry_with_sim(&mut counter, &mut out) {
        Ok(_) => Ok(counter.logical_resources()),
        Err(err) => {
            let mut messages = Vec::new();
            for e in err {
                let report = Report::from(e);
                messages.push(format!("{report:?}"));
            }
            Err(format!("evaluation failed:\n{}", messages.join("\n")))
        }
    }
}

fn run_logical_counts(source: &str) -> LogicalResourceCounts {
    run_logical_counts_result(source, None)
        .unwrap_or_else(|err| panic!("failed to compute logical counts: {err}"))
}

fn verify_logical_counts(source: &str, entry: Option<&str>, expect: &Expect) {
    let logical_counts = run_logical_counts_result(source, entry)
        .unwrap_or_else(|err| panic!("failed to compute logical counts: {err}"));
    expect.assert_debug_eq(&logical_counts);
}
fn source_global(interpreter: &Interpreter, name: &str) -> Value {
    interpreter
        .source_globals()
        .into_iter()
        .find_map(
            |PackageGlobal {
                 name: global_name,
                 value,
                 ..
             }| (global_name.as_ref() == name).then_some(value),
        )
        .unwrap_or_else(|| panic!("{name} should be present in source globals"))
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
fn logical_counts_call_counts_callable_with_udt_output() {
    // The callable returns a UDT so stricter backend-preparation paths would
    // impose output-shape constraints here. logical_counts_call should still
    // count gates by invoking the live interpreter directly.
    let source = indoc! {r#"
        namespace Test {
            struct Data {
                tally : Int
            }

            operation Counted() : Data {
                use q = Qubit();
                T(q);
                MResetZ(q);
                new Data { tally = 0 }
            }
        }
    "#};
    let source_map = SourceMap::new([("test".into(), source.into())], None);
    let (std_id, store) = qsc::compile::package_store_with_stdlib(Profile::Base.into());

    let mut interpreter = Interpreter::new(
        source_map,
        PackageType::Lib,
        Profile::Base.into(),
        LanguageFeatures::default(),
        store,
        &[(std_id, None)],
    )
    .expect("compilation should succeed");

    let callable = source_global(&interpreter, "Counted");
    let counts = logical_counts_call(&mut interpreter, callable, Value::unit())
        .expect("logical counting should stay on the live interpreter path");

    expect![[r#"
        LogicalResourceCounts {
            num_qubits: 1,
            t_count: 1,
            rotation_count: 0,
            rotation_depth: 0,
            ccz_count: 0,
            ccix_count: 0,
            measurement_count: 1,
            num_compute_qubits: None,
            read_from_memory_count: None,
            write_to_memory_count: None,
        }
    "#]]
    .assert_debug_eq(&counts);
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
                ccz_count: 24,
                ccix_count: 0,
                measurement_count: 0,
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

#[test]
fn post_selection_to_zero_skips_one_branch() {
    // This shows no T gates are counted because the branch is not taken due
    // to the post-selection.
    verify_logical_counts(
        indoc! {"
                import Std.Diagnostics.PostSelectZ;

                operation Main() : Unit {
                    use q = Qubit();
                    H(q);
                    PostSelectZ(Zero, q);
                    if M(q) == One {
                        T(q);
                    }
                }
            "},
        None,
        &expect![[r#"
            LogicalResourceCounts {
                num_qubits: 1,
                t_count: 0,
                rotation_count: 0,
                rotation_depth: 0,
                ccz_count: 0,
                ccix_count: 0,
                measurement_count: 1,
                num_compute_qubits: None,
                read_from_memory_count: None,
                write_to_memory_count: None,
            }
        "#]],
    );
}

#[test]
fn post_selection_to_one_takes_one_branch() {
    // This shows one T gate is counted because the branch is taken due
    // to the post-selection.
    verify_logical_counts(
        indoc! {"
                import Std.Diagnostics.PostSelectZ;

                operation Main() : Unit {
                    use q = Qubit();
                    H(q);
                    PostSelectZ(One, q);
                    if M(q) == One {
                        T(q);
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
                measurement_count: 1,
                num_compute_qubits: None,
                read_from_memory_count: None,
                write_to_memory_count: None,
            }
        "#]],
    );
}

#[test]
fn post_selection_can_take_impossible_branch() {
    // This shows one T gate is counted because the branch is taken due
    // to the post-selection.
    verify_logical_counts(
        indoc! {"
                import Std.Diagnostics.PostSelectZ;

                operation Main() : Unit {
                    use q = Qubit();
                    PostSelectZ(One, q);
                    if M(q) == One {
                        T(q);
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
                measurement_count: 1,
                num_compute_qubits: None,
                read_from_memory_count: None,
                write_to_memory_count: None,
            }
        "#]],
    );
}

#[test]
fn manual_memory_load_store() {
    let counts = run_logical_counts(indoc! {"
        operation Main() : Unit {
            Std.ResourceEstimation.EnableManualMemoryComputeArchitecture();

            use qs = Qubit[2];
            Std.Memory.Store(qs[0]);
            Std.Memory.Load(qs[0]);
        }
    "});
    assert_eq!(counts.write_to_memory_count, Some(1));
    assert_eq!(counts.read_from_memory_count, Some(1));
    assert_eq!(counts.num_compute_qubits, Some(1));
    assert_eq!(counts.num_qubits, 2);
}

#[test]
fn manual_memory_complex_circuit_counts() {
    let counts = run_logical_counts(indoc! {"
        operation Main() : Unit {
            Std.ResourceEstimation.EnableManualMemoryComputeArchitecture();

            use qs = Qubit[4];

            // Move two qubits to memory.
            Std.Memory.Store(qs[2]);
            Std.Memory.Store(qs[3]);

            // Compute on hot qubits.
            H(qs[0]);
            CNOT(qs[0], qs[1]);

            // Bring one memory qubit back and use it in a 3-qubit gate.
            Std.Memory.Load(qs[2]);
            CCNOT(qs[0], qs[1], qs[2]);

            // Evict another qubit and load a different one.
            Std.Memory.Store(qs[1]);
            Std.Memory.Load(qs[3]);

            // Two-qubit operation on currently loaded qubits.
            Controlled Z([qs[2]], qs[3]);
        }
    "});

    assert_eq!(counts.write_to_memory_count, Some(3));
    assert_eq!(counts.read_from_memory_count, Some(2));
    assert_eq!(counts.num_compute_qubits, Some(3));
    assert_eq!(counts.num_qubits, 5);
    assert_eq!(counts.ccz_count, 1);
}

#[test]
fn manual_memory_rejects_gate_application() {
    let result = run_logical_counts_result(
        indoc! {"
                operation Main() : Unit {
                    Std.ResourceEstimation.EnableManualMemoryComputeArchitecture();

                    use q = Qubit();
                    Std.Memory.Store(q);
                    X(q);
                }
            "},
        None,
    );

    let err = result.expect_err("expected gate application on memory qubit to fail");
    assert!(
        err.contains("cannot perform computation on memory qubit"),
        "unexpected error: {err}"
    );
}

#[test]
fn manual_memory_ghz_sample() {
    let counts = run_logical_counts(indoc! {"
        operation PrepareGhzStateInMemory(qs: Qubit[]) : Unit {
            let n = Length(qs);
            H(qs[0]);
            for i in 1..n-1 {
                CNOT(qs[i-1], qs[i]);
                Std.Memory.Store(qs[i-1]);
            }
            Std.Memory.Store(qs[n-1]);
        }

        operation Main() : Unit {
            Std.ResourceEstimation.EnableManualMemoryComputeArchitecture();
            use qs = Qubit[10];
            PrepareGhzStateInMemory(qs);
            for i in 0..9 {
                Std.Memory.Load(qs[i]);
                MResetZ(qs[i]);
            }
        }
    "});
    assert_eq!(counts.num_qubits, 12);
    assert_eq!(counts.num_compute_qubits, Some(2));
    assert_eq!(counts.read_from_memory_count, Some(10));
    assert_eq!(counts.write_to_memory_count, Some(10));
}

#[test]
fn manual_memory_rejects_store_on_memory_qubit() {
    let result = run_logical_counts_result(
        indoc! {"
                operation Main() : Unit {
                    Std.ResourceEstimation.EnableManualMemoryComputeArchitecture();

                    use q = Qubit();
                    Std.Memory.Store(q);
                    Std.Memory.Store(q);
                }
            "},
        None,
    );

    let err = result.expect_err("expected storing a memory qubit to fail");
    assert!(
        err.contains("cannot perform Store on memory qubit"),
        "unexpected error: {err}"
    );
}

#[test]
fn manual_memory_rejects_load_on_compute_qubit() {
    let result = run_logical_counts_result(
        indoc! {"
                operation Main() : Unit {
                    Std.ResourceEstimation.EnableManualMemoryComputeArchitecture();

                    use q = Qubit();
                    Std.Memory.Load(q);
                }
            "},
        None,
    );

    let err = result.expect_err("expected loading a compute qubit to fail");
    assert!(
        err.contains("cannot perform Load on compute qubit"),
        "unexpected error: {err}"
    );
}

#[test]
fn is_resource_estimating_is_true() {
    let counts = run_logical_counts(indoc! {r#"
            namespace Test {
                import Std.ResourceEstimation.*;

                @EntryPoint()
                operation Main() : Unit {
                    use q = Qubit();
                    if IsResourceEstimating() {
                        T(q);
                    }
                }
            }
        "#});
    assert_eq!(counts.t_count, 1);
}
