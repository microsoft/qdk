// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::cmp::Reverse;
use std::path::PathBuf;

use super::TemporalProfiler;
use crate::models::{
    QpuModel,
    demo_model1::{DemoModel1, DemoModel1_NativeOperation, DemoModel1_Resource},
};
use crate::profilers::profile_types::LoggedInstructionParameter::{
    FactoryId as FactoryParameter, QubitId as LoggedQubitId,
};
use crate::tests::create_qsharp_interpreter;
use crate::{
    Parameter::QubitId, Profiler, QuantumBackend, QuantumBackendInstruction, ResultAdapter,
    TemporalProfilerConfig, TranspilingAdapter,
};
use expect_test::expect;

/// Creates a repeatable five-factory profiler with tracing enabled for timing assertions.
fn seeded_profiler() -> TemporalProfiler<DemoModel1> {
    TemporalProfiler::new(
        DemoModel1::new(5_000, 1_000, 5, 5),
        TemporalProfilerConfig {
            seed: Some(42),
            record_trace: true,
            ..TemporalProfilerConfig::default()
        },
    )
}

/// Wraps the seeded profiler with result-register and default instruction adapters.
fn adapted_profiler() -> ResultAdapter<TranspilingAdapter<TemporalProfiler<DemoModel1>>> {
    ResultAdapter::new(TranspilingAdapter::with_defaults(seeded_profiler()))
}

/// Advances scheduler-focused tests through all submitted instructions, not background rounds.
fn complete_scheduled_instructions(profiler: &mut TemporalProfiler<DemoModel1>) {
    while pending_instruction_count(profiler) > 0 {
        let Reverse((time, _, event)) = profiler.state.events.pop().expect("pending instruction");
        profiler.state.current_time_nanos = time;
        profiler.complete_operation(event);
    }
}

/// Counts unfinished user instructions, excluding automatically scheduled factory rounds.
fn pending_instruction_count(profiler: &TemporalProfiler<DemoModel1>) -> usize {
    profiler
        .state
        .operations
        .iter()
        .filter(|operation| {
            !operation.done
                && !matches!(
                    operation.operation,
                    Some(DemoModel1_NativeOperation::DistillationRound(_))
                )
        })
        .count()
}

/// Checks measured lifetime error, including the additional error from an injected T state.
#[test]
fn computes_demo_model_error() -> Result<(), String> {
    for (names, expected_time, expected_error) in [
        (&["h", "mz"][..], 6_000, 6_000.0 * 6e-12),
        (
            &["h", "t", "mz"][..],
            67_000,
            (67_000.0 + 11_000.0) * 6e-12 + 35.0 * 1e-12 + 7.1 * 3e-8,
        ),
    ] {
        let mut profiler = seeded_profiler();
        for factory in &mut profiler.model.t_factories {
            factory.probability_success = 1.0;
        }
        assert_eq!(profiler.get_result().totals.error, 0.0);
        let instructions: Vec<_> = names
            .iter()
            .map(|&instruction_name| QuantumBackendInstruction {
                instruction_name,
                parameters: smallvec::smallvec![QubitId(0)],
            })
            .collect();
        assert_eq!(profiler.execute_batch(&instructions)?.results.len(), 1);
        assert_eq!(profiler.state.current_time_nanos, expected_time);
        let actual_error = profiler.get_result().totals.error;
        assert!(
            (actual_error - expected_error).abs() < 1e-12,
            "expected error {expected_error}, got {actual_error} for {names:?}"
        );
    }
    Ok(())
}

