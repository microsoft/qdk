// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! One call across a boundary ([`BoundaryCrossing`]): the receiver as its
//! sender calls it, with the protocol's checks on what crosses.
//!
//! So that no part checks what it is sent, nor shapes the readouts of a
//! batch it fills: each call across a boundary of a chain goes through a
//! boundary crossing, built anew for it, both inside a
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
//! aborts). An `execute_each` makes the same checks before the receiver
//! gives any row, and an `execute_stream` checks a checked stream a piece
//! at a time, in `checked_pieces`, in the pieces the receiver's default
//! cuts: the first before the receiver is
//! called, the others as the receiver pulls them; a stream's readouts are
//! shaped by its receiver (by default, as `execute_stream` cuts it). A
//! panic in the caller's `each`, or in its stream's instructions, is
//! raised again on the caller's side, so that it names the caller, not
//! the receiver.
//! The limits it checks batches against are in `batch_limits`.

mod batch_limits;
mod checked_pieces;

pub(crate) use batch_limits::BatchLimits;

use std::cell::Cell;
use std::iter;

use super::part_panics::{CallersCallback, catch_panic};
use crate::roles::most_per_piece;
use crate::{
    Batch, BatchReadouts, Execute, ExecuteError, ExecuteRefusal, ExecutorSetup, Failure, FinalRow,
    Instruction, InstructionSet, InstructionStream, Needs, NotReady, Offer, PartId, Seed, ShotSet,
};
use batch_limits::BatchFault;
use checked_pieces::CheckedPieces;

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

// Every method stated: one left at its default would not reach the
// receiver (a default `end_shots` does nothing; a default `execute_each`
// gives no row before `execute` answers; a default `execute_stream` cuts
// the stream into batches the receiver may take in one call), so not
// passing a call on is written here, never inherited.
#[deny(clippy::missing_trait_methods)]
impl<E: Execute> Execute for BoundaryCrossing<'_, E> {
    /// `Ok`, as by default: the runner asks each part itself, at most once
    /// per run, before the driver starts; a part above that asks at run
    /// time does not ask the receiver again.
    fn ready(&mut self, needs: &Needs) -> impl Future<Output = Result<(), NotReady>> {
        let _ = needs;
        std::future::ready(Ok(()))
    }

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
        let Some(first) = self.check_and_shape(batches, readouts)? else {
            return Ok(());
        };
        match catch_panic(|| self.inner.execute(batches, seed, readouts)).await {
            Ok(ran) => ran,
            Err(message) => Err(self.panic_failure(first.shots, message)),
        }
    }

    /// The same checks and shaping as [`execute`](Self::execute), then the
    /// receiver's rows, each given to `each` as the receiver gives it. A
    /// panic in `each` is the caller's, not the receiver's: `each` is not
    /// called again, and the panic is raised again once the receiver
    /// answers, through the caller's call, so that the run fails naming the
    /// caller.
    async fn execute_each(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        readouts: &mut [BatchReadouts],
        each: &mut impl FnMut(FinalRow<'_>),
    ) -> Result<(), ExecuteError> {
        let Some(first) = self.check_and_shape(batches, readouts)? else {
            return Ok(());
        };
        let mut callback = CallersCallback::new(each);
        let mut give = |row: FinalRow<'_>| callback.call(|each| each(row));
        let give = &mut give;
        let ran = catch_panic(|| self.inner.execute_each(batches, seed, readouts, give)).await;
        callback.raise_panic();
        match ran {
            Ok(ran) => ran,
            Err(message) => Err(self.panic_failure(first.shots, message)),
        }
    }

    /// Passes `stream` to the receiver, its instructions pulled from the
    /// caller in a catch, so that a panic there is the caller's: the
    /// stream ends there, and the panic is raised again once the receiver
    /// answers, or once the stream's first piece is refused, through the
    /// caller's call. The stream ends at the caller's first `None`: the
    /// receiver gets nothing after it, however often it pulls. A
    /// [`checked`](InstructionStream::checked) stream is checked a piece
    /// at a time, each piece of at most the most instructions the receiver
    /// takes and at most about a thousand (the pieces its `execute_stream`
    /// cuts by default), before any of it reaches the receiver, so the
    /// boundary holds one such piece, however much the receiver takes at
    /// once. A fault in the first piece, checked before
    /// the receiver is called, refuses the stream, named as batch 0,
    /// without calling it. At a later piece's fault, the stream ends
    /// there, and the fault is the answer, as a failure, whatever the
    /// receiver answers to the stream's end, or if it panics. Once the
    /// receiver answers `Ok`, it must have taken the stream whole: an
    /// instruction left of its checked piece, or else one the caller's
    /// stream gives when asked once more (unless it already ended), fails
    /// the stream, naming the receiver ([`Failure::StreamNotTakenWhole`]).
    /// Makes a failure of the receiver's panic, named by the stream's
    /// first shot.
    async fn execute_stream(
        &mut self,
        stream: InstructionStream<'_, impl Iterator<Item = Instruction>>,
        seed: Seed,
        readouts: &mut BatchReadouts,
    ) -> Result<(), ExecuteError> {
        let checked = stream.is_checked();
        let InstructionStream {
            shots,
            instructions,
            ..
        } = stream;
        let mut callers = CallersCallback::new(instructions);
        // Fused: the caller's source need not be, and no receiver may get
        // an instruction after the stream's end.
        let mut ended = false;
        let mut pulled = iter::from_fn(|| {
            if ended {
                return None;
            }
            let mut next = None;
            callers.call(|instructions| next = instructions.next());
            ended = next.is_none();
            next
        });
        let later_fault = Cell::new(None);
        // The receiver gets the stream by `&mut`, so that once it answers
        // `Ok`, the boundary can ask once more whether it took it whole.
        let (ran, cut_short) = if checked {
            let most = most_per_piece(self.offer.max_instructions);
            match CheckedPieces::first_checked(pulled, self.batch_limits, most, &later_fault) {
                Ok(mut pieces) => {
                    let stream = InstructionStream::new(shots, pieces.by_ref()).checked();
                    let ran =
                        catch_panic(|| self.inner.execute_stream(stream, seed, readouts)).await;
                    let cut_short = matches!(ran, Ok(Ok(()))) && !pieces.ends_here();
                    (ran, cut_short)
                }
                Err(fault) => {
                    callers.raise_panic();
                    return Err(refusal(0, shots, fault).of_a_stream_after(0));
                }
            }
        } else {
            let stream = InstructionStream::new(shots, pulled.by_ref());
            let ran = catch_panic(|| self.inner.execute_stream(stream, seed, readouts)).await;
            let cut_short = matches!(ran, Ok(Ok(()))) && pulled.next().is_some();
            (ran, cut_short)
        };
        callers.raise_panic();
        if let Some((fault, before)) = later_fault.into_inner() {
            return Err(refusal(0, shots, fault).of_a_stream_after(before));
        }
        if cut_short {
            return Err(ExecuteError::Failed {
                shot: shots.first(),
                failure: Failure::StreamNotTakenWhole {
                    part: self.part.clone(),
                },
            });
        }
        match ran {
            Ok(ran) => ran,
            Err(message) => Err(self.panic_failure(shots, message)),
        }
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        self.inner.end_shots(shots);
    }
}

