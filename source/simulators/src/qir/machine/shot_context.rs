// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The state of one shot's run of a [`QirMachine`]'s program
//! ([`QirShotContext`]), and what the walk asks of it: the value of an
//! operand or of a result, the outcomes of its last batch, a block entered,
//! a quantum call built from its registers, its memory grown, and its
//! records at the program's end. Each function call under way is a
//! [`Frame`].

use super::super::program::prepared_quantum_calls::RegisterOperands;
use super::super::quantum_instruction_encoding::{self, CallOperand, Register};
use super::super::{QirCall, QirOutcome, QirReadout, QirTrap};
use super::block_start;
#[cfg(doc)]
use super::{QirMachine, QirStop};
use crate::bytecode::encoding::{
    FLAG_AUX0_IMM, FLAG_AUX1_IMM, FLAG_AUX2_IMM, FLAG_SRC0_IMM, FLAG_SRC1_IMM,
};
use crate::bytecode::{AdaptiveProgram, Instruction};
use crate::{MeasurementResult, OutputRecord};

/// The state of one shot's run of a [`QirMachine`]'s program, between two
/// stops: where it is, its registers, memory and function calls under way, the
/// results it has, the batch it awaits outcomes for, and what it recorded.
/// It is plain data: a clone runs on as the original would, from the same
/// outcomes, and a context may move to another thread.
#[derive(Clone, Debug)]
pub struct QirShotContext {
    /// The instruction the context runs on from.
    pub(super) pc: usize,
    /// The block it runs in, for the `PHI`s of the block a jump, a branch
    /// or a switch goes to next.
    pub(super) block: u64,
    /// The block the last jump, branch or switch came from, for the
    /// `PHI`s; 0 before any, which no `PHI` reads: building refuses one in
    /// an entry block.
    pub(super) previous_block: u64,
    /// The register file: the program's registers.
    pub(super) registers: Vec<u64>,
    /// The function calls under way, the innermost last.
    pub(super) frames: Vec<Frame>,
    /// The program's registers as each function call under way found them, a
    /// frame's after its caller's: one stack for all, so that a call
    /// allocates nothing once the stack has grown to its depth.
    pub(super) saved: Vec<u64>,
    /// The memory, in words: the program's constant data, then the
    /// `ALLOCA`s of the function calls under way.
    pub(super) memory: Vec<u64>,
    /// Each result, as the outcomes and `WRITE_RESULT` gave it; `Zero`
    /// before any.
    pub(super) results: Vec<QirReadout>,
    /// Whether a quantum call of the batch writes each result.
    pub(super) pending: Vec<bool>,
    /// The quantum calls of the batch, which [`QirStop::Batch`] lends;
    /// cleared once their outcomes are taken.
    pub(super) batch: Vec<QirCall>,
    /// The results the batch's quantum calls write, in their order.
    pub(super) batch_results: Vec<u32>,
    /// What the shot recorded, which [`QirStop::Ended`] lends; at the end,
    /// every result when the program records nothing.
    pub(super) records: Vec<OutputRecord>,
}

/// A function call under way. Its caller's registers are on the context's
/// `saved` stack. It does not save the last edge, which `run_shot` restores on
/// return: no `PHI` could read it before the next edge sets it, since the
/// check refused a `CALL` before a `PHI` in its block.
#[derive(Clone, Copy, Debug)]
pub(super) struct Frame {
    /// The instruction after the call.
    pub(super) return_pc: u32,
    /// The block of the call, which the context runs in again on return.
    pub(super) caller_block: u64,
    /// The register the function's value is written to.
    pub(super) keep: Option<Register>,
    /// The words of memory at the call: the function's `ALLOCA`s are past
    /// them, and freed at its return.
    pub(super) memory: usize,
}

impl QirShotContext {
    pub(super) fn get(&self, register: Register) -> u64 {
        self.registers[register as usize]
    }

    /// The value of an operand field: `word` itself when `instruction`'s
    /// `flag` makes it a constant, else the register `word`'s.
    #[inline]
    #[allow(clippy::cast_possible_truncation)]
    pub(super) fn operand(&self, instruction: &Instruction<u64>, flag: u64, word: u64) -> u64 {
        if instruction.opcode & flag != 0 {
            word
        } else {
            self.registers[word as usize]
        }
    }

    #[inline]
    pub(super) fn src0(&self, instruction: &Instruction<u64>) -> u64 {
        self.operand(instruction, FLAG_SRC0_IMM, instruction.src0)
    }