/// Repeated measurements charge new exposure only, and reset discards the old state.
#[test]
fn does_not_charge_measured_error_twice() -> Result<(), String> {
    let mut profiler = seeded_profiler();
    for factory in &mut profiler.model.t_factories {
        factory.probability_success = 1.0;
    }
    let instructions: Vec<_> = ["h", "t", "mz", "mz"]
        .into_iter()
        .map(|instruction_name| QuantumBackendInstruction {
            instruction_name,
            parameters: smallvec::smallvec![QubitId(0)],
        })
        .collect();
    assert_eq!(profiler.execute_batch(&instructions)?.results.len(), 2);
    let expected_error = (68_000.0 + 11_000.0) * 6e-12 + 35.0 * 1e-12 + 7.1 * 3e-8;
    assert!((profiler.get_result().totals.error - expected_error).abs() < 1e-18);

    let instructions: Vec<_> = ["reset", "h", "mz"]
        .into_iter()
        .map(|instruction_name| QuantumBackendInstruction {
            instruction_name,
            parameters: smallvec::smallvec![QubitId(0)],
        })
        .collect();
    assert_eq!(profiler.execute_batch(&instructions)?.results.len(), 1);
    assert_eq!(profiler.state.current_time_nanos, 75_000);
    assert!((profiler.get_result().totals.error - expected_error - 6_000.0 * 6e-12).abs() < 1e-18);
    Ok(())
}

/// A qubit first seen in a later batch does not accrue error before it exists.
#[test]
fn starts_qubit_error_at_first_use() -> Result<(), String> {
    let mut profiler = seeded_profiler();
    profiler.execute_batch(&[
        QuantumBackendInstruction {
            instruction_name: "h",
            parameters: smallvec::smallvec![QubitId(0)],
        },
        QuantumBackendInstruction {
            instruction_name: "mz",
            parameters: smallvec::smallvec![QubitId(0)],
        },
    ])?;
    profiler.execute_batch(&[QuantumBackendInstruction {
        instruction_name: "mz",
        parameters: smallvec::smallvec![QubitId(1)],
    }])?;
    assert_eq!(profiler.state.current_time_nanos, 7_000);
    assert!((profiler.get_result().totals.error - 7_000.0 * 6e-12).abs() < 1e-18);
    Ok(())
}

/// Waits for every result despite out-of-order completion, without draining unrelated work.
#[test]
fn waits_for_all_results_not_all_instructions() -> Result<(), String> {
    use rand::{RngExt, SeedableRng, rngs::StdRng};

    let mut profiler = seeded_profiler();
    for factory in &mut profiler.model.t_factories {
        factory.probability_success = 1.0;
    }
    let instructions: Vec<_> = [("t", 2), ("h", 0), ("mz", 0), ("mz", 1), ("x", 0)]
        .into_iter()
        .map(|(instruction_name, qubit)| QuantumBackendInstruction {
            instruction_name,
            parameters: smallvec::smallvec![QubitId(qubit)],
        })
        .collect();
    let result = profiler.execute_batch(&instructions)?;
    let mut rng = StdRng::seed_from_u64(42);
    let first_completed = crate::QuantumBackendResult::from(rng.random::<bool>());
    let second_completed = crate::QuantumBackendResult::from(rng.random::<bool>());
    assert_eq!(
        result.results.as_slice(),
        &[second_completed, first_completed]
    );
    assert_eq!(profiler.state.current_time_nanos, 6_000);
    assert!(
        profiler
            .state
            .pending_operations_producing_results
            .is_empty()
    );
    assert_eq!(profiler.state.operations_offset, 0);
    let first_instruction = profiler.model.t_factories.len();
    assert!(!profiler.state.operations[first_instruction].done);
    assert!(!profiler.state.operations[first_instruction + 4].done);

    let result = profiler.execute_batch(&[QuantumBackendInstruction {
        instruction_name: "mz",
        parameters: smallvec::smallvec![QubitId(1)],
    }])?;
    assert_eq!(
        result.results.as_slice(),
        &[crate::QuantumBackendResult::from(rng.random::<bool>())]
    );
    assert_eq!(profiler.state.current_time_nanos, 7_000);

    let result = profiler.execute_batch(&[QuantumBackendInstruction {
        instruction_name: "mz",
        parameters: smallvec::smallvec![QubitId(2)],
    }])?;
    assert_eq!(result.results.len(), 1);
    assert_ne!(result.results[0], crate::QuantumBackendResult::Error);
    assert_eq!(profiler.state.current_time_nanos, 67_000);
    assert!(
        profiler
            .state
            .pending_operations_producing_results
            .is_empty()
    );
    assert_eq!(pending_instruction_count(&profiler), 0);
    Ok(())
}

