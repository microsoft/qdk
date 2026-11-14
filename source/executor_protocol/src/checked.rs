// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The protocol's check of every `execute` a part receives: [`Checked`], the
//! receiver as the part above calls it.
//!
//! So that no part checks what it is sent, nor what it answers: each
//! boundary of a chain is crossed through one `Checked`, which refuses a
//! batch before anything of it runs (an instruction the receiver does not
//! accept, a block beyond the shot's, more instructions than the receiver
//! takes), and makes a failure of an answer of the wrong shape or of a
//! panic (where a panic unwinds: on wasm32, it aborts), so that the part
//! above gets readouts it can index without a check of its own.

use smallvec::SmallVec;

use crate::instruction_set::instruction::batch_check::{BatchFault, BatchLimits, check_batch};
use crate::panics::catch_panic;
use crate::{
    Batch, BatchReadouts, Capabilities, ChainPart, Execute, ExecuteError, ExecuteRefusal, Failure,
    InstructionSet, Needs, Seed, ShapeFault, ShotId, ShotSet,
};

/// `inner`, the receiver of a boundary, with every `execute` it receives
/// checked against `batch_limits` and its answer checked against the batches.
pub(crate) struct Checked<'a, E> {
    inner: &'a mut E,
    /// The part that receives `inner`'s calls, as the admission listed it:
    /// what a panic of `inner` names.
    part: &'a ChainPart,
    /// The offer that crosses the boundary, read before any part was
    /// provisioned.
    offer: &'a Capabilities,
    batch_limits: &'a BatchLimits,
}

impl<'a, E: Execute> Checked<'a, E> {
    /// `inner`, whose calls `part` receives, and whose offer was `offer`
    /// before any part was provisioned, checked against `batch_limits`,
    /// built by [`batch_limits`] for it from that offer.
    pub(crate) fn new(
        inner: &'a mut E,
        part: &'a ChainPart,
        offer: &'a Capabilities,
        batch_limits: &'a BatchLimits,
    ) -> Self {
        Self {
            inner,
            part,
            offer,
            batch_limits,
        }
    }
}

impl<E: Execute> Execute for Checked<'_, E> {
    /// The receiver's.
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn accepts(&self) -> &InstructionSet {
        self.inner.accepts()
    }

    /// The offer read before any part was provisioned, which the admission
    /// checked and `batch_limits` holds, not the receiver's answer now.
    fn offers(&self) -> Capabilities {
        self.offer.clone()
    }

    /// Refuses `batches` if any is refused, before any runs; an empty
    /// `execute` answers no readouts without reaching the receiver. Then
    /// runs them on the receiver, and makes a failure of its panic (where a
    /// panic unwinds), or of an answer whose shape is not the batches'.
    async fn execute(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
    ) -> Result<Vec<BatchReadouts>, ExecuteError> {
        let Some(first) = batches.first() else {
            return Ok(Vec::new());
        };
        let mut slots: SmallVec<[u32; 4]> = SmallVec::with_capacity(batches.len());
        for (index, batch) in batches.iter().enumerate() {
            #[cfg(test)]
            BATCHES_CHECKED.with(|checked| checked.set(checked.get() + 1));
            let readouts = check_batch(self.inner.accepts(), batch.instructions, self.batch_limits)
                .map_err(|fault| ExecuteError::Refused(refusal(index, batch, fault)))?;
            slots.push(readouts);
        }
        let answer = match catch_panic(|| self.inner.execute(batches, seed)).await {
            Ok(answer) => answer?,
            Err(message) => {
                return Err(ExecuteError::Failed {
                    shot: first_shot(first.shots),
                    failure: Failure::Panicked {
                        part: self.part.clone(),
                        message,
                    },
                });
            }
        };
        check_shape(batches, &slots, &answer)?;
        Ok(answer)
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        self.inner.end_shots(shots);
    }
}

/// The limits of the batches sent to `receiver`, whose offer is `offer`,
/// by a part whose shots need `needs`: the receiver's most instructions,
/// and the blocks of each of its block types.
pub(crate) fn batch_limits(
    receiver: &InstructionSet,
    offer: &Capabilities,
    needs: &Needs,
) -> BatchLimits {
    BatchLimits::new(receiver, offer.max_instructions, |block_type| {
        needs.blocks_of(block_type)
    })
}

/// The refusal of `batches[index]`, `batch`, for `fault`, named by its
/// first shot.
fn refusal(index: usize, batch: &Batch<'_>, fault: BatchFault) -> ExecuteRefusal {
    let shot = first_shot(batch.shots);
    match fault {
        BatchFault::TooManyInstructions { max, got } => ExecuteRefusal::TooManyInstructions {
            shot,
            batch: index,
            max,
            got,
        },
        BatchFault::Instruction { at, fault } => ExecuteRefusal::Instruction {
            shot,
            batch: index,
            at,
            fault,
        },
    }
}

/// Whether `answer` has the shape of `batches`, whose instructions have
/// `slots` readout slots: a readouts per batch, a row per shot, a value per
/// slot. A failure of the batch's first shot if not; of the first batch's,
/// for a count of readouts that is not the batches'.
fn check_shape(
    batches: &[Batch<'_>],
    slots: &[u32],
    answer: &[BatchReadouts],
) -> Result<(), ExecuteError> {
    let failed = |shots: &ShotSet, fault: ShapeFault| ExecuteError::Failed {
        shot: first_shot(shots),
        failure: Failure::Part(Box::new(fault)),
    };
    if answer.len() != batches.len() {
        return Err(failed(
            batches[0].shots,
            ShapeFault::Batches {
                expected: batches.len(),
                got: answer.len(),
            },
        ));
    }
    for (batch, ((sent, &expected), readouts)) in batches.iter().zip(slots).zip(answer).enumerate()
    {
        if readouts.shots() != sent.shots.len() {
            return Err(failed(
                sent.shots,
                ShapeFault::Rows {
                    batch,
                    expected: sent.shots.len(),
                    got: readouts.shots(),
                },
            ));
        }
        if readouts.readouts() != expected {
            return Err(failed(
                sent.shots,
                ShapeFault::Slots {
                    batch,
                    expected,
                    got: readouts.readouts(),
                },
            ));
        }
    }
    Ok(())
}

/// The first of `shots`, in its order.
fn first_shot(shots: &ShotSet) -> ShotId {
    shots.iter().next().expect("a shot set is never empty")
}

#[cfg(test)]
thread_local! {
    /// How many batches this thread's `Checked`s checked: so the tests see
    /// that each boundary is checked once, which no answer shows.
    pub(crate) static BATCHES_CHECKED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
