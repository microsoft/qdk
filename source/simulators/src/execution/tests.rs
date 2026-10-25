// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests of the execution core: region partitioning, the adaptive walk and its
//! command protocol, the Runtime adapter, and the legacy simulators as
//! terminals of the region ISA, with parity against the legacy runtime on the
//! CPU and Clifford simulators.

use executor_protocol::{
    Batch, Close, ErrorKind, Executor, Fold, Immediate, Outcome, Rejected, Value, wait,
};

use super::{
    AdaptiveCommand, AdaptiveExecution, AdaptiveExecutionError, AdaptiveResponse, MeasuredQubit,
    MeasurementKind, MeasurementRequest, OP_QUANTUM_GATE, PreparedAdaptiveProgram,
    QuantumEvolutionRegion, RegionId, RegionInstruction, RegionPartitionError, RegionSite,
    RuntimeAdapter, RuntimeError, RuntimeReport, SimulatorReport, SimulatorTerminal,
    UnitaryOperation, partition_unitary_regions,
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

fn measure_without_output_program() -> AdaptiveProgram<u64> {
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
                aux2: 1,
                ..Instruction::default()
            },
            Instruction {
                opcode: 0x02,
                ..Instruction::default()
            },
        ],
        vec![Block {
            instr_offset: 0,
            instr_count: 3,
        }],
        vec![operation(5), operation(21)],
    );
    program.num_results = 3;
    program
}

fn unsupported_instruction_program() -> AdaptiveProgram<u64> {
    adaptive_program(
        vec![instruction(0x20, 0)],
        vec![Block {
            instr_offset: 0,
            instr_count: 1,
        }],
        Vec::new(),
    )
}

/// X q0; reset q0; mz q0 -> r0; ret (no output recording).
fn x_reset_measure_program() -> AdaptiveProgram<u64> {
    const IMMEDIATE_AUX1: u64 = 1 << 20;
    const IMMEDIATE_AUX2: u64 = 1 << 21;

    adaptive_program(
        vec![
            Instruction {
                opcode: OP_QUANTUM_GATE | IMMEDIATE_AUX1,
                aux0: 0,
                aux1: 0,
                ..Instruction::default()
            },
            Instruction {
                opcode: 0x12 | IMMEDIATE_AUX1,
                aux0: 1,
                aux1: 0,
                ..Instruction::default()
            },
            Instruction {
                opcode: 0x11 | IMMEDIATE_AUX1 | IMMEDIATE_AUX2,
                aux0: 2,
                aux1: 0,
                aux2: 0,
                ..Instruction::default()
            },
            Instruction {
                opcode: 0x02,
                ..Instruction::default()
            },
        ],
        vec![Block {
            instr_offset: 0,
            instr_count: 4,
        }],
        vec![operation(2), operation(1), operation(21)],
    )
}

/// The instruction of the region ISA a [`TestDoubleTerminal`] fails on.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TestFailure {
    ExecuteRegion,
    Measure,
    Reset,
}

/// What a [`TestDoubleTerminal`] ran, as a report.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct TestReport {
    regions: usize,
}

impl Fold for TestReport {
    fn fold(&mut self, other: Self) {
        self.regions += other.regions;
    }
}

/// A terminal of the region ISA with no state: every measurement reads
/// `measurement_result`. It can fail on one kind of instruction, or reject
/// every call from its `reject_from`-th on.
struct TestDoubleTerminal {
    failure: Option<TestFailure>,
    reject: Option<(usize, Rejected)>,
    measurement_result: MeasurementResult,
    calls: usize,
    report: TestReport,
    closed: bool,
}

impl TestDoubleTerminal {
    fn new(failure: Option<TestFailure>) -> Self {
        Self {
            failure,
            reject: None,
            measurement_result: MeasurementResult::Zero,
            calls: 0,
            report: TestReport::default(),
            closed: false,
        }
    }

    fn with_measurement_result(mut self, measurement_result: MeasurementResult) -> Self {
        self.measurement_result = measurement_result;
        self
    }

    fn rejecting_from(mut self, call: usize, rejected: Rejected) -> Self {
        self.reject = Some((call, rejected));
        self
    }
}