/// Result-free and empty batches do not advance time; later results honor queued dependencies.
#[test]
fn queues_result_free_batches_without_a_completion_barrier() -> Result<(), String> {
    let mut profiler = seeded_profiler();
    for qubit in [0, 1] {
        let result = profiler.execute_batch(&[QuantumBackendInstruction {
            instruction_name: "h",
            parameters: smallvec::smallvec![QubitId(qubit)],
        }])?;
        assert!(result.results.is_empty());
        assert_eq!(profiler.state.current_time_nanos, 0);
    }
    assert!(profiler.execute_batch(&[])?.results.is_empty());
    assert_eq!(profiler.state.current_time_nanos, 0);
    assert_eq!(pending_instruction_count(&profiler), 2);

    let instructions: Vec<_> = [0, 1]
        .into_iter()
        .map(|qubit| QuantumBackendInstruction {
            instruction_name: "mz",
            parameters: smallvec::smallvec![QubitId(qubit)],
        })
        .collect();
    assert_eq!(profiler.execute_batch(&instructions)?.results.len(), 2);
    assert_eq!(profiler.state.current_time_nanos, 6_000);
    assert!(
        profiler
            .state
            .pending_operations_producing_results
            .is_empty()
    );
    assert_eq!(pending_instruction_count(&profiler), 0);
    Ok(())
}

