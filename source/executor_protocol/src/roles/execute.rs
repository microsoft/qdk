// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The executor's role at run time, [`Execute`], the rows it gives as they
//! are final when its caller asks for them ([`FinalRow`]), and what an
//! `execute` returns when it does not answer: [`ExecuteError`], with
//! [`ExecuteRefusal`] when nothing of it ran.

mod final_row;

use std::error::Error;
use std::fmt;
use std::slice;

use super::stream_pieces::{StreamPieces, most_per_piece};
use crate::part_panics::await_part;
use crate::{
    Batch, BatchReadouts, ExecutorSetup, Failure, Instruction, InstructionFault, InstructionStream,
    Needs, NotReady, Seed, ShotId, ShotSet,
};

pub use final_row::FinalRow;
pub(crate) use final_row::give_every_row;

/// Runs batches of the instructions it accepts on their shots' states.
///
/// It is the part at the bottom of a chain, and the part a driver or an
/// adapter calls below it: the rest of the chain, composed, is one executor
/// to the part above. The protocol shapes each batch's readouts from the
/// batch before the executor runs it, so the executor only fills them,
/// and the part above indexes its batches, rows and slots.
/// A batch its sender marked [`checked`](crate::Batch::checked) the
/// protocol also checks as that method says, against the executor's set
/// and the needs it was admitted for, so the executor runs only what it
/// accepts; an unchecked batch, the default, the executor runs as the part
/// above sends it, once the protocol has checked that it holds no more
/// instructions than the executor takes.
///
/// This is the executor at run time: on every run, the runner gets it
/// ready ([`ready`](Self::ready)) and ends its shots, and the part above
/// calls it, once `>>` has set it up as an [`ExecutorSetup`].
pub trait Execute: ExecutorSetup {
    /// Gets this part ready for a run, before anything of the run runs, for
    /// `needs`, the driver's needs as adapted down to it, as the admission
    /// checked them against its offer: the blocks of each type a shot holds
    /// at once, whether its instructions use readout parities, and whether
    /// a shot is run again alone. It takes what it needs to run, such as a
    /// device, or a layout of its blocks; or, when it cannot take a run
    /// now, such as when its device is busy or its resources are not
    /// available, answers [`NotReady`], which refuses the run
    /// ([`RunRefusal::PartNotReady`](crate::RunRefusal::PartNotReady)). By
    /// default `Ok`. `needs` are the same on every run, so work that
    /// depends only on them may be done once and kept.
    ///
    /// The runner asks the parts in turn, each at most once per run, the
    /// part below first, before the driver starts, and stops at the first
    /// that refuses: no part above it is asked, nor any part when the run
    /// is refused for its shots or its seed, nor a part the run has not
    /// reached when its future is dropped, or is never polled.
    fn ready(&mut self, needs: &Needs) -> impl Future<Output = Result<(), NotReady>> {
        let _ = needs;
        async { Ok(()) }
    }

