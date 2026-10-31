// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{BackendSimulator, run_adaptive_program};
use crate::{
    BatchResult, Batcher, DEFAULT_MAX_BATCH_SIZE, OutputValue,
    Parameter::{Float, QubitId},
    QuantumBackend, QuantumBackendInstruction, QuantumBackendResult, QuantumBackends,
};
use qdk_simulators::{
    MeasurementResult, Simulator,
    bytecode::{AdaptiveProgram, Block, Instruction, Op},
};
use smallvec::smallvec;
use std::collections::VecDeque;

const SRC0_IMM: u64 = 1 << 16;
const SRC1_IMM: u64 = 1 << 17;
const DST_IMM: u64 = 1 << 18;
const AUX1_IMM: u64 = 1 << 20;
const AUX2_IMM: u64 = 1 << 21;

#[derive(Default)]
struct Recorder {
    batches: Vec<Vec<QuantumBackendInstruction>>,
    responses: VecDeque<Result<BatchResult, String>>,
}

impl QuantumBackend for Recorder {
    /// Records a batch and returns its configured response.
    fn execute_batch(
        &mut self,
        instructions: &[QuantumBackendInstruction],
    ) -> Result<BatchResult, String> {
        self.batches.push(instructions.to_vec());
        self.responses
            .pop_front()
            .unwrap_or(Ok(BatchResult::default()))
    }
}

/// Creates a minimal program with one block and static qubit/result resources.
fn program(instructions: Vec<Instruction<u64>>, quantum_ops: Vec<Op<u64>>) -> AdaptiveProgram<u64> {
    AdaptiveProgram {
        num_qubits: 2,
        num_results: 2,
        num_registers: 1,
        entry_block: 0,
        block_table: vec![Block {
            instr_offset: 0,
            instr_count: u64::try_from(instructions.len()).expect("test program fits in u64"),
        }],
        instructions,
        function_table: vec![],
        phi_entries: vec![],
        switch_cases: vec![],
        call_args: vec![],
        constant_data: vec![],
        quantum_ops,
    }
}

/// Creates an instruction with only an opcode.
fn instruction(opcode: u64) -> Instruction<u64> {
    Instruction {
        opcode,
        ..Default::default()
    }
}

/// Creates a quantum instruction with immediate qubit and result operands.
fn quantum(opcode: u64, op_index: u64, qubit: u64, result: u64) -> Instruction<u64> {
    Instruction {
        opcode: opcode | SRC0_IMM | AUX1_IMM | AUX2_IMM,
        aux0: op_index,
        aux1: qubit,
        aux2: result,
        ..Default::default()
    }
}

/// Creates a quantum operation descriptor with the requested simulator operation ID.
fn op(op_id: u64) -> Op<u64> {
    Op::from_tuple((op_id, 0, 0, 0, 0))
}

/// Creates an output-recording instruction with an immediate value or result ID.
fn record(kind: u64, value: u64) -> Instruction<u64> {
    Instruction {
        opcode: 0x14 | SRC0_IMM,
        src0: value,
        aux1: kind,
        ..Default::default()
    }
}

/// Creates a successful entry-point return.
fn ret() -> Instruction<u64> {
    instruction(0x02 | DST_IMM)
}

/// Returns batch instruction names for compact ordering assertions.
fn names(backend: &Recorder) -> Vec<Vec<&'static str>> {
    backend
        .batches
        .iter()
        .map(|batch| batch.iter().map(|inst| inst.instruction_name).collect())
        .collect()
}

#[test]
/// Flushes gates at capacity and submits the trailing partial batch.
fn batches_gates_and_flushes_at_completion() -> Result<(), String> {
    let mut instructions = vec![quantum(0x10, 0, 0, 0); DEFAULT_MAX_BATCH_SIZE];
    instructions.extend([quantum(0x10, 1, 0, 0), quantum(0x10, 2, 1, 0), ret()]);
    let program = program(instructions, vec![op(5), op(2), op(4)]);
    let mut backend = Recorder::default();
    let output = run_adaptive_program(&program, &mut backend)?;
    assert_eq!(
        names(&backend),
        vec![vec!["h"; DEFAULT_MAX_BATCH_SIZE], vec!["x", "z"]]
    );
    assert!(output.records.is_empty());
    assert_eq!(output.return_value, None);
    Ok(())
}