#[test]
/// Snapshots instruction timing and dependencies for a simple program.
fn profiles_simple_program() -> Result<(), String> {
    let source = r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result[] {
                use q = Qubit[2];
                X(q[0]);
                H(q[0]);
                H(q[1]);
                CNOT(q[0], q[1]);
                T(q[0]);
                Adjoint T(q[1]);
                [M(q[0]), M(q[1])]
            }
        }
    "#;
    let mut interpreter = create_qsharp_interpreter(vec![("test.qs".into(), source.into())])?;
    let mut profiler = adapted_profiler();

    let result = profiler.profile_qsharp_expr(&mut interpreter, "Test.Main()")?;
    let profile_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tmp/small_profile.json");
    assert_eq!(result.totals.logical_compute_qubits, 2);
    assert_eq!(result.totals.logical_qubits, 157);
    result.to_json(&profile_path)?;
    expect![[r#"
        0-55000: round 0
        0-55000: round 1
        0-55000: round 2
        0-55000: round 3
        0-55000: round 4
        0-5000: x 0
        0-5000: h 1
        5000-10000: h 0
        10000-15000: cx 0 1
        55000-66000: inject_t 0 0
        55000-66000: inject_tadj 1 1
        66000-121000: round 0
        66000-67000: mz 0
        66000-121000: round 1
        67000-68000: mz 1"#]]
    .assert_eq(&format!(
        "{:?}",
        result
            .trace
            .expect("temporal profiler should record a trace")
    ));
    Ok(())
}

/// Checks each instruction category against the fixed nanosecond benchmark.
#[test]
fn instruction_durations_use_logical_timing_assumptions() {
    for (names, expected) in [
        (&["cx", "h", "s", "sadj", "x", "y", "z"][..], 5_000),
        (&["mz", "reset"][..], 1_000),
        (&["t", "tadj"][..], 11_000),
    ] {
        for &instruction_name in names {
            let mut profiler = seeded_profiler();
            let parameters = if instruction_name == "cx" {
                smallvec::smallvec![QubitId(0), QubitId(1)]
            } else {
                smallvec::smallvec![QubitId(0)]
            };
            profiler
                .model
                .on_instruction_received(
                    &QuantumBackendInstruction {
                        instruction_name,
                        parameters,
                    },
                    &mut profiler.state,
                )
                .expect("supported instruction");
            assert_eq!(
                profiler
                    .state
                    .operations
                    .back()
                    .expect("scheduled instruction")
                    .duration_nanos,
                expected,
                "{instruction_name}",
            );
        }
    }
}

/// Checks that configured durations reach the scheduler, including T injection.
#[test]
fn schedules_configured_instruction_durations() -> Result<(), String> {
    let mut profiler = TemporalProfiler::new(
        DemoModel1::new(700, 300, 10, 5),
        TemporalProfilerConfig {
            record_trace: true,
            seed: Some(42),
            ..TemporalProfilerConfig::default()
        },
    );
    for factory in &mut profiler.model.t_factories {
        factory.probability_success = 1.0;
    }
    let instructions: Vec<_> = ["x", "mz", "reset", "t", "tadj"]
        .into_iter()
        .map(|instruction_name| QuantumBackendInstruction {
            instruction_name,
            parameters: smallvec::smallvec![QubitId(0)],
        })
        .collect();
    profiler.execute_batch(&instructions)?;
    complete_scheduled_instructions(&mut profiler);
    let durations: Vec<_> = profiler
        .state
        .event_recorder
        .logs
        .iter()
        .filter(|entry| entry.instruction.instruction_name != "round")
        .map(|entry| entry.end_time_nanos - entry.start_time_nanos)
        .collect();
    assert_eq!(durations, [700, 300, 300, 1_700, 1_700]);
    Ok(())
}

/// Checks that T and T-adjoint each consume one state while H needs none.
#[test]
fn schedules_optional_single_resource_requirements() -> Result<(), String> {
    let mut profiler = seeded_profiler();
    for factory in &mut profiler.model.t_factories {
        factory.probability_success = 1.0;
    }
    let instructions: Vec<_> = [("t", 0), ("tadj", 1), ("h", 2)]
        .into_iter()
        .map(|(instruction_name, qubit)| QuantumBackendInstruction {
            instruction_name,
            parameters: smallvec::smallvec![QubitId(qubit)],
        })
        .collect();

    profiler.execute_batch(&instructions)?;
    complete_scheduled_instructions(&mut profiler);

    let intervals: Vec<_> = profiler
        .state
        .event_recorder
        .logs
        .iter()
        .map(|trace| {
            (
                trace.instruction.instruction_name.as_ref(),
                trace.start_time_nanos,
                trace.end_time_nanos,
            )
        })
        .collect();
    assert_eq!(
        intervals,
        vec![
            ("round", 0, 55_000),
            ("round", 0, 55_000),
            ("round", 0, 55_000),
            ("round", 0, 55_000),
            ("round", 0, 55_000),
            ("h", 0, 5_000),
            ("inject_t", 55_000, 66_000),
            ("inject_tadj", 55_000, 66_000),
            ("round", 66_000, 121_000),
            ("round", 66_000, 121_000),
        ]
    );
    assert_eq!(
        profiler.state.available_resources[&DemoModel1_Resource::TState],
        std::collections::VecDeque::from([2, 3, 4])
    );
    // Five states were produced and two were consumed.
    assert_eq!(
        profiler
            .state
            .event_recorder
            .event_aggregator
            .total_event_count(),
        profiler.state.event_recorder.logs.len() + 7
    );
    assert!(
        profiler
            .state
            .operations_blocked_on_resources
            .values()
            .all(std::collections::VecDeque::is_empty)
    );
    Ok(())
}

#[test]
/// Preserves measurement-dependent Q# branches and their scheduling dependencies.
fn profiles_measurement_dependent_qsharp() -> Result<(), String> {
    use rand::{RngExt, SeedableRng, rngs::StdRng};

    let source = r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                let result = M(q);
                if result == One {
                    X(q);
                } else {
                    H(q);
                }
                M(q);
            }
        }
    "#;
    let mut interpreter = create_qsharp_interpreter(vec![("test.qs".into(), source.into())])?;
    let mut profiler = adapted_profiler();

    let result = profiler.profile_qsharp_expr(&mut interpreter, "Test.Main()")?;
    assert_eq!(result.totals.logical_compute_qubits, 1);
    assert!((result.totals.total_runtime_sec - 7e-6).abs() < f64::EPSILON);
    assert!(result.infos.is_empty());
    let trace = result
        .trace
        .expect("temporal profiler should record a trace");
    let intervals: Vec<_> = trace
        .instructions
        .iter()
        .filter(|trace| trace.instruction.instruction_name.as_ref() != "round")
        .map(|trace| {
            (
                trace.instruction.instruction_name.as_ref(),
                trace.start_time_nanos,
                trace.end_time_nanos,
            )
        })
        .collect();
    let mut rng = StdRng::seed_from_u64(42);
    let branch = if rng.random::<bool>() { "x" } else { "h" };
    assert_eq!(
        intervals,
        vec![
            ("mz", 0, 1_000),
            (branch, 1_000, 6_000),
            ("mz", 6_000, 7_000)
        ]
    );

    Ok(())
}

