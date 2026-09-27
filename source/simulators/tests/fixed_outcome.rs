// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qdk_simulators::{
    MeasurementResult::{One, Zero},
    bytecode::{AdaptiveProgram, Block, Instruction, Op},
    execution::{
        AdaptiveExecutionError, FixedOutcomeCircuit, FixedOutcomeCircuitError, FixedOutcomeError,
        FixedOutcomeOperation, PreparedAdaptiveProgram, UnitaryOperation,
    },
};

const IMMEDIATE_SRC0: u64 = 1 << 16;
const IMMEDIATE_AUX1: u64 = 1 << 20;
const IMMEDIATE_AUX2: u64 = 1 << 21;
const OP_RET: u64 = 0x02;
const OP_JUMP: u64 = 0x04;
const OP_BRANCH: u64 = 0x05;
const OP_QUANTUM_GATE: u64 = 0x10;
const OP_MEASURE: u64 = 0x11;
const OP_RESET: u64 = 0x12;
const OP_READ_RESULT: u64 = 0x13;
const OP_READ_LOSS: u64 = 0x15;
const OP_OR: u64 = 0x29;
const OPID_RESETZ: u64 = 1;
const OPID_X: u64 = 2;
const OPID_H: u64 = 5;
const OPID_CX: u64 = 15;
const OPID_MZ: u64 = 21;
const OPID_MRESETZ: u64 = 22;

/// Assembles straight-line and branching adaptive bytecode with immediate
/// operands, one quantum-op table entry per instruction.
#[derive(Default)]
struct ProgramBuilder {
    instructions: Vec<Instruction<u64>>,
    quantum_ops: Vec<Op<u64>>,
    blocks: Vec<Block<u64>>,
    block_start: u64,
}

impl ProgramBuilder {
    fn quantum(&mut self, opcode: u64, op_id: u64, q1: u64, q2: u64, result: u64) -> &mut Self {
        let aux0 = self.quantum_ops.len() as u64;
        self.quantum_ops.push(Op {
            op_id,
            q1,
            q2,
            q3: 0,
            angle: 0,
        });
        self.instructions.push(Instruction {
            opcode: opcode | IMMEDIATE_AUX1 | IMMEDIATE_AUX2,
            aux0,
            aux1: q1,
            aux2: if opcode == OP_MEASURE { result } else { q2 },
            ..Instruction::default()
        });
        self
    }

    fn gate(&mut self, op_id: u64, qubit: u64) -> &mut Self {
        self.quantum(OP_QUANTUM_GATE, op_id, qubit, 0, 0)
    }

    fn cx(&mut self, control: u64, target: u64) -> &mut Self {
        self.quantum(OP_QUANTUM_GATE, OPID_CX, control, target, 0)
    }

    fn mz(&mut self, qubit: u64, result: u64) -> &mut Self {
        self.quantum(OP_MEASURE, OPID_MZ, qubit, 0, result)
    }

    fn mresetz(&mut self, qubit: u64, result: u64) -> &mut Self {
        self.quantum(OP_MEASURE, OPID_MRESETZ, qubit, 0, result)
    }

    fn reset(&mut self, qubit: u64) -> &mut Self {
        self.quantum(OP_RESET, OPID_RESETZ, qubit, 0, 0)
    }

    fn branch_on(&mut self, result: u64, if_one: u64, if_zero: u64) -> &mut Self {
        self.instructions.push(Instruction {
            opcode: OP_READ_RESULT | IMMEDIATE_SRC0,
            dst: 0,
            src0: result,
            ..Instruction::default()
        });
        self.instructions.push(Instruction {
            opcode: OP_BRANCH,
            src0: 0,
            aux0: if_one,
            aux1: if_zero,
            ..Instruction::default()
        });
        self.end_block()
    }

    /// What `qdk.stim.compile` emits for `REQUIRE` on one record:
    /// `restart = read_loss(r) | read_result(r)`, then branch.
    fn require(&mut self, result: u64, restart: u64, proceed: u64) -> &mut Self {
        for (opcode, dst) in [(OP_READ_LOSS, 0), (OP_READ_RESULT, 1)] {
            self.instructions.push(Instruction {
                opcode: opcode | IMMEDIATE_SRC0,
                dst,
                src0: result,
                ..Instruction::default()
            });
        }
        self.instructions.push(Instruction {
            opcode: OP_OR,
            dst: 2,
            src0: 0,
            src1: 1,
            ..Instruction::default()
        });
        self.instructions.push(Instruction {
            opcode: OP_BRANCH,
            src0: 2,
            aux0: restart,
            aux1: proceed,
            ..Instruction::default()
        });
        self.end_block()
    }