impl Executor<Batch<RegionInstruction>> for TestDoubleTerminal {
    type Response = Immediate<Outcome>;

    fn call(&mut self, batch: &[RegionInstruction]) -> Result<Self::Response, Rejected> {
        if self.closed {
            return Err(Rejected::Closed);
        }
        if let Some((from, rejected)) = &self.reject
            && self.calls >= *from
        {
            return Err(rejected.clone());
        }
        self.calls += 1;
        let mut outcomes = Vec::new();
        for instruction in batch {
            let kind = match instruction {
                RegionInstruction::ExecuteRegion { .. } => TestFailure::ExecuteRegion,
                RegionInstruction::Measure(_) => TestFailure::Measure,
                RegionInstruction::Reset { .. } => TestFailure::Reset,
            };
            if self.failure == Some(kind) {
                return Ok(Immediate::failed(outcomes, format!("{kind:?}")));
            }
            match instruction {
                RegionInstruction::ExecuteRegion { .. } => self.report.regions += 1,
                RegionInstruction::Measure(request) => outcomes.push(Outcome {
                    tag: u32::try_from(request.result_id).expect("test results fit a tag"),
                    value: self.measurement_result.into(),
                }),
                RegionInstruction::Reset { .. } => {}
            }
        }
        Ok(Immediate::done(outcomes))
    }
}

impl Close for TestDoubleTerminal {
    type Report = TestReport;

    fn close(&mut self) -> TestReport {
        self.closed = true;
        std::mem::take(&mut self.report)
    }
}

/// Runs `program` through the Runtime adapter on a new [`SimulatorTerminal`],
/// and returns its records and the simulator.
fn run_prepared_shot<S: Simulator>(
    program: &PreparedAdaptiveProgram<u64>,
    qubits: usize,
    results: usize,
    seed: u32,
) -> (Result<Vec<OutputRecord>, RuntimeError>, S) {
    let mut runtime = RuntimeAdapter::new(SimulatorTerminal::<S>::new(
        qubits,
        results,
        seed,
        Default::default(),
    ));
    let mut records = Vec::new();
    let ran = runtime.run(program, &mut records).map(|()| records);
    (ran, runtime.into_inner().into_simulator())
}

fn run_on_test_double(
    program: &PreparedAdaptiveProgram<u64>,
    terminal: TestDoubleTerminal,
) -> (Result<Vec<OutputRecord>, RuntimeError>, TestDoubleTerminal) {
    let mut runtime = RuntimeAdapter::new(terminal);
    let mut records = Vec::new();
    let ran = runtime.run(program, &mut records).map(|()| records);
    (ran, runtime.into_inner())
}

#[test]
fn terminal_executes_singleton_and_multi_operation_regions_in_order() {
    let singleton = QuantumEvolutionRegion::new([UnitaryOperation::X { target: 0 }]);
    let multi = QuantumEvolutionRegion::new([
        UnitaryOperation::I { target: 1 },
        UnitaryOperation::Cx {
            control: 0,
            target: 1,
        },
    ]);
    let mut terminal = SimulatorTerminal::<FullStateSimulator>::new(2, 2, 42, Default::default());

    let reply = terminal
        .call(&[
            RegionInstruction::ExecuteRegion {
                region_id: RegionId::new(0),
                region: singleton,
            },
            RegionInstruction::ExecuteRegion {
                region_id: RegionId::new(1),
                region: multi,
            },
            RegionInstruction::Measure(MeasurementRequest {
                kind: MeasurementKind::MeasureZ,
                qubit: 0,
                result_id: 0,
            }),
            RegionInstruction::Measure(MeasurementRequest {
                kind: MeasurementKind::MeasureZ,
                qubit: 1,
                result_id: 1,
            }),
        ])
        .expect("the batch is within the terminal's qubits and results");
    let mut outcomes = Vec::new();
    assert_eq!(wait(reply, &mut outcomes), Ok(()));

    assert_eq!(
        outcomes,
        [
            Outcome {
                tag: 0,
                value: Value::One
            },
            Outcome {
                tag: 1,
                value: Value::One
            }
        ]
    );
    assert_eq!(
        terminal.simulator().measurements(),
        [MeasurementResult::One, MeasurementResult::One]
    );
    assert_eq!(
        terminal.close(),
        SimulatorReport {
            batches: 1,
            regions: 2,
            operations: 3,
            measurements: 2,
            resets: 0,
        }
    );
    assert_eq!(terminal.close(), SimulatorReport::default());
}

