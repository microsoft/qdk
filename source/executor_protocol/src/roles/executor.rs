// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The executor's role, [`Executor`], and what an `execute` returns when
//! it does not answer: [`ExecuteError`], with [`ExecuteRefusal`] when
//! nothing of it ran.

use std::error::Error;
use std::fmt;

use crate::composition::DeclaredParts;
use crate::part_id::ChainPosition;
use crate::{
    Batch, BatchFault, BatchReadouts, Capabilities, Failure, InstructionSet, NotReady, Seed,
    ShotId, ShotSet,
};

/// Runs batches of the instructions it accepts on their shots' states.
///
/// It is the part at the bottom of a chain, and the part a driver or an
/// adapter calls below it: the rest of the chain, joined, is one executor
/// to the part above. The protocol shapes each batch's readouts from the
/// batch before the executor runs it, so the executor only fills them,
/// and the part above indexes its batches, rows and slots.
/// A batch its sender marked [`checked`](crate::Batch::checked) the
/// protocol also checks as that method says, against what the executor
/// accepts and provides and what arrives at it, so the executor runs only
/// what it accepts; an unchecked batch, the default, the executor runs as
/// the part above sends it, once the protocol has checked that it holds no
/// more instructions than the executor takes.
///
/// # Declarations
///
/// The joins ([`DriverJoins`](crate::DriverJoins) and
/// [`AdapterJoins`](crate::AdapterJoins)) read them before any run:
/// [`name`](Self::name), [`accepts`](Self::accepts) and
/// [`provides`](Self::provides). It gets no hand-back: it sends nothing,
/// so it gets no opcode map. A join only decides, from what the parts
/// declare; what prepares a part to run belongs to each run, which starts
/// with [`ready`](Self::ready), given what arrives at it.
///
/// What it accepts and provides must not change once it is joined: the
/// fit is made once, from them, and each batch sent to it is checked
/// against what the join read.
///
/// # Run
///
/// On every run, the runner gets it ready ([`ready`](Self::ready)) and
/// ends its shots ([`end_shots`](Self::end_shots)), and the part above
/// calls it ([`execute`](Self::execute)).
pub trait Executor {
    /// The part's name in messages, such as a composition fault or a
    /// run's failure; by default its type's, `std::any::type_name`.
    ///
    /// A part overrides it when its type does not say what it is: a part
    /// built at run time, such as a runtime made from an instruction set's
    /// description, has one Rust type for every configuration; and the
    /// default's text is a module path, whose format the standard library
    /// does not guarantee. Two parts of one name are told apart by their
    /// position ([`PartId`](crate::PartId)). Parts joined by
    /// [`DriverJoins::through`](crate::DriverJoins::through),
    /// [`AdapterJoins::through`](crate::AdapterJoins::through) or
    /// [`AdapterJoins::on`](crate::AdapterJoins::on) answer the name of
    /// their top part, the one written first. The protocol's messages use
    /// the name read by the [`on`](crate::DriverJoins::on) that makes a
    /// runner.
    fn name(&self) -> &str {
        std::any::type_name::<Self>()
    }

    /// The instructions it runs: what the part above sends must fit them.
    /// Each checked batch it receives is checked against them as read by
    /// the `on` that made the runner, or by the join that made the joined
    /// part it is in.
    fn accepts(&self) -> &InstructionSet;

    /// Its fixed limits on what arrives from above, such as its most
    /// qubits, its most shots in a run, or whether a run may fix its seed:
    /// what arrives must fit them, as [`Capabilities`] says.
    ///
    /// By default [`Capabilities::UNCONSTRAINED`]: it states no limit.
    /// Stating its real limits is the writer's job: an executor that does
    /// not lets `on` accept a chain it cannot run, which then fails as it
    /// runs.
    fn provides(&self) -> &Capabilities {
        &crate::capabilities::NO_LIMITS
    }

