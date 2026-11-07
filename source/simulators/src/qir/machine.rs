// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The machine that runs a [`QirProgram`]'s classical part, the same for
//! every target ([`QirMachine`]): it runs QIR's classical semantics
//! (branches, `PHI`s, arithmetic, memory) and does no quantum work: rather
//! than call a simulator, it returns to its caller at each batch of
//! quantum calls.
//!
//! A program makes two kinds of calls. Its classical calls, to its own
//! functions (`CALL`, `CALL_RETURN`) or to QIR's runtime
//! (`__quantum__rt__*`, which the pass lowers to opcodes such as
//! `RECORD_OUTPUT` and `READ_RESULT`), the machine runs itself. Its quantum
//! calls, of QIR's quantum instructions (`__quantum__qis__*`), it does not
//! run: it appends each one it reaches, as a [`QirCall`], to the context's
//! batch, which a resumer runs. To make that cheap, building the program
//! prepares each quantum instruction's call
//! (`program/prepared_quantum_calls.rs`): built then when its operands are
//! all constants, or else built by the machine when it reaches the
//! instruction, from the registers that hold the operands.
//!
//! A shot's state is held in a [`QirShotContext`]: where it is, its
//! registers and memory, the results it awaits, and its records.
//! [`resume`](QirMachine::resume) runs a context until it needs outcomes it
//! does not have, and stops there, yielding the batch of quantum calls whose
//! outcomes it needs ([`QirStop::Batch`]); or until the program returns,
//! yielding its last batch first, if any, then ending with the records
//! ([`QirStop::Ended`]). A resumer ([`QirResumer`]) runs the batch
//! wherever it likes (a simulator, a device, a debugger)
//! and resumes the context with the outcomes ([`QirOutcome`]).
//!
//! The machine knows no resumer and calls nothing: a resumer drives it,
//! resuming each context once it has the outcomes. So a resumer may hold
//! many contexts and resume them in any order (to run several shots'
//! batches together, say), clone a context to run its rest twice, or stop
//! between two batches.
//!
//! For example, a resumer that runs each batch of a shot on the shot's own
//! quantum state drives the shot's context as below, where `run` runs a
//! batch on that state and gives the batch's outcomes:
//!
//! ```
//! # use qdk_simulators::OutputRecord;
//! # use qdk_simulators::qir::{
//! #     QirCall, QirMachine, QirOutcome, QirShotContext, QirStop, QirTrap,
//! # };
//! # fn shot(
//! #     machine: QirMachine<'_>,
//! #     context: &mut QirShotContext,
//! #     mut run: impl FnMut(&[QirCall], &mut Vec<QirOutcome>),
//! # ) -> Result<Vec<OutputRecord>, QirTrap> {
//! machine.restart_shot(context);
//! let mut outcomes = Vec::new();
//! loop {
//!     match machine.resume(context, &outcomes)? {
//!         QirStop::Batch(batch) => {
//!             outcomes.clear();
//!             run(batch, &mut outcomes);
//!         }
//!         QirStop::Ended(records) => return Ok(records.to_vec()),
//!     }
//! }
//! # }
//! ```
//!
//! The machine walks the bytecode itself, instruction by instruction,
//! reading each operand as the encoding gives it (a register, or a
//! constant by its immediate flag): the bytecode is the one form of a
//! program to maintain, with no decoded copy to keep in step with it.
//! Only the quantum instructions are prepared when the program is built
//! (`program/prepared_quantum_calls.rs`), since building their calls on
//! every shot is where a walk spends its time.
//!
//! The walk is in `machine/walk.rs`; the shot's context, and what the walk
//! asks of it, in `machine/shot_context.rs`; what the classical
//! instructions compute, in `machine/classical_ops.rs`.
//!
//! The model: `README.md`; why the machine is pulled rather than pushing
//! its batches, one shot traced from QIR to its records, and the design's
//! other notes: `design.md`.

mod classical_ops;
mod shot_context;
mod walk;

pub use shot_context::QirShotContext;

#[cfg(doc)]
use super::QirResumer;
use super::program::prepared_quantum_calls::PreparedQuantumCalls;
use super::{QirCall, QirOutcome, QirProgram, QirReadout, QirTrap};
use crate::OutputRecord;
use crate::bytecode::AdaptiveProgram;
use walk::Stop;

