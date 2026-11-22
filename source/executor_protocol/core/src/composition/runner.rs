// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Running shots on a chain: the [`Runner`], which only the
//! [`on`](crate::DriverJoins::on) of a driver with an executor makes, and
//! what a run returns when it gives no outputs, [`RunError`].
//!
//! A runner holds a chain that joined: each part accepts what the part
//! above sends, and provides what arrives at it from a run that states
//! nothing. Each run states what it requires, its shots and whether its
//! seed is fixed, and checks again, down the chain, that each part provides
//! what arrives at it; then it gets every part ready for what arrives at
//! it, or is refused by the first that cannot take a run now. Then the
//! driver runs the shots, calling the
//! chain below it as one executor, the readouts of each of whose batches the
//! protocol shapes from the batch, after checking every batch's count of
//! instructions, and every batch its sender marked
//! [`checked`](crate::Batch::checked). Once the run ends, whether it gave
//! outputs, failed, or its future was dropped, its shots are ended on every
//! part, so that none keeps them: each joined driver ends them on its adapters,
//! and the runner on the executor.

mod run_error;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod wait;

use std::panic::resume_unwind;

use super::DeclaredParts;
use super::boundary_crossing::BoundaryCrossing;
use super::part_panics::{PanicEndingShots, await_part, catch_panic, panic_message};
use super::unended_shots::UnendedShots;
use crate::part_id::ChainPosition;
use crate::{
    Boundary, Capabilities, DriveError, Driver, Executor, Failure, Features, InstructionSet,
    PartId, Resources, Seed, ShotSet,
};

pub use run_error::{RunError, RunRefusal};

/// A driver and an executor, joined by [`on`](crate::DriverJoins::on)
/// with any adapters between them, every part's instructions accepted and
/// what arrives at it provided: what runs shots.
///
/// Its type names the driver and the executor `on` joined. It has no
/// constructor: only `on` makes one, so that every runner's chain joined.
/// A joined part's type cannot be named, so code that keeps the runner of
/// a chain with adapters is generic over its type.
pub struct Runner<D: Driver, E: Executor<Accepts = D::Sends>> {
    pub(super) driver: D,
    pub(super) executor: E,
    /// Each part, top to bottom, by the name `on` read: what a refusal or a
    /// panic names, the driver's top part first.
    pub(super) parts: Vec<PartId>,
    /// The boundary between the driver and the executor, as the joins' fit
    /// found it: what the executor gets ready for on each run, what the
    /// driver's `below.accepts()` and `below.provides()` answer, the limits
    /// of the batches the driver sends, and the executor's top part, which
    /// its panic in an `execute` and its refusal name.
    pub(super) boundary: Boundary<D::Sends>,
}