    /// Gets this part ready for a run, before anything of the run runs, for
    /// `required`, what arrives from the part above: each
    /// [`Resource`](crate::Resource) and [`Feature`](crate::Feature) it
    /// states, such as the blocks of each type a shot holds at once, the most
    /// shots of a run, or whether a shot is run again alone. It takes what it
    /// needs to run, such as a device, or a layout of its blocks; or, when it
    /// cannot take a run now, such as when its device is busy or its resources
    /// are not available, answers [`NotReady`], which refuses the run
    /// ([`RunRefusal::PartNotReady`](crate::RunRefusal::PartNotReady)). By
    /// default `Ok`. `required` follows from the run's shots and whether its
    /// seed is fixed, so work that depends only on it may be done once and
    /// kept for the runs that give the same.
    ///
    /// The runner asks the parts in turn, each at most once per run, the
    /// part below first, before the driver starts, and stops at the first
    /// that refuses: no part above it is asked, nor any part when the run
    /// is refused for what it requires, nor a part the run has not
    /// reached when its future is dropped, or is never polled.
    fn ready(&mut self, required: &Capabilities) -> impl Future<Output = Result<(), NotReady>> {
        let _ = required;
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
    /// first batch, from what it got ready for and, for a
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
    ///   each shot's readouts are drawn as they would be run alone: at a
    ///   fixed seed, the same values, where it provides replay; from one
    ///   stream for the run's shots, where it does not, as [`Seed`] says;
    /// - the executor leaves a shot in no batch untouched.
    ///
    /// The protocol does not check the first rule, which the caller keeps:
    /// a check across batches needs a set of their shots per `execute`,
    /// where the protocol's other checks of an `execute` are per
    /// instruction and per batch.
    ///
    /// Its future may be dropped before it answers: when its run's future
    /// is dropped, when a part above panics while it waits, or when a part
    /// above stops waiting for it. The protocol ends the run's shots
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
    /// holds, such as when the protocol ends every shot of a run, some of
    /// which the driver had already ended: it does nothing for those. It
    /// may also follow an `execute` dropped before it answered, such as
    /// when its run's future was dropped, a part above panicked while it
    /// waited, or a part above stopped waiting for it: the part then
    /// cancels the work that `execute` started for these shots, or ignores
    /// its readouts. A panic here, in an end a part above calls (the driver,
    /// or an adapter in `adapt`), fails the run naming that part; in the
    /// protocol's end, it is raised again from the run
    /// ([`Runner::run_async`](crate::Runner::run_async)).
    fn end_shots(&mut self, shots: &ShotSet) {
        let _ = shots;
    }

    // Protocol hooks: sealed and hidden, as `Driver`'s are.

    /// Adds this part to `parts`, given `above`, what arrives at it. Only
    /// the crate overrides it, to add each part it holds, top to bottom.
    /// The [`on`](crate::DriverJoins::on) that makes a runner calls it,
    /// and the runner on each run.
    #[doc(hidden)]
    fn declare<'a>(&'a self, above: &Capabilities, parts: &mut DeclaredParts<'a>) {
        parts.push(Executor::name(self));
        parts.receive(self.accepts(), self.provides(), above);
    }

    /// [`ready`](Self::ready), for this part at `at` in its chain. Only
    /// the crate overrides it, to name a refusal of a part it holds by
    /// that part's position.
    #[doc(hidden)]
    fn ready_at(
        &mut self,
        required: &Capabilities,
        at: ChainPosition<'_>,
    ) -> impl Future<Output = Result<(), NotReady>> {
        let _ = at;
        self.ready(required)
    }
}

// A lent executor is the executor itself: every method forwards, and the
// lint fails the build if a new one falls back to its default instead.
#[deny(clippy::missing_trait_methods)]
impl<E: Executor + ?Sized> Executor for &mut E {
    fn name(&self) -> &str {
        (**self).name()
    }

    fn accepts(&self) -> &InstructionSet {
        (**self).accepts()
    }

    fn provides(&self) -> &Capabilities {
        (**self).provides()
    }

    fn ready(&mut self, required: &Capabilities) -> impl Future<Output = Result<(), NotReady>> {
        (**self).ready(required)
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

    fn declare<'a>(&'a self, above: &Capabilities, parts: &mut DeclaredParts<'a>) {
        (**self).declare(above, parts);
    }

    fn ready_at(
        &mut self,
        required: &Capabilities,
        at: ChainPosition<'_>,
    ) -> impl Future<Output = Result<(), NotReady>> {
        (**self).ready_at(required, at)
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
/// Two makers share it. The protocol makes the batch refusals
/// ([`Batch`](Self::Batch)), at the boundary of the part that would
/// receive the batch, before that part runs any batch of the `execute`,
/// so the part does not check them itself. The part makes
/// the shot refusals, since only it knows which shots it holds:
/// [`ShotNotHeld`](Self::ShotNotHeld) and
/// [`EarlierFailure`](Self::EarlierFailure). They share one type because
/// the boundary is itself an [`Executor`] to the part above, which gets
/// either kind from the `execute` it calls.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExecuteRefusal {
    /// `batches[batch]` is refused, for `fault`; `shot` is that batch's
    /// first, in its set's order.
    Batch {
        /// The batch's first shot.
        shot: ShotId,
        /// The batch's position in the `execute`.
        batch: usize,
        /// Why it is refused.
        fault: BatchFault,
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
            ExecuteRefusal::Batch { shot, .. }
            | ExecuteRefusal::ShotNotHeld(shot)
            | ExecuteRefusal::EarlierFailure(shot) => *shot,
        }
    }
}

impl fmt::Display for ExecuteRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExecuteRefusal::Batch { shot, batch, fault } => {
                write!(f, "batch {batch} (first shot {shot}) is refused: {fault}")
            }
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