/// All five factories retry failures and then each hold exactly one ready state.
#[test]
fn retries_failed_rounds_and_holds_one_ready_state() {
    let mut profiler = seeded_profiler();
    for factory in &mut profiler.model.t_factories {
        factory.probability_success = 0.0;
    }
    for factory in 0..5 {
        let Reverse((time, _, event)) = profiler.state.events.pop().expect("initial round");
        assert_eq!(time, 55_000);
        assert!(matches!(
            profiler.state.operations[event.0 - profiler.state.operations_offset].operation,
            Some(DemoModel1_NativeOperation::DistillationRound(id)) if id == factory
        ));
        profiler.state.current_time_nanos = time;
        profiler.complete_operation(event);
        let trace = &profiler.state.event_recorder.logs[5 + factory];
        assert_eq!(
            trace.instruction.parameters.as_slice(),
            &[FactoryParameter(factory)]
        );
        assert_eq!(
            (trace.start_time_nanos, trace.end_time_nanos),
            (55_000, 110_000)
        );
    }
    assert!(profiler.state.available_resources.is_empty());
    assert_eq!(profiler.state.event_recorder.logs.len(), 10);

    for factory in &mut profiler.model.t_factories {
        factory.probability_success = 1.0;
    }
    for factory in 0..5 {
        let Reverse((time, _, event)) = profiler.state.events.pop().expect("retry round");
        assert_eq!(time, 110_000);
        assert!(matches!(
            profiler.state.operations[event.0 - profiler.state.operations_offset].operation,
            Some(DemoModel1_NativeOperation::DistillationRound(id)) if id == factory
        ));
        profiler.state.current_time_nanos = time;
        profiler.complete_operation(event);
    }
    assert_eq!(
        profiler.state.available_resources[&DemoModel1_Resource::TState],
        (0..5).collect::<std::collections::VecDeque<_>>()
    );
    assert!(profiler.state.events.is_empty());
    assert_eq!(profiler.state.event_recorder.logs.len(), 10);
}

