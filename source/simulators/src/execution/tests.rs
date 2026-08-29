// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::{convert::Infallible, mem::size_of};

use super::{
    AdaptiveCommand, AdaptiveExecution, AdaptiveExecutionError, AdaptiveResponse,
    ImmediatePreparedRegion, ImmediateSimulatorConsumer, MeasurementKind, MeasurementRequest,
    OP_QUANTUM_GATE, PreparedAdaptiveProgram, QuantumEvolutionRegion, RegionConsumer, RegionId,
    RegionPartitionError, RegionSite, UnitaryOperation, partition_unitary_regions,
    run_prepared_shot,
};
use crate::{
    MeasurementResult, OutputRecord, Simulator,
    bytecode::{AdaptiveProgram, Block, Instruction, Op, runtime::run_shot},
    cpu_full_state_simulator::FullStateSimulator,
    stabilizer_simulator::StabilizerSimulator,
};

fn instruction(opcode: u64, operation_index: u64) -> Instruction<u64> {
    Instruction {
        opcode,
        aux0: operation_index,
        ..Instruction::default()
    }
}

fn operation(operation_id: u64) -> Op<u64> {
    Op {
        op_id: operation_id,
        q1: 0,
        q2: 0,
        q3: 0,
        angle: 0,
    }
}

fn adaptive_program(
    instructions: Vec<Instruction<u64>>,
    blocks: Vec<Block<u64>>,
    quantum_ops: Vec<Op<u64>>,
) -> AdaptiveProgram<u64> {
    AdaptiveProgram {
        num_qubits: 2,
        num_results: 1,
        num_registers: 1,
        entry_block: 0,
        instructions,
        block_table: blocks,
        function_table: Vec::new(),
        phi_entries: Vec::new(),
        switch_cases: Vec::new(),
        call_args: Vec::new(),
        constant_data: Vec::new(),
        quantum_ops,
    }
}

fn measure_then_branch_program() -> AdaptiveProgram<u64> {
    const IMMEDIATE_SRC0: u64 = 1 << 16;
    const IMMEDIATE_AUX1: u64 = 1 << 20;
    const IMMEDIATE_AUX2: u64 = 1 << 21;

    let mut program = adaptive_program(
        vec![
            Instruction {
                opcode: OP_QUANTUM_GATE | IMMEDIATE_AUX1,
                aux0: 0,
                aux1: 0,
                ..Instruction::default()
            },
            Instruction {
                opcode: 0x11 | IMMEDIATE_AUX1 | IMMEDIATE_AUX2,
                aux0: 1,
                aux1: 0,
                aux2: 0,
                ..Instruction::default()
            },
            Instruction {
                opcode: 0x13 | IMMEDIATE_SRC0,
                dst: 0,
                src0: 0,
                ..Instruction::default()
            },
            Instruction {
                opcode: 0x05,
                src0: 0,
                aux0: 1,
                aux1: 2,
                ..Instruction::default()
            },
            Instruction {
                opcode: OP_QUANTUM_GATE | IMMEDIATE_AUX1,
                aux0: 2,
                aux1: 1,
                ..Instruction::default()
            },
            Instruction {
                opcode: 0x04,
                dst: 3,
                ..Instruction::default()
            },
            Instruction {
                opcode: OP_QUANTUM_GATE | IMMEDIATE_AUX1,
                aux0: 3,
                aux1: 1,
                ..Instruction::default()
            },
            Instruction {
                opcode: 0x04,
                dst: 3,
                ..Instruction::default()
            },
            Instruction {
                opcode: 0x11 | IMMEDIATE_AUX1 | IMMEDIATE_AUX2,
                aux0: 4,
                aux1: 1,
                aux2: 1,
                ..Instruction::default()
            },
            Instruction {
                opcode: 0x14 | IMMEDIATE_SRC0,
                src0: 1,
                aux1: 0,
                ..Instruction::default()
            },
            Instruction {
                opcode: 0x02,
                ..Instruction::default()
            },
        ],
        vec![
            Block {
                instr_offset: 0,
                instr_count: 4,
            },
            Block {
                instr_offset: 4,
                instr_count: 2,
            },
            Block {
                instr_offset: 6,
                instr_count: 2,
            },
            Block {
                instr_offset: 8,
                instr_count: 3,
            },
        ],
        vec![
            operation(5),
            operation(22),
            operation(2),
            operation(4),
            operation(21),
        ],
    );
    program.num_results = 2;
    program
}

