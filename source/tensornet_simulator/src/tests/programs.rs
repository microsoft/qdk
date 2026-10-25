// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! M0's ④ preflight and replay, ported: `cutensornet_mps/execution.rs`'s
//! tests (on `MpsStack`) and `consumer.rs`'s (on `plan` and `replay`). M0
//! said "before device discovery"; here, before the engine prepares a state,
//! which a counting engine shows.

use num_complex::Complex64;
use qdk_simulators::{MeasurementResult, OutputRecord};

use super::{
    Bytecode, OPID_H, OPID_MRESETZ, OPID_MZ, TestDoubleEngine, cases::expectation, pauli,
    region_run_twice,
};
use crate::{
    DEFAULT_MAX_BOND, Evaluate, MpsStack, ProgramError, QueryError, StackError, StateQuery,
    program::{plan, replay},
};

/// A stack over the counting double.
fn counting_stack() -> MpsStack<TestDoubleEngine> {
    MpsStack::new(TestDoubleEngine::counting())
}

fn program_error(error: StackError) -> ProgramError {
    assert!(!error.is_environment_error(), "{error}");
    match error {
        StackError::Program(error) => error,
        other => panic!("expected a program error, got {other:?}"),
    }
}

#[test]
fn state_queries_reject_program_errors_before_prepare() {
    let stack = counting_stack();
    let program = Bytecode::new(2).h(0).mz(0, 0).build();
    let mut absent = pauli(1.0, "Z", &[2]);
    absent
        .push_labels(Complex64::new(1.0, 0.0), "Z", &[0])
        .expect("a well-formed term");
    let cases = [
        (Vec::new(), "queries must contain at least one query"),
        (
            vec![
                StateQuery::Expectation(pauli(1.0, "Z", &[1])),
                StateQuery::Expectation(absent),
            ],
            "Expectation acts on qubit 2, but the program has 2 qubits",
        ),
    ];
    for (queries, message) in cases {
        let error = stack.evaluate(&program, &queries).expect_err("rejected");
        assert_eq!(error.to_string(), message);
        assert!(!error.is_environment_error());
    }
    assert_eq!(stack.engine().prepared(), 0);
}

#[test]
fn state_queries_share_the_preflight() {
    let stack = counting_stack().with_max_bond_dimension(Some(4));
    let program = Bytecode::new(2).mz(0, 0).h(1).build();
    let error = stack
        .evaluate(&program, &[StateQuery::Cost])
        .expect_err("feedforward is rejected");
    assert!(
        error.to_string().contains("after measuring qubit 0"),
        "{error}"
    );
    assert!(matches!(
        error,
        QueryError::Program(ProgramError::Feedforward { qubit: 0 })
    ));
    assert!(!error.is_environment_error());
    assert_eq!(stack.engine().prepared(), 0);
}

#[test]
fn preflight_rejects_multiple_regions_before_prepare() {
    let stack = counting_stack();
    let program = Bytecode::new(2).h(0).mz(0, 0).h(1).build();
    let error = program_error(stack.sample(&program, 1, Some(42)).expect_err("rejected"));
    assert_eq!(
        error.to_string(),
        "MPS batch sampling requires exactly one quantum evolution region, found 2"
    );
    assert_eq!(stack.engine().prepared(), 0);
}

#[test]
fn preflight_rejects_feedforward_before_prepare() {
    let stack = counting_stack();
    let program = Bytecode::new(2).mz(0, 0).h(1).build();
    let error = program_error(stack.sample(&program, 1, Some(42)).expect_err("rejected"));
    assert!(
        error.to_string().contains(
            "MPS batch sampling cannot execute a quantum evolution region after measuring qubit 0"
        ),
        "{error}"
    );
    assert_eq!(stack.engine().prepared(), 0);
}

#[test]
fn preflight_rejects_reset_before_prepare() {
    let stack = counting_stack();
    let program = Bytecode::new(2).h(0).reset(0).mz(0, 0).build();
    let error = program_error(stack.sample(&program, 1, Some(42)).expect_err("rejected"));
    // M0's message was "consumer execution failed: reset is not supported by
    // cuTensorNet MPS batch sampling (qubit 0)": the walk's wrapper and the
    // vendor are gone (DECISIONS 6).
    assert_eq!(
        error.to_string(),
        "reset is not supported by MPS batch sampling (qubit 0)"
    );
    assert_eq!(stack.engine().prepared(), 0);
}

#[test]
fn preflight_accepts_every_unitary_operation() {
    // I, X, Y, Z, H, S, S†, T, T†, SX, SX†, Rx, Ry, Rz, CX, CZ, Rxx, Ryy,
    // Rzz, SWAP and CY, on qubits (0, 1) where they take two. M0 only
    // prepared the run; here each also runs on the reference.
    let stack = counting_stack();
    for operation_id in (0..=19).filter(|&id| id != 1).chain([24, 29]) {
        let program = Bytecode::new(2)
            .gate(operation_id, 0, 1, 0.0)
            .mz(0, 0)
            .build();
        stack
            .sample(&program, 1, Some(42))
            .unwrap_or_else(|error| panic!("operation {operation_id} should run: {error}"));
    }
}

