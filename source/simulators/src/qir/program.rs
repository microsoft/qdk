// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A program the machine runs ([`QirProgram`]): a bytecode program of the
//! Adaptive Profile, built once and then run for every shot.
//!
//! Whoever runs a program's quantum calls must know, before any shot, what
//! the program needs: its qubits, results and QIR instructions, whether a
//! shot continues after a batch, and its two-qubit gates and measurements
//! ([`TwoQubitGate`], [`MeasuredQubit`]). And a fault the bytecode shows is
//! best refused once, when the program is built, rather than met in a shot.
//!
//! So building a program checks the bytecode, with every fault
//! (`program/check.rs`), finds what it tells, and prepares its quantum
//! calls (`program/prepared_quantum_calls.rs`); a
//! [`QirMachine`](super::QirMachine) then walks the bytecode itself.

mod check;
mod continuation;
pub(super) mod prepared_quantum_calls;

use std::sync::Arc;

use check::CheckedCall;
use prepared_quantum_calls::PreparedQuantumCalls;

use super::QirInstruction;
use super::fault::QirFaults;
use super::quantum_instruction_encoding::{self, CallOperand, QIR_INSTRUCTIONS, Shape};
use crate::bytecode::AdaptiveProgram;
use crate::bytecode::encoding::OP_RECORD_OUTPUT;