    /// Runs each batch's instructions on its shots' states, and writes their
    /// readouts into `readouts`, the caller's: `readouts[i]` holds those of
    /// `batches[i]`, shaped by the protocol as a row per shot, in its set's
    /// order, of a value per readout slot of its instructions, as its
    /// sender stated them ([`Batch::readout_slots`]), each value
    /// `Missing(ReadoutFailure::Other)` and each shot clear of conditions
    /// ([`BatchReadouts`]). The executor fills each row
    /// ([`row_mut`](BatchReadouts::row_mut)) and each shot's conditions
    /// ([`set_conditions`](BatchReadouts::set_conditions)), and writes
    /// every value: one it does not write stays missing, unreported, since
    /// the protocol checks no value at run time. It keeps the shape of each
    /// batch's readouts, which the protocol does not check again once it
    /// answers: readouts replaced by others of another shape reach the
    /// caller as they are. A conformance kit, planned, will check both in
    /// the implementer's tests. What `readouts` hold after an error, or
    /// once its future is dropped before it answers, is unspecified: the
    /// caller reads them after an `Ok` only. A shot's state is made at its
    /// first batch, from the needs it got ready for and, for a
    /// replayable executor, [`seed.shot(id)`](Seed::shot), as [`Seed`]
    /// says, and kept until [`end_shots`](Self::end_shots), across any
    /// number of `execute`s, whether or not they read anything: a shot's
    /// instructions may come in as many batches as its caller sends, each
    /// within the most this executor takes, and its readouts across them
    /// are those it would get from the same instructions sent in one
    /// batch. The protocol holds only the batches of one `execute`, so the
    /// caller chooses how much of a program it holds;
    /// [`execute_stream`](Self::execute_stream) cuts a stream of
    /// instructions into such batches for it.
    ///
    /// The caller passes one `BatchReadouts` per batch, of any shape, as
    /// [`Readouts::readouts_for`](crate::Readouts::readouts_for) gives them;
    /// for another count, the protocol panics in the caller's `execute`,
    /// once polled, which fails the run naming the caller. `batches` is
    /// never empty: every `execute` reaches the executor through the
    /// protocol's boundary, which answers one of no batches `Ok` itself,
    /// so an executor need not handle that case.
    ///
    /// Three rules hold between the part that calls it and the executor:
    ///
    /// - the caller puts a shot in at most one batch of an `execute`;
    /// - shots are independent, so the executor may run the batches in any
    ///   order, or at once, and may share work between shots, such as one
    ///   state for shots whose instructions are equal so far, as long as
    ///   each shot's readouts are those it would get run alone;
    /// - the executor leaves a shot in no batch untouched.
    ///
    /// The protocol does not check the first rule, which the caller keeps:
    /// a check across batches needs a set of their shots per `execute`,
    /// where the protocol's other checks of an `execute` are per
    /// instruction and per batch.
    ///
    /// Its future may be dropped before it answers: when its run's future
    /// is dropped, when a part above panics while it waits, or when a part
    /// above stops waiting for it. The runner ends the run's shots
    /// ([`end_shots`](Self::end_shots)) once the run ends, whether it gave
    /// outputs, failed, or its future was dropped; the driver may also end
    /// each shot earlier, as it ends.
    ///
    /// An `execute` does its work when its future is first polled, not when
    /// it is called, as Rust's futures do by convention (an
    /// `async fn execute`'s does): a future dropped before its first poll
    /// has then changed nothing, even when `execute` is called directly
    /// rather than through the protocol. It is a rule for implementers,
    /// which the protocol does not check; the protocol's own boundary keeps
    /// it, calling the receiver's `execute` only when its own future is
    /// first polled.
    fn execute(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        readouts: &mut [BatchReadouts],
    ) -> impl Future<Output = Result<(), ExecuteError>>;

