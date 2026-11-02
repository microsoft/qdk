// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A program of fixed batches ([`StaticProgram`]), and how the crate's
//! interpreter ([`DefaultInterpreter`]) runs it: each batch to every shot
//! of the job, in order, so that a static program needs no driver code.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::fmt;

#[cfg(doc)]
use crate::{Capabilities, Chain, interpreter};
use crate::{
    DefaultInterpreter, Executor, Instruction, InstructionBatch, InstructionBatchRef, Interpret,
    Needs, Rejected, ShotOutcome, ShotSet, wait_async,
};

/// A program of fixed batches of `I`s, run in order on every shot, and
/// what it needs of the chain: a program that decides nothing on its
/// outcomes, so that it sends the same batches whatever the shots read.
///
/// A program whose calls are all in its last batch adds no need. One with
/// a call in a batch before its last continues each shot in a later call,
/// so it needs `feedback`, an executor that keeps each shot's state
/// between calls ([`Capabilities::feedback`]), and is refused when it is
/// given to a chain whose executor does not offer it ([`Chain::run`]). An
/// empty batch is a call like any other, sent to every shot, so a program
/// of calls then an empty last batch needs `feedback` too.
///
/// A program of calls by opcode states the set they are numbered in
/// ([`Needs::with_instructions`]), and its calls are of that set, or it is
/// refused when it is given to a chain ([`Chain::run`]).
///
/// When it is given to a chain, the qubits it states
/// ([`Needs::with_qubits`]) are checked against the executor's, but its
/// calls' qubits are not read: a call on a qubit the executor lacks is
/// found as it runs, at its batch, only if that executor's check refuses
/// it, and then the outcomes of earlier batches are dropped
/// ([`StaticProgramError`]). So a program states the qubits it uses, so
/// that a chain too small for it refuses it before it runs. Deriving them
/// from its calls is planned.
///
/// A program of no batches adds no need to those it states (a program of
/// calls by opcode states its set all the same), sends no call and gives
/// no outcomes.
///
/// Its outcomes are one list, told apart by shot and tag, so each tag
/// names one readout in the whole program, not only within its batch (a
/// driver's later batch may use a tag again, docs/replies.md, "Readout
/// tags"; a static program's may not). A program of calls by opcode that
/// repeats a tag is refused when it is given to a chain, at the repeat.
///
/// Known issue: a typed call holds its tags in fields of its own, which
/// the crate cannot read, so a typed program that repeats a tag is not
/// refused, and its outcomes under that tag are told apart only by their
/// order. Its writer keeps its tags unique.
///
/// It runs through the crate's interpreter, [`interpreter()`]:
/// `(interpreter() >> executor)?.run(program, shots)`.
#[derive(Clone, Debug)]
pub struct StaticProgram<I> {
    needs: Needs,
    batches: Vec<Vec<I>>,
}

impl<I> StaticProgram<I> {
    /// `batches`, in order, needing `needs`.
    #[must_use]
    pub fn new(needs: Needs, batches: Vec<Vec<I>>) -> Self {
        Self { needs, batches }
    }
}

/// A [`StaticProgram`], run by the crate's interpreter.
///
/// It sends each batch to every shot in one call, and waits for the call's
/// reply to end before it sends the next. It awaits each reply, so a run
/// awaited leaves its thread to other tasks while a reply is pending. It
/// gives the outcomes of every shot, batch after batch, each batch's in
/// the order its reply delivers them. At the first batch the chain refuses
/// or that fails, it stops and sends no later batch: it gives that batch
/// and why ([`StaticProgramError`]), the outcomes of earlier batches
/// dropped. A program of no batches sends no call and gives no outcomes.
impl<I: Instruction> Interpret<StaticProgram<I>> for DefaultInterpreter {
    type Instruction = I;
    type Output = Result<Vec<ShotOutcome>, StaticProgramError>;

    /// The needs the program states, and `feedback` when a batch before
    /// its last holds a call: the later batches, empty ones too, continue
    /// each shot where that call left it, so the executor must keep each
    /// shot's state between calls ([`Capabilities::feedback`]), whether or
    /// not the shot was read. A program whose calls are all in its last
    /// batch (its earlier batches empty, if any) adds no need, nor does a
    /// program of no batches.
    fn needs(&self, program: &StaticProgram<I>) -> Needs {
        let needs = program.needs.clone();
        if calls_before_its_last_batch(&program.batches) {
            needs.with_feedback(true)
        } else {
            needs
        }
    }