#[test]
/// Resolves measurement-dependent branches and converts all supported leaf outputs.
fn branches_on_measurement_and_converts_outputs() -> Result<(), String> {
    let mut program = program(
        vec![
            quantum(0x10, 0, 0, 0),
            quantum(0x11, 1, 0, 0),
            Instruction {
                opcode: 0x13 | SRC0_IMM,
                ..Default::default()
            },
            Instruction {
                opcode: 0x05,
                aux0: 1,
                aux1: 2,
                ..Default::default()
            },
            quantum(0x10, 0, 1, 0),
            record(0, 0),
            record(3, 1),
            record(4, u64::from_ne_bytes((-7_i64).to_ne_bytes())),
            record(5, 1.25_f64.to_bits()),
            ret(),
            ret(),
        ],
        vec![op(2), op(21)],
    );
    program.block_table = vec![
        Block::from_tuple((0, 4)),
        Block::from_tuple((4, 6)),
        Block::from_tuple((10, 1)),
    ];
    for (result, expected_names, expected_values) in [
        (
            QuantumBackendResult::One,
            vec![vec!["x", "mz"], vec!["x"]],
            vec![
                OutputValue::Result(true),
                OutputValue::Bool(true),
                OutputValue::Int(-7),
                OutputValue::Double(1.25),
            ],
        ),
        (QuantumBackendResult::Zero, vec![vec!["x", "mz"]], vec![]),
    ] {
        let mut backend = Recorder {
            responses: VecDeque::from([Ok(BatchResult {
                results: smallvec![result],
            })]),
            ..Default::default()
        };
        let output = run_adaptive_program(&program, &mut backend)?;
        assert_eq!(names(&backend), expected_names);
        assert_eq!(
            output
                .records
                .iter()
                .map(|r| r.value.clone())
                .collect::<Vec<_>>(),
            expected_values
        );
        assert!(output.records.iter().all(|record| record.label.is_none()));
    }
    Ok(())
}

#[test]
/// Runs measurement-reset, result-slot reuse, and classical writes against a real backend.
fn result_slots_and_reset_work_with_sparse_backend() -> Result<(), String> {
    let program = program(
        vec![
            quantum(0x10, 0, 0, 0),
            quantum(0x11, 1, 0, 1),
            record(0, 1),
            quantum(0x11, 2, 0, 1),
            record(0, 1),
            Instruction {
                opcode: 0x18 | SRC0_IMM | SRC1_IMM,
                src0: 1,
                src1: 0,
                ..Default::default()
            },
            record(0, 0),
            ret(),
        ],
        vec![op(2), op(22), op(21)],
    );
    let mut backend = QuantumBackends::sparse_sim();
    let output = run_adaptive_program(&program, &mut backend)?;
    assert_eq!(
        output
            .records
            .into_iter()
            .map(|r| r.value)
            .collect::<Vec<_>>(),
        vec![
            OutputValue::Result(true),
            OutputValue::Result(false),
            OutputValue::Result(true),
        ]
    );
    Ok(())
}

#[test]
/// Maps every gate and rotation to QEP names and canonical parameter ordering.
fn maps_simulator_gates_to_qep() -> Result<(), String> {
    let mut backend = Recorder::default();
    {
        let mut sim = BackendSimulator {
            batcher: Batcher::new(&mut backend, 100),
            num_qubits: 2,
            measurements: vec![],
            error: None,
        };
        sim.x(0);
        sim.y(0);
        sim.z(0);
        sim.h(0);
        sim.s(0);
        sim.s_adj(0);
        sim.sx(0);
        sim.sx_adj(0);
        sim.t(0);
        sim.t_adj(0);
        sim.cx(0, 1);
        sim.cy(0, 1);
        sim.cz(0, 1);
        sim.swap(0, 1);
        sim.resetz(0);
        sim.rx(0.25, 0);
        sim.ry(0.25, 0);
        sim.rz(0.25, 0);
        sim.rxx(0.25, 0, 1);
        sim.ryy(0.25, 0, 1);
        sim.rzz(0.25, 0, 1);
        assert_eq!(sim.error, None);
        sim.batcher.finish()?;
    }
    assert_eq!(
        names(&backend),
        vec![vec![
            "x", "y", "z", "h", "s", "sadj", "sx", "h", "sadj", "h", "t", "tadj", "cx", "cy", "cz",
            "swap", "reset", "rx", "ry", "rz", "rxx", "ryy", "rzz",
        ]]
    );
    for inst in &backend.batches[0] {
        crate::validate_instruction(inst);
    }
    assert_eq!(
        backend.batches[0][17].parameters.as_slice(),
        &[QubitId(0), Float(0.25)]
    );
    assert_eq!(
        backend.batches[0][20].parameters.as_slice(),
        &[QubitId(0), QubitId(1), Float(0.25)]
    );
    Ok(())
}

#[test]
/// Reports invalid measurement responses and suppresses subsequent backend calls.
fn propagates_measurement_errors() {
    let program = program(
        vec![
            quantum(0x10, 0, 0, 0),
            quantum(0x11, 1, 0, 0),
            quantum(0x10, 0, 1, 0),
            quantum(0x11, 1, 1, 0),
            ret(),
        ],
        vec![op(2), op(21)],
    );
    for (response, expected) in [
        (Err("backend failed".into()), "backend failed"),
        (Ok(BatchResult::default()), "no result"),
        (
            Ok(BatchResult {
                results: smallvec![QuantumBackendResult::Error],
            }),
            "error result",
        ),
        (
            Ok(BatchResult {
                results: smallvec![QuantumBackendResult::Zero, QuantumBackendResult::One],
            }),
            "multiple results",
        ),
    ] {
        let mut backend = Recorder {
            responses: VecDeque::from([response]),
            ..Default::default()
        };
        let error = run_adaptive_program(&program, &mut backend)
            .expect_err("invalid measurement response must fail");
        assert!(error.contains(expected), "{error}");
        assert_eq!(names(&backend), vec![vec!["x", "mz"]]);
    }
}