#[test]
fn runtime_reports_its_programs_and_batches_and_the_terminals_report() {
    let program = PreparedAdaptiveProgram::new(measure_then_branch_program())
        .expect("measure-then-branch scenario should prepare");
    let mut runtime = RuntimeAdapter::new(TestDoubleTerminal::new(None));
    let mut records = Vec::new();

    runtime
        .run(&program, &mut records)
        .expect("test double should complete the prepared shot");

    assert_eq!(records, [OutputRecord::Result(MeasurementResult::Zero)]);
    // One batch per measurement: each ends at the measurement whose result
    // the walk needs.
    assert_eq!(
        runtime.close(),
        RuntimeReport {
            programs: 1,
            batches: 2,
            inner: TestReport { regions: 2 },
        }
    );
    assert!(runtime.inner().closed);
}

#[test]
fn no_output_fallback_preserves_loss_and_unmeasured_result_slots() {
    let program = PreparedAdaptiveProgram::new(measure_without_output_program())
        .expect("measurement-only scenario should prepare");
    let terminal = TestDoubleTerminal::new(None).with_measurement_result(MeasurementResult::Loss);

    let (records, _) = run_on_test_double(&program, terminal);

    assert_eq!(
        records,
        Ok(vec![
            OutputRecord::Result(MeasurementResult::Zero),
            OutputRecord::Result(MeasurementResult::Loss),
            OutputRecord::Result(MeasurementResult::Zero),
        ])
    );
}

#[test]
fn terminal_no_output_fallback_matches_simulator_measurement_register() {
    let program = PreparedAdaptiveProgram::new(measure_without_output_program())
        .expect("measurement-only scenario should prepare");

    let (actual, simulator) = run_prepared_shot::<FullStateSimulator>(&program, 2, 3, 42);
    let actual = actual.expect("supported prepared scenario should execute");
    let expected = simulator
        .measurements()
        .iter()
        .copied()
        .map(OutputRecord::Result)
        .collect::<Vec<_>>();

    assert_eq!(actual, expected);
    assert_eq!(
        actual,
        [
            OutputRecord::Result(MeasurementResult::Zero),
            OutputRecord::Result(MeasurementResult::One),
            OutputRecord::Result(MeasurementResult::Zero),
        ]
    );
}

#[test]
fn runtime_propagates_execute_region_failure() {
    let program = PreparedAdaptiveProgram::new(measure_then_branch_program())
        .expect("measure-then-branch scenario should prepare");

    let (records, _) = run_on_test_double(
        &program,
        TestDoubleTerminal::new(Some(TestFailure::ExecuteRegion)),
    );

    assert_eq!(records, Err(RuntimeError::Failed("ExecuteRegion".into())));
}

#[test]
fn runtime_propagates_measurement_failure() {
    let program = PreparedAdaptiveProgram::new(measure_then_branch_program())
        .expect("measure-then-branch scenario should prepare");

    let (records, _) = run_on_test_double(
        &program,
        TestDoubleTerminal::new(Some(TestFailure::Measure)),
    );

    assert_eq!(records, Err(RuntimeError::Failed("Measure".into())));
}

#[test]
fn runtime_propagates_terminal_rejection() {
    let program = PreparedAdaptiveProgram::new(measure_then_branch_program())
        .expect("measure-then-branch scenario should prepare");
    let terminal =
        TestDoubleTerminal::new(None).rejecting_from(1, Rejected::Unavailable("busy".into()));

    let (records, terminal) = run_on_test_double(&program, terminal);

    assert_eq!(
        records,
        Err(RuntimeError::Rejected(Rejected::Unavailable("busy".into())))
    );
    assert_eq!(terminal.calls, 1);
}