/// A bytecode program of the Adaptive Profile, checked when it is built,
/// whose quantum instructions are calls of the machine's QIR
/// instructions ([`QirCall`](super::QirCall)). It keeps what it tells
/// about its instructions, a copy of the bytecode, which a
/// [`QirMachine`](super::QirMachine) walks, and its prepared calls.
///
/// Building it refuses the content on which a run would stop that the
/// bytecode shows before any run, and a `RET` that would end the program
/// inside a function ([`ProgramEndInsideFunction`]), with every fault
/// ([`QirFaults`]); a value held in a register (a qubit, a divisor) is
/// known only during a run, and is not checked. What it tells about its
/// instructions counts each instruction once, whether a shot runs it many
/// times (in a loop) or never (on a branch no shot takes).
///
/// # Why here, not in the Python pass
///
/// The checks and analyses (the rules on calls and returns, refusing a
/// `RET` inside a called function among them; the results and records;
/// the measured qubits; whether a shot continues after a batch) are made
/// here, not by the Python pass that writes the bytecode
/// (`AdaptiveProfilePass`, `qdk/_adaptive_pass.py`), and should stay here:
///
/// - building a program is a public API taking any [`AdaptiveProgram`],
///   whoever wrote it (tests write bytecode by hand); a safe API checks
///   what it will run, so a check in the pass would be a second copy, not
///   a replacement;
/// - the mapping to the machine's instructions and the analyses are in
///   the machine's terms ([`QirInstruction`], [`QirCall`](super::QirCall)),
///   which exist only in Rust; the pass's other consumers (for example the GPU
///   simulator, Qodec's interpreter, and the CPU runtime `run_shot`) need
///   none of it;
/// - it is paid once per program, not per shot.
///
/// [`ProgramEndInsideFunction`]: super::QirError::ProgramEndInsideFunction
#[derive(Clone, Debug)]
pub struct QirProgram {
    /// The QIR instructions it calls, each once, in the machine's order
    /// ([`Self::instructions`]).
    instructions: Vec<QirInstruction>,
    /// Whether a shot may send a batch after the outcomes of an earlier
    /// one ([`Self::continues_after_a_batch`]).
    continues_after_a_batch: bool,
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

/// A measurement of a [`QirProgram`] (`mz` or `mresetz`), as the program
/// gives its qubit and result before any run
/// ([`QirProgram::measured_qubits`]). From them a resumer learns, before
/// any shot, which qubits a program measures and into which results. For
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
    /// prepares its quantum calls.
    ///
    /// # Errors
    ///
    /// Every fault of `bytecode`, when it has any ([`QirFaults`]).
    pub fn new(bytecode: &AdaptiveProgram<u64>) -> Result<Self, QirFaults> {
        let calls = check::check(bytecode)?;
        let mut used = [false; QIR_INSTRUCTIONS.len()];
        let mut records_output = false;
        let mut two_qubit_gates = Vec::new();
        let mut measured_qubits = Vec::new();
        // The check refused overlapping blocks, and instructions outside
        // every block: each instruction is counted once.
        for block in &bytecode.block_table {
            let start = usize::try_from(block.instr_offset)
                .expect("the check refused a block past the instructions");
            let count = usize::try_from(block.instr_count)
                .expect("the check refused a block past the instructions");
            let range = start..start + count;
            for (call, instruction) in calls[range.clone()]
                .iter()
                .zip(&bytecode.instructions[range])
            {
                let Some(call) = call else {
                    records_output |= instruction.primary_opcode() == OP_RECORD_OUTPUT;
                    continue;
                };
                let instruction = call.instruction();
                used[quantum_instruction_encoding::index(instruction)] = true;
                if let CheckedCall::Call { operands, .. } = call {
                    if matches!(
                        quantum_instruction_encoding::shape(instruction),
                        Shape::TwoQubits | Shape::TwoQubitsAngle
                    ) {
                        two_qubit_gates.push(match operands.qubits {
                            [CallOperand::Known(q1), CallOperand::Known(q2)] => {
                                TwoQubitGate::Known { q1, q2 }
                            }
                            _ => TwoQubitGate::FromRegisters,
                        });
                    }
                    if matches!(
                        instruction,
                        QirInstruction::Measure | QirInstruction::MeasureReset
                    ) {
                        measured_qubits.push(match (operands.qubits[0], operands.result) {
                            (CallOperand::Known(qubit), Some(CallOperand::Known(result))) => {
                                MeasuredQubit::Known { qubit, result }
                            }
                            _ => MeasuredQubit::FromRegisters,
                        });
                    }
                }
            }
        }
        let prepared_quantum_calls = PreparedQuantumCalls::new(&calls);
        let bytecode = AdaptiveProgram {
            num_qubits: bytecode.num_qubits,
            num_results: bytecode.num_results,
            num_registers: bytecode.num_registers,
            entry_block: bytecode.entry_block,
            instructions: bytecode.instructions.clone(),
            block_table: bytecode.block_table.clone(),
            function_table: bytecode.function_table.clone(),
            phi_entries: bytecode.phi_entries.clone(),
            switch_cases: bytecode.switch_cases.clone(),
            call_args: bytecode.call_args.clone(),
            constant_data: bytecode.constant_data.clone(),
            quantum_ops: Vec::new(),
        };
        Ok(QirProgram {
            instructions: QIR_INSTRUCTIONS
                .iter()
                .zip(used)
                .filter_map(|((_, instruction), used)| used.then_some(*instruction))
                .collect(),
            continues_after_a_batch: continuation::continues_after_a_batch(
                &bytecode,
                &prepared_quantum_calls,
            ),
            two_qubit_gates,
            measured_qubits,
            bytecode: Arc::new(bytecode),
            prepared_quantum_calls: Arc::new(prepared_quantum_calls),
            records_output,
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

    /// Whether a shot may continue after a batch: send a batch of calls after
    /// the outcomes of an earlier one. A shot stops where it needs an outcome
    /// its batch has not given: where it reads a result (`READ_RESULT`,
    /// `READ_LOSS`), records it or writes it (`WRITE_RESULT`) after a call that
    /// writes it, or where a call writes a result that a call of its batch
    /// writes (readout noise after a measurement, a measurement in a loop); it
    /// continues when a call (of any quantum instruction, correlated noise
    /// included) may run after such a stop. Found on every path of the program,
    /// whatever the values, so it may say yes for a program no shot of which
    /// continues (a path no shot takes, a read of a result no earlier call
    /// writes, code after a function call, since every return is taken to go
    /// on after every call: a function called once, after another function's
    /// call, counts as called twice), but never no for one a shot of which
    /// does. A program with no loop and no function call does not
    /// continue when its quantum calls name the results they write by
    /// constants, no two the same, and it reads, records and writes results
    /// only after its last quantum call, as a Base Profile program usually does
    /// (one whose quantum calls write a result twice, as a second measurement
    /// into it or readout noise on it, continues).
    #[must_use]
    pub fn continues_after_a_batch(&self) -> bool {
        self.continues_after_a_batch
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