#[test]
fn immediate_consumer_executes_singleton_and_multi_operation_regions_in_order() {
    let mut simulator = FullStateSimulator::new(2, 2, 42, Default::default());
    let singleton = QuantumEvolutionRegion::new([UnitaryOperation::X { target: 0 }]);
    let multi = QuantumEvolutionRegion::new([
        UnitaryOperation::I { target: 1 },
        UnitaryOperation::Cx {
            control: 0,
            target: 1,
        },
    ]);

    let mut consumer = ImmediateSimulatorConsumer::new(&mut simulator);
    let prepared = consumer
        .prepare_region(&singleton)
        .expect("immediate preparation is infallible");
    assert_eq!(
        consumer
            .execute_region(prepared)
            .expect("immediate execution is infallible")
            .operation_count,
        1
    );
    let prepared = consumer
        .prepare_region(&multi)
        .expect("immediate preparation is infallible");
    assert_eq!(
        consumer
            .execute_region(prepared)
            .expect("immediate execution is infallible")
            .operation_count,
        2
    );
    consumer
        .finish_execution()
        .expect("immediate completion is infallible");
    consumer.close().expect("immediate close is infallible");
    consumer
        .close()
        .expect("immediate close remains idempotent");

    simulator.mz(0, 0);
    simulator.mz(1, 1);
    assert_eq!(
        simulator.measurements(),
        [MeasurementResult::One, MeasurementResult::One]
    );
}

#[test]
fn immediate_preparation_token_is_only_a_borrowed_slice() {
    fn assert_infallible<C: RegionConsumer<Error = Infallible>>() {}

    assert_eq!(
        size_of::<ImmediatePreparedRegion<'_>>(),
        size_of::<&[UnitaryOperation]>()
    );
    assert_infallible::<ImmediateSimulatorConsumer<'static, FullStateSimulator>>();
}

#[test]
fn partitioner_splits_at_classical_events_outputs_and_block_boundaries() {
    let program = adaptive_program(
        vec![
            instruction(0x20, 0),
            instruction(OP_QUANTUM_GATE, 0),
            instruction(OP_QUANTUM_GATE, 1),
            instruction(0x20, 0),
            instruction(OP_QUANTUM_GATE, 2),
            instruction(0x11, 0),
            instruction(OP_QUANTUM_GATE, 3),
            instruction(OP_QUANTUM_GATE, 4),
            instruction(OP_QUANTUM_GATE, 5),
            instruction(0x14, 0),
        ],
        vec![
            Block {
                instr_offset: 0,
                instr_count: 6,
            },
            Block {
                instr_offset: 6,
                instr_count: 4,
            },
        ],
        vec![
            operation(5),
            operation(15),
            operation(2),
            operation(28),
            operation(0),
            operation(29),
        ],
    );

    assert_eq!(
        partition_unitary_regions(&program),
        Ok(vec![
            RegionSite {
                id: RegionId::new(0),
                block_id: 0,
                instruction_range: 1..3,
            },
            RegionSite {
                id: RegionId::new(1),
                block_id: 0,
                instruction_range: 4..5,
            },
            RegionSite {
                id: RegionId::new(2),
                block_id: 1,
                instruction_range: 7..9,
            },
        ])
    );
}

#[test]
fn partitioner_rejects_invalid_block_and_operation_references() {
    let invalid_block = adaptive_program(
        vec![instruction(OP_QUANTUM_GATE, 0)],
        vec![Block {
            instr_offset: 1,
            instr_count: 1,
        }],
        vec![operation(5)],
    );
    assert_eq!(
        partition_unitary_regions(&invalid_block),
        Err(RegionPartitionError::InvalidBlockRange {
            block_id: 0,
            start: 1,
            end: 2,
        })
    );

    let invalid_operation = adaptive_program(
        vec![instruction(OP_QUANTUM_GATE, 1)],
        vec![Block {
            instr_offset: 0,
            instr_count: 1,
        }],
        vec![operation(5)],
    );
    assert_eq!(
        partition_unitary_regions(&invalid_operation),
        Err(RegionPartitionError::InvalidOperationIndex {
            instruction_index: 0,
            operation_index: 1,
        })
    );
}

