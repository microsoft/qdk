// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use indoc::indoc;
use miette::Report;
use qsc::{
    Backend, LanguageFeatures, PackageType, SourceMap, TargetCapabilityFlags,
    interpret::Interpreter, target::Profile,
};

use crate::{
    instruction_ids::*,
    trace::Gate,
    trace_builder::{TraceBuilder, trace_expr},
};

fn build_interpreter(source: &str) -> Result<Interpreter, String> {
    let source_map = SourceMap::new([("test".into(), source.into())], None);
    let (std_id, store) = qsc::compile::package_store_with_stdlib(TargetCapabilityFlags::all());

    match Interpreter::new(
        source_map,
        PackageType::Exe,
        Profile::Unrestricted.into(),
        LanguageFeatures::default(),
        store,
        &[(std_id, None)],
    ) {
        Ok(interpreter) => Ok(interpreter),
        Err(err) => {
            let mut messages = Vec::new();
            for e in err {
                let report = Report::from(e);
                messages.push(format!("{report:?}"));
            }
            Err(format!("compilation failed:\n{}", messages.join("\n")))
        }
    }
}

fn run_trace_result(source: &str) -> Result<crate::Trace, String> {
    let mut interpreter = build_interpreter(source)?;

    trace_expr(&mut interpreter, "Test.Main()")
        .map_err(|err| format!("evaluation failed:\n{}", format_errors(err)))
}

fn run_trace(source: &str) -> crate::Trace {
    run_trace_result(source).unwrap_or_else(|err| panic!("failed to build trace: {err}"))
}

fn run_trace_expect_error(source: &str) -> String {
    match run_trace_result(source) {
        Err(err) => err,
        Ok(_) => panic!("expected an error but trace succeeded"),
    }
}

fn format_errors(errors: Vec<qsc::interpret::Error>) -> String {
    errors
        .into_iter()
        .map(|e| format!("{:?}", Report::from(e)))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn supports_x_cx_ccx_and_measurements() {
    let trace = run_trace(indoc! {
        "
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                use (a, b, c) = (Qubit(), Qubit(), Qubit());
                X(a);
                CNOT(a, b);
                CCNOT(a, b, c);
                let _ = M(a);
                let _ = MResetZ(b);
            }
        }
        "
    });

    assert_eq!(trace.compute_qubits(), 3);
    assert_eq!(trace.num_gates(), 5);

    let ids: Vec<u64> = trace.walk_iter().map(Gate::id).collect();
    assert_eq!(ids, vec![PAULI_X, CX, CCX, MEAS_Z, MEAS_RESET_Z]);
}

#[test]
fn supports_single_qubit_and_phase_family() {
    let mut builder = TraceBuilder::default();
    let q = builder.qubit_allocate().expect("allocate should succeed");

    builder.h(q).expect("H should be supported");
    builder.s(q).expect("S should be supported");
    builder.sadj(q).expect("SAdj should be supported");
    builder.sx(q).expect("SX should be supported");
    builder.t(q).expect("T should be supported");
    builder.tadj(q).expect("TAdj should be supported");
    builder.y(q).expect("Y should be supported");
    builder.z(q).expect("Z should be supported");

    let trace = builder.into_trace();
    let ids: Vec<u64> = trace.walk_iter().map(Gate::id).collect();
    assert_eq!(ids, vec![H, S, S_DAG, SQRT_X, T, T_DAG, PAULI_Y, PAULI_Z]);
}

#[test]
fn supports_controlled_and_rotation_families() {
    let mut builder = TraceBuilder::default();
    let q0 = builder
        .qubit_allocate()
        .expect("first allocate should succeed");
    let q1 = builder
        .qubit_allocate()
        .expect("second allocate should succeed");

    builder.cy(q0, q1).expect("CY should be supported");
    builder.cz(q0, q1).expect("CZ should be supported");
    builder.swap(q0, q1).expect("SWAP should be supported");

    builder.rx(0.1, q0).expect("Rx should be supported");
    builder.ry(0.2, q0).expect("Ry should be supported");
    builder.rz(0.3, q0).expect("Rz should be supported");
    builder.rxx(0.4, q0, q1).expect("Rxx should be supported");
    builder.ryy(0.5, q0, q1).expect("Ryy should be supported");
    builder.rzz(0.6, q0, q1).expect("Rzz should be supported");

    // Label permutation should remap subsequent operations and not emit a gate.
    builder
        .qubit_swap_id(q0, q1)
        .expect("qubit_swap_id should be supported");
    builder.x(q0).expect("X after relabel should be supported");
    builder.x(q1).expect("X after relabel should be supported");

    let trace = builder.into_trace();
    let ops: Vec<_> = trace.walk_iter().collect();

    let ids: Vec<u64> = ops.iter().map(|g| g.id()).collect();
    assert_eq!(
        ids,
        vec![CY, CZ, SWAP, RX, RY, RZ, RXX, RYY, RZZ, PAULI_X, PAULI_X]
    );

    let params: Vec<Vec<f64>> = ops.iter().map(|g| g.params().to_vec()).collect();
    assert_eq!(
        params,
        vec![
            vec![],
            vec![],
            vec![],
            vec![0.1],
            vec![0.2],
            vec![0.3],
            vec![0.4],
            vec![0.5],
            vec![0.6],
            vec![],
            vec![],
        ]
    );

    // After qubit_swap_id(q0, q1), operations on q0/q1 target swapped trace qubits.
    assert_eq!(ops[9].qubits(), &[q1 as u64]);
    assert_eq!(ops[10].qubits(), &[q0 as u64]);
}

