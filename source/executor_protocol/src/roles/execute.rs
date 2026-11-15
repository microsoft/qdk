// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The executor's role, [`Execute`], and what an `execute` returns when it
//! appends no readouts: [`ExecuteError`], with [`ExecuteRefusal`] when
//! nothing of it ran.

use std::error::Error;
use std::fmt;

use crate::admission::by_part::ByPart;
use crate::checked::BatchChecks;
use crate::{
    Batch, Capabilities, Failure, InstructionFault, InstructionSet, Needs, Readouts, RunRefusal,
    Seed, ShotId, ShotSet,
};

/// Runs batches of the instructions it accepts on their shots' states.
///
/// It is the part at the bottom of a chain, and the part a driver or an
/// adapter calls below it: the rest of the chain, composed, is one executor
/// to the part above. The protocol checks the shape of the readouts each
/// `execute` it receives appends, so the part above can index them. When
/// the runner turns the batch checks on
/// ([`Runner::set_batch_checks`](crate::Runner::set_batch_checks)), it also
/// checks every instruction against its set and the needs it was
/// provisioned for, and each batch against its most instructions, so it
/// runs only what it accepts; with them off, the default, it runs what the
/// part above sends.
pub trait Execute {
    // Composition time.

    /// The part's name in messages, such as an admission fault or a
    /// run's failure; by default its type's, `std::any::type_name`.
    ///
    /// A part overrides it when its type does not say what it is: a part
    /// built at run time, such as a runtime made from an instruction set's
    /// description, has one Rust type for every configuration; and the
    /// default's text is a module path, whose format the standard library
    /// does not guarantee. Two parts of one name are told apart by their
    /// position ([`ChainPart`](crate::ChainPart)). Parts composed by `>>`
    /// answer the name of their top part, the one that receives their
    /// calls. The protocol's messages use the answer read by the `>>` that
    /// makes a runner.
    fn name(&self) -> &str {
        std::any::type_name::<Self>()
    }

    /// The instructions it runs, which the part above must fit.
    ///
    /// Its answer must not change once the part is composed: the fit and
    /// `bind`'s map are made once, by `>>`, and the set the batch checks
    /// hold each batch it receives to is read once, before it is
    /// provisioned.
    fn accepts(&self) -> &InstructionSet;

    /// What it can ever serve, such as its most qubits, which the driver's
    /// needs, as adapted down to it, must fit. Its answer must not change
    /// once the executor is composed, not even by its own `provision`: the
    /// `>>` that makes a runner reads it once, before any part is
    /// provisioned, and the admission and every check of a run use that
    /// answer. Its most shots and whether it is seedable bound every run of
    /// the chain: no adapter above can raise them.
    fn offers(&self) -> Capabilities;

    /// Provisions this executor for `needs`, the driver's needs as adapted
    /// down to it, once the chain is admitted: the blocks of each type a
    /// shot holds at once, and whether its instructions use readout
    /// parities. The `>>` that makes a runner calls it once; by default it
    /// does nothing. It cannot fail: the admission checked `needs` against
    /// [`offers`](Self::offers).
    fn provision(&mut self, needs: Needs) {
        let _ = needs;
    }

    /// The needs that reach this executor from `above`, listing those of
    /// each adapter it holds; only the crate overrides it.
    #[doc(hidden)]
    fn needs_by_part(&self, above: Needs, by_part: &mut ByPart) -> Needs {
        let _ = by_part;
        above
    }

    /// Its offer, listed as this part's; only the crate overrides it, to
    /// list each adapter's too, and to keep the offer that crosses each
    /// boundary it holds. The `>>` that makes a runner calls it once, before
    /// any part is provisioned.
    #[doc(hidden)]
    fn offers_by_part(&mut self, by_part: &mut ByPart) -> Capabilities {
        let offer = self.offers();
        by_part.note_executor_offer(Execute::name(self), &offer);
        offer
    }

    /// Turns the batch checks of each boundary it holds on or off; by
    /// default it holds none. Only the crate overrides it.
    #[doc(hidden)]
    fn set_batch_checks(&mut self, checks: BatchChecks) {
        let _ = checks;
    }

    // Run time.

