// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Running shots on a chain: the [`Runner`], which only the `>>` of a
//! driver with an executor makes, and what a run returns when it gives no
//! outputs, [`RunError`].
//!
//! A runner holds a chain that composed: its boundaries fit, its executor
//! serves its driver's needs, and every part is provisioned. Each run
//! checks what only a run can: its shots against the most shots a run may
//! have (by the offer that reaches the driver), its seed against whether
//! the chain is seedable, and whether each part can take a run now. Then
//! the driver runs the shots, calling the chain below it as one executor,
//! whose every `execute` the protocol checks. Once the run ends, whether it
//! gave outputs, failed, or its future was dropped, the runner ends its
//! shots on every part, so that none keeps them.

mod run_error;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod wait;

use crate::checked::Checked;
use crate::instruction_set::instruction::batch_check::BatchLimits;
use crate::panics::{catch_panic, end_both};
use crate::roles::EndedRun;
use crate::{
    Capabilities, ChainPart, Drive, DriveError, Execute, Failure, RunRefusal, Seed, ShotSet,
};

pub use run_error::RunError;

/// A driver and an executor, composed by `>>` with any adapters between
/// them, admitted and provisioned: what runs shots.
///
/// Its type names the two parts the last `>>` joined, such as
/// `Runner<Composed<D, A>, E>`. It has no constructor: only `Driver >>
/// Executor` makes one, so that every runner's chain composed.
pub struct Runner<D: Drive, E: Execute> {
    driver: D,
    executor: E,
    /// The driver's top part, as the admission listed it: what its panic
    /// names.
    driver_part: ChainPart,
    /// The executor's top part, as the admission listed it: what its panic
    /// in an `execute` names.
    executor_part: ChainPart,
    /// The offer that reaches the driver, the executor's as each adapter
    /// adapts it: its most shots and whether it is seedable, for each run.
    offer: Capabilities,
    /// The executor's offer, which the driver's `below.offers()` answers.
    executor_offer: Capabilities,
    /// The limits of the batches the driver sends the executor.
    batch_limits: BatchLimits,
}

impl<D: Drive, E: Execute> Runner<D, E> {
    /// The runner of `driver` and `executor`, composed, admitted and
    /// provisioned, each with its top part as the admission listed it, with
    /// `offer`, the offer that reaches the driver, and `executor_offer`,
    /// both read before any part was provisioned.
    pub(crate) fn new(
        (driver, driver_part): (D, ChainPart),
        (executor, executor_part): (E, ChainPart),
        offer: Capabilities,
        executor_offer: Capabilities,
        batch_limits: BatchLimits,
    ) -> Self {
        Self {
            driver,
            executor,
            driver_part,
            executor_part,
            offer,
            executor_offer,
            batch_limits,
        }
    }

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
    /// [`Drive::drive`]'s rule makes one per shot, in the order of `shots`;
    /// the runner does not check their count or order.
    ///
    /// A `Some` seed gives the same outputs each time; a run with one is
    /// refused when the chain is not seedable. `None` draws a seed that is
    /// not reported, so the run cannot be repeated. A run is refused, and
    /// nothing of it runs, when it has more shots than a run may have (by
    /// the offer that reaches the driver), or when a part's `ready` refuses
    /// it; each part is asked, the executor first. Then the driver runs the
    /// shots, and a failure ends the run with the shot it names, including
    /// a part's panic in `drive`, `execute` or an `end_shots` the driver
    /// calls, where a panic unwinds (on wasm32, it aborts).
    ///
    /// Once `drive` returns, fails or panics, or the future is dropped
    /// before it ends, the run's shots are ended on every part, every
    /// adapter top to bottom, then the executor at the bottom, so that no
    /// part carries them into the next run, whether or not the driver ended
    /// them. Every part is ended even if one panics in `end_shots`; the
    /// first such panic, top to bottom, is then raised again from the run,
    /// or from the drop of its future, unless the thread is already
    /// unwinding. So a part that panics each time it ends shots makes every
    /// run that started its `drive` panic, once every other part is ended.
    /// A panic in a part's `ready` is not caught.
    ///
    /// The future is `Send` exactly when every part and every part's
    /// future is: no trait bounds the futures its parts return, and the
    /// runner does not box them.
    pub fn run_async(
        &mut self,
        shots: &ShotSet,
        seed: impl Into<Option<u64>>,
    ) -> impl Future<Output = Result<Vec<D::Output>, RunError>> {
        let seed = seed.into();
        async move {
            if let Some(most) = self.offer.max_shots
                && shots.len() > most as usize
            {
                return Err(RunError::Refused(RunRefusal::TooManyShots {
                    asked: u32::try_from(shots.len()).unwrap_or(u32::MAX),
                    most,
                }));
            }
            if seed.is_some() && !self.offer.seedable {
                return Err(RunError::Refused(RunRefusal::NotSeedable));
            }
            self.executor.ready().await.map_err(RunError::Refused)?;
            self.driver.ready().await.map_err(RunError::Refused)?;
            let seed = Seed::new(seed.unwrap_or_else(drawn_seed));
            let unended = UnendedShots {
                driver: &mut self.driver,
                executor: &mut self.executor,
                shots,
            };
            let mut below = Checked::new(
                &mut *unended.executor,
                &self.executor_part,
                &self.executor_offer,
                &self.batch_limits,
            );
            let driver = &mut *unended.driver;
            let failure = match catch_panic(|| driver.drive(shots, seed, &mut below)).await {
                Ok(Ok(outputs)) => {
                    drop(unended);
                    return Ok(outputs);
                }
                Ok(Err(failure)) => failure,
                Err(message) => DriveError {
                    shot: shots.iter().next().expect("a shot set is never empty"),
                    failure: Failure::Panicked {
                        part: self.driver_part.clone(),
                        message,
                    },
                },
            };
            drop(unended);
            Err(RunError::Failed(failure))
        }
    }
}

/// The shots of a run whose `drive` started, which any part may hold:
/// dropped once the run ends, whether it gave outputs, failed, or its
/// future was dropped, it ends them on every part, as [`Drive::end_run`]
/// says, then on the executor. One end for the three, so that freeing a
/// run's shots never depends on the driver.
struct UnendedShots<'a, D: Drive, E: Execute> {
    driver: &'a mut D,
    executor: &'a mut E,
    shots: &'a ShotSet,
}

impl<D: Drive, E: Execute> Drop for UnendedShots<'_, D, E> {
    fn drop(&mut self) {
        let shots = self.shots;
        end_both(
            || self.driver.end_run(EndedRun { shots }),
            || self.executor.end_shots(shots),
        );
    }
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
