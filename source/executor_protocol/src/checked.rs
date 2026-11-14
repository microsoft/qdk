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
//! panic, so that the part above gets readouts it can index without a
//! check of its own.

use std::any::Any;
use std::future::poll_fn;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::pin;
use std::task::Poll;

use smallvec::SmallVec;

use crate::admission::part_list::PartList;
use crate::instruction_set::instruction::batch_check::{BatchFault, BatchLimits, check_batch};
use crate::{
    Batch, BatchReadouts, Capabilities, Execute, ExecuteError, ExecuteRefusal, Failure,
    InstructionSet, Needs, RunRefusal, Seed, ShapeFault, ShotId, ShotSet,
};

/// `inner`, the receiver of a boundary, with every `execute` it receives
/// checked against `limits` and its answer checked against the batches.
pub(crate) struct Checked<'a, E> {
    inner: &'a mut E,
    limits: &'a BatchLimits,
}

impl<'a, E: Execute> Checked<'a, E> {
    /// `inner`, checked against `limits`, built by [`limits`] for it.
    pub(crate) fn new(inner: &'a mut E, limits: &'a BatchLimits) -> Self {
        Self { inner, limits }
    }
}

/// The limits of the batches sent to `receiver`, whose offer is `offer`,
/// by a part whose shots need `needs`: the receiver's most instructions,
/// and the blocks of each of its block types.
pub(crate) fn limits(
    receiver: &InstructionSet,
    offer: &Capabilities,
    needs: &Needs,
) -> BatchLimits {
    BatchLimits::new(receiver, offer.max_instructions, |block_type| {
        needs.blocks_of(block_type)
    })
}

impl<E: Execute> Execute for Checked<'_, E> {
    fn accepts(&self) -> &InstructionSet {
        self.inner.accepts()
    }

    fn offers(&self) -> Capabilities {
        self.inner.offers()
    }

    fn provision(&mut self, needs: Needs) {
        self.inner.provision(needs);
    }

    fn needs_by_part(&self, above: Needs, parts: &mut PartList) -> Needs {
        self.inner.needs_by_part(above, parts)
    }

    fn offers_by_part(&self, parts: &mut PartList) -> Capabilities {
        self.inner.offers_by_part(parts)
    }

    fn ready(&mut self) -> impl Future<Output = Result<(), RunRefusal>> {
        self.inner.ready()
    }

    /// Refuses `batches` if any is refused, before any runs; an empty
    /// `execute` answers no readouts without reaching the receiver. Then
    /// runs them on the receiver, and makes a failure of its panic, or of
    /// an answer whose shape is not the batches'.
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
            let readouts = check_batch(self.inner.accepts(), batch.instructions, self.limits)
                .map_err(|fault| ExecuteError::Refused(refusal(index, batch, fault)))?;
            slots.push(readouts);
        }
        let answer = match catch_panic(|| self.inner.execute(batches, seed)).await {
            Ok(answer) => answer?,
            Err(message) => {
                return Err(ExecuteError::Failed {
                    shot: first_shot(first.shots),
                    failure: Failure::Panicked(message),
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
                ShapeFault::Width {
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

/// Makes a part's future with `make`, and awaits it: its output, or the
/// message of the part's panic, when the message is text.
pub(crate) async fn catch_panic<F: Future>(
    make: impl FnOnce() -> F,
) -> Result<F::Output, Option<String>> {
    let future = catch_unwind(AssertUnwindSafe(make)).map_err(panic_message)?;
    let mut future = pin!(future);
    poll_fn(
        |context| match catch_unwind(AssertUnwindSafe(|| future.as_mut().poll(context))) {
            Ok(Poll::Ready(output)) => Poll::Ready(Ok(output)),
            Ok(Poll::Pending) => Poll::Pending,
            Err(payload) => Poll::Ready(Err(panic_message(payload))),
        },
    )
    .await
}

/// The message of a panic whose payload is `payload`, when it is text.
fn panic_message(payload: Box<dyn Any + Send>) -> Option<String> {
    match payload.downcast::<String>() {
        Ok(message) => Some(*message),
        Err(payload) => payload
            .downcast_ref::<&str>()
            .map(|message| (*message).to_owned()),
    }
}
