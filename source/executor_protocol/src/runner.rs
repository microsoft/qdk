// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Running shots on a chain: the [`Runner`], which only the `>>` of a
//! driver with an executor makes, and what a run returns when it gives no
//! outputs, [`RunError`].
//!
//! A runner holds a chain that composed: its boundaries fit, its executor
//! serves its driver's needs, and every part is provisioned. Each run
//! checks what only a run can: its shots against the executor's most, its
//! seed against whether the chain is seedable, and whether each part can
//! take a run now. Then the driver runs the shots, calling the chain below
//! it as one executor, whose every `execute` the protocol checks.

mod run_error;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod wait;

use crate::checked::{Checked, catch_panic};
use crate::instruction_set::instruction::batch_check::BatchLimits;
use crate::{Capabilities, Drive, DriveError, Execute, Failure, RunRefusal, Seed, ShotSet};

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
    /// The offer that reaches the driver, the executor's as each adapter
    /// adapts it: its most shots and whether it is seedable, for each run.
    limits: Capabilities,
    /// The limits of the batches the driver sends the executor.
    batch_limits: BatchLimits,
}

impl<D: Drive, E: Execute> Runner<D, E> {
    /// The runner of `driver` and `executor`, composed, admitted and
    /// provisioned, with `limits`, the offer that reaches the driver.
    pub(crate) fn new(
        driver: D,
        executor: E,
        limits: Capabilities,
        batch_limits: BatchLimits,
    ) -> Self {
        Self {
            driver,
            executor,
            limits,
            batch_limits,
        }
    }

    /// Runs `shots` with `seed`, and blocks until the run ends: each shot's
    /// output, in the order of `shots`. It is [`run_async`](Self::run_async)'s
    /// future, waited for on this thread with no async runtime, sleeping
    /// while it waits for a part. Not on wasm32, which cannot block: there,
    /// use `run_async`.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn run(
        &mut self,
        shots: &ShotSet,
        seed: impl Into<Option<u64>>,
    ) -> Result<Vec<D::Output>, RunError> {
        wait::wait(self.run_async(shots, seed))
    }

    /// Runs `shots` with `seed`, as a future, for a caller that is already
    /// async: each shot's output, in the order of `shots`.
    ///
    /// A `Some` seed gives the same outputs each time; a run with one is
    /// refused when the chain is not seedable. `None` draws a seed that is
    /// not reported, so the run cannot be repeated. A run is refused, and
    /// nothing of it runs, when it has more shots than the executor's most,
    /// or when a part's `ready` refuses it; each part is asked, the
    /// executor first. Then the driver runs the shots, and a failure ends
    /// the run with the shot it names, including a part's panic.
    pub fn run_async(
        &mut self,
        shots: &ShotSet,
        seed: impl Into<Option<u64>>,
    ) -> impl Future<Output = Result<Vec<D::Output>, RunError>> {
        let seed = seed.into();
        async move {
            if let Some(most) = self.limits.max_shots
                && shots.len() > most as usize
            {
                return Err(RunError::Refused(RunRefusal::TooManyShots {
                    asked: u32::try_from(shots.len()).unwrap_or(u32::MAX),
                    most,
                }));
            }
            if seed.is_some() && !self.limits.seedable {
                return Err(RunError::Refused(RunRefusal::NotSeedable));
            }
            self.executor.ready().await.map_err(RunError::Refused)?;
            self.driver.ready().await.map_err(RunError::Refused)?;
            let seed = Seed::new(seed.unwrap_or_else(drawn_seed));
            let mut below = Checked::new(&mut self.executor, &self.batch_limits);
            let driver = &mut self.driver;
            match catch_panic(|| driver.drive(shots, seed, &mut below)).await {
                Ok(outputs) => outputs.map_err(RunError::Failed),
                Err(message) => Err(RunError::Failed(DriveError {
                    shot: shots.iter().next().expect("a shot set is never empty"),
                    failure: Failure::Panicked(message),
                })),
            }
        }
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