    fn jump(&mut self, block: u64) -> &mut Self {
        self.instructions.push(Instruction {
            opcode: OP_JUMP,
            dst: block,
            ..Instruction::default()
        });
        self.end_block()
    }

    fn ret(&mut self) -> &mut Self {
        self.instructions.push(Instruction {
            opcode: OP_RET,
            ..Instruction::default()
        });
        self.end_block()
    }

    fn end_block(&mut self) -> &mut Self {
        let end = self.instructions.len() as u64;
        self.blocks.push(Block {
            instr_offset: self.block_start,
            instr_count: end - self.block_start,
        });
        self.block_start = end;
        self
    }

    fn prepare(&mut self, num_qubits: u32, num_results: u32) -> PreparedAdaptiveProgram<u64> {
        PreparedAdaptiveProgram::new(AdaptiveProgram {
            num_qubits,
            num_results,
            num_registers: 3,
            entry_block: 0,
            instructions: std::mem::take(&mut self.instructions),
            block_table: std::mem::take(&mut self.blocks),
            function_table: Vec::new(),
            phi_entries: Vec::new(),
            switch_cases: Vec::new(),
            call_args: Vec::new(),
            constant_data: Vec::new(),
            quantum_ops: std::mem::take(&mut self.quantum_ops),
        })
        .expect("test program should prepare")
    }
}

fn unitary(operation: UnitaryOperation) -> FixedOutcomeOperation {
    FixedOutcomeOperation::Unitary(operation)
}

fn measure(
    qubit: usize,
    result_id: usize,
    outcome: qdk_simulators::MeasurementResult,
    reset: bool,
) -> FixedOutcomeOperation {
    FixedOutcomeOperation::Measure {
        qubit,
        result_id,
        outcome,
        reset,
    }
}

fn bell_with_mresetz() -> PreparedAdaptiveProgram<u64> {
    ProgramBuilder::default()
        .gate(OPID_H, 0)
        .cx(0, 1)
        .mresetz(0, 0)
        .mresetz(1, 1)
        .ret()
        .prepare(2, 2)
}

#[test]
fn bell_with_measure_reset_fixes_every_outcome() {
    let circuit = FixedOutcomeCircuit::from_prepared_program(&bell_with_mresetz(), &[true, false])
        .expect("valid record");

    assert_eq!(circuit.qubit_count(), 2);
    assert_eq!(
        circuit.operations(),
        &[
            unitary(UnitaryOperation::H { target: 0 }),
            unitary(UnitaryOperation::Cx {
                control: 0,
                target: 1,
            }),
            measure(0, 0, One, true),
            measure(1, 1, Zero, true),
        ]
    );
}

#[test]
fn reset_of_a_fresh_qubit_is_dropped_and_after_measurement_is_folded() {
    // The Fire-and-Ice pattern: reset · U · mz · reset on every qubit.
    let prepared = ProgramBuilder::default()
        .reset(0)
        .gate(OPID_H, 0)
        .mz(0, 0)
        .reset(0)
        .reset(0)
        .ret()
        .prepare(1, 1);

    let circuit =
        FixedOutcomeCircuit::from_prepared_program(&prepared, &[true]).expect("valid record");

    assert_eq!(
        circuit.operations(),
        &[
            unitary(UnitaryOperation::H { target: 0 }),
            measure(0, 0, One, true),
        ]
    );
}

#[test]
fn measure_without_reset_keeps_the_outcome_state() {
    let prepared = ProgramBuilder::default()
        .gate(OPID_H, 0)
        .mz(0, 0)
        .ret()
        .prepare(1, 1);

    let circuit =
        FixedOutcomeCircuit::from_prepared_program(&prepared, &[false]).expect("valid record");

    assert_eq!(
        circuit.operations(),
        &[
            unitary(UnitaryOperation::H { target: 0 }),
            measure(0, 0, Zero, false),
        ]
    );
}