    /// As [`execute`](Self::execute), and gives `each` each shot's row as
    /// soon as it is final, before the `execute` answers: so that a caller
    /// can use a shot's readouts (record them, show progress, start the
    /// shot's classical work) while the executor runs the other shots. An
    /// executor that finishes its shots one by one, or a group or a shard
    /// at a time, overrides it; by default, it is `execute`, then, once it
    /// answers `Ok`, every row, batch by batch, each batch in its shots'
    /// order, so every executor serves it; an `execute` that fails gives no
    /// row. A row is its batch's in this call: for a shot whose
    /// instructions come over several calls, each call gives the row of
    /// that call's instructions.
    ///
    /// The rules between the caller and the executor, which the executor
    /// keeps and the protocol does not check:
    ///
    /// - `each` is given each row at most once; after an `Ok`, it was given
    ///   each row of every batch exactly once; after an error, any rows it
    ///   was given are final, and the others are unspecified;
    /// - after an `Ok`, a row given to `each` holds the values and
    ///   conditions `readouts[batch]` holds for that row when the call
    ///   answers: the executor does not write the row again; after an
    ///   error, a row given is final as given, and the readouts are
    ///   unspecified, as after any error;
    /// - `each` is called on the thread that polls this future: an
    ///   executor that runs shots on other threads passes their rows back
    ///   to it, so a caller's `each` need not be `Send`.
    ///
    /// Through the protocol's boundary, a panic in `each` is the caller's,
    /// not the executor's: the boundary gives no more rows once `each`
    /// panicked, and raises the panic again once the executor answers,
    /// through the caller's call, so that the run fails naming the caller,
    /// even when the executor panicked or failed too. If this call is
    /// dropped before the executor answers, that panic is dropped with it.
    fn execute_each(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        readouts: &mut [BatchReadouts],
        each: &mut impl FnMut(FinalRow<'_>),
    ) -> impl Future<Output = Result<(), ExecuteError>> {
        async move {
            await_part(self.execute(batches, seed, readouts)).await?;
            give_every_row(batches, readouts, each);
            Ok(())
        }
    }

    /// Runs `stream`'s instructions on its shots' states, pulling them as
    /// it runs them, and writes their readouts into `readouts`, the
    /// caller's, which it shapes as the protocol shapes a batch's: a row per
    /// shot, in its set's order, of a value per readout slot of the
    /// stream's instructions. They are the readouts the same instructions
    /// would get sent as one batch, and the shots' states are kept after it
    /// as after an `execute`, so a sender may send a shot's program over any
    /// number of streams and `execute`s. A stream may be empty, and still
    /// reaches the executor, unlike an `execute` of no batches: its
    /// readouts are then a row of no slot per shot.
    ///
    /// For a sender whose program is too long to hold
    /// ([`InstructionStream`]). By default, it runs the stream in pieces, each
    /// one `execute` of one batch of at most the most instructions this
    /// executor takes ([`Offer::max_instructions`](crate::Offer), or 1 if it
    /// takes none, or a thousand or so when it sets none), pulling each piece
    /// only once the one before it has answered, so every executor serves it.
    /// It then holds one piece and its readouts, and the rows of the pieces
    /// that ran, joined as they come into one buffer that becomes
    /// `readouts`: at most about twice the stream's readouts, however small
    /// its pieces. An executor that applies
    /// instructions as they come, such as a simulator, or one that passes
    /// them on over a network, overrides it, to take the stream whole.
    ///
    /// A refusal ends the stream: of the first piece, as an
    /// [`ExecuteError::Refused`], nothing having run; of a later one, as a
    /// failure ([`Failure::Refused`]), the pieces before it having run. A
    /// refusal names the stream as batch 0, and an instruction by its
    /// position in the stream. As after an `execute`'s error, the readouts
    /// are then unspecified, and the shots' states are kept until
    /// [`end_shots`](Self::end_shots); so they are when its future is
    /// dropped mid-stream. A sender whose production fails ends its
    /// iterator, and reports its own error once this call answers.
    ///
    /// Through the protocol's boundary, a
    /// [`checked`](InstructionStream::checked) stream is checked a piece at
    /// a time, the first before the executor is called, so that a stream
    /// refused in its first piece never reaches it; and a panic in the
    /// stream's iterator is the caller's, not the executor's: the stream
    /// ends there, and the panic is raised again once the executor
    /// answers, through the caller's call, as a panic in
    /// [`execute_each`](Self::execute_each)'s `each` is.
    fn execute_stream(
        &mut self,
        stream: InstructionStream<'_, impl Iterator<Item = Instruction>>,
        seed: Seed,
        readouts: &mut BatchReadouts,
    ) -> impl Future<Output = Result<(), ExecuteError>> {
        async move {
            let most = most_per_piece(self.offers().max_instructions);
            StreamPieces::run(
                stream,
                most,
                self,
                Self::accepts,
                async |executor, batch, piece_readouts| {
                    await_part(executor.execute(
                        slice::from_ref(batch),
                        seed,
                        slice::from_mut(piece_readouts),
                    ))
                    .await
                },
                readouts,
            )
            .await
        }
    }

    /// Frees what it holds for shots that ended. The protocol calls it on
    /// every part, so a part never forwards it below; by default it does
    /// nothing. `shots` may hold shots it does not hold, or no longer
    /// holds, such as when the runner ends every shot of a run, some of
    /// which the driver had already ended: it does nothing for those. It
    /// may also follow an `execute` dropped before it answered, such as
    /// when its run's future was dropped, a part above panicked while it
    /// waited, or a part above stopped waiting for it: the part then
    /// cancels the work that `execute` started for these shots, or ignores
    /// its readouts. A panic here, in an end a part above calls (the driver,
    /// or an adapter in one of its `adapt` methods), fails the run naming
    /// that part; in the runner's end, it is raised again from the run
    /// ([`Runner::run_async`](crate::Runner::run_async)).
    fn end_shots(&mut self, shots: &ShotSet) {
        let _ = shots;
    }
}

// Every method forwarded: one left at its default would not reach `E`'s
// own (a default `end_shots` does nothing; a default `execute_each` gives
// no row before `execute` answers; a default `execute_stream` cuts the
// stream into batches `E` may take whole).
#[deny(clippy::missing_trait_methods)]
impl<E: Execute + ?Sized> Execute for &mut E {
    fn ready(&mut self, needs: &Needs) -> impl Future<Output = Result<(), NotReady>> {
        (**self).ready(needs)
    }