impl<D: Driver, E: Executor<Accepts = D::Sends>> Runner<D, E> {
    /// Runs `shots` with `seed`, and blocks until the run ends: the driver's
    /// outputs, as [`run_async`](Self::run_async) gives them. It is
    /// `run_async`'s future, waited for on this thread with no async
    /// runtime, sleeping while it waits for a part. Not on wasm32, which
    /// cannot block: there, use `run_async`.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn run(
        &mut self,
        shots: &ShotSet,
        seed: impl Into<Option<u64>>,
    ) -> Result<Vec<D::Output>, RunError> {
        wait::wait(self.run_async(shots, seed))
    }

    /// Runs `shots` with `seed`, as a future, for a caller that is already
    /// async: the driver's outputs, as it gives them, which
    /// [`Driver::drive`]'s rule makes one per shot, in the order of `shots`;
    /// the runner does not check their count or order.
    ///
    /// A `Some` seed gives the same outputs each time. `None` draws a seed
    /// that is not reported, so the run cannot be repeated. On wasm32,
    /// where the runner reads no clock, a drawn seed differs between
    /// processes only where the platform gives random numbers, so a caller
    /// that needs unseeded runs to differ across processes there passes its
    /// own seed.
    ///
    /// A run requires its shots
    /// ([`Resources::shots`](crate::Resources::shots)), and seeded runs
    /// when its seed is fixed
    /// ([`Features::seeded_runs`](crate::Features::seeded_runs)), stating
    /// nothing of the set's capabilities:
    /// the driver's [`requires`](Driver::requires) gets that, and each part
    /// below gets what the part above requires given what arrived there.
    /// The run is refused, and nothing of it runs, when a part does not
    /// provide what arrives at it, before any part is asked
    /// ([`RunRefusal::NotProvided`]). Or when a part's `ready` refuses it:
    /// each part gets ready for what arrives at it, in turn, the executor
    /// first, up to the first that refuses; a joined part inside a part of
    /// the user's, which the runner cannot see into, refuses then a run at
    /// a boundary inside it that does not fit, in its sets or in what
    /// arrives there. Then the driver
    /// runs the shots, and a failure ends the run with the shot it names, a
    /// part's panic included, as the Panics section says.
    ///
    /// Once `drive` returns, fails or panics, or the future is dropped
    /// before it ends, the run's shots are ended on every part, every
    /// adapter top to bottom, then the executor at the bottom, so that no
    /// part carries them into the next run, whether or not the driver ended
    /// them. Every part is ended even if one panics in `end_shots`.
    ///
    /// The future is `Send` exactly when every part and every part's
    /// future is: no trait bounds the futures its parts return, and the
    /// runner does not box them.
    ///
    /// # Panics
    ///
    /// Where a panic unwinds (on wasm32, it aborts), a part's panic fails
    /// the run ([`Failure::Panicked`]) when it is in `drive`, `execute` or
    /// an `end_shots` a part above calls, or in the destructor of the
    /// future its `drive`, `adapt` or `execute` returned. The failure names
    /// the part whose future it is when the protocol drops that future,
    /// once it is ready or has panicked; when a part above stops waiting
    /// and drops it unfinished, it names that part above, through whose
    /// call the panic unwinds, as [`Failure::Panicked`] says.
    ///
    /// The run raises a part's panic again, instead of failing:
    /// - in a part's `ready`, which is not caught: the first one when the
    ///   future of that `ready` panics again as it is dropped;
    /// - in the protocol's end of the shots: the first such panic, top to
    ///   bottom, once every other part is ended, from the run, or from the
    ///   drop of its future, and the driver's outputs are lost. So a part
    ///   that panics each time it ends shots makes every run that started
    ///   its `drive` panic;
    /// - in the destructor of a part's future, when the run's future is
    ///   dropped before that future is ready: from the drop.
    ///
    /// The last two are not raised again while the thread is already
    /// unwinding, such as when a caller's panic drops the run's future: a
    /// second panic would abort the process, so its payload is dropped.
    ///
    /// The protocol guards the futures it drops, not the values parts
    /// return, as std's containers do not: when a part's future panics as
    /// it is dropped once ready, or the protocol's end of the shots panics,
    /// the output or refusal then held is dropped as that panic unwinds,
    /// and if its own destructor panics too, the process aborts, as a
    /// second panic does anywhere in Rust.
    ///
    /// So a part's panic in `end_shots` leaves the run in one of two ways,
    /// by who ended the shots: in an end a part above calls (the driver in
    /// `drive`, or an adapter in `adapt`), it unwinds through that part's
    /// call, and the run fails naming that part; in the protocol's end, it
    /// is raised again from the run.
    pub fn run_async(
        &mut self,
        shots: &ShotSet,
        seed: impl Into<Option<u64>>,
    ) -> impl Future<Output = Result<Vec<D::Output>, RunError>> {
        let seed = seed.into();
        async move {
            let run = run_requirement(shots.len(), seed.is_some());
            let required = self.required_below(&run).map_err(RunError::Refused)?;
            self.boundary.require(required);
            let executor_part = self
                .boundary
                .part
                .as_ref()
                .expect("the driver's `on` names the part below the boundary");
            let executor_at = ChainPosition::at(executor_part.position, &self.parts);
            await_part(self.executor.ready_at(&self.boundary.required, executor_at))
                .await
                .map_err(|not_ready| {
                    RunError::Refused(RunRefusal::part_not_ready(not_ready, executor_part))
                })?;
            await_part(
                self.driver
                    .ready_at(&run, ChainPosition::at(0, &self.parts)),
            )
            .await
            .map_err(|not_ready| {
                RunError::Refused(RunRefusal::part_not_ready(not_ready, &self.parts[0]))
            })?;
            let seed = Seed::new(seed.unwrap_or_else(drawn_seed));
            let unended = UnendedShots::new(&mut self.executor, shots, |executor, shots| {
                executor.end_shots(shots);
            });
            let mut below = BoundaryCrossing::new(&mut *unended.part, &self.boundary);
            let driver = &mut self.driver;
            let failure = match catch_panic(|| driver.drive(shots, seed, &mut below)).await {
                Ok(Ok(outputs)) => {
                    drop(unended);
                    return Ok(outputs);
                }
                Ok(Err(failure)) => failure,
                Err(payload) => match payload.downcast::<PanicEndingShots>() {
                    Ok(ending) => {
                        drop(unended.end());
                        resume_unwind(ending.0)
                    }
                    Err(payload) => DriveError {
                        shot: shots.first(),
                        failure: Failure::Panicked {
                            part: self.parts[0].clone(),
                            message: panic_message(payload),
                        },
                    },
                },
            };
            drop(unended);
            Err(RunError::Failed(failure))
        }
    }
}

