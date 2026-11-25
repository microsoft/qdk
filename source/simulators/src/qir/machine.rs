// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The machine that runs a [`QirProgram`]'s classical part, the same for
//! every target ([`QirMachine`]). Rather than call a simulator, it returns
//! to its caller at each batch of quantum calls.
//!
//! The machine walks the bytecode itself, reading each operand as the
//! encoding gives it. So the bytecode is the one form of a program to
//! maintain, with no decoded copy to keep in step with it. Only the
//! quantum instructions are prepared when the program is built
//! (`program/prepared_quantum_calls.rs`). Building their calls on every
//! shot is where a walk would spend its time.
//!
//! The walk is in `machine/walk.rs`. The shot's state is in
//! `machine/shot_context.rs`. What the classical instructions compute is in
//! `machine/classical_ops.rs`. The model is in `README.md`, and the
//! design's notes are in `design.md`.

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
///   result, `WRITE_RESULT`);
/// - it stops before a call that writes a result a call of the batch
///   already writes. So a result names one call of a batch;
/// - a read of a result whose outcome it already has does not stop it;
/// - at the program's return, while its batch holds calls, it yields the
///   batch, and ends on the next resume.
///
/// A context traps where it reaches a value that building the program
/// could not check. [`QirTrap`] lists the cases. On a result with no value
/// ([`QirReadout::Missing`]), it traps only where the program uses it. A new
/// measurement or a `WRITE_RESULT` gives the result a value again.
///
/// The machine calls nothing: a resumer ([`QirResumer`]) drives each
/// context. So a resumer may hold many contexts and resume them in any
/// order (to run several shots' batches together, say). It may clone a
/// context to run its rest twice, or stop between two batches.
///
/// For example, a resumer that runs each batch of a shot on the shot's own
/// quantum state drives the shot's context as below. `run` runs a batch on
/// that state and gives the batch's outcomes.
///
/// ```
/// # use qdk_simulators::OutputRecord;
/// # use qdk_simulators::qir::{
/// #     QirCall, QirMachine, QirOutcome, QirShotContext, QirStop, QirTrap,
/// # };
/// # fn shot(
/// #     machine: QirMachine<'_>,
/// #     context: &mut QirShotContext,
/// #     mut run: impl FnMut(&[QirCall], &mut Vec<QirOutcome>),
/// # ) -> Result<Vec<OutputRecord>, QirTrap> {
/// machine.restart_shot(context);
/// let mut outcomes = Vec::new();
/// loop {
///     match machine.resume(context, &outcomes)? {
///         QirStop::Batch(batch) => {
///             outcomes.clear();
///             run(batch, &mut outcomes);
///         }
///         QirStop::Ended(records) => return Ok(records.to_vec()),
///     }
/// }
/// # }
/// ```
///
/// The machine holds only the program, which it lends to a resumer
/// ([`program`](Self::program)). It draws nothing: a context's randomness is
/// its resumer's, through the outcomes. It caps neither the instructions a
/// context runs nor the quantum calls a batch holds. A limit is planned. Nor
/// does it cap a context's memory or its depth of function calls. So a
/// program that does not end runs forever, or until memory runs out.
///
/// Where these behaviours differ from the bytecode runtime's `run_shot`, the
/// test that pins them says why (`COMPATIBILITY NOTE`).
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
    /// the context again with no outcomes gives them again; an outcome
    /// given then is [`QirTrap::UnexpectedOutcome`].
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
    /// `context` comes from any machine of this machine's [`QirProgram`]
    /// ([`start_shot`](Self::start_shot),
    /// [`restart_shot`](Self::restart_shot)). So a shot may move between
    /// machines of one program. Resuming another program's context is
    /// unspecified, and may panic.
    ///
    /// # Errors
    ///
    /// [`QirTrap`] when the outcomes do not answer the last batch, or when the
    /// context reaches a value that building the program could not check. A
    /// trap comes at once, the calls of its batch not yielded, except where
    /// the context stops before the instruction or call that traps: it then
    /// comes on the next resume, after the batch is yielded. A context that
    /// traps must not be resumed again ([`QirTrap`] says why);
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