    fn execute(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        readouts: &mut [BatchReadouts],
    ) -> impl Future<Output = Result<(), ExecuteError>> {
        (**self).execute(batches, seed, readouts)
    }

    fn execute_each(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        readouts: &mut [BatchReadouts],
        each: &mut impl FnMut(FinalRow<'_>),
    ) -> impl Future<Output = Result<(), ExecuteError>> {
        (**self).execute_each(batches, seed, readouts, each)
    }

    fn execute_stream(
        &mut self,
        stream: InstructionStream<'_, impl Iterator<Item = Instruction>>,
        seed: Seed,
        readouts: &mut BatchReadouts,
    ) -> impl Future<Output = Result<(), ExecuteError>> {
        (**self).execute_stream(stream, seed, readouts)
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        (**self).end_shots(shots);
    }
}

/// What an `execute` returns when it does not answer: it was refused, and
/// nothing of it ran, or it failed for a shot. What its readouts hold is
/// then unspecified.
#[derive(Debug)]
#[non_exhaustive]
pub enum ExecuteError {
    /// Nothing of the `execute` ran.
    Refused(ExecuteRefusal),
    /// Work started and broke; `shot` is the one the failure names.
    Failed {
        /// The shot the failure names: the failing part's own choice, or
        /// the first shot of the batch or run that failed, for a failure
        /// the protocol makes ([`Failure`]).
        shot: ShotId,
        /// Why.
        failure: Failure,
    },
}

impl fmt::Display for ExecuteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExecuteError::Refused(refusal) => write!(f, "execute refused: {refusal}"),
            ExecuteError::Failed { shot, failure } => {
                write!(f, "shot {shot} failed: {failure}")
            }
        }
    }
}

impl Error for ExecuteError {}