impl<D: Driver, E: Executor<Accepts = D::Sends>> Runner<D, E> {
    /// What arrives at the executor on a run that requires `run`, if each
    /// part provides what arrives at it; else the refusal, which lists
    /// every part that does not.
    fn required_below(
        &self,
        run: &Capabilities<D::Sends>,
    ) -> Result<Capabilities<D::Sends>, RunRefusal> {
        let mut parts = DeclaredParts::checking();
        let required = self.driver.declare(run, &mut parts);
        self.executor.declare(&required, &mut parts);
        if parts.fit() {
            Ok(required)
        } else {
            Err(self.not_provided(run))
        }
    }

    /// The refusal of a run that requires `run`: every part that does not
    /// provide what arrives at it, named as `on` named it. Out of line and
    /// cold, as the walk again, listing its faults.
    #[cold]
    #[inline(never)]
    fn not_provided(&self, run: &Capabilities<D::Sends>) -> RunRefusal {
        let mut parts = DeclaredParts::listing_faults();
        let required = self.driver.declare(run, &mut parts);
        self.executor.declare(&required, &mut parts);
        RunRefusal::NotProvided(parts.faults(&self.parts))
    }
}

/// What a run of `shots` shots requires: that many shots, and seeded runs
/// if `seeded`, its seed being fixed; nothing of the set's capabilities,
/// which the driver states.
fn run_requirement<S: InstructionSet>(shots: usize, seeded: bool) -> Capabilities<S> {
    // A run of more shots than a `u32` counts is checked as `u32::MAX`
    // shots, the most a part can state.
    let shots = u32::try_from(shots).unwrap_or(u32::MAX);
    let features = if seeded {
        Features::unconstrained().with_seeded_runs(true)
    } else {
        Features::unconstrained()
    };
    Capabilities::new(
        Resources::unconstrained().with_shots(shots),
        features,
        S::unconstrained().instructions().clone(),
    )
}

/// A seed for a run that fixes none: from the standard library's random
/// hasher keys, which differ between processes where the platform gives
/// random numbers, the time, where there is a clock, and a count of the
/// seeds drawn, so that two runs of one process differ even without either.
fn drawn_seed() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};
    static DRAWN: AtomicU64 = AtomicU64::new(0);
    let mut hasher = std::hash::RandomState::new().build_hasher();
    hasher.write_u64(DRAWN.fetch_add(1, Ordering::Relaxed));
    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(since) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        hasher.write_u128(since.as_nanos());
    }
    hasher.finish()
}
