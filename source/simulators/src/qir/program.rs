// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A program the machine runs ([`QirProgram`]): a bytecode program of the
//! Adaptive Profile, built once and then run for every shot.
//!
//! Whoever runs a program's quantum calls must know, before any shot, what
//! the program needs: its qubits, results and QIR instructions, and its
//! two-qubit gates and measurements ([`TwoQubitGate`], [`MeasuredQubit`]).
//! And a fault the bytecode shows is best refused once, when the program
//! is built, rather than met in a shot.
//!
//! So building a program checks the bytecode, with every fault
//! (`program/check.rs`), learns what a shot needs, and prepares its quantum
//! calls (`program/prepared_quantum_calls.rs`); a
//! [`QirMachine`](super::QirMachine) then walks the bytecode itself.

mod check;
pub(super) mod prepared_quantum_calls;

use std::ops::Range;
use std::sync::Arc;

use check::CheckedCall;
use prepared_quantum_calls::PreparedQuantumCalls;

use super::QirInstruction;
use super::fault::QirFaults;
use super::quantum_instruction_encoding::{
    self, CallOperand, CallOperands, QIR_INSTRUCTIONS, Shape,
};
use crate::bytecode::encoding::OP_RECORD_OUTPUT;
use crate::bytecode::{AdaptiveProgram, Block, Instruction};

/// A bytecode program of the Adaptive Profile, checked when it is built,
/// whose quantum instructions are calls of the machine's QIR
/// instructions ([`QirCall`](super::QirCall)).
///
/// Building it refuses what the bytecode shows would stop a run, reporting
/// every fault ([`QirFaults`]); [`new`](Self::new) says what is checked.
/// It also refuses a `RET` that would end the program inside a function
/// ([`ProgramEndInsideFunction`]). What it tells about its instructions
/// counts each instruction once, whether a shot runs it many times (in a
/// loop) or never (on a branch no shot takes).
///
/// [`ProgramEndInsideFunction`]: super::QirError::ProgramEndInsideFunction
#[derive(Clone, Debug)]
pub struct QirProgram {
    /// The QIR instructions it calls, each once, in the machine's order
    /// ([`Self::instructions`]).
    instructions: Vec<QirInstruction>,
    /// Its two-qubit gates, in the order of its blocks and instructions
    /// ([`Self::two_qubit_gates`]).
    two_qubit_gates: Vec<TwoQubitGate>,
    /// Its measurements, in the order of its blocks and instructions
    /// ([`Self::measured_qubits`]).
    measured_qubits: Vec<MeasuredQubit>,
    /// The bytecode, which the machine walks; without the GPU's quantum
    /// op pool, which it does not read.
    bytecode: Arc<AdaptiveProgram<u64>>,
    /// The calls of its quantum instructions, prepared for the machine.
    prepared_quantum_calls: Arc<PreparedQuantumCalls>,
    /// Whether it records output (`RECORD_OUTPUT`); else a shot records
    /// every result at its end.
    records_output: bool,
}

/// A two-qubit gate of a [`QirProgram`], as the program gives its qubits
/// before any run ([`QirProgram::two_qubit_gates`]). For instance, a matrix
/// product state simulator benefits from knowing them before any shot: a
/// gate costs it more the farther apart its two qubits lie in the
/// simulator's chain, so it orders its qubits to keep each gate's close.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TwoQubitGate {
    /// Both qubits are constants of the program: the gate acts on `q1` and
    /// `q2`, in its operand order, in every shot that reaches it.
    Known { q1: u32, q2: u32 },
    /// A qubit is held in a register, so it is known only when a shot
    /// reaches the gate.
    FromRegisters,
}

