// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! One call across a boundary ([`BoundaryCrossing`]): the receiver as its
//! sender calls it, with the protocol's checks on what crosses.
//!
//! So that no part checks what it is sent, nor shapes the readouts it
//! fills: each call across a boundary of a chain goes through a boundary
//! crossing, built anew for it, both inside a
//! [`Composed`](crate::Composed) and where the runner's driver meets its
//! executor; the limits it applies were read by the `>>` that made the
//! runner. It takes the batches in order, and
//! for each, refuses it if it holds more instructions than the receiver
//! takes ([`ExecuteRefusal::TooManyInstructions`]), checked or not, or,
//! for a batch its sender marked [`checked`](Batch::checked), makes the
//! refusals [`Batch::checked`] lists; then shapes its readouts from it, a
//! row per shot of a value per readout slot its sender stated, so that the
//! receiver only fills them and the part above indexes them without a
//! check of its own. An unchecked batch's instructions are not read. A
//! refused batch ends the `execute` before the receiver runs any batch;
//! the readouts are then unspecified, as after any error. It makes a
//! failure of the receiver's panic (where a panic unwinds: on wasm32, it
//! aborts). The limits it checks batches against are in `batch_limits`.

mod batch_limits;

pub(crate) use batch_limits::BatchLimits;

use super::part_panics::catch_panic;
use crate::{
    Batch, BatchReadouts, Execute, ExecuteError, ExecuteRefusal, ExecutorSetup, Failure,
    InstructionSet, Offer, PartId, Seed, ShotId, ShotSet,
};
use batch_limits::BatchFault;

/// A call across a boundary to `inner`, its receiver, with each batch
/// checked against `batch_limits`, which hold the receiver's set as the
/// `>>` that makes the runner read it and its most instructions: the count
/// of every batch, and the instructions of each checked one. The readouts
/// of each batch are shaped from the batch.
pub(crate) struct BoundaryCrossing<'a, E> {
    inner: &'a mut E,
    /// The part that receives `inner`'s calls, as the admission listed it:
    /// what a panic of `inner` names.
    part: &'a PartId,
    /// The offer that crosses the boundary, as the `>>` that makes the
    /// runner read it.
    offer: &'a Offer,
    batch_limits: &'a BatchLimits,
}

impl<'a, E: Execute> BoundaryCrossing<'a, E> {
    /// `inner`, whose calls `part` receives, and whose offer was `offer`
    /// when the `>>` that makes the runner read it, with its batches
    /// checked against `batch_limits`, built for it from that offer.
    pub(crate) fn new(
        inner: &'a mut E,
        part: &'a PartId,
        offer: &'a Offer,
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

impl<E: ExecutorSetup> ExecutorSetup for BoundaryCrossing<'_, E> {
    /// The receiver's.
    fn name(&self) -> &str {
        self.inner.name()
    }

    /// The receiver's set as the `>>` that makes the runner read it, which
    /// `batch_limits` holds and every checked batch is checked against, not
    /// the receiver's answer now: a receiver whose set changes later gets
    /// checked batches checked against the set read then, not the new one.
    fn accepts(&self) -> &InstructionSet {
        self.batch_limits.set()
    }

    /// The offer the `>>` that makes the runner read, which the admission
    /// checked and `batch_limits` holds, not the receiver's answer now.
    fn offers(&self) -> Offer {
        self.offer.clone()
    }
}

impl<E: Execute> Execute for BoundaryCrossing<'_, E> {
    /// Takes `batches` in order: refuses a batch that holds more
    /// instructions than the receiver takes, or a checked batch that is
    /// refused, ending the `execute` before the receiver runs any batch,
    /// with `readouts` unspecified; else shapes `readouts[i]` from
    /// `batches[i]`. Then runs them on the receiver, and makes a failure of
    /// its panic (where a panic unwinds). An empty `execute` returns `Ok`
    /// at once, and does not reach the receiver.
    async fn execute(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        readouts: &mut [BatchReadouts],
    ) -> Result<(), ExecuteError> {
        assert_eq!(
            readouts.len(),
            batches.len(),
            "an `execute` takes one `BatchReadouts` per batch, as `Readouts::readouts_for` gives"
        );
        let Some(first) = batches.first() else {
            return Ok(());
        };
        for (index, (batch, batch_readouts)) in batches.iter().zip(readouts.iter_mut()).enumerate()
        {
            let refused = |fault| ExecuteError::Refused(refusal(index, batch, fault));
            self.batch_limits
                .check_count(batch.instructions)
                .map_err(refused)?;
            if batch.is_checked() {
                self.batch_limits
                    .check(batch.instructions, batch.readout_slots())
                    .map_err(refused)?;
            }
            batch_readouts.shape(
                batch.shots.len(),
                batch.readout_slots(),
                self.batch_limits.set().declares_readout_details(),
            );
        }
        match catch_panic(|| self.inner.execute(batches, seed, readouts)).await {
            Ok(ran) => ran,
            Err(message) => Err(ExecuteError::Failed {
                shot: first_shot(first.shots),
                failure: Failure::Panicked {
                    part: self.part.clone(),
                    message,
                },
            }),
        }
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
        BatchFault::TooManyReadoutSlots { at } => ExecuteRefusal::TooManyReadoutSlots {
            shot,
            batch: index,
            at,
        },
        BatchFault::ReadoutSlotsMisstated { stated, counted } => {
            ExecuteRefusal::ReadoutSlotsMisstated {
                shot,
                batch: index,
                stated,
                counted,
            }
        }
    }
}

/// The first of `shots`, in its order.
fn first_shot(shots: &ShotSet) -> ShotId {
    shots.iter().next().expect("a shot set is never empty")
}