    /// Every call by opcode outside the set the program states, and every
    /// readout whose tag an earlier readout of the program has, each a
    /// [`Rejected::Instruction`] at its call's index in its batch, the
    /// batch named in the reason, in the program's order, so that none is
    /// found only once earlier batches have run. A program that states no
    /// set is refused for its needs already; typed calls are checked by
    /// the compiler, and their tags are not read.
    fn check(&self, program: &StaticProgram<I>) -> Vec<Rejected> {
        let set = program.needs.instructions.as_deref();
        // Once per job, not per batch: a map costs 0.14 s for 1e6 readouts.
        let mut first_reads: BTreeMap<u32, (usize, usize)> = BTreeMap::new();
        let mut faults = Vec::new();
        for (batch, calls) in program.batches.iter().enumerate() {
            for (at, call) in calls.iter().enumerate() {
                if let Some(set) = set
                    && let Err(mismatch) = call.check_in(set)
                {
                    faults.push(at_instruction_of_batch(batch, at, mismatch));
                }
                for tag in call.readout_tags() {
                    let (first_batch, first_at) = match first_reads.entry(tag) {
                        Entry::Vacant(first) => {
                            first.insert((batch, at));
                            continue;
                        }
                        Entry::Occupied(first) => *first.get(),
                    };
                    let named = if (first_batch, first_at) == (batch, at) {
                        "another readout of this instruction".to_owned()
                    } else {
                        format!("a readout of instruction {first_at} of batch {first_batch}")
                    };
                    faults.push(at_instruction_of_batch(
                        batch,
                        at,
                        format!(
                            "tag {tag} already names {named}: a static program's outcomes \
                             are told apart by their tags, so each tag names one readout in \
                             the program"
                        ),
                    ));
                }
            }
        }
        faults
    }

    async fn interpret<E>(
        &self,
        program: StaticProgram<I>,
        executor: &mut E,
        shots: &ShotSet,
    ) -> Self::Output
    where
        E: Executor<InstructionBatch<I>>,
    {
        let mut outcomes = Vec::new();
        for (batch, instructions) in program.batches.iter().enumerate() {
            let reply = executor
                .call(InstructionBatchRef {
                    shots,
                    instructions,
                })
                .map_err(|rejected| StaticProgramError::Refused { batch, rejected })?;
            let (events, end) = wait_async(reply).await;
            end.map_err(|reason| StaticProgramError::Failed { batch, reason })?;
            outcomes.extend(events);
        }
        Ok(outcomes)
    }
}

/// Why a [`StaticProgram`] stopped before its end: the batch it stopped at,
/// by its index in the program, refused or failed. A refusal stays typed,
/// so that its caller can match it; a failure is its reply's text. A
/// refusal at an instruction reads as a fault the program's check finds
/// when the job is given to the chain (`instruction 0: batch 1: …`).
///
/// Only the whole job can be retried (after a refusal of class
/// [`RetryClass::Later`](crate::RetryClass::Later), for one), on a chain
/// built again: the outcomes of the batches before are dropped, and the
/// chain was consumed by its run, which has closed its executor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StaticProgramError {
    /// The chain refused the batch, a refusal kept as the chain typed it: none
    /// of the batch ran, unless the refusal's class is
    /// [`RetryClass::Resend`](crate::RetryClass::Resend), such as
    /// [`Rejected::NoReply`]'s, which says that no verdict came over a link, so
    /// the work may have run.
    Refused {
        /// The batch's index in the program.
        batch: usize,
        /// Why the chain refused it.
        rejected: Rejected,
    },
    /// The batch's reply ended in a failure, or was cancelled
    /// ([`wait_async`] gives both as text).
    Failed {
        /// The batch's index in the program.
        batch: usize,
        /// The reply's reason.
        reason: String,
    },
}

impl fmt::Display for StaticProgramError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StaticProgramError::Refused {
                batch,
                rejected: Rejected::Instruction { at, reason },
            } => write!(f, "{}", at_instruction_of_batch(*batch, *at, reason)),
            StaticProgramError::Refused { batch, rejected } => {
                write!(f, "batch {batch} was refused: {rejected}")
            }
            StaticProgramError::Failed { batch, reason } => {
                write!(f, "batch {batch} failed: {reason}")
            }
        }
    }
}

impl std::error::Error for StaticProgramError {}

/// Whether a batch of `batches` before the last holds a call.
fn calls_before_its_last_batch<I>(batches: &[Vec<I>]) -> bool {
    batches
        .split_last()
        .is_some_and(|(_, earlier)| earlier.iter().any(|batch| !batch.is_empty()))
}

/// The fault of instruction `at` of batch `batch`, for `reason`: named by
/// the instruction, then the batch, whether the program's check finds it
/// when the job is given to the chain or the chain refuses the batch as it
/// runs (`StaticProgramError`'s `Display`), so that both read alike.
fn at_instruction_of_batch(batch: usize, at: usize, reason: impl fmt::Display) -> Rejected {
    Rejected::Instruction {
        at,
        reason: format!("batch {batch}: {reason}"),
    }
}