    #[inline]
    pub(super) fn src1(&self, instruction: &Instruction<u64>) -> u64 {
        self.operand(instruction, FLAG_SRC1_IMM, instruction.src1)
    }

    #[inline]
    pub(super) fn aux0(&self, instruction: &Instruction<u64>) -> u64 {
        self.operand(instruction, FLAG_AUX0_IMM, instruction.aux0)
    }

    #[inline]
    pub(super) fn aux1(&self, instruction: &Instruction<u64>) -> u64 {
        self.operand(instruction, FLAG_AUX1_IMM, instruction.aux1)
    }

    #[inline]
    pub(super) fn aux2(&self, instruction: &Instruction<u64>) -> u64 {
        self.operand(instruction, FLAG_AUX2_IMM, instruction.aux2)
    }

    pub(super) fn set(&mut self, register: Register, value: u64) {
        self.registers[register as usize] = value;
    }

    /// Whether a call of the batch writes `result`.
    pub(super) fn is_pending(&self, result: u64) -> bool {
        usize::try_from(result)
            .ok()
            .and_then(|index| self.pending.get(index))
            .is_some_and(|pending| *pending)
    }

    /// The value of `result`, for the instruction at `pc` that reads it,
    /// checks it for loss or records it, or its trap: past the program's
    /// results, or with no value.
    pub(super) fn result(
        &self,
        bytecode: &AdaptiveProgram<u64>,
        pc: usize,
        result: u64,
    ) -> Result<MeasurementResult, QirTrap> {
        let index = program_result(bytecode, pc, result)? as usize;
        held(self.results[index], pc, index)
    }

    /// Takes the outcomes of the batch, which then ends.
    pub(super) fn take_outcomes(&mut self, outcomes: &[QirOutcome]) -> Result<(), QirTrap> {
        for outcome in outcomes {
            let result = outcome.result;
            let pending = self
                .pending
                .get_mut(result as usize)
                .filter(|pending| **pending)
                .ok_or(QirTrap::UnexpectedOutcome { result })?;
            *pending = false;
            self.results[result as usize] = outcome.value;
        }
        if let Some(result) = self
            .batch_results
            .iter()
            .find(|result| self.pending[**result as usize])
        {
            return Err(QirTrap::MissingOutcome { result: *result });
        }
        self.batch.clear();
        self.batch_results.clear();
        Ok(())
    }

    /// Goes to `block` from the block it runs in, and gives the
    /// block's first instruction: its `PHI`s run as instructions.
    pub(super) fn enter(&mut self, bytecode: &AdaptiveProgram<u64>, block: u64) -> usize {
        self.previous_block = self.block;
        self.block = block;
        block_start(bytecode, block)
    }

    /// The result the call of `operands` writes, its register read, if it
    /// writes one.
    pub(super) fn written(&self, operands: &RegisterOperands) -> Option<u64> {
        Some(match operands.operands.result? {
            CallOperand::Known(result) => u64::from(result),
            CallOperand::InRegister(register) => self.registers[register as usize],
        })
    }

    /// The call of `operands`, their registers read, at the instruction
    /// `pc`.
    #[inline]
    pub(super) fn call(
        &self,
        bytecode: &AdaptiveProgram<u64>,
        pc: usize,
        call: &RegisterOperands,
    ) -> Result<QirCall, QirTrap> {
        let operands = &call.operands;
        let mut qubits = [0; 2];
        for (qubit, operand) in qubits.iter_mut().zip(operands.qubits) {
            *qubit = match operand {
                CallOperand::Known(qubit) => qubit,
                CallOperand::InRegister(register) => {
                    program_qubit(bytecode, pc, self.registers[register as usize])?
                }
            };
        }
        let reals = operands.reals.map(|operand| match operand {
            CallOperand::Known(real) => real,
            CallOperand::InRegister(register) => f64::from_bits(self.registers[register as usize]),
        });
        // Zero for a call that writes no result: `build` does not read it.
        let result = match operands.result {
            None => 0,
            Some(CallOperand::Known(result)) => result,
            Some(CallOperand::InRegister(register)) => {
                program_result(bytecode, pc, self.registers[register as usize])?
            }
        };
        Ok(quantum_instruction_encoding::build(
            call.instruction,
            (qubits, reals, result),
        ))
    }