/// A measurement of a [`QirProgram`] (`mx`, `my`, `mz` or `mresetz`), as
/// the program gives its qubit and result before any run
/// ([`QirProgram::measured_qubits`]). The basis of each is in its call
/// ([`QirCall::Measure`](super::QirCall::Measure)). For
/// instance, a simulator that samples the measurements at a program's end
/// (planned) benefits from them: it draws those qubits only, all at once,
/// rather than measuring each as the program reaches it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeasuredQubit {
    /// Both are constants of the program: the measurement reads `qubit`
    /// into `result` in every shot that reaches it.
    Known { qubit: u32, result: u32 },
    /// The qubit or the result is held in a register, so it is known only
    /// when a shot reaches the measurement.
    FromRegisters,
}

impl QirProgram {
    /// Checks `bytecode`, keeps what it tells about its instructions, and
    /// prepares its quantum calls. It takes the bytecode, which it keeps
    /// and walks, so that building a program copies nothing; its quantum
    /// operations are dropped, since the prepared calls replace them.
    ///
    /// # Errors
    ///
    /// Every fault of `bytecode`, when it has any ([`QirFaults`]). Every
    /// instruction is checked, the untaken branches' too, and each operand
    /// as a run reads it. A value held in a register is known only during a
    /// run, so the run checks it, and traps on a wrong one
    /// ([`QirTrap`](super::QirTrap)).
    pub fn new(bytecode: AdaptiveProgram<u64>) -> Result<Self, QirFaults> {
        let calls = check::check(&bytecode)?;
        let summary = InstructionSummary::of(&bytecode, &calls);
        let prepared_quantum_calls = PreparedQuantumCalls::new(&calls);
        // The walk reads the prepared calls, never the quantum operation
        // table: building the program resolved each quantum instruction's
        // operation once (into its call, or for operands held in registers,
        // into what building its call at run time needs), so the program
        // keeps no second, unread form of it.
        let bytecode = AdaptiveProgram {
            quantum_ops: Vec::new(),
            ..bytecode
        };
        Ok(QirProgram {
            instructions: summary.instructions(),
            two_qubit_gates: summary.two_qubit_gates,
            measured_qubits: summary.measured_qubits,
            bytecode: Arc::new(bytecode),
            prepared_quantum_calls: Arc::new(prepared_quantum_calls),
            records_output: summary.records_output,
        })
    }

    /// Its qubits: those a shot's quantum state holds, which its calls
    /// name by their indices.
    #[must_use]
    pub fn qubits(&self) -> u32 {
        self.bytecode.num_qubits
    }

    /// The QIR instructions it calls, each once, in the machine's order
    /// (that of the bytecode's operation ids): so that two programs
    /// calling the same instructions list them alike, and whoever runs its
    /// calls knows, before any shot, which it must run.
    #[must_use]
    pub fn instructions(&self) -> &[QirInstruction] {
        &self.instructions
    }

    /// Its results: the readouts a shot may write, each named by its
    /// index, as the calls that write it name it
    /// ([`QirCall::result`](super::QirCall::result)).
    #[must_use]
    pub fn results(&self) -> u32 {
        self.bytecode.num_results
    }

    /// Its two-qubit gates, in the order of its blocks and then of their
    /// instructions ([`TwoQubitGate`] says who benefits). Correlated noise
    /// is not one, on any number of qubits: the faults it draws act on
    /// each qubit alone (a Pauli, or a loss), so they entangle none.
    #[must_use]
    pub fn two_qubit_gates(&self) -> &[TwoQubitGate] {
        &self.two_qubit_gates
    }

    /// Its measurements, in the order of its blocks and then of their
    /// instructions.
    #[must_use]
    pub fn measured_qubits(&self) -> &[MeasuredQubit] {
        &self.measured_qubits
    }

    pub(super) fn bytecode(&self) -> &AdaptiveProgram<u64> {
        &self.bytecode
    }

    pub(super) fn prepared_quantum_calls(&self) -> &PreparedQuantumCalls {
        &self.prepared_quantum_calls
    }

    pub(super) fn records_output(&self) -> bool {
        self.records_output
    }
}

