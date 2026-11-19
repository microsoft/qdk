// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The executor's role at run time, [`Execute`], and what an `execute`
//! returns when it does not answer: [`ExecuteError`], with
//! [`ExecuteRefusal`] when nothing of it ran, whose [`RetryClass`] says
//! whether the part above may send it again.

mod retry_class;

use std::error::Error;
use std::fmt;

use crate::{
    Batch, BatchReadouts, ExecutorSetup, Failure, InstructionFault, Needs, NotReady, Seed, ShotId,
    ShotSet,
};

pub use retry_class::RetryClass;

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
    /// instructions may come in as many batches as its caller sends.
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
    /// or an adapter in `adapt`), fails the run naming that part; in the
    /// runner's end, it is raised again from the run
    /// ([`Runner::run_async`](crate::Runner::run_async)).
    fn end_shots(&mut self, shots: &ShotSet) {
        let _ = shots;
    }
}

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
/// [`ReadoutSlotsMisstated`](Self::ReadoutSlotsMisstated). The part makes
/// the shot refusals, since only it knows which shots it holds:
/// [`ShotNotHeld`](Self::ShotNotHeld) and
/// [`EarlierFailure`](Self::EarlierFailure); and
/// [`NotReady`](Self::NotReady), since only it knows whether it can take
/// the `execute` now. They share one type because the boundary is itself
/// an [`Execute`] to the part above, which gets either kind from the
/// `execute` it calls. Each says, by its
/// [`retry_class`](Self::retry_class), whether the part above may send the
/// same `execute` again.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExecuteRefusal {
    /// The instruction `batches[batch].instructions[at]` is not one the
    /// receiver accepts, for `fault`; `shot` is that batch's first, in its
    /// set's order. Checked for a [`checked`](crate::Batch::checked) batch
    /// only.
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
    /// [`checked`](crate::Batch::checked) batch only.
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
    /// The receiver cannot take the `execute` now, such as a device that is
    /// busy or calibrating, or a remote executor whose link is down, and
    /// why; `shot` is the first of its first batch. Unlike the other
    /// refusals, the same `execute` sent again later may run
    /// ([`RetryClass::Later`], which says on what that relies). Where a
    /// part's [`ready`](Execute::ready) refuses a run before anything of it
    /// runs, this refuses one `execute` of a run under way.
    NotReady {
        /// The first shot of the first batch.
        shot: ShotId,
        /// Why it cannot take it now. A text, not an error as
        /// [`NotReady::why`](crate::NotReady::why) is, so that the refusal
        /// stays `Clone` and `Eq`.
        why: String,
    },
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
            | ExecuteRefusal::EarlierFailure(shot)
            | ExecuteRefusal::NotReady { shot, .. } => *shot,
        }
    }

    /// Whether the part above may send the same `execute` again:
    /// [`RetryClass::Later`] for [`NotReady`](Self::NotReady),
    /// [`RetryClass::Never`] for every other refusal. A part that retries
    /// asks this, rather than matching variants, so that a refusal added
    /// later with its class is retried, or not, with no change to the part.
    #[must_use]
    pub fn retry_class(&self) -> RetryClass {
        match self {
            ExecuteRefusal::NotReady { .. } => RetryClass::Later,
            ExecuteRefusal::Instruction { .. }
            | ExecuteRefusal::TooManyInstructions { .. }
            | ExecuteRefusal::TooManyReadoutSlots { .. }
            | ExecuteRefusal::ReadoutSlotsMisstated { .. }
            | ExecuteRefusal::ShotNotHeld(_)
            | ExecuteRefusal::EarlierFailure(_) => RetryClass::Never,
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
            ExecuteRefusal::NotReady { shot, why } => {
                write!(f, "not ready (first shot {shot}): {why}")
            }
        }
    }
}

impl Error for ExecuteRefusal {}
