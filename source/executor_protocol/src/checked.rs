// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The protocol's checks of the `execute`s a part receives: [`Checked`],
//! the receiver as the part above calls it.
//!
//! So that no part checks what it answers, nor what it is sent in a
//! batch its sender marked [`checked`](Batch::checked): each boundary of a
//! chain is crossed through one `Checked`. It always makes a failure of a
//! panic (where a panic unwinds: on wasm32, it aborts), and of an answer
//! whose shape is not the batches', as [`ShapeFault`] says, so that the
//! part above can index an answer without a check of its own. For a
//! checked batch, it also makes the refusals [`Batch::checked`] lists,
//! before anything of the `execute` runs. An unchecked batch costs it no
//! pass over its instructions. An `execute` that fails, or is dropped
//! before it answers, leaves the caller's [`Readouts`] as they were, so
//! the caller never reads part of an answer.

use smallvec::SmallVec;

use crate::batch_readouts::Loan;
use crate::instruction_set::instruction::batch_check::{BatchFault, BatchLimits, check_batch};
use crate::panics::catch_panic;
use crate::{
    Batch, BatchReadouts, Capabilities, ChainPart, Execute, ExecuteError, ExecuteRefusal, Failure,
    InstructionSet, Needs, Readouts, Seed, ShapeFault, ShotId, ShotSet,
};

/// `inner`, the receiver of a boundary, with each checked batch it
/// receives checked against `batch_limits`, which hold the receiver's set
/// as read before the receiver is provisioned, and its answers checked
/// against the batches.
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
    /// before any part was provisioned, with its checked batches checked
    /// against `batch_limits`, built by [`batch_limits`] for it from that
    /// offer.
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

    /// The receiver's set as read before the receiver is provisioned (by
    /// the runner's `>>`, or by the provision of the `Composed` holding the
    /// boundary), which `batch_limits` holds and every checked batch is
    /// checked against, not the receiver's answer now: a receiver whose set
    /// changes later gets checked batches checked against that set.
    fn accepts(&self) -> &InstructionSet {
        self.batch_limits.set()
    }

    /// The offer read before any part was provisioned, which the admission
    /// checked and `batch_limits` holds, not the receiver's answer now.
    fn offers(&self) -> Capabilities {
        self.offer.clone()
    }

    /// Refuses `batches` if any checked batch is refused, before any runs;
    /// an empty `execute` appends no readouts without reaching the
    /// receiver. Then runs them on the receiver, and makes a failure of its
    /// panic (where a panic unwinds), or of an answer whose shape is not
    /// the batches', as [`check_shape`] checks it. On a failure, or when
    /// its future is dropped before it answers, the receiver's answer is
    /// removed from `readouts`.
    async fn execute(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        readouts: &mut Readouts,
    ) -> Result<(), ExecuteError> {
        let Some(first) = batches.first() else {
            return Ok(());
        };
        let mut checked_slots: SmallVec<[Option<u32>; 4]> = SmallVec::with_capacity(batches.len());
        for (index, batch) in batches.iter().enumerate() {
            checked_slots.push(if batch.is_checked() {
                Some(
                    check_batch(batch.instructions, self.batch_limits)
                        .map_err(|fault| ExecuteError::Refused(refusal(index, batch, fault)))?,
                )
            } else {
                None
            });
        }
        let mut answer = PendingAnswer::new(readouts);
        let ran = match catch_panic(|| self.inner.execute(batches, seed, answer.readouts())).await {
            Ok(ran) => ran,
            Err(message) => Err(ExecuteError::Failed {
                shot: first_shot(first.shots),
                failure: Failure::Panicked {
                    part: self.part.clone(),
                    message,
                },
            }),
        };
        ran.and_then(|()| check_shape(batches, &checked_slots, answer.appended()))?;
        answer.keep();
        Ok(())
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        self.inner.end_shots(shots);
    }
}

/// The limits of the batches sent to `receiver`, whose offer is `offer`,
/// by a part whose shots need `needs`: the receiver's most instructions,
/// the blocks of each of its block types, and whether readout parities
/// may be sent.
pub(crate) fn batch_limits(
    receiver: &InstructionSet,
    offer: &Capabilities,
    needs: &Needs,
) -> BatchLimits {
    BatchLimits::new(
        receiver,
        offer.max_instructions,
        |block_type| needs.blocks_of(block_type),
        needs.uses_readout_parity,
    )
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
    }
}

/// The caller's `readouts` while they are lent to an `execute` that
/// appends its answer to them: dropped unkept, it removes the answer, so
/// that an `execute` that fails, panics or is dropped leaves them as they
/// were.
struct PendingAnswer<'a> {
    readouts: &'a mut Readouts,
    /// The loan, taken when it ends, on drop.
    loan: Option<Loan>,
    kept: bool,
}

impl<'a> PendingAnswer<'a> {
    fn new(readouts: &'a mut Readouts) -> Self {
        let loan = readouts.lend();
        Self {
            readouts,
            loan: Some(loan),
            kept: false,
        }
    }

    /// The readouts, to lend to the receiver.
    fn readouts(&mut self) -> &mut Readouts {
        self.readouts
    }

    /// The readouts the receiver appended.
    fn appended(&self) -> &[BatchReadouts] {
        let loan = self.loan.as_ref().expect("the loan ends only on drop");
        self.readouts.appended(loan)
    }

    /// Keeps the answer, once checked.
    fn keep(mut self) {
        self.kept = true;
    }
}

impl Drop for PendingAnswer<'_> {
    fn drop(&mut self) {
        if let Some(loan) = self.loan.take() {
            self.readouts.end_loan(loan, self.kept);
        }
    }
}

/// Whether `answer`, the readouts appended for `batches`, has their shape,
/// as [`ShapeFault`] states it: `checked_slots` holds each checked batch's
/// readout slots, `None` for an unchecked one.
fn check_shape(
    batches: &[Batch<'_>],
    checked_slots: &[Option<u32>],
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
    for (batch, ((sent, &expected), readouts)) in
        batches.iter().zip(checked_slots).zip(answer).enumerate()
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
        if let Some(expected) = expected
            && readouts.readouts() != expected
        {
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