    /// Says whether this part can take a run now, and refuses it if not,
    /// before anything runs. The runner asks the parts in turn, each at
    /// most once per run, the part below first, before the driver starts,
    /// and stops at the first that refuses: no part above it is asked, nor
    /// any part when the run is refused for its shots or its seed, nor a
    /// part the run has not reached when its future is dropped, or is never
    /// polled. By default `Ok`. A part
    /// that cannot take a run now, such as a busy device, refuses with
    /// [`RunRefusal::Unavailable`].
    fn ready(&mut self) -> impl Future<Output = Result<(), RunRefusal>> {
        async { Ok(()) }
    }

    /// Runs each batch's instructions on its shots' states, and appends
    /// their readouts to `readouts`, which the caller lends: one
    /// [`BatchReadouts`](crate::BatchReadouts) per batch, in the batches'
    /// order, a row per shot, in its set's order, of a value per readout
    /// slot. So, `start` being the length of `readouts` before, entry
    /// `start + i` answers `batches[i]`. Called through the protocol, it
    /// cannot change the readouts it found: it reads them as a slice, and
    /// [`Readouts::clear`](crate::Readouts::clear) keeps them; only
    /// assigning a whole new value to `*readouts` would replace them,
    /// which no executor may do. An `execute` that fails, or is dropped
    /// before it answers, leaves `readouts` as they were when it is called
    /// through the protocol, which removes what was appended. A shot's
    /// state is made at
    /// its first batch, from the needs it was provisioned for and
    /// [`seed.shot(id)`](Seed::shot), and kept until
    /// [`end_shots`](Self::end_shots).
    ///
    /// Three rules hold between the part that calls it and the executor:
    ///
    /// - the caller puts a shot in at most one batch of an `execute`;
    /// - shots are independent, so the executor may run the batches in any
    ///   order, or at once;
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
    fn execute(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        readouts: &mut Readouts,
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
    /// its answer. A panic here, in an end a part above calls (the driver,
    /// or an adapter in `adapt`), fails the run naming that part; in the
    /// runner's end, it is raised again from the run
    /// ([`Runner::run_async`](crate::Runner::run_async)).
    fn end_shots(&mut self, shots: &ShotSet) {
        let _ = shots;
    }
}

impl<E: Execute + ?Sized> Execute for &mut E {
    fn name(&self) -> &str {
        (**self).name()
    }

    fn accepts(&self) -> &InstructionSet {
        (**self).accepts()
    }

    fn offers(&self) -> Capabilities {
        (**self).offers()
    }

    fn provision(&mut self, needs: Needs) {
        (**self).provision(needs);
    }

    fn needs_by_part(&self, above: Needs, by_part: &mut ByPart) -> Needs {
        (**self).needs_by_part(above, by_part)
    }

    fn offers_by_part(&mut self, by_part: &mut ByPart) -> Capabilities {
        (**self).offers_by_part(by_part)
    }

    fn set_batch_checks(&mut self, checks: BatchChecks) {
        (**self).set_batch_checks(checks);
    }

    fn ready(&mut self) -> impl Future<Output = Result<(), RunRefusal>> {
        (**self).ready()
    }

    fn execute(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        readouts: &mut Readouts,
    ) -> impl Future<Output = Result<(), ExecuteError>> {
        (**self).execute(batches, seed, readouts)
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        (**self).end_shots(shots);
    }
}

/// What an `execute` returns when it appends no readouts: it was refused,
/// and nothing of it ran, or it failed for a shot.
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
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExecuteRefusal {
    /// The instruction `batches[batch].instructions[at]` is not one the
    /// receiver accepts, for `fault`; `shot` is that batch's first, in its
    /// set's order.
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
    /// ([`Capabilities::max_instructions`]); `shot` is its first.
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
    /// than a `u32` counts, from the instruction at `at` on, so no
    /// answer's width could hold them; `shot` is its first.
    TooManyReadoutSlots {
        /// The batch's first shot.
        shot: ShotId,
        /// The batch's position in the `execute`.
        batch: usize,
        /// The position, in its batch, of the instruction whose slots no
        /// longer fit.
        at: usize,
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
            ExecuteRefusal::ShotNotHeld(shot) => write!(f, "shot {shot} is not held"),
            ExecuteRefusal::EarlierFailure(shot) => {
                write!(f, "shot {shot} failed in an earlier execute")
            }
        }
    }
}

impl Error for ExecuteRefusal {}