impl<E: Execute> BoundaryCrossing<'_, E> {
    /// The checks and shaping of `execute` and `execute_each`, before the
    /// receiver runs: the first batch, or `None` for an empty `execute`.
    #[inline]
    fn check_and_shape<'b>(
        &self,
        batches: &'b [Batch<'b>],
        readouts: &mut [BatchReadouts],
    ) -> Result<Option<&'b Batch<'b>>, ExecuteError> {
        assert_eq!(
            readouts.len(),
            batches.len(),
            "an `execute` takes one `BatchReadouts` per batch, as `Readouts::readouts_for` gives"
        );
        let Some(first) = batches.first() else {
            return Ok(None);
        };
        for (index, (batch, batch_readouts)) in batches.iter().zip(readouts.iter_mut()).enumerate()
        {
            let refused = |fault| ExecuteError::Refused(refusal(index, batch.shots, fault));
            self.batch_limits
                .check_count(batch.instructions)
                .map_err(refused)?;
            if batch.is_checked() {
                self.batch_limits
                    .check(batch.instructions, batch.readout_slots())
                    .map_err(refused)?;
            }
            batch_readouts.shape(batch.shots.len(), batch.readout_slots());
        }
        Ok(Some(first))
    }

    /// The failure of the receiver's panic, named by the first of `shots`,
    /// those of its call's first batch or of its stream.
    fn panic_failure(&self, shots: &ShotSet, message: Option<String>) -> ExecuteError {
        ExecuteError::Failed {
            shot: shots.first(),
            failure: Failure::Panicked {
                part: self.part.clone(),
                message,
            },
        }
    }
}

/// The refusal of `batches[index]`, whose shots are `shots`, for `fault`,
/// named by its first shot.
fn refusal(index: usize, shots: &ShotSet, fault: BatchFault) -> ExecuteRefusal {
    let shot = shots.first();
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