#[test]
fn identity_does_not_make_a_qubit_live() {
    let prepared = ProgramBuilder::default()
        .gate(0, 0)
        .reset(0)
        .ret()
        .prepare(1, 0);

    let circuit = FixedOutcomeCircuit::from_prepared_program(&prepared, &[]).expect("valid");

    assert_eq!(
        circuit.operations(),
        &[unitary(UnitaryOperation::I { target: 0 })]
    );
}

#[test]
fn reset_of_a_live_qubit_is_rejected() {
    let prepared = ProgramBuilder::default()
        .gate(OPID_H, 0)
        .reset(0)
        .ret()
        .prepare(1, 0);

    assert_eq!(
        FixedOutcomeCircuit::from_prepared_program(&prepared, &[]),
        Err(FixedOutcomeError::ResetOfLiveQubit { qubit: 0 })
    );

    let entangled_partner = ProgramBuilder::default()
        .gate(OPID_H, 0)
        .cx(0, 1)
        .mz(0, 0)
        .reset(1)
        .ret()
        .prepare(2, 1);
    assert_eq!(
        FixedOutcomeCircuit::from_prepared_program(&entangled_partner, &[false]),
        Err(FixedOutcomeError::ResetOfLiveQubit { qubit: 1 })
    );

    let followed_by_measurement = ProgramBuilder::default()
        .gate(OPID_H, 0)
        .reset(0)
        .mz(0, 0)
        .ret()
        .prepare(1, 1);
    assert_eq!(
        FixedOutcomeCircuit::from_prepared_program(&followed_by_measurement, &[false]),
        Err(FixedOutcomeError::ResetOfLiveQubit { qubit: 0 })
    );
}

#[test]
fn branch_follows_the_fixed_outcome() {
    // block 0: H q0; mz q0 -> r0; branch r0 ? block 1 : block 2
    // block 1: X q1; jump 3      block 2: H q1; jump 3
    // block 3: mz q1 -> r1; ret
    let program = || {
        ProgramBuilder::default()
            .gate(OPID_H, 0)
            .mz(0, 0)
            .branch_on(0, 1, 2)
            .gate(OPID_X, 1)
            .jump(3)
            .gate(OPID_H, 1)
            .jump(3)
            .mz(1, 1)
            .ret()
            .prepare(2, 2)
    };

    for (first, second_gate) in [
        (true, UnitaryOperation::X { target: 1 }),
        (false, UnitaryOperation::H { target: 1 }),
    ] {
        let circuit = FixedOutcomeCircuit::from_prepared_program(&program(), &[first, true])
            .expect("valid record");
        let first = if first { One } else { Zero };
        assert_eq!(
            circuit.operations(),
            &[
                unitary(UnitaryOperation::H { target: 0 }),
                measure(0, 0, first, false),
                unitary(second_gate),
                measure(1, 1, One, false),
            ]
        );
    }
}

#[test]
fn record_failing_a_selection_check_reports_result_measured_again() {
    // SELECT as emitted by qdk.stim.compile: block 0 measures r0 and restarts
    // itself while read_loss(r0) | read_result(r0).
    let prepared = ProgramBuilder::default()
        .gate(OPID_H, 0)
        .mresetz(0, 0)
        .require(0, 0, 1)
        .ret()
        .prepare(1, 1);

    assert_eq!(
        FixedOutcomeCircuit::from_prepared_program(&prepared, &[true]),
        Err(FixedOutcomeError::ResultMeasuredAgain { result_id: 0 })
    );
    let accepted = FixedOutcomeCircuit::from_prepared_program(&prepared, &[false])
        .expect("an accepted record takes the continue path once");
    assert_eq!(
        accepted.operations(),
        &[
            unitary(UnitaryOperation::H { target: 0 }),
            measure(0, 0, Zero, true),
        ]
    );
}

#[test]
fn failed_selection_check_is_reported_although_the_restart_resets_live_qubits() {
    // The example documented on `FixedOutcomeError::ResultMeasuredAgain`:
    //
    //   SELECT { R 0 1; H 1; CX 1 0; MR 0; REQUIRE rec[-1] }  M 1
    //
    // A failed check restarts the block, which resets q1 while it is still
    // entangled with q0, before r0 is measured again.
    let prepared = ProgramBuilder::default()
        .reset(0)
        .reset(1)
        .gate(OPID_H, 1)
        .cx(1, 0)
        .mresetz(0, 0)
        .require(0, 0, 1)
        .mz(1, 1)
        .ret()
        .prepare(2, 2);

    assert_eq!(
        FixedOutcomeCircuit::from_prepared_program(&prepared, &[true, true]),
        Err(FixedOutcomeError::ResultMeasuredAgain { result_id: 0 })
    );
    let accepted = FixedOutcomeCircuit::from_prepared_program(&prepared, &[false, false])
        .expect("a record that passes the check runs the block once");
    assert_eq!(
        accepted.operations(),
        &[
            unitary(UnitaryOperation::H { target: 1 }),
            unitary(UnitaryOperation::Cx {
                control: 1,
                target: 0,
            }),
            measure(0, 0, Zero, true),
            measure(1, 1, Zero, false),
        ]
    );
}

