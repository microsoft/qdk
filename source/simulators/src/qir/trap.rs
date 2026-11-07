// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What ends a context of a [`QirMachine`] before its program returns
//! ([`QirTrap`]).
//!
//! Building a program checks what the bytecode shows, but some values are
//! known only during a shot (a qubit held in a register, a divisor), and
//! a resumer may answer a batch wrongly. A context that meets such a
//! fault cannot run on, so [`QirMachine::resume`] gives its reason: the
//! shot stops with it, and a resumer's run fails with it, naming the shot
//! ([`QirResumeError::Trapped`]).

use std::fmt;

use crate::bytecode::AdaptiveProgram;

#[cfg(doc)]
use super::{QirFaults, QirMachine, QirProgram, QirReadout, QirResumeError};

/// What ends a context before its program returns: a value known only as
/// it runs, which building the [`QirProgram`] could not check (for example
/// a qubit, a result, a divisor or a bit held in a register, a memory
/// address, the call arguments of correlated noise whose count or offset
/// is held in one), a use of a result whose readout
/// came back with no value, or outcomes that do not answer the batch they
/// are given for. (Faults found when the program is built are
/// [`QirFaults`].) When one instruction has several faults found as it
/// runs, which one is reported is unspecified. A context that traps is not
/// to be resumed again: what a later resume does is unspecified. Variants
/// are added as checks are, hence `#[non_exhaustive]`.
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
    /// Correlated noise, its count or offset held in a register, whose
    /// call arguments (the registers holding its qubits) reach past the
    /// program's: also with no argument, from an offset past them, as the
    /// check refuses a constant range.
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
    /// A division or remainder by zero, held in a register.
    DivisionByZero { instruction: usize },
    /// A value other than 0 and 1, held in a register, where a bit is
    /// read: a branch's or a select's condition, a bool recorded, or a
    /// result written.
    NotABit { instruction: usize, value: u64 },
    /// An instruction reads, checks for loss or records a result whose
    /// last readout came back with no value, for a reason other than a
    /// loss ([`QirReadout::Missing`]), for which QIR has no value. The
    /// `RET` that ends a program that records nothing records every
    /// result, so it traps on such a result too.
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
    /// The outcomes given lack the outcome of a call of the batch that
    /// writes a result: of the first such call, in the batch's order, by
    /// the result it writes.
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
                write!(f, "instruction {instruction}: a division by zero")
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