/// What a [`QirProgram`] tells about its instructions, learnt in one pass
/// over its blocks: each instruction is noted once, whether a shot runs it
/// many times (in a loop) or never (on a branch no shot takes).
struct InstructionSummary {
    /// Whether it calls each QIR instruction, in the machine's order.
    used: [bool; QIR_INSTRUCTIONS.len()],
    records_output: bool,
    two_qubit_gates: Vec<TwoQubitGate>,
    measured_qubits: Vec<MeasuredQubit>,
}

impl InstructionSummary {
    /// Notes every instruction of `bytecode`, whose quantum calls the
    /// check gave in `calls` (`None` for an instruction that is no call).
    fn of(bytecode: &AdaptiveProgram<u64>, calls: &[Option<CheckedCall>]) -> Self {
        let mut summary = InstructionSummary {
            used: [false; QIR_INSTRUCTIONS.len()],
            records_output: false,
            two_qubit_gates: Vec::new(),
            measured_qubits: Vec::new(),
        };
        // The check refused overlapping blocks, and instructions outside
        // every block: each instruction is noted once.
        for block in &bytecode.block_table {
            let range = instruction_range(block);
            let block_calls = &calls[range.clone()];
            for (call, instruction) in block_calls.iter().zip(&bytecode.instructions[range]) {
                match call {
                    Some(call) => summary.note_call(call),
                    None => summary.note_classical(instruction),
                }
            }
        }
        summary
    }

    fn note_call(&mut self, call: &CheckedCall) {
        let instruction = call.instruction();
        self.used[quantum_instruction_encoding::index(instruction)] = true;
        // Correlated noise is neither a gate nor a measurement.
        let CheckedCall::Call { operands, .. } = call else {
            return;
        };
        if is_two_qubit_gate(instruction) {
            self.two_qubit_gates.push(TwoQubitGate::of(operands));
        }
        if is_measurement(instruction) {
            self.measured_qubits.push(MeasuredQubit::of(operands));
        }
    }

    fn note_classical(&mut self, instruction: &Instruction<u64>) {
        self.records_output |= instruction.primary_opcode() == OP_RECORD_OUTPUT;
    }

    /// The QIR instructions it calls, each once, in the machine's order.
    fn instructions(&self) -> Vec<QirInstruction> {
        QIR_INSTRUCTIONS
            .iter()
            .zip(self.used)
            .filter_map(|((_, instruction), used)| used.then_some(*instruction))
            .collect()
    }
}

/// The indices of `block`'s instructions in the bytecode.
fn instruction_range(block: &Block<u64>) -> Range<usize> {
    let start = usize::try_from(block.instr_offset)
        .expect("the check refused a block past the instructions");
    let count = usize::try_from(block.instr_count)
        .expect("the check refused a block past the instructions");
    start..start + count
}

fn is_two_qubit_gate(instruction: QirInstruction) -> bool {
    matches!(
        quantum_instruction_encoding::shape(instruction),
        Shape::TwoQubits | Shape::TwoQubitsAngle
    )
}

fn is_measurement(instruction: QirInstruction) -> bool {
    matches!(
        instruction,
        QirInstruction::Measure(_) | QirInstruction::MeasureReset
    )
}

impl TwoQubitGate {
    fn of(operands: &CallOperands) -> Self {
        match operands.qubits {
            [CallOperand::Known(q1), CallOperand::Known(q2)] => TwoQubitGate::Known { q1, q2 },
            _ => TwoQubitGate::FromRegisters,
        }
    }
}

impl MeasuredQubit {
    fn of(operands: &CallOperands) -> Self {
        match (operands.qubits[0], operands.result) {
            (CallOperand::Known(qubit), Some(CallOperand::Known(result))) => {
                MeasuredQubit::Known { qubit, result }
            }
            _ => MeasuredQubit::FromRegisters,
        }
    }
}

#[cfg(test)]
mod tests;