#[test]
fn prepared_adaptive_program_retains_control_and_computes_regions_once() {
    let program = adaptive_program(
        vec![
            instruction(OP_QUANTUM_GATE, 0),
            instruction(OP_QUANTUM_GATE, 1),
            instruction(0x11, 0),
        ],
        vec![Block {
            instr_offset: 0,
            instr_count: 3,
        }],
        vec![operation(5), operation(15)],
    );

    let prepared =
        PreparedAdaptiveProgram::new(program).expect("well-formed adaptive control should prepare");
    assert_eq!(prepared.program().num_qubits, 2);
    assert_eq!(
        prepared.regions(),
        [RegionSite {
            id: RegionId::new(0),
            block_id: 0,
            instruction_range: 0..2,
        }]
    );
    assert_eq!(prepared.into_program().instructions.len(), 3);
}

#[test]
fn adaptive_measurement_result_selects_only_the_reached_region() {
    fn run_case(
        branch_result: MeasurementResult,
        expected_region: RegionId,
        expected_operation: UnitaryOperation,
    ) {
        let program = PreparedAdaptiveProgram::new(measure_then_branch_program())
            .expect("measure-then-branch scenario should prepare");
        let mut execution = AdaptiveExecution::new(&program);

        assert_eq!(
            execution.next_command(None),
            Ok(AdaptiveCommand::ExecuteRegion {
                region_id: RegionId::new(0),
                region: QuantumEvolutionRegion::new([UnitaryOperation::H { target: 0 }]),
            })
        );
        assert_eq!(
            execution.next_command(Some(AdaptiveResponse::RegionComplete)),
            Ok(AdaptiveCommand::Measure(MeasurementRequest {
                kind: MeasurementKind::MeasureResetZ,
                qubit: 0,
                result_id: 0,
            }))
        );
        assert_eq!(
            execution.next_command(Some(AdaptiveResponse::Measurement(branch_result))),
            Ok(AdaptiveCommand::ExecuteRegion {
                region_id: expected_region,
                region: QuantumEvolutionRegion::new([expected_operation]),
            })
        );
        assert_eq!(
            execution.next_command(Some(AdaptiveResponse::RegionComplete)),
            Ok(AdaptiveCommand::Measure(MeasurementRequest {
                kind: MeasurementKind::MeasureZ,
                qubit: 1,
                result_id: 1,
            }))
        );
        assert_eq!(
            execution.next_command(Some(AdaptiveResponse::Measurement(branch_result))),
            Ok(AdaptiveCommand::Complete(vec![OutputRecord::Result(
                branch_result
            )]))
        );
    }

    run_case(
        MeasurementResult::One,
        RegionId::new(1),
        UnitaryOperation::X { target: 1 },
    );
    run_case(
        MeasurementResult::Zero,
        RegionId::new(2),
        UnitaryOperation::Z { target: 1 },
    );
}

#[test]
fn adaptive_execution_reports_typed_control_failures() {
    let unsupported_instruction = adaptive_program(
        vec![instruction(0x20, 0)],
        vec![Block {
            instr_offset: 0,
            instr_count: 1,
        }],
        Vec::new(),
    );
    let unsupported_instruction = PreparedAdaptiveProgram::new(unsupported_instruction)
        .expect("unsupported classical control should still prepare");
    assert_eq!(
        AdaptiveExecution::new(&unsupported_instruction).next_command(None),
        Err(AdaptiveExecutionError::UnsupportedInstruction {
            opcode: 0x20,
            instruction_index: 0,
        })
    );

    let unsupported_measurement = adaptive_program(
        vec![instruction(0x11, 0)],
        vec![Block {
            instr_offset: 0,
            instr_count: 1,
        }],
        vec![operation(999)],
    );
    let unsupported_measurement = PreparedAdaptiveProgram::new(unsupported_measurement)
        .expect("unsupported measurement should still prepare");
    assert_eq!(
        AdaptiveExecution::new(&unsupported_measurement).next_command(None),
        Err(AdaptiveExecutionError::UnsupportedMeasurement {
            operation_id: 999,
            instruction_index: 0,
        })
    );
}