#[test]
/// Propagates capacity and completion flush failures without executing further batches.
fn propagates_gate_flush_errors() {
    for gate_count in [2, DEFAULT_MAX_BATCH_SIZE + 2] {
        let mut instructions = vec![quantum(0x10, 0, 0, 0); gate_count];
        instructions.push(ret());
        let program = program(instructions, vec![op(2)]);
        let mut backend = Recorder {
            responses: VecDeque::from([Err("gate failed".into())]),
            ..Default::default()
        };
        assert_eq!(
            run_adaptive_program(&program, &mut backend),
            Err("gate failed".into())
        );
        assert_eq!(backend.batches.len(), 1);
        assert_eq!(
            backend.batches[0].len(),
            gate_count.min(DEFAULT_MAX_BATCH_SIZE)
        );
    }
}

#[test]
/// Rejects unsupported bytecode operations without flushing pending gates.
fn rejects_unsupported_operations() {
    for (unsupported, op_id, expected) in [
        (quantum(0x10, 1, 0, 0), 28, "movement"),
        (quantum(0x10, 1, 0, 0), 131, "correlated noise intrinsics"),
        (
            instruction(0x17 | (1 << 19) | AUX1_IMM | AUX2_IMM),
            28,
            "readout noise",
        ),
        (
            instruction(0x16 | (1 << 19) | AUX1_IMM),
            28,
            "loss inspection",
        ),
    ] {
        let program = program(
            vec![quantum(0x10, 0, 0, 0), unsupported, ret()],
            vec![op(2), op(op_id)],
        );
        let mut backend = Recorder::default();
        let error = run_adaptive_program(&program, &mut backend)
            .expect_err("unsupported operation must fail");
        assert!(error.contains(expected), "{error}");
        assert!(backend.batches.is_empty());
    }
}

#[test]
/// Rejects out-of-range operands before backend execution.
fn rejects_invalid_resources() {
    for inst in [quantum(0x10, 0, 2, 0), quantum(0x11, 1, 0, 2)] {
        let program = program(vec![inst, ret()], vec![op(2), op(21)]);
        let mut backend = Recorder::default();
        assert!(
            run_adaptive_program(&program, &mut backend)
                .expect_err("out-of-range resource must fail")
                .contains("exceeds the declared")
        );
        assert!(backend.batches.is_empty());
    }
}

#[test]
/// Classical result writes remain local and can be retrieved through the simulator trait.
fn caches_classical_results_locally() {
    let mut backend = Recorder::default();
    let mut sim = BackendSimulator {
        batcher: Batcher::new(&mut backend, 10),
        num_qubits: 0,
        measurements: vec![MeasurementResult::Zero],
        error: None,
    };
    sim.write_result(true, 0);
    assert_eq!(sim.measurements(), &[MeasurementResult::One]);
    assert_eq!(sim.take_measurements(), vec![MeasurementResult::One]);
    assert!(sim.measurements().is_empty());
    assert_eq!(sim.error, None);
    assert!(backend.batches.is_empty());
}

#[test]
/// Preserves backend quantum state between independent bytecode executions.
fn preserves_backend_state_between_runs() -> Result<(), String> {
    let prepare = program(vec![quantum(0x10, 0, 0, 0), ret()], vec![op(2)]);
    let measure = program(
        vec![quantum(0x11, 0, 0, 0), record(0, 0), ret()],
        vec![op(21)],
    );
    let mut backend = QuantumBackends::sparse_sim();
    assert!(
        run_adaptive_program(&prepare, &mut backend)?
            .records
            .is_empty()
    );
    let output = run_adaptive_program(&measure, &mut backend)?;
    assert_eq!(output.records[0].value, OutputValue::Result(true));
    Ok(())
}

#[test]
/// Confirms the SX-adjoint decomposition cancels SX on a real simulator.
fn sx_adjoint_cancels_sx() -> Result<(), String> {
    let program = program(
        vec![
            quantum(0x10, 0, 0, 0),
            quantum(0x10, 1, 0, 0),
            quantum(0x11, 2, 0, 0),
            record(0, 0),
            ret(),
        ],
        vec![op(10), op(11), op(21)],
    );
    let mut backend = QuantumBackends::sparse_sim();
    let output = run_adaptive_program(&program, &mut backend)?;
    assert_eq!(output.records[0].value, OutputValue::Result(false));
    Ok(())
}
