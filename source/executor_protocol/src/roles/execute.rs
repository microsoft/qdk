// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The executor's role, [`Execute`], and what an `execute` returns when it
//! gives no readouts: [`ExecuteError`], with [`ExecuteRefusal`] when nothing
//! of it ran.

use std::error::Error;
use std::fmt;

use crate::admission::part_list::PartList;
use crate::{
    Batch, BatchReadouts, Capabilities, Failure, InstructionFault, InstructionSet, Needs,
    RunRefusal, Seed, ShotId, ShotSet,
};

/// Runs batches of the instructions it accepts on their shots' states.
///
/// It is the part at the bottom of a chain, and the part a driver or an
/// adapter calls below it: the rest of the chain, composed, is one executor
/// to the part above. The protocol checks each `execute` it receives: every
/// instruction against its set and the needs it was provisioned for, each
/// batch against its most instructions, and its answer's shape; so it runs
/// only what it accepts.
pub trait Execute {
    // Composition time.

    /// The instructions it runs, which the part above must fit.
    fn accepts(&self) -> &InstructionSet;

    /// What it can ever serve, such as its most qubits, which the driver's
    /// needs, as adapted down to it, must fit.
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
    fn needs_by_part(&self, above: Needs, parts: &mut PartList) -> Needs {
        let _ = parts;
        above
    }

    /// Its offer, listed as this part's; only the crate overrides it, to
    /// list each adapter's too.
    #[doc(hidden)]
    fn offers_by_part(&self, parts: &mut PartList) -> Capabilities {
        let offer = self.offers();
        parts.note_offer::<Self>(&offer);
        offer
    }

    // Run time.

    /// Says whether this part can take a run now, and refuses it if not,
    /// before anything runs. The runner asks every part once per run, the
    /// part below first, before the driver starts; by default `Ok`. A device
    /// opens its session here, or refuses with
    /// [`RunRefusal::Unavailable`].
    fn ready(&mut self) -> impl Future<Output = Result<(), RunRefusal>> {
        async { Ok(()) }
    }

    /// Runs each batch's instructions on its shots' states, and returns
    /// their readouts: `readouts[i]` answers `batches[i]`, a row per shot, in
    /// its set's order, of a value per readout slot. A shot's state is made
    /// at its first batch, from the needs it was provisioned for and
    /// [`seed.shot(id)`](Seed::shot), and kept until
    /// [`end_shots`](Self::end_shots).
    fn execute(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
    ) -> impl Future<Output = Result<Vec<BatchReadouts>, ExecuteError>>;

    /// Frees what it holds for shots that ended. The protocol calls it on
    /// every part, so a part never forwards it below; by default it does
    /// nothing. `shots` may hold shots it does not hold, or no longer
    /// holds, such as when the runner ends every shot of a failed run,
    /// some of which the driver had already ended: it does nothing for
    /// those.
    fn end_shots(&mut self, shots: &ShotSet) {
        let _ = shots;
    }
}

impl<E: Execute + ?Sized> Execute for &mut E {
    fn accepts(&self) -> &InstructionSet {
        (**self).accepts()
    }

    fn offers(&self) -> Capabilities {
        (**self).offers()
    }

    fn provision(&mut self, needs: Needs) {
        (**self).provision(needs);
    }

    fn needs_by_part(&self, above: Needs, parts: &mut PartList) -> Needs {
        (**self).needs_by_part(above, parts)
    }

    fn offers_by_part(&self, parts: &mut PartList) -> Capabilities {
        (**self).offers_by_part(parts)
    }

    fn ready(&mut self) -> impl Future<Output = Result<(), RunRefusal>> {
        (**self).ready()
    }

    fn execute(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
    ) -> impl Future<Output = Result<Vec<BatchReadouts>, ExecuteError>> {
        (**self).execute(batches, seed)
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        (**self).end_shots(shots);
    }
}

/// What an `execute` returns when it gives no readouts: it was refused, and
/// nothing of it ran, or it failed for a shot.
#[derive(Debug)]
#[non_exhaustive]
pub enum ExecuteError {
    /// Nothing of the `execute` ran.
    Refused(ExecuteRefusal),
    /// Work started and broke for `shot`.
    Failed {
        /// The shot whose work broke.
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
            ExecuteRefusal::ShotNotHeld(shot) => write!(f, "shot {shot} is not held"),
            ExecuteRefusal::EarlierFailure(shot) => {
                write!(f, "shot {shot} failed in an earlier execute")
            }
        }
    }
}

impl Error for ExecuteRefusal {}