/// Runs a [`QirShotContext`] of a [`QirProgram`] to its next stop
/// ([`resume`](Self::resume)): the program's classical instructions run,
/// and its quantum ones are collected into a batch, as calls of its QIR
/// instructions ([`QirCall`]).
///
/// A context stops only where it uses an outcome it does not have, and at
/// the program's end:
///
/// - a measurement does not stop it: the call joins the batch;
/// - it stops before an instruction that reads or writes a result a call
///   of the batch writes (`READ_RESULT`, `READ_LOSS`, the record of a
///   result, `WRITE_RESULT`), and before a call that writes a result a call
///   of the batch already writes, so that a result names one call of a
///   batch:
///   readout noise on a result its batch writes ends the batch before it;
///   it stops there even when that write or call would trap, since a valid
///   one would end the batch there too: the trap comes on the next resume,
///   once the batch has run, as `run_shot` measures before it panics; any
///   other trap comes at once, the calls of its batch not yielded;
/// - a read of a result whose outcome it already has does not stop it;
/// - at the program's return, while its batch holds calls, it yields the
///   batch, and ends on the next resume.
///
/// A measurement into, a write of, a read of, a loss check of or a record
/// of a result past the program's traps ([`QirTrap::NoSuchResult`]; a
/// constant one is refused at build: [`QirError::NoSuchResult`]). Even a
/// read, a loss check or a record of one traps: no write can ever reach it.
///
/// A result whose readout came back with no value, for a reason other than
/// a loss ([`QirReadout::Missing`]), holds no value: an instruction that
/// reads it, checks it for loss or records it traps
/// ([`QirTrap::MissingValue`]), and so does the program's end when the
/// program records nothing, since it then records every result. A program
/// that does not use it runs on, and a new measurement or a `WRITE_RESULT`
/// gives it a value again.
///
/// A branch's or a select's condition, a bool record and a `WRITE_RESULT`
/// read a bit: a value other than 0 and 1 traps ([`QirTrap::NotABit`]).
///
/// A `STORE` to an address past the context's memory (the program's
/// constant data, then the words of the `ALLOCA`s of the function calls
/// under way) traps ([`QirTrap::StorePastMemory`]), as a load from one traps
/// ([`QirTrap::LoadPastMemory`]).
///
/// The machine holds only the program, which it lends to a resumer
/// ([`program`](Self::program)). It draws nothing: a context's randomness is
/// its resumer's, through the outcomes. It does not cap the instructions a
/// context runs, nor the quantum calls a batch holds (a limit is planned). A
/// loop that makes no quantum call, or that stops each round (reading a result
/// its batch writes, say), runs forever if it never ends; a loop of quantum
/// calls that never stops adds each call to one batch, which grows until
/// memory runs out.
/// Nor does it cap a context's memory or the function calls under way: an
/// `ALLOCA` of a huge number of words grows the memory by that many, and an
/// allocation too large panics or aborts the process. An end past the last
/// address panics ("capacity overflow"). A recursion without end grows the
/// function calls under way, and the registers they save, without end.
///
/// Where these behaviours differ from the bytecode runtime's `run_shot`, the
/// test that pins them says why (`COMPATIBILITY NOTE`).
///
/// [`QirError::NoSuchResult`]: super::QirError::NoSuchResult
#[derive(Clone, Copy, Debug)]
pub struct QirMachine<'p> {
    /// The program it runs ([`Self::program`]).
    program: &'p QirProgram,
    /// The program's bytecode, which the machine walks; held apart from the
    /// program, as are the prepared quantum calls, so that the walk reaches
    /// it in one step.
    bytecode: &'p AdaptiveProgram<u64>,
    /// The program's prepared quantum calls: for each quantum instruction
    /// the walk reaches, its prebuilt calls, which the machine appends to
    /// the batch as they are, or the operands it builds its call from.
    prepared_quantum_calls: &'p PreparedQuantumCalls,
}

