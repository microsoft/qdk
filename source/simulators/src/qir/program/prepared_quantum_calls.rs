// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The quantum calls of a program, prepared when the program is built
//! ([`PreparedQuantumCalls`]): the calls whose operands are all constants
//! are built then, so that the machine running a context appends them to
//! its batch instead of building them on every shot.
//!
//! A QIR program makes two kinds of calls. Its classical calls, to its own
//! functions (`CALL`) or to QIR's runtime (`__quantum__rt__*`, which the
//! pass lowers to opcodes such as `RECORD_OUTPUT` and `READ_RESULT`), the
//! machine runs itself, as it runs every classical instruction
//! (`machine/walk.rs`). Its quantum calls, of QIR's quantum instructions
//! (`__quantum__qis__*`), the machine does not run: it turns each one it
//! reaches into a [`QirCall`] and appends it to the batch a resumer runs.
//! Only the quantum calls are prepared here, and "call" below means one of
//! them. A [`QirProgram`](super::QirProgram) prepares them with the rest
//! of the program; a [`QirMachine`] reads them as it runs a context.
//!
//! Building each call from its bytecode instruction, on every shot, is
//! where a walk of the bytecode spends its time on a program with many
//! quantum instructions. Most of them have only constant operands (their
//! qubits, angles and results), so their calls are the same on every shot:
//! they are built once, here. And since a block's quantum instructions
//! often follow one another, the machine appends a run of such calls as
//! one slice. The benchmark `benches/qir_machine.rs`, in this crate,
//! measures quantum calls with constant operands against calls with
//! operands read from registers.
//!
//! Each quantum instruction has a [`CallSource`], found by its position:
//! its calls are prebuilt, with those of the prebuilt instructions after
//! it in its run ([`CallSource::Prebuilt`]); or its call is built when a
//! context reaches it, from the registers that hold some of its operands
//! ([`CallSource::BuiltFromRegisters`], [`CallSource::CorrelatedNoise`]).

use super::super::QirCall;
use super::super::QirInstruction;
#[cfg(doc)]
use super::super::QirMachine;
use super::super::quantum_instruction_encoding::{self, CallOperands};
use super::check::CheckedCall;

/// The prepared calls of a program's quantum instructions: where the
/// calls that each appends to a batch come from, and the prebuilt calls.
#[derive(Debug)]
pub(in crate::qir) struct PreparedQuantumCalls {
    /// For each instruction of the program, the index of its source in
    /// [`Self::sources`], or [`NONE`] for a classical one.
    at: Vec<u32>,
    /// The sources of the quantum instructions, in their order.
    sources: Vec<CallSource>,
    /// The calls of the instructions whose operands are all constants, in
    /// their order: a run of them is a slice.
    prebuilt: Vec<QirCall>,
}

/// The index in [`PreparedQuantumCalls::at`] of a classical instruction.
const NONE: u32 = u32::MAX;

/// Where the calls that a quantum instruction appends to a batch come
/// from.
#[derive(Clone, Debug)]
pub(in crate::qir) enum CallSource {
    /// Its operands are all constants, so its call was built with the
    /// program. It appends the prebuilt calls from
    /// [`PreparedQuantumCalls::prebuilt`]`[first]` for the instructions
    /// from here to `end`: its own, then those of the prebuilt
    /// instructions after it in its run, as one slice. A run ends at its
    /// first call that writes a result, `result`, since that call may have
    /// to start the next batch: it is the only call of the run that writes
    /// one.
    Prebuilt {
        first: u32,
        end: u32,
        result: Option<u32>,
    },
    /// Some of its operands are held in registers, so its call is built
    /// when a context reaches it, from the registers' values
    /// ([`quantum_instruction_encoding::build`]): one call.
    BuiltFromRegisters(RegisterOperands),
    /// Correlated noise from the noise table `table`, built when a context
    /// reaches it, on the qubits held in the registers that its call
    /// arguments name, their count and offset read from `aux1` and `aux2`:
    /// one call.
    CorrelatedNoise { table: u32 },
}

/// A quantum instruction, any but correlated noise, and its operands,
/// some held in registers: what a context builds its call from.
#[derive(Clone, Copy, Debug)]
pub(in crate::qir) struct RegisterOperands {
    pub(in crate::qir) instruction: QirInstruction,
    /// A real held in a register holds its bits.
    pub(in crate::qir) operands: CallOperands,
}

impl PreparedQuantumCalls {
    /// The prepared calls of a program whose instructions, by position,
    /// the check decoded as `calls`: `None` for a classical one.
    pub(super) fn new(calls: &[Option<CheckedCall>]) -> Self {
        let mut prepared = PreparedQuantumCalls {
            at: vec![NONE; calls.len()],
            sources: Vec::new(),
            prebuilt: Vec::new(),
        };
        for (pc, call) in calls.iter().enumerate() {
            if let Some(call) = call {
                prepared.at[pc] = as_u32(prepared.sources.len());
                let source = prepared.source_of(pc, call);
                prepared.sources.push(source);
            }
        }
        prepared.join_runs();
        prepared
    }

    /// The source of the quantum instruction at `pc`, `call` as the check
    /// decoded it, building its call when its operands are all constants.
    fn source_of(&mut self, pc: usize, call: &CheckedCall) -> CallSource {
        match *call {
            CheckedCall::CorrelatedNoise { table, .. } => CallSource::CorrelatedNoise { table },
            CheckedCall::Call {
                instruction,
                operands,
            } => match operands.known() {
                Some(known) => {
                    let first = as_u32(self.prebuilt.len());
                    self.prebuilt
                        .push(quantum_instruction_encoding::build(instruction, known));
                    CallSource::Prebuilt {
                        first,
                        end: as_u32(pc + 1),
                        result: operands.result.map(|_| known.2),
                    }
                }
                None => CallSource::BuiltFromRegisters(RegisterOperands {
                    instruction,
                    operands,
                }),
            },
        }
    }

    /// Joins each prebuilt instruction to the prebuilt one after it, up to
    /// the first whose call writes a result: each instruction of a run
    /// appends the calls from itself to the run's end, so that a context
    /// may run on from any of them.
    fn join_runs(&mut self) {
        for pc in (0..self.at.len().saturating_sub(1)).rev() {
            let (Some(this), Some(next)) = (self.index(pc), self.index(pc + 1)) else {
                continue;
            };
            let CallSource::Prebuilt {
                end: next_end,
                result: next_result,
                ..
            } = self.sources[next]
            else {
                continue;
            };
            if let CallSource::Prebuilt { end, result, .. } = &mut self.sources[this]
                && result.is_none()
            {
                *end = next_end;
                *result = next_result;
            }
        }
    }

    fn index(&self, pc: usize) -> Option<usize> {
        let index = self.at[pc];
        (index != NONE).then_some(index as usize)
    }

    /// The source of the instruction at `pc`, if it is a quantum one.
    #[inline]
    pub(in crate::qir) fn source(&self, pc: usize) -> Option<&CallSource> {
        self.index(pc).map(|index| &self.sources[index])
    }

    /// The prebuilt calls that the instruction at `pc` appends, its
    /// source being `Prebuilt { first, end, .. }`.
    #[inline]
    pub(in crate::qir) fn prebuilt(&self, pc: usize, first: u32, end: u32) -> &[QirCall] {
        &self.prebuilt[first as usize..][..end as usize - pc]
    }
}

/// `len`, a position in the program or in a table of it, as held.
fn as_u32(len: usize) -> u32 {
    u32::try_from(len).expect("a program holds fewer than 2^32 instructions and table entries")
}