#[test]
fn post_selection_one_controls_branch() {
    let trace = run_trace(indoc! {
        "
        namespace Test {
            import Std.Diagnostics.PostSelectZ;

            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                PostSelectZ(One, q);
                if M(q) == One {
                    X(q);
                }
            }
        }
        "
    });
    let ids: Vec<u64> = trace.walk_iter().map(Gate::id).collect();
    assert_eq!(ids, vec![MEAS_Z, PAULI_X]);
}

#[test]
fn post_selection_zero_controls_branch() {
    let trace = run_trace(indoc! {
        "
        namespace Test {
            import Std.Diagnostics.PostSelectZ;

            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                PostSelectZ(Zero, q);
                if M(q) == One {
                    X(q);
                }
            }
        }
        "
    });
    let ids: Vec<u64> = trace.walk_iter().map(Gate::id).collect();
    assert_eq!(ids, vec![MEAS_Z]);
}

#[test]
fn measurement_branch_is_observed_both_ways_over_multiple_runs() {
    let trace = run_trace(indoc! {
        "
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                for _ in 1..100 {
                    H(q);
                    if M(q) == One {
                        X(q);
                    }
                }
            }
        }
        "
    });

    let gate_counts = trace.gate_counts();
    assert_eq!(gate_counts.get(&H), Some(&100));
    assert_eq!(gate_counts.get(&MEAS_Z), Some(&100));

    let x_count = gate_counts.get(&PAULI_X).copied().unwrap_or(0);
    assert!(
        (25..=75).contains(&x_count),
        "expected X count to be between 25 and 75, got {x_count}"
    );
}

#[test]
fn repeat_estimates_creates_repeated_block() {
    let trace = run_trace(indoc! {
        "
        namespace Test {
            import Std.ResourceEstimation.*;

            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                BeginRepeatEstimates(3);
                X(q);
                MResetZ(q);
                EndRepeatEstimates();
            }
        }
        "
    });

    // The repeated body has two gates and is repeated 3 times.
    assert_eq!(trace.num_gates(), 6);
    assert_eq!(trace.depth(), 6);

    let gate_counts = trace.gate_counts();
    assert_eq!(gate_counts.get(&PAULI_X), Some(&3));
    assert_eq!(gate_counts.get(&MEAS_RESET_Z), Some(&3));

    let rendered = trace.to_string();
    assert!(
        rendered.contains("repeat 3"),
        "expected rendered trace to include repeat block, got:\n{rendered}"
    );
}

#[test]
fn estimate_caching_is_a_no_op() {
    let trace = run_trace(indoc! {r#"
        namespace Test {
            import Std.ResourceEstimation.*;

            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                if BeginEstimateCaching("Rotate", SingleVariant()) {
                    X(q);
                    EndEstimateCaching();
                }
            }
        }
    "#});

    assert_eq!(trace.compute_qubits(), 1);
    assert_eq!(trace.num_gates(), 1);

    let ids: Vec<u64> = trace.walk_iter().map(Gate::id).collect();
    assert_eq!(ids, vec![PAULI_X]);
}

#[test]
fn load_and_store_emit_memory_gates() {
    let mut builder = TraceBuilder::default();
    let q = builder.qubit_allocate().expect("allocate should succeed");

    builder.load(q);
    builder.store(q);

    let trace = builder.into_trace();
    let ids: Vec<u64> = trace.walk_iter().map(Gate::id).collect();
    assert_eq!(ids, vec![READ_FROM_MEMORY, WRITE_TO_MEMORY]);
}

#[test]
fn enable_memory_compute_architecture_is_a_no_op() {
    let trace = run_trace(indoc! {r#"
        namespace Test {
            import Std.ResourceEstimation.*;

            @EntryPoint()
            operation Main() : Unit {
                EnableMemoryComputeArchitecture(10, LeastRecentlyUsed());
            }
        }
    "#});

    assert_eq!(trace.num_gates(), 0);
}

#[test]
fn account_for_estimates_is_rejected() {
    let err = run_trace_expect_error(indoc! {r#"
        namespace Test {
            import Std.ResourceEstimation.*;

            @EntryPoint()
            operation Main() : Unit {
                use qs = Qubit[1];
                AccountForEstimates([TCount(1)], PSSPCLayout(), qs);
            }
        }
    "#});

    assert!(
        err.contains("IntrinsicFail"),
        "unexpected error message: {err}"
    );
}

#[test]
fn end_repeat_without_begin_fails() {
    let err = run_trace_expect_error(indoc! {
        "
        namespace Test {
            import Std.ResourceEstimation.*;

            @EntryPoint()
            operation Main() : Unit {
                EndRepeatEstimates();
            }
        }
        "
    });

    assert!(
        err.contains("IntrinsicFail"),
        "unexpected error message: {err}"
    );
}