    /// The call of correlated noise from the noise table `table`, at the
    /// instruction `pc`: on the qubits held in the registers that its
    /// `count` call arguments from `offset` name.
    pub(super) fn correlated_noise_call(
        &self,
        bytecode: &AdaptiveProgram<u64>,
        pc: usize,
        table: u32,
        count: u64,
        offset: u64,
    ) -> Result<QirCall, QirTrap> {
        let arguments = usize::try_from(offset)
            .ok()
            .zip(usize::try_from(count).ok())
            .and_then(|(start, len)| bytecode.call_args.get(start..start.checked_add(len)?))
            .ok_or_else(|| QirTrap::past_call_args(bytecode, pc, offset, count))?;
        for &register in arguments {
            let register = u32::try_from(register)
                .ok()
                .filter(|register| *register < bytecode.num_registers)
                .ok_or_else(|| QirTrap::no_such_register(bytecode, pc, register))?;
            program_qubit(bytecode, pc, self.get(register))?;
        }
        // Built after the checks, from an infallible iterator, so that its
        // size hint is exact and a call on more qubits than fit in place
        // allocates once.
        #[allow(clippy::cast_possible_truncation)]
        let qubits = arguments
            .iter()
            .map(|&register| self.get(register as u32) as u32)
            .collect();
        Ok(QirCall::CorrelatedNoise { qubits, table })
    }

    /// Grows the memory to at least `end` words, new ones zero; an `end`
    /// that is no `usize` asks for every address, which panics.
    pub(super) fn grow_memory(&mut self, end: u64) {
        let end = usize::try_from(end).unwrap_or(usize::MAX);
        if end > self.memory.len() {
            self.memory.resize(end, 0);
        }
    }

    /// Ends the context at the `RET` at `pc`: its records are final. A
    /// program that records nothing (not `records_output`) records every
    /// result, so a result with no value traps there; a resume after the
    /// end takes no outcome, so they are the same again.
    pub(super) fn finish(&mut self, records_output: bool, pc: usize) -> Result<(), QirTrap> {
        if !records_output {
            self.records.clear();
            for (index, value) in self.results.iter().enumerate() {
                let recorded = held(*value, pc, index)?;
                self.records.push(OutputRecord::Result(recorded));
            }
        }
        Ok(())
    }
}

/// The program's qubit `value`, read from a register by the instruction at
/// `pc`, or its trap past the program's qubits.
#[inline]
fn program_qubit(bytecode: &AdaptiveProgram<u64>, pc: usize, value: u64) -> Result<u32, QirTrap> {
    match u32::try_from(value) {
        Ok(qubit) if qubit < bytecode.num_qubits => Ok(qubit),
        _ => Err(QirTrap::no_such_qubit(bytecode, pc, value)),
    }
}

/// The program's result `value`, read from a register by the instruction
/// at `pc`, or its trap past the program's results. A read, a loss check
/// or a record of such a result traps too, where `run_shot` reads `Zero`:
/// no write can ever reach it, so reading it is a producer's bug that
/// would give plausible output (see `QirMachine`). The check refuses a
/// constant one at build, so only a register's value reaches this trap.
#[inline]
pub(super) fn program_result(
    bytecode: &AdaptiveProgram<u64>,
    pc: usize,
    value: u64,
) -> Result<u32, QirTrap> {
    match u32::try_from(value) {
        Ok(result) if result < bytecode.num_results => Ok(result),
        _ => Err(QirTrap::no_such_result(bytecode, pc, value)),
    }
}

/// What the result at `index`, holding `value`, gives the instruction at
/// `pc` that reads it, checks it for loss or records it; its trap when it
/// holds no value.
#[inline]
fn held(value: QirReadout, pc: usize, index: usize) -> Result<MeasurementResult, QirTrap> {
    match value {
        QirReadout::Zero => Ok(MeasurementResult::Zero),
        QirReadout::One => Ok(MeasurementResult::One),
        QirReadout::Loss => Ok(MeasurementResult::Loss),
        // A QIR result has no "unknown" value, so the result keeps none
        // and traps only where the program uses it: a missing value does
        // not end the run (see `QirReadout::Missing`). `run_shot` has no
        // such case: its simulators always give a value.
        QirReadout::Missing => Err(QirTrap::MissingValue {
            instruction: pc,
            result: u32::try_from(index).expect("a program has fewer than 2^32 results"),
        }),
    }
}