/// Where [`QirMachine::resume`] stopped a context, lent from the context
/// until it is resumed.
#[derive(Debug, PartialEq)]
pub enum QirStop<'c> {
    /// The quantum calls to run next, in order, on the program's qubits;
    /// the next resume takes the outcome of each that writes a result
    /// ([`QirCall::result`]), by that result.
    Batch(&'c [QirCall]),
    /// The program returned: the context's records. When the program
    /// records nothing (no `RECORD_OUTPUT`), they are every result, in the
    /// order of their ids, as `Result` records; a result with no value
    /// then traps at the program's return, as its record would. Resuming
    /// the context again gives them again.
    Ended(&'c [OutputRecord]),
}

impl<'p> QirMachine<'p> {
    /// The machine running `program`'s contexts.
    #[must_use]
    pub fn new(program: &'p QirProgram) -> Self {
        Self {
            program,
            bytecode: program.bytecode(),
            prepared_quantum_calls: program.prepared_quantum_calls(),
        }
    }

    /// The program it runs: what a resumer learns of it before running a
    /// shot (its qubits, say, to make a quantum state).
    #[must_use]
    pub fn program(&self) -> &'p QirProgram {
        self.program
    }

    /// A new shot's context, before its first resume: at the start of the
    /// program's entry block, its registers zero, its memory the program's
    /// constant data, and every result `Zero`.
    #[must_use]
    pub fn start_shot(&self) -> QirShotContext {
        let mut context = QirShotContext {
            pc: 0,
            block: 0,
            previous_block: 0,
            registers: Vec::new(),
            frames: Vec::new(),
            saved: Vec::new(),
            memory: Vec::new(),
            results: Vec::new(),
            pending: Vec::new(),
            batch: Vec::new(),
            batch_results: Vec::new(),
            records: Vec::new(),
        };
        self.restart_shot(&mut context);
        context
    }

    /// Makes `context`, of any machine and stopped anywhere, a new shot's
    /// context of this one, as [`start_shot`](Self::start_shot) gives it,
    /// reusing the memory it holds: for a resumer that runs shots one after
    /// another.
    pub fn restart_shot(&self, context: &mut QirShotContext) {
        let bytecode = self.bytecode;
        let results = bytecode.num_results as usize;
        context.pc = block_start(bytecode, bytecode.entry_block);
        context.block = bytecode.entry_block;
        context.previous_block = 0;
        context.registers.clear();
        context.registers.resize(bytecode.num_registers as usize, 0);
        context.frames.clear();
        context.saved.clear();
        context.memory.clone_from(&bytecode.constant_data);
        context.results.clear();
        context.results.resize(results, QirReadout::Zero);
        context.pending.clear();
        context.pending.resize(results, false);
        context.batch.clear();
        context.batch_results.clear();
        context.records.clear();
    }

    /// Takes the outcomes of the context's last batch, one per call that
    /// writes a result, in any order (empty on the first resume), and runs
    /// the context to its next stop.
    ///
    /// `context` is a context of this machine's [`QirProgram`]: one that
    /// any machine of it gave ([`start_shot`](Self::start_shot),
    /// [`restart_shot`](Self::restart_shot)), so a shot may move between
    /// machines of one program; what a resume of another program's
    /// context does is unspecified, and it may panic.
    ///
    /// # Errors
    ///
    /// When the outcomes do not answer the last batch (one missing, or given
    /// for no call of it, or twice), or when the context reaches a value that
    /// building the program could not check: a qubit or a result past the
    /// program's, a division by zero, a value other than 0 and 1 where a bit is
    /// read, a load or a store past the context's memory, call arguments of
    /// correlated noise past the program's or naming no register of it, a use
    /// of a result with no value ([`QirTrap`]). An instruction or call that
    /// would trap, where the context stops before it, traps on the next resume,
    /// after its batch is yielded. Any other trap comes at once: the calls of
    /// its batch are not yielded. An `ALLOCA` of a huge number of words or a
    /// recursion without end is no trap: the machine caps neither memory nor
    /// function calls (see [`QirMachine`]). A context that traps is not to be
    /// resumed again: what a later resume does is unspecified;
    /// [`restart_shot`](Self::restart_shot) reuses it.
    pub fn resume<'c>(
        &self,
        context: &'c mut QirShotContext,
        outcomes: &[QirOutcome],
    ) -> Result<QirStop<'c>, QirTrap> {
        context.take_outcomes(outcomes)?;
        Ok(match self.walk(context)? {
            Stop::Batch => QirStop::Batch(&context.batch),
            Stop::Ended => QirStop::Ended(&context.records),
        })
    }
}

/// The first instruction of `block`.
#[inline]
#[allow(clippy::cast_possible_truncation)]
fn block_start(bytecode: &AdaptiveProgram<u64>, block: u64) -> usize {
    // The check refused a block or an offset past the tables.
    bytecode.block_table[block as usize].instr_offset as usize
}
