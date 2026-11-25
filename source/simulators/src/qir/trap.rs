// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What ends a context of a [`QirMachine`] before its program returns
//! ([`QirTrap`]). [`QirMachine::resume`] gives it, and a resumer's run
//! fails with it, naming the shot ([`QirResumeError::Trapped`]).

use std::fmt;

use crate::bytecode::AdaptiveProgram;

#[cfg(doc)]
use super::{QirFaults, QirMachine, QirProgram, QirReadout, QirResumeError};

/// What ends a context before its program returns. Faults found when the
/// [`QirProgram`] is built are [`QirFaults`].
///
/// A trap has one of three causes:
///
/// - The resumer's: outcomes that do not answer the batch
///   ([`MissingOutcome`](Self::MissingOutcome),
///   [`UnexpectedOutcome`](Self::UnexpectedOutcome)). The resumer has a bug.
/// - The target's: a readout that came back with no value
///   ([`QirReadout::Missing`]), which the program then uses
///   ([`MissingValue`](Self::MissingValue)). QIR has no value for it.
/// - The program's: every other variant. A value held in a register is
///   wrong (a qubit past the program's, a zero divisor). Building the
///   program could not check it.
///
/// When an instruction has several faults, which one is reported is
/// unspecified.
///
/// A context that traps must not be resumed. It is left partway. After a
/// resumer's trap, the outcomes are taken up to the fault, and the batch
/// is kept. After any other, the context's position is where it last
/// stopped, but its registers, memory, batch and records hold what ran
/// since. A resume would not go on with the shot as it was.
/// [`QirMachine::restart_shot`] reuses the context for a new shot.
///
/// Variants are added as checks are, hence `#[non_exhaustive]`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum QirTrap {
    /// A quantum instruction acts on a qubit, held in a register, past the
    /// program's qubits.
    NoSuchQubit {
        instruction: usize,
        qubit: u64,
        qubits: u32,
    },
    /// An instruction uses a result, held in a register, past the
    /// program's results.
    NoSuchResult {
        instruction: usize,
        result: u64,
        results: u32,
    },
    /// Correlated noise whose call arguments reach past the program's. Its
    /// count or offset is held in a register. An empty range that starts
    /// past them traps too.
    PastCallArgs {
        instruction: usize,
        offset: u64,
        count: u64,
        len: usize,
    },
    /// Correlated noise, its count or offset held in a register, whose
    /// call argument names a register past the program's registers.
    NoSuchRegister {
        instruction: usize,
        register: u64,
        registers: u32,
    },
    /// An integer division or remainder by zero, held in a register. A
    /// floating-point one gives what IEEE 754 defines (an infinity, or
    /// NaN), as `run_shot` does.
    DivisionByZero { instruction: usize },
    /// A value other than 0 and 1, held in a register, where a bit is
    /// read: a branch's or a select's condition, a bool recorded, or a
    /// result written.
    NotABit { instruction: usize, value: u64 },
    /// An instruction reads, checks for loss or records a result whose
    /// last readout is [`QirReadout::Missing`]. QIR has no value for it. A
    /// program that records nothing records every result at the `RET` that
    /// ends it, so that `RET` traps too.
    MissingValue { instruction: usize, result: u32 },
    /// A load from an address past the context's memory.
    LoadPastMemory {
        instruction: usize,
        address: u64,
        words: usize,
    },
    /// A store to an address past the context's memory.
    StorePastMemory {
        instruction: usize,
        address: u64,
        words: usize,
    },
    /// The outcomes lack one for a result the batch writes. It names the
    /// first such result, in the batch's order.
    MissingOutcome { result: u32 },
    /// An outcome given for a result that no call of the batch writes, or
    /// given twice.
    UnexpectedOutcome { result: u32 },
}

impl QirTrap {
    /// The trap of a quantum call on the qubit `value`, past `bytecode`'s
    /// qubits, at the instruction `pc`.
    #[cold]
    pub(super) fn no_such_qubit(bytecode: &AdaptiveProgram<u64>, pc: usize, value: u64) -> QirTrap {
        QirTrap::NoSuchQubit {
            instruction: pc,
            qubit: value,
            qubits: bytecode.num_qubits,
        }
    }

    /// The trap of correlated noise at the instruction `pc` whose `count`
    /// entries of the `call_args` table from `offset` reach past the table.
    #[cold]
    pub(super) fn past_call_args(
        bytecode: &AdaptiveProgram<u64>,
        pc: usize,
        offset: u64,
        count: u64,
    ) -> QirTrap {
        QirTrap::PastCallArgs {
            instruction: pc,
            offset,
            count,
            len: bytecode.call_args.len(),
        }
    }

    /// The trap of correlated noise at the instruction `pc` whose entry of
    /// the `call_args` table names the register `register`, past
    /// `bytecode`'s registers.
    #[cold]
    pub(super) fn no_such_register(
        bytecode: &AdaptiveProgram<u64>,
        pc: usize,
        register: u64,
    ) -> QirTrap {
        QirTrap::NoSuchRegister {
            instruction: pc,
            register,
            registers: bytecode.num_registers,
        }
    }

    /// The trap of an instruction or a quantum call on the result `value`,
    /// past `bytecode`'s results, at the instruction `pc`.
    #[cold]
    pub(super) fn no_such_result(
        bytecode: &AdaptiveProgram<u64>,
        pc: usize,
        value: u64,
    ) -> QirTrap {
        QirTrap::NoSuchResult {
            instruction: pc,
            result: value,
            results: bytecode.num_results,
        }
    }
}

impl fmt::Display for QirTrap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QirTrap::NoSuchQubit {
                instruction,
                qubit,
                qubits,
            } => write!(
                f,
                "instruction {instruction}: qubit {qubit} does not exist (the program has {qubits})"
            ),
            QirTrap::NoSuchResult {
                instruction,
                result,
                results,
            } => write!(
                f,
                "instruction {instruction}: result {result} does not exist (the program has \
                 {results})"
            ),
            QirTrap::PastCallArgs {
                instruction,
                offset,
                count,
                len,
            } => write!(
                f,
                "instruction {instruction}: its {count} call arguments from {offset} reach past \
                 the program's {len}"
            ),
            QirTrap::NoSuchRegister {
                instruction,
                register,
                registers,
            } => write!(
                f,
                "instruction {instruction}: register {register} does not exist (the program has \
                 {registers})"
            ),
            QirTrap::DivisionByZero { instruction } => {
                write!(
                    f,
                    "instruction {instruction}: an integer division or remainder by zero"
                )
            }
            QirTrap::NotABit { instruction, value } => write!(
                f,
                "instruction {instruction}: {value} where a bit is read, neither 0 nor 1"
            ),
            QirTrap::MissingValue {
                instruction,
                result,
            } => write!(
                f,
                "instruction {instruction}: result {result} has no value (its readout failed, \
                 not by a loss)"
            ),
            QirTrap::LoadPastMemory {
                instruction,
                address,
                words,
            } => write!(
                f,
                "instruction {instruction}: a load from address {address}, past the {words} \
                 words of memory"
            ),
            QirTrap::StorePastMemory {
                instruction,
                address,
                words,
            } => write!(
                f,
                "instruction {instruction}: a store to address {address}, past the {words} \
                 words of memory"
            ),
            QirTrap::MissingOutcome { result } => {
                write!(
                    f,
                    "no outcome was given for result {result}, which the batch writes"
                )
            }
            QirTrap::UnexpectedOutcome { result } => write!(
                f,
                "an outcome was given for result {result}, which the batch does not write, or \
                 twice"
            ),
        }
    }
}

impl std::error::Error for QirTrap {}