#[test]
fn adaptive_execution_enforces_command_response_protocol() {
    let program = PreparedAdaptiveProgram::new(measure_then_branch_program())
        .expect("measure-then-branch scenario should prepare");

    let mut unexpected_initial_response = AdaptiveExecution::new(&program);
    assert_eq!(
        unexpected_initial_response.next_command(Some(AdaptiveResponse::RegionComplete)),
        Err(AdaptiveExecutionError::UnexpectedResponse)
    );

    let mut missing_region_response = AdaptiveExecution::new(&program);
    assert!(matches!(
        missing_region_response.next_command(None),
        Ok(AdaptiveCommand::ExecuteRegion { .. })
    ));
    assert_eq!(
        missing_region_response.next_command(None),
        Err(AdaptiveExecutionError::MissingResponse)
    );

    let mut wrong_region_response = AdaptiveExecution::new(&program);
    assert!(matches!(
        wrong_region_response.next_command(None),
        Ok(AdaptiveCommand::ExecuteRegion { .. })
    ));
    assert_eq!(
        wrong_region_response.next_command(Some(AdaptiveResponse::Measurement(
            MeasurementResult::Zero,
        ))),
        Err(AdaptiveExecutionError::UnexpectedResponse)
    );

    let mut missing_measurement_response = AdaptiveExecution::new(&program);
    assert!(matches!(
        missing_measurement_response.next_command(None),
        Ok(AdaptiveCommand::ExecuteRegion { .. })
    ));
    assert!(matches!(
        missing_measurement_response.next_command(Some(AdaptiveResponse::RegionComplete)),
        Ok(AdaptiveCommand::Measure(_))
    ));
    assert_eq!(
        missing_measurement_response.next_command(None),
        Err(AdaptiveExecutionError::MissingResponse)
    );

    let mut wrong_measurement_response = AdaptiveExecution::new(&program);
    assert!(matches!(
        wrong_measurement_response.next_command(None),
        Ok(AdaptiveCommand::ExecuteRegion { .. })
    ));
    assert!(matches!(
        wrong_measurement_response.next_command(Some(AdaptiveResponse::RegionComplete)),
        Ok(AdaptiveCommand::Measure(_))
    ));
    assert_eq!(
        wrong_measurement_response.next_command(Some(AdaptiveResponse::RegionComplete)),
        Err(AdaptiveExecutionError::UnexpectedResponse)
    );
}

#[test]
fn prepared_measurement_branch_matches_legacy_cpu_and_clifford() {
    fn assert_parity<S>()
    where
        S: Simulator,
        S::Noise: Default,
    {
        let mut observed_results = [false; 2];
        for seed in 0..64 {
            let legacy_program = measure_then_branch_program();
            let prepared_program = PreparedAdaptiveProgram::new(measure_then_branch_program())
                .expect("measure-then-branch scenario should prepare");
            let mut legacy = S::new(2, 2, seed, Default::default());
            let mut prepared = S::new(2, 2, seed, Default::default());

            let expected = run_shot(&legacy_program, &mut legacy);
            let actual = run_prepared_shot(&prepared_program, &mut prepared)
                .expect("supported prepared scenario should execute");

            assert_eq!(actual, expected);
            assert_eq!(prepared.measurements(), legacy.measurements());
            match actual.as_slice() {
                [OutputRecord::Result(MeasurementResult::Zero)] => observed_results[0] = true,
                [OutputRecord::Result(MeasurementResult::One)] => observed_results[1] = true,
                output => panic!("unexpected output {output:?}"),
            }
        }
        assert_eq!(observed_results, [true, true]);
    }

    assert_parity::<FullStateSimulator>();
    assert_parity::<StabilizerSimulator>();
}