#[test]
fn max_bond_dimension_reaches_the_capacity() {
    // M0: None kept the base policy's cap (128), Some(7) set it to 7.
    let stack = counting_stack();
    assert_eq!(stack.max_bond(), DEFAULT_MAX_BOND);
    assert_eq!(DEFAULT_MAX_BOND, 128);
    let uncapped = stack.batch(20).expect("a chain");
    assert_eq!(uncapped.capacity().max_bond(), 128);
    let capped = counting_stack().with_max_bond_dimension(Some(7));
    assert_eq!(capped.max_bond(), 7);
    assert_eq!(capped.batch(20).expect("a chain").capacity().max_bond(), 7);
    assert_eq!(
        capped
            .per_shot(20)
            .expect("a chain")
            .0
            .capacity()
            .max_bond(),
        7
    );
    assert_eq!(
        counting_stack().with_max_bond_dimension(None).max_bond(),
        DEFAULT_MAX_BOND
    );
    // χ = 0 is no chain: refused before anything is prepared.
    let zero = counting_stack().with_max_bond_dimension(Some(0));
    let error = zero.sample(&Bytecode::new(2).h(0).mz(0, 0).build(), 1, Some(1));
    assert!(
        matches!(error, Err(StackError::Capacity { max_bond: 0, .. })),
        "{error:?}"
    );
    assert_eq!(zero.engine().prepared(), 0);
}

#[test]
fn replay_maps_qubits_and_leaves_output_to_control() {
    // M0's consumer test: qubit 1 is measured first, so it is column 0; the
    // records follow the program's record order, not the columns'.
    let program = Bytecode::new(2)
        .h(0)
        .mz(1, 0)
        .mz(0, 1)
        .record(1)
        .record(0)
        .build();
    let plan = plan(&program).expect("a Base program");
    assert_eq!(plan.columns, [1, 0]);
    // M0's matrix [0, 1, 1, 0], shot 1: row [1, 0].
    assert_eq!(
        replay(&program, &plan, &[true, false]).expect("replays"),
        [
            OutputRecord::Result(MeasurementResult::Zero),
            OutputRecord::Result(MeasurementResult::One),
        ]
    );
    assert_eq!(
        replay(&program, &plan, &[false, true]).expect("replays"),
        [
            OutputRecord::Result(MeasurementResult::One),
            OutputRecord::Result(MeasurementResult::Zero),
        ]
    );
}

#[test]
fn a_repeated_measurement_reads_its_qubits_column() {
    let program = Bytecode::new(2)
        .h(0)
        .mz(0, 0)
        .mz(0, 1)
        .record(0)
        .record(1)
        .build();
    let plan = plan(&program).expect("a Base program");
    assert_eq!(plan.columns, [0]);
    assert_eq!(
        replay(&program, &plan, &[true]).expect("replays"),
        [OutputRecord::Result(MeasurementResult::One); 2]
    );
}

#[test]
fn a_region_a_loop_runs_twice_is_applied_twice() {
    // M0 appended the region's gates at each execution: HH = I.
    let program = region_run_twice();
    let plan = plan(&program).expect("a Base program");
    assert_eq!(plan.regions.len(), 2);
    assert_eq!(plan.columns, [0]);
    let stack = MpsStack::new(tensornet::reference::ExactReference::new());
    let shots = stack.sample(&program, 64, Some(3)).expect("samples");
    assert!(
        shots
            .iter()
            .all(|records| *records == [OutputRecord::Result(MeasurementResult::Zero)]),
        "{shots:?}"
    );
    assert!((expectation(&stack, &program, "Z", &[0]) - 1.0).abs() < 1e-12);
}

#[test]
fn plan_rejects_programs_without_exactly_one_region() {
    let two = Bytecode::new(2).h(0).record(0).h(1).build();
    assert_eq!(
        plan(&two).err(),
        Some(ProgramError::RegionCount { found: 2 })
    );
    let none = Bytecode::new(2).mz(0, 0).record(0).build();
    assert_eq!(
        plan(&none).err(),
        Some(ProgramError::RegionCount { found: 0 })
    );
}

#[test]
fn plan_rejects_feedforward_after_a_measurement() {
    let program = Bytecode::new(2).mz(0, 0).h(1).build();
    assert_eq!(
        plan(&program).err(),
        Some(ProgramError::Feedforward { qubit: 0 })
    );
}

#[test]
fn plan_rejects_a_measurement_after_measure_reset_z() {
    let program = Bytecode::new(2)
        .gate(OPID_H, 0, 0, 0.0)
        .measurement(OPID_MRESETZ, 0, 0)
        .measurement(OPID_MZ, 0, 1)
        .build();
    assert_eq!(
        plan(&program).err(),
        Some(ProgramError::MeasurementAfterReset { qubit: 0 })
    );
}

#[test]
fn plan_rejects_reset() {
    let program = Bytecode::new(2).h(0).reset(0).mz(0, 0).build();
    assert_eq!(plan(&program).err(), Some(ProgramError::Reset { qubit: 0 }));
}

#[test]
fn plan_rejects_an_operation_or_a_measurement_beyond_the_program() {
    for program in [
        Bytecode::new(2).h(2).mz(0, 0),
        Bytecode::new(2).cx(1, 1).mz(0, 0),
        Bytecode::new(2).rz(f64::NAN, 0).mz(0, 0),
    ] {
        let Ok(program) = program.try_build() else {
            // `PreparedAdaptiveProgram` refused it first: nothing to plan.
            continue;
        };
        assert!(
            matches!(
                plan(&program),
                Err(ProgramError::Operation { region: 0, .. })
            ),
            "{:?}",
            plan(&program).err()
        );
    }
    if let Ok(program) = Bytecode::new(2).h(0).mz(2, 0).try_build() {
        assert_eq!(
            plan(&program).err(),
            Some(ProgramError::Qubit {
                qubit: 2,
                qubits: 2
            })
        );
    }
}