/// Two factories serve concurrent gates and cannot be reused until injection finishes.
#[test]
fn reserves_factories_until_injection_completes() -> Result<(), String> {
    let mut profiler = TemporalProfiler::new(
        DemoModel1::new(5_000, 1_000, 2, 5),
        TemporalProfilerConfig {
            seed: Some(42),
            record_trace: true,
            ..TemporalProfilerConfig::default()
        },
    );
    for factory in &mut profiler.model.t_factories {
        factory.probability_success = 1.0;
    }
    let instructions: Vec<_> = [("t", 0), ("tadj", 1), ("t", 2)]
        .into_iter()
        .map(|(instruction_name, qubit)| QuantumBackendInstruction {
            instruction_name,
            parameters: smallvec::smallvec![QubitId(qubit)],
        })
        .collect();
    profiler.execute_batch(&instructions)?;
    complete_scheduled_instructions(&mut profiler);
    let injections: Vec<_> = profiler
        .state
        .event_recorder
        .logs
        .iter()
        .filter(|trace| trace.instruction.instruction_name != "round")
        .collect();
    assert_eq!(injections.len(), 3);
    for (trace, name, factory, qubit, start) in [
        (injections[0], "inject_t", 0, 0, 55_000),
        (injections[1], "inject_tadj", 1, 1, 55_000),
        (injections[2], "inject_t", 0, 2, 121_000),
    ] {
        assert_eq!(trace.instruction.instruction_name, name);
        assert_eq!(
            trace.instruction.parameters.as_slice(),
            &[FactoryParameter(factory), LoggedQubitId(qubit)]
        );
        assert_eq!(
            (trace.start_time_nanos, trace.end_time_nanos),
            (start, start + 11_000)
        );
    }
    assert_eq!(profiler.state.current_time_nanos, 132_000);
    assert_eq!(
        profiler.state.available_resources[&DemoModel1_Resource::TState],
        std::collections::VecDeque::from([1])
    );
    assert!(profiler.state.event_recorder.logs.iter().any(
        |trace| trace.instruction.instruction_name == "round"
            && trace.instruction.parameters.as_slice() == [FactoryParameter(0)]
            && trace.start_time_nanos == 132_000
            && trace.end_time_nanos == 187_000
    ));

    profiler.execute_batch(&[QuantumBackendInstruction {
        instruction_name: "tadj",
        parameters: smallvec::smallvec![QubitId(0)],
    }])?;
    complete_scheduled_instructions(&mut profiler);
    assert!(profiler.state.event_recorder.logs.iter().any(
        |trace| trace.instruction.instruction_name == "inject_tadj"
            && trace.instruction.parameters.as_slice() == [FactoryParameter(1), LoggedQubitId(0)]
            && trace.start_time_nanos == 132_000
            && trace.end_time_nanos == 143_000
    ));
    // Four states have been produced and consumed, including the second batch.
    assert_eq!(
        profiler
            .state
            .event_recorder
            .event_aggregator
            .total_event_count(),
        profiler.state.event_recorder.logs.len() + 8
    );
    Ok(())
}

/// The same seed reproduces distillation, immediate results, and reads across batches.
#[test]
fn seeded_randomness_reproduces_execution() -> Result<(), String> {
    let mut first = adapted_profiler();
    let mut second = adapted_profiler();
    for _ in 0..8 {
        let instructions = [
            QuantumBackendInstruction {
                instruction_name: "t",
                parameters: smallvec::smallvec![QubitId(0)],
            },
            QuantumBackendInstruction {
                instruction_name: "mz",
                parameters: smallvec::smallvec![QubitId(0)],
            },
            QuantumBackendInstruction {
                instruction_name: "mz_store_reset",
                parameters: smallvec::smallvec![QubitId(0), crate::Parameter::ResultId(0)],
            },
            QuantumBackendInstruction {
                instruction_name: "read_result",
                parameters: smallvec::smallvec![crate::Parameter::ResultId(0)],
            },
        ];
        assert_eq!(
            first.execute_batch(&instructions)?,
            second.execute_batch(&instructions)?
        );
    }
    assert_eq!(first.get_result(), second.get_result());
    Ok(())
}