/// Why an `execute` was refused: nothing of it ran.
///
/// Two makers share it. The protocol makes the batch refusals, at the
/// boundary of the part that would receive the batch, before that part
/// runs any batch of the `execute`, so the part does not check them
/// itself: [`Instruction`](Self::Instruction),
/// [`TooManyInstructions`](Self::TooManyInstructions),
/// [`TooManyReadoutSlots`](Self::TooManyReadoutSlots) and
/// [`ReadoutSlotsMisstated`](Self::ReadoutSlotsMisstated); and, for a
/// stream, the default [`Execute::execute_stream`] and
/// [`Adapt::adapt_stream`](crate::Adapt::adapt_stream) make
/// [`TooManyReadoutSlots`](Self::TooManyReadoutSlots) too, as they count
/// its slots. The part makes
/// the shot refusals, since only it knows which shots it holds:
/// [`ShotNotHeld`](Self::ShotNotHeld) and
/// [`EarlierFailure`](Self::EarlierFailure). They share one type because
/// the boundary is itself an [`Execute`] to the part above, which gets
/// either kind from the `execute` it calls.
///
/// For a stream ([`InstructionStream`]), `batch` is 0, and `at` an
/// instruction's position in the stream.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExecuteRefusal {
    /// The instruction `batches[batch].instructions[at]` is not one the
    /// receiver accepts, for `fault`; `shot` is that batch's first, in its
    /// set's order. Checked for a [`checked`](crate::Batch::checked) batch,
    /// or a [`checked`](InstructionStream::checked) stream a piece at a
    /// time, only.
    Instruction {
        /// The batch's first shot.
        shot: ShotId,
        /// The batch's position in the `execute`.
        batch: usize,
        /// The instruction's position in its batch.
        at: usize,
        /// Why it is refused.
        fault: InstructionFault,
    },
    /// `batches[batch]` holds more instructions than the receiver's most
    /// ([`Offer::max_instructions`](crate::Offer::max_instructions)); `shot`
    /// is its first. Checked for every batch,
    /// [`checked`](crate::Batch::checked) or not.
    TooManyInstructions {
        /// The batch's first shot.
        shot: ShotId,
        /// The batch's position in the `execute`.
        batch: usize,
        /// The most instructions a batch may hold.
        max: u32,
        /// The batch's instructions.
        got: usize,
    },
    /// `batches[batch]`'s instructions together read more readout slots
    /// than a `u32` counts, from the instruction at `at` on, so no batch's
    /// readouts could hold them; `shot` is its first. Checked for a
    /// [`checked`](crate::Batch::checked) batch, a checked stream, and any
    /// stream the default `execute_stream` or `adapt_stream` runs, checked
    /// or not.
    TooManyReadoutSlots {
        /// The batch's first shot.
        shot: ShotId,
        /// The batch's position in the `execute`.
        batch: usize,
        /// The position, in its batch, of the instruction whose slots no
        /// longer fit.
        at: usize,
    },
    /// `batches[batch]`'s instructions read `counted` readout slots, and
    /// its sender stated `stated`
    /// ([`Batch::readout_slots`](crate::Batch::readout_slots)), which its
    /// readouts would have been shaped with; `shot` is its first.
    /// Checked for a [`checked`](crate::Batch::checked) batch only.
    ReadoutSlotsMisstated {
        /// The batch's first shot.
        shot: ShotId,
        /// The batch's position in the `execute`.
        batch: usize,
        /// The readout slots its sender stated.
        stated: u32,
        /// The readout slots of its instructions.
        counted: u32,
    },
    /// The shot ended, or never started, at the part that refuses: a bug of
    /// the part that sent it.
    ShotNotHeld(ShotId),
    /// The shot failed in an earlier `execute`: a bug of the part that sent
    /// it again.
    EarlierFailure(ShotId),
}

impl ExecuteRefusal {
    /// The shot the refusal names, so that a driver's `?` reports it as that
    /// shot's failure.
    #[must_use]
    pub fn shot(&self) -> ShotId {
        match self {
            ExecuteRefusal::Instruction { shot, .. }
            | ExecuteRefusal::TooManyInstructions { shot, .. }
            | ExecuteRefusal::TooManyReadoutSlots { shot, .. }
            | ExecuteRefusal::ReadoutSlotsMisstated { shot, .. }
            | ExecuteRefusal::ShotNotHeld(shot)
            | ExecuteRefusal::EarlierFailure(shot) => *shot,
        }
    }
}

impl fmt::Display for ExecuteRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExecuteRefusal::Instruction {
                shot,
                batch,
                at,
                fault,
            } => write!(
                f,
                "instruction {at} of batch {batch} (first shot {shot}): {fault}"
            ),
            ExecuteRefusal::TooManyInstructions {
                shot,
                batch,
                max,
                got,
            } => write!(
                f,
                "batch {batch} (first shot {shot}) holds {got} instructions, and at most {max} are accepted"
            ),
            ExecuteRefusal::TooManyReadoutSlots { shot, batch, at } => write!(
                f,
                "batch {batch} (first shot {shot}) reads more readout slots than a `u32` \
                 counts, from instruction {at} on"
            ),
            ExecuteRefusal::ReadoutSlotsMisstated {
                shot,
                batch,
                stated,
                counted,
            } => write!(
                f,
                "batch {batch} (first shot {shot}) states {stated} readout slots, and its \
                 instructions read {counted}"
            ),
            ExecuteRefusal::ShotNotHeld(shot) => {
                write!(f, "shot {shot} is not held: it ended, or never started")
            }
            ExecuteRefusal::EarlierFailure(shot) => {
                write!(f, "shot {shot} failed in an earlier execute")
            }
        }
    }
}

impl Error for ExecuteRefusal {}