#[test]
fn with_outcome_changes_one_projection_and_keeps_the_path() {
    let circuit = FixedOutcomeCircuit::from_prepared_program(&bell_with_mresetz(), &[true, true])
        .expect("valid record");

    let flipped = circuit
        .with_outcome(1, false)
        .expect("result 1 is measured");

    assert_eq!(flipped.qubit_count(), circuit.qubit_count());
    assert_eq!(flipped.operations()[..3], circuit.operations()[..3]);
    assert_eq!(flipped.operations()[3], measure(1, 1, Zero, true));
    assert_eq!(
        flipped.with_outcome(1, true).expect("result 1 is measured"),
        circuit
    );
}

#[test]
fn with_outcome_rejects_a_result_that_is_not_measured() {
    let circuit = FixedOutcomeCircuit::from_prepared_program(&bell_with_mresetz(), &[true, true])
        .expect("valid record");

    assert_eq!(
        circuit.with_outcome(2, false),
        Err(FixedOutcomeCircuitError::UnknownResult { result_id: 2 })
    );
}

#[test]
fn result_measured_twice_on_every_path_is_reported_the_same_way() {
    // The second cause documented on `ResultMeasuredAgain`: the program itself
    // reuses the result id, whatever the record.
    let prepared = ProgramBuilder::default()
        .gate(OPID_H, 0)
        .mz(0, 0)
        .mz(0, 0)
        .ret()
        .prepare(1, 1);

    for outcome in [false, true] {
        assert_eq!(
            FixedOutcomeCircuit::from_prepared_program(&prepared, &[outcome]),
            Err(FixedOutcomeError::ResultMeasuredAgain { result_id: 0 })
        );
    }
}

#[test]
fn outcome_count_must_match_the_program_results() {
    assert_eq!(
        FixedOutcomeCircuit::from_prepared_program(&bell_with_mresetz(), &[true]),
        Err(FixedOutcomeError::OutcomeCountMismatch {
            expected: 2,
            actual: 1,
        })
    );
}

#[test]
fn out_of_range_operands_are_rejected_without_panicking() {
    let result = ProgramBuilder::default().mz(0, 1).ret().prepare(1, 1);
    assert_eq!(
        FixedOutcomeCircuit::from_prepared_program(&result, &[false]),
        Err(FixedOutcomeError::ResultOutOfRange {
            result_id: 1,
            result_count: 1,
        })
    );

    let gate = ProgramBuilder::default()
        .gate(OPID_H, 1)
        .ret()
        .prepare(1, 0);
    assert_eq!(
        FixedOutcomeCircuit::from_prepared_program(&gate, &[]),
        Err(FixedOutcomeError::QubitOutOfRange {
            qubit: 1,
            qubit_count: 1,
        })
    );

    let reset = ProgramBuilder::default().reset(2).ret().prepare(1, 0);
    assert_eq!(
        FixedOutcomeCircuit::from_prepared_program(&reset, &[]),
        Err(FixedOutcomeError::QubitOutOfRange {
            qubit: 2,
            qubit_count: 1,
        })
    );
}

#[test]
fn control_errors_are_wrapped() {
    let prepared = ProgramBuilder::default()
        .quantum(OP_RESET, OPID_MZ, 0, 0, 0)
        .ret()
        .prepare(1, 0);

    let error = FixedOutcomeCircuit::from_prepared_program(&prepared, &[])
        .expect_err("reset with a measurement op id");
    assert_eq!(
        error,
        FixedOutcomeError::Control(AdaptiveExecutionError::UnsupportedReset {
            operation_id: OPID_MZ,
            instruction_index: 0,
        })
    );
    assert!(std::error::Error::source(&error).is_some());
}