#[test]
fn runtime_propagates_control_error() {
    let program = PreparedAdaptiveProgram::new(unsupported_instruction_program())
        .expect("unsupported classical control should still prepare");

    let (records, terminal) = run_on_test_double(&program, TestDoubleTerminal::new(None));

    assert_eq!(
        records,
        Err(RuntimeError::Control(
            AdaptiveExecutionError::UnsupportedInstruction {
                opcode: 0x20,
                instruction_index: 0,
            }
        ))
    );
    assert_eq!(terminal.calls, 0);
}

#[test]
fn runtime_reports_a_measurement_error_it_cannot_branch_on() {
    struct TestDoubleErrorTerminal;

    impl Executor<Batch<RegionInstruction>> for TestDoubleErrorTerminal {
        type Response = Immediate<Outcome>;

        fn call(&mut self, batch: &[RegionInstruction]) -> Result<Self::Response, Rejected> {
            let outcomes = batch
                .iter()
                .filter_map(|instruction| match instruction {
                    RegionInstruction::Measure(request) => Some(Outcome {
                        tag: u32::try_from(request.result_id).expect("test results fit a tag"),
                        value: Value::Error(ErrorKind::Other),
                    }),
                    _ => None,
                })
                .collect();
            Ok(Immediate::done(outcomes))
        }
    }

    let program = PreparedAdaptiveProgram::new(measure_then_branch_program())
        .expect("measure-then-branch scenario should prepare");
    let mut runtime = RuntimeAdapter::new(TestDoubleErrorTerminal);

    assert_eq!(
        runtime.run(&program, &mut Vec::new()),
        Err(RuntimeError::MeasurementError { result_id: 0 })
    );
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
    const IMMEDIATE_AUX1: u64 = 1 << 20;
    const IMMEDIATE_AUX2: u64 = 1 << 21;

    let program = adaptive_program(
        vec![
            instruction(OP_QUANTUM_GATE, 0),
            instruction(OP_QUANTUM_GATE, 1),
            Instruction {
                opcode: 0x11 | IMMEDIATE_AUX1 | IMMEDIATE_AUX2,
                aux0: 2,
                aux1: 1,
                aux2: 0,
                ..Instruction::default()
            },
        ],
        vec![Block {
            instr_offset: 0,
            instr_count: 3,
        }],
        vec![operation(5), operation(15), operation(21)],
    );

    let prepared =
        PreparedAdaptiveProgram::new(program).expect("well-formed adaptive control should prepare");
    assert_eq!(prepared.program().num_qubits, 2);
    assert_eq!(
        prepared
            .measured_qubits()
            .expect("measurement operands should be immediate"),
        [MeasuredQubit {
            qubit: 1,
            result_id: 0,
        }]
    );
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
fn prepared_adaptive_program_has_no_measured_qubits_without_measurements() {
    let program = adaptive_program(
        vec![instruction(OP_QUANTUM_GATE, 0)],
        vec![Block {
            instr_offset: 0,
            instr_count: 1,
        }],
        vec![operation(5)],
    );

    let prepared =
        PreparedAdaptiveProgram::new(program).expect("unitary-only control should prepare");

    assert!(
        prepared
            .measured_qubits()
            .expect("unitary-only metadata should be available")
            .is_empty()
    );
}

#[test]
fn prepared_adaptive_program_orders_mid_circuit_measurements() {
    let prepared = PreparedAdaptiveProgram::new(measure_then_branch_program())
        .expect("measure-then-branch scenario should prepare");

    assert_eq!(
        prepared
            .measured_qubits()
            .expect("measurement operands should be immediate"),
        [
            MeasuredQubit {
                qubit: 0,
                result_id: 0,
            },
            MeasuredQubit {
                qubit: 1,
                result_id: 1,
            },
        ]
    );
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
    let unsupported_instruction = PreparedAdaptiveProgram::new(unsupported_instruction_program())
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

            let expected = run_shot(&legacy_program, &mut legacy);
            let (actual, prepared) = run_prepared_shot::<S>(&prepared_program, 2, 2, seed);
            let actual = actual.expect("supported prepared scenario should execute");

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

#[test]
fn reset_is_a_command_between_regions_and_measurements() {
    let program = PreparedAdaptiveProgram::new(x_reset_measure_program())
        .expect("reset scenario should prepare");
    assert_eq!(program.regions().len(), 1);

    let mut execution = AdaptiveExecution::new(&program);
    assert!(matches!(
        execution.next_command(None),
        Ok(AdaptiveCommand::ExecuteRegion { .. })
    ));
    assert_eq!(
        execution.next_command(Some(AdaptiveResponse::RegionComplete)),
        Ok(AdaptiveCommand::Reset { qubit: 0 })
    );
    assert_eq!(
        execution.next_command(Some(AdaptiveResponse::ResetComplete)),
        Ok(AdaptiveCommand::Measure(MeasurementRequest {
            kind: MeasurementKind::MeasureZ,
            qubit: 0,
            result_id: 0,
        }))
    );
    assert_eq!(
        execution.next_command(Some(AdaptiveResponse::Measurement(MeasurementResult::One))),
        Ok(AdaptiveCommand::Complete(vec![OutputRecord::Result(
            MeasurementResult::One
        )]))
    );
}

#[test]
fn reset_requires_its_own_completion_response() {
    let program = PreparedAdaptiveProgram::new(x_reset_measure_program())
        .expect("reset scenario should prepare");
    let reach_reset = |execution: &mut AdaptiveExecution<'_>| {
        execution.next_command(None).expect("region");
        assert_eq!(
            execution.next_command(Some(AdaptiveResponse::RegionComplete)),
            Ok(AdaptiveCommand::Reset { qubit: 0 })
        );
    };

    let mut missing = AdaptiveExecution::new(&program);
    reach_reset(&mut missing);
    assert_eq!(
        missing.next_command(None),
        Err(AdaptiveExecutionError::MissingResponse)
    );

    let mut wrong = AdaptiveExecution::new(&program);
    reach_reset(&mut wrong);
    assert_eq!(
        wrong.next_command(Some(AdaptiveResponse::RegionComplete)),
        Err(AdaptiveExecutionError::UnexpectedResponse)
    );

    let mut unexpected = AdaptiveExecution::new(&program);
    unexpected.next_command(None).expect("region");
    assert_eq!(
        unexpected.next_command(Some(AdaptiveResponse::ResetComplete)),
        Err(AdaptiveExecutionError::UnexpectedResponse)
    );
}

#[test]
fn adaptive_execution_rejects_invalid_reset_operations() {
    let reset_program = |operations| {
        PreparedAdaptiveProgram::new(adaptive_program(
            vec![instruction(0x12, 0)],
            vec![Block {
                instr_offset: 0,
                instr_count: 1,
            }],
            operations,
        ))
        .expect("reset should prepare")
    };

    let unsupported = reset_program(vec![operation(21)]);
    assert_eq!(
        AdaptiveExecution::new(&unsupported).next_command(None),
        Err(AdaptiveExecutionError::UnsupportedReset {
            operation_id: 21,
            instruction_index: 0,
        })
    );

    let missing = reset_program(Vec::new());
    assert_eq!(
        AdaptiveExecution::new(&missing).next_command(None),
        Err(AdaptiveExecutionError::InvalidResetOperationIndex {
            operation_index: 0,
            instruction_index: 0,
        })
    );
}

#[test]
fn terminal_resets_and_matches_legacy_cpu_and_clifford() {
    fn assert_parity<S>()
    where
        S: Simulator,
        S::Noise: Default,
    {
        for seed in 0..8 {
            let mut legacy = S::new(2, 1, seed, Default::default());
            let prepared_program = PreparedAdaptiveProgram::new(x_reset_measure_program())
                .expect("reset scenario should prepare");

            run_shot(&x_reset_measure_program(), &mut legacy);
            let (actual, prepared) = run_prepared_shot::<S>(&prepared_program, 2, 1, seed);
            let actual = actual.expect("reset scenario should execute");

            assert_eq!(prepared.measurements(), legacy.measurements());
            assert_eq!(prepared.measurements(), &[MeasurementResult::Zero]);
            assert_eq!(actual, vec![OutputRecord::Result(MeasurementResult::Zero)]);
        }
    }

    assert_parity::<FullStateSimulator>();
    assert_parity::<StabilizerSimulator>();
}

#[test]
fn runtime_propagates_reset_failure() {
    let program = PreparedAdaptiveProgram::new(x_reset_measure_program())
        .expect("reset scenario should prepare");

    let (records, _) =
        run_on_test_double(&program, TestDoubleTerminal::new(Some(TestFailure::Reset)));

    assert_eq!(records, Err(RuntimeError::Failed("Reset".into())));
}

/// The shape `qdk.stim.compile` emits for `SELECT`/`REQUIRE`:
///
/// ```text
/// block 0: H q0; mresetz q0 -> r0
///          l = read_loss r0; r = read_result r0; restart = or l, r
///          branch restart ? block 1 : block 2
/// block 1: X q1; mz q1 -> r1; ret        (restart path)
/// block 2: mz q1 -> r1; ret              (continue path)
/// ```
///
/// `or_immediate_rhs` replaces `r` with an immediate `0`, so the `src1`
/// immediate flag is exercised too.
fn loss_or_branch_program(or_immediate_rhs: bool) -> AdaptiveProgram<u64> {
    const IMMEDIATE_SRC0: u64 = 1 << 16;
    const IMMEDIATE_SRC1: u64 = 1 << 17;
    const IMMEDIATE_AUX1: u64 = 1 << 20;
    const IMMEDIATE_AUX2: u64 = 1 << 21;
    let measure = |operation_index, qubit, result| Instruction {
        opcode: 0x11 | IMMEDIATE_AUX1 | IMMEDIATE_AUX2,
        aux0: operation_index,
        aux1: qubit,
        aux2: result,
        ..Instruction::default()
    };
    let gate = |operation_index, qubit| Instruction {
        opcode: OP_QUANTUM_GATE | IMMEDIATE_AUX1,
        aux0: operation_index,
        aux1: qubit,
        ..Instruction::default()
    };
    let ret = Instruction {
        opcode: 0x02,
        ..Instruction::default()
    };

    let mut program = adaptive_program(
        vec![
            gate(0, 0),
            measure(1, 0, 0),
            Instruction {
                opcode: 0x15 | IMMEDIATE_SRC0,
                dst: 0,
                src0: 0,
                ..Instruction::default()
            },
            Instruction {
                opcode: 0x13 | IMMEDIATE_SRC0,
                dst: 1,
                src0: 0,
                ..Instruction::default()
            },
            Instruction {
                opcode: 0x29 | if or_immediate_rhs { IMMEDIATE_SRC1 } else { 0 },
                dst: 2,
                src0: 0,
                src1: u64::from(!or_immediate_rhs),
                ..Instruction::default()
            },
            Instruction {
                opcode: 0x05,
                src0: 2,
                aux0: 1,
                aux1: 2,
                ..Instruction::default()
            },
            gate(2, 1),
            measure(3, 1, 1),
            ret,
            measure(3, 1, 1),
            ret,
        ],
        vec![
            Block {
                instr_offset: 0,
                instr_count: 6,
            },
            Block {
                instr_offset: 6,
                instr_count: 3,
            },
            Block {
                instr_offset: 9,
                instr_count: 2,
            },
        ],
        vec![operation(5), operation(22), operation(2), operation(21)],
    );
    program.num_results = 2;
    program.num_registers = 3;
    program
}

#[test]
fn read_loss_or_branch_restarts_on_loss_or_one() {
    for (or_immediate_rhs, outcome, restarts) in [
        (false, MeasurementResult::Zero, false),
        (false, MeasurementResult::One, true),
        (false, MeasurementResult::Loss, true),
        (true, MeasurementResult::Zero, false),
        (true, MeasurementResult::One, false),
        (true, MeasurementResult::Loss, true),
    ] {
        let program = PreparedAdaptiveProgram::new(loss_or_branch_program(or_immediate_rhs))
            .expect("loss-or-branch scenario should prepare");
        let mut execution = AdaptiveExecution::new(&program);
        execution.next_command(None).expect("leading region");
        assert!(matches!(
            execution.next_command(Some(AdaptiveResponse::RegionComplete)),
            Ok(AdaptiveCommand::Measure(_))
        ));
        let next = execution
            .next_command(Some(AdaptiveResponse::Measurement(outcome)))
            .expect("classical control should continue");
        let case = format!("immediate rhs {or_immediate_rhs}, outcome {outcome:?}");
        if restarts {
            let AdaptiveCommand::ExecuteRegion { region, .. } = next else {
                panic!("{case}: expected the restart region, got {next:?}");
            };
            assert_eq!(
                region.operations(),
                &[UnitaryOperation::X { target: 1 }],
                "{case}"
            );
        } else {
            assert!(
                matches!(next, AdaptiveCommand::Measure(request) if request.qubit == 1),
                "{case}: expected the continue path, got {next:?}"
            );
        }
    }
}

#[test]
fn read_loss_or_branch_matches_legacy_cpu_and_clifford() {
    fn assert_parity<S>()
    where
        S: Simulator,
        S::Noise: Default,
    {
        let mut observed = [false; 2];
        for seed in 0..64 {
            let mut legacy = S::new(2, 2, seed, Default::default());
            let prepared_program = PreparedAdaptiveProgram::new(loss_or_branch_program(false))
                .expect("loss-or-branch scenario should prepare");

            run_shot(&loss_or_branch_program(false), &mut legacy);
            let (actual, prepared) = run_prepared_shot::<S>(&prepared_program, 2, 2, seed);
            actual.expect("loss-or-branch scenario should execute");

            assert_eq!(prepared.measurements(), legacy.measurements());
            // r1 = 1 exactly on the restart path, which applies X q1.
            assert_eq!(prepared.measurements()[1], prepared.measurements()[0]);
            observed[usize::from(prepared.measurements()[0] == MeasurementResult::One)] = true;
        }
        assert_eq!(observed, [true, true]);
    }

    assert_parity::<FullStateSimulator>();
    assert_parity::<StabilizerSimulator>();
}

#[test]
fn unitary_operation_names_are_the_variant_names() {
    let operations = [
        (UnitaryOperation::I { target: 0 }, "I"),
        (UnitaryOperation::X { target: 0 }, "X"),
        (UnitaryOperation::Y { target: 0 }, "Y"),
        (UnitaryOperation::Z { target: 0 }, "Z"),
        (UnitaryOperation::H { target: 0 }, "H"),
        (UnitaryOperation::S { target: 0 }, "S"),
        (UnitaryOperation::SAdj { target: 0 }, "SAdj"),
        (UnitaryOperation::Sx { target: 0 }, "Sx"),
        (UnitaryOperation::SxAdj { target: 0 }, "SxAdj"),
        (UnitaryOperation::T { target: 0 }, "T"),
        (UnitaryOperation::TAdj { target: 0 }, "TAdj"),
        (
            UnitaryOperation::Rx {
                angle: 0.5,
                target: 0,
            },
            "Rx",
        ),
        (
            UnitaryOperation::Ry {
                angle: 0.5,
                target: 0,
            },
            "Ry",
        ),
        (
            UnitaryOperation::Rz {
                angle: 0.5,
                target: 0,
            },
            "Rz",
        ),
        (
            UnitaryOperation::Cx {
                control: 0,
                target: 1,
            },
            "Cx",
        ),
        (
            UnitaryOperation::Cy {
                control: 0,
                target: 1,
            },
            "Cy",
        ),
        (
            UnitaryOperation::Cz {
                control: 0,
                target: 1,
            },
            "Cz",
        ),
        (
            UnitaryOperation::Rxx {
                angle: 0.5,
                q1: 0,
                q2: 1,
            },
            "Rxx",
        ),
        (
            UnitaryOperation::Ryy {
                angle: 0.5,
                q1: 0,
                q2: 1,
            },
            "Ryy",
        ),
        (
            UnitaryOperation::Rzz {
                angle: 0.5,
                q1: 0,
                q2: 1,
            },
            "Rzz",
        ),
        (UnitaryOperation::Swap { q1: 0, q2: 1 }, "Swap"),
    ];
    for (operation, name) in operations {
        assert_eq!(operation.name(), name);
    }
}

mod protocol;
mod sessions;
