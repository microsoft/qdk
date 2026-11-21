// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The driver's role, [`Driver`], and what a `drive` returns when it gives
//! no outputs: [`DriveError`].

use std::error::Error;
use std::fmt;

use crate::composition::chain_check::ChainCheck;
use crate::{
    ExecuteError, Executor, Failure, InstructionSet, Needs, NotReady, Offer, OpcodeMap, Seed,
    ShotId, ShotSet,
};

/// Calls the part below with batches of the instructions it sends, each
/// with its shots, and the seed, and makes each shot's output from the
/// readouts the part below fills, which the driver keeps in a
/// [`Readouts`](crate::Readouts) to reuse from one call to the next.
///
/// Such as a program's interpreter, which sends each shot's instructions
/// up to its next branch on a readout, or a generator of a program too
/// long to hold, which sends it in bounded batches and reads only at the
/// end: a driver calls the part below as many times as it likes per shot.
/// It is the part at the top of a chain, and the one a runner runs shots
/// on.
///
/// # Declarations
///
/// The joins ([`DriverJoins`](crate::DriverJoins)) read them once, before
/// any run: [`name`](Self::name), [`sends`](Self::sends) and
/// [`needs`](Self::needs). The join to the part below hands back what it
/// made of them: [`bind`](Self::bind) gets the fit's opcode map. A join
/// only decides, from what the parts declare; what prepares a part to run
/// belongs to each run.
///
/// Its instruction set and its needs must not change once it is joined:
/// the fit, `bind`'s map and the admission are made once, from them, and
/// each part below gets ready, on every run, for the needs read then.
///
/// # Run
///
/// On every run of the runner that
/// [`on`](crate::DriverJoins::on) makes, the runner gets it ready
/// ([`ready`](Self::ready)), then runs the shots on it
/// ([`drive`](Self::drive)).
pub trait Driver {
    /// What a run gives for each shot.
    type Output;

    /// As [`Executor::name`].
    fn name(&self) -> &str {
        std::any::type_name::<Self>()
    }

    /// The instructions it sends, which must fit the part below.
    fn sends(&self) -> &InstructionSet;

    /// What its shots need of the chain below, such as their qubits, which
    /// the offer below it must meet, and, adapted by each adapter, the
    /// offer of each part further below. The
    /// [`on`](crate::DriverJoins::on) that makes a runner reads it once.
    fn needs(&self) -> Needs;

    /// Receives, once, the translation of its opcodes into the part
    /// below's. An opcode is an instruction's place in its set, and the
    /// part below may number the same instructions differently, so every
    /// instruction it sends must carry the receiver's opcode,
    /// `below.map(opcode)`: an untranslated one names whatever instruction
    /// the receiver has at that place. The join to the part below calls it
    /// once, after the fit and before any run; a driver rewrites its
    /// prebuilt instructions then, or keeps the map to translate each one
    /// it sends.
    fn bind(&mut self, below: OpcodeMap);

    /// As [`Executor::ready`], asked last; a driver knows its own needs,
    /// so it gets none.
    fn ready(&mut self) -> impl Future<Output = Result<(), NotReady>> {
        async { Ok(()) }
    }

    /// Runs `shots` by calling `below`, the rest of the chain as one
    /// executor, until each shot ends, and gives each shot's output, in the
    /// order of `shots`. An error from below passes up with `?`, through
    /// `From<ExecuteError>`. The runner returns the outputs as given: it
    /// does not check their count or order.
    ///
    /// Each batch it sends below holds only shots of `shots`: a rule the
    /// driver keeps, which the protocol does not check. A shot outside them
    /// is not counted against the most shots a run may have
    /// ([`Offer::max_shots`](crate::Offer::max_shots)), and the runner
    /// does not end it, so a part may carry its state into a later run,
    /// which then does not repeat with its seed.
    ///
    /// It decides whether each batch it sends is checked before it runs, as
    /// [`Batch::checked`](crate::Batch::checked) says.
    ///
    /// Once `drive` returns, fails or panics, or its run's future is
    /// dropped, the protocol ends every shot of `shots` on every part below
    /// ([`Executor::end_shots`]), so a driver need not end them. It may end
    /// a shot below as soon as that shot ends, to free its state early; the
    /// protocol's second end of it does nothing. A part's panic in an end
    /// the driver calls fails the run naming the driver (and in one an
    /// adapter calls in `adapt`, naming that adapter), where the same panic
    /// in the protocol's end is raised again from the run
    /// ([`Runner::run_async`](crate::Runner::run_async)).
    ///
    /// A `drive` does its work, calling `below` included, when its future is
    /// first polled, not when it is called, as Rust's futures do by
    /// convention (an `async fn drive`'s does), and as
    /// [`Executor::execute`] asks of an `execute`: a future dropped before
    /// its first poll has then sent nothing below. It is a rule for
    /// implementers, which the protocol does not check.
    fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Executor,
    ) -> impl Future<Output = Result<Vec<Self::Output>, DriveError>>;

    // Protocol hook: sealed and hidden. Its parameter type, `ChainCheck`
    // (`src/composition/chain_check.rs`), cannot be named outside this
    // crate, so no other crate can call or override it. It is in the trait
    // because Rust cannot specialise a default method: the protocol's
    // joined parts (`Composed`, in `src/composition/composed/`) override
    // it to add each inner part's facts, so that a refusal names the inner
    // part that was short, and the chain's runner names each inner part
    // by its position. Correctness does not depend on it: a joined part
    // the check does not reach, such as one inside a part of the user's
    // that forwards only the public methods, runs the same.

    /// Adds its needs, as this part's, to `chain`, then has the chain
    /// `below` add its facts for them, and gives the offer that reaches it
    /// from below. Only the crate overrides it, to add each part it holds,
    /// with the fit faults of each boundary it holds, top to bottom. The
    /// [`on`](crate::DriverJoins::on) that makes a runner calls it once.
    #[doc(hidden)]
    fn check_chain(
        &mut self,
        below: &mut dyn FnMut(Needs, &mut ChainCheck) -> Offer,
        chain: &mut ChainCheck,
    ) -> Offer {
        let needs = self.needs();
        chain.add_needs(Driver::name(self), &needs);
        below(needs, chain)
    }
}

/// What a `drive` returns to the runner when it gives no outputs: the shot
/// the failure names, and why.
///
/// A driver can only fail, never refuse: once a run started, an `execute`'s
/// refusal is a failure of the run, [`Failure::Refused`].
#[derive(Debug)]
pub struct DriveError {
    /// The shot the failure names: the failing part's own choice, or the
    /// first shot of the batch or run that failed, for a failure the
    /// protocol makes ([`Failure`]).
    pub shot: ShotId,
    /// Why.
    pub failure: Failure,
}

/// So that a driver passes an error from below with `?`: a failure as it
/// is, a refusal as [`Failure::Refused`], at the shot it names.
impl From<ExecuteError> for DriveError {
    fn from(error: ExecuteError) -> Self {
        match error {
            ExecuteError::Refused(refusal) => Self {
                shot: refusal.shot(),
                failure: Failure::Refused(refusal),
            },
            ExecuteError::Failed { shot, failure } => Self { shot, failure },
        }
    }
}

impl fmt::Display for DriveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "shot {} failed: {}", self.shot, self.failure)
    }
}

impl Error for DriveError {}