/// Idle factories retain one state across batches without delaying resource-free work.
#[test]
fn holds_ready_states_across_resource_free_batches() -> Result<(), String> {
    let mut profiler = seeded_profiler();
    for factory in &mut profiler.model.t_factories {
        factory.probability_success = 1.0;
    }
    profiler.execute_batch(&[])?;
    assert_eq!(profiler.state.current_time_nanos, 0);
    let h = QuantumBackendInstruction {
        instruction_name: "h",
        parameters: smallvec::smallvec![QubitId(0)],
    };
    profiler.execute_batch(&vec![h; 40])?;
    assert_eq!(profiler.state.current_time_nanos, 0);
    complete_scheduled_instructions(&mut profiler);
    assert_eq!(profiler.state.current_time_nanos, 200_000);
    assert!(profiler.state.events.is_empty());
    assert_eq!(
        profiler.state.available_resources[&DemoModel1_Resource::TState],
        (0..5).collect::<std::collections::VecDeque<_>>()
    );
    assert_eq!(
        profiler
            .state
            .event_recorder
            .logs
            .iter()
            .filter(|trace| trace.instruction.instruction_name == "round")
            .count(),
        5
    );

    profiler.execute_batch(&[QuantumBackendInstruction {
        instruction_name: "t",
        parameters: smallvec::smallvec![QubitId(0)],
    }])?;
    complete_scheduled_instructions(&mut profiler);
    assert_eq!(profiler.state.current_time_nanos, 211_000);
    assert_eq!(
        profiler.state.available_resources[&DemoModel1_Resource::TState],
        (1..5).collect::<std::collections::VecDeque<_>>()
    );
    let trace = profiler
        .state
        .event_recorder
        .logs
        .last()
        .expect("restarted round");
    assert_eq!(trace.instruction.instruction_name, "round");
    assert_eq!(
        (trace.start_time_nanos, trace.end_time_nanos),
        (211_000, 266_000)
    );
    assert_eq!(profiler.get_result().totals.total_operations_count, 53);
    assert!((profiler.get_result().totals.total_runtime_sec - 211_000e-9).abs() < 1e-18);
    let metrics = profiler.get_result().profiles.profiles;
    let utilization_profile = metrics
        .iter()
        .find(|profile| profile.metric_name == "Factory utilization")
        .expect("factory utilization is reported separately");
    assert_eq!(utilization_profile.metric_units, "factories");
    assert_eq!(utilization_profile.series.len(), 1);
    assert!(utilization_profile.series[0].0.abs() < 1e-18);
    assert!((utilization_profile.series[0].1 - 0.00033).abs() < 1e-15);
    let round_profile = metrics
        .iter()
        .find(|profile| profile.metric_name == "round rate")
        .expect("round starts are aggregated");
    assert_eq!(round_profile.series, vec![(0.0, 6.0)]);
    assert!(
        metrics
            .iter()
            .any(|profile| profile.metric_name == "inject_t rate")
    );
    assert!(
        metrics
            .iter()
            .all(|profile| profile.metric_name != "t rate")
    );
    Ok(())
}

/// Reports unscaled factory utilization, preserving idle windows and timestamps.
#[test]
fn reports_factory_utilization_in_factories() -> Result<(), String> {
    for factory_count in [1, 2, 5] {
        let mut profiler = TemporalProfiler::new(
            DemoModel1::new(5_000, 1_000, factory_count, 5),
            TemporalProfilerConfig {
                aggregation_window_nanos: 55_000,
                seed: Some(42),
                ..TemporalProfilerConfig::default()
            },
        );
        for factory in &mut profiler.model.t_factories {
            factory.probability_success = 1.0;
        }
        profiler.execute_batch(&vec![
            QuantumBackendInstruction {
                instruction_name: "h",
                parameters: smallvec::smallvec![QubitId(0)],
            };
            22
        ])?;
        profiler.execute_batch(&[QuantumBackendInstruction {
            instruction_name: "t",
            parameters: smallvec::smallvec![QubitId(0)],
        }])?;
        complete_scheduled_instructions(&mut profiler);

        let result = profiler.get_result();
        assert_eq!(result, profiler.get_result());
        assert_eq!(result.profiles.aggregation_window_sec, Some(55_000e-9));
        assert!(result.trace.is_none());
        assert!(
            result
                .profiles
                .profiles
                .windows(2)
                .all(|pair| { pair[0].metric_name <= pair[1].metric_name })
        );
        let utilization = result
            .profiles
            .profiles
            .iter()
            .find(|profile| profile.metric_name == "Factory utilization")
            .expect("factory utilization is reported");
        assert_eq!(utilization.metric_units, "factories");
        assert_eq!(utilization.group_name, "Utilization");
        #[allow(clippy::cast_precision_loss)]
        let expected_factories = factory_count as f64;
        assert_eq!(
            utilization.series,
            vec![
                (0.0, expected_factories),
                (55_000e-9, 0.0),
                (110_000e-9, 1.0)
            ]
        );
    }
    Ok(())
}

/// Construction rejects configurations that could permanently block resource consumers.
#[test]
#[should_panic(expected = "at least one T factory is required")]
fn rejects_zero_factories() {
    let _ = DemoModel1::new(5_000, 1_000, 0, 5);
}
