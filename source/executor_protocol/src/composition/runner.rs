// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Running shots on a chain: the [`Runner`], which only the `>>` of a
//! driver with an executor makes, and what a run returns when it gives no
//! outputs, [`RunError`].
//!
//! A runner holds a chain that composed: its boundaries fit, and its
//! executor serves its driver's needs. Each run checks what only a run can:
//! its shots against the most shots a run may have, and its seed against
//! whether the chain is seedable, both by every part's offer
//! ([`RunLimits`]); then it gets every part ready for the needs the
//! admission checked, or is refused by the first that cannot take a run
//! now. Then the driver
//! runs the shots, calling the chain below it as one executor, the readouts
//! of each of whose batches the protocol shapes from the batch, after
//! checking every batch's count of instructions, and every batch its sender
//! marked [`checked`](crate::Batch::checked). Once the run ends, whether it
//! gave outputs, failed, or its future was dropped, the runner ends its
//! shots on every part, so that none keeps them.
//!
//! A run gives its outputs at the end ([`Runner::run_async`]), or each as
//! the driver gives it ([`Runner::run_each_async`]). Both also have a
//! blocking form (`run`, `run_each`), not on wasm32. `run_async` and
//! `run_each_async` share one body (`run_with`), and differ only in how it
//! calls the driver (`delivery`), and in raising a panic of `each` once
//! the shots are ended.

mod delivery;
mod run_error;
mod run_limits;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod wait;

use super::boundary_crossing::{BatchLimits, BoundaryCrossing};
use super::part_panics::{CallersCallback, await_part, catch_panic, end_both};
use crate::roles::EndedRun;
use crate::{Drive, DriveError, Execute, Failure, Needs, Offer, PartId, Seed, ShotId, ShotSet};
use delivery::{AsTheDriverGives, AtTheEnd, Delivery};

pub use run_error::{RunError, RunRefusal};
pub(crate) use run_limits::RunLimits;

/// A driver and an executor, composed by `>>` with any adapters between
/// them, and admitted: what runs shots.
///
/// Its type names the two parts the last `>>` joined, such as
/// `Runner<Composed<D, A>, E>`. It has no constructor: only `Driver >>
/// Executor` makes one, so that every runner's chain composed.
pub struct Runner<D: Drive, E: Execute> {
    driver: D,
    executor: E,
    /// The driver's top part, as the admission listed it: what its panic
    /// names.
    driver_part: PartId,
    /// The executor's top part, as the admission listed it: what its panic
    /// in an `execute` names.
    executor_part: PartId,
    /// The most shots and whether a seed may be fixed, by every part's
    /// offer, for each run.
    limits: RunLimits,
    /// The needs that reach the executor, as the admission checked them:
    /// what it gets ready for on every run.
    executor_needs: Needs,
    /// The executor's offer, which the driver's `below.offers()` answers.
    executor_offer: Offer,
    /// The limits of the batches the driver sends the executor.
    batch_limits: BatchLimits,
}

impl<D: Drive, E: Execute> Runner<D, E> {
    /// The runner of `driver` and `executor`, composed and admitted, each
    /// with its top part as the admission listed it, with the run `limits`
    /// of every part's offer, and `executor_needs` and `executor_offer`,
    /// all as the admission read them.
    pub(crate) fn new(
        (driver, driver_part): (D, PartId),
        (executor, executor_part): (E, PartId),
        limits: RunLimits,
        (executor_needs, executor_offer): (Needs, Offer),
        batch_limits: BatchLimits,
    ) -> Self {
        Self {
            driver,
            executor,
            driver_part,
            executor_part,
            limits,
            executor_needs,
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
    /// A `Some` seed gives the same outputs each time. `None` draws a seed
    /// that is not reported, so the run cannot be repeated. On wasm32,
    /// where the runner reads no clock, a drawn seed differs between
    /// processes only where the platform gives random numbers, so a caller
    /// that needs unseeded runs to differ across processes there passes its
    /// own seed.
    ///
    /// A run is refused, and nothing of it runs, when it has more shots
    /// than a run may have (the fewest any part offers), or a seed while the
    /// chain is not seedable, before any part is asked; or when a part's
    /// `ready` refuses it: each part gets ready for the needs the admission
    /// checked, in turn, the executor first, up to the first that refuses.
    /// Then the driver runs the shots, and a
    /// failure ends the run with the shot it names, a part's panic
    /// included, as the Panics section says.
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
    /// the run ([`Failure::Panicked`]) when it is in `drive`, `adapt` or
    /// `execute`, their `_each` and `_stream` forms included, in what a
    /// part gives to a call below (the `each` of an `execute_each`, the
    /// instructions of an `execute_stream`; named as that part's), or in
    /// an `end_shots` a part above calls, or in the destructor of the
    /// future its `drive`, `adapt` or `execute`, any form, returned. The
    /// failure names the part whose future it is when the protocol drops
    /// that future, once it is ready or has panicked; when a part above
    /// stops waiting and drops it unfinished, it names that part above,
    /// through whose call the panic unwinds, as [`Failure::Panicked`] says.
    ///
    /// The run raises a part's panic again, instead of failing:
    /// - in a part's `ready`, which is not caught: the first one when the
    ///   future of that `ready` panics again as it is dropped;
    /// - in the runner's end of the shots: the first such panic, top to
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
    /// it is dropped once ready, or the runner's end of the shots panics,
    /// the output or refusal then held is dropped as that panic unwinds,
    /// and if its own destructor panics too, the process aborts, as a
    /// second panic does anywhere in Rust.
    ///
    /// So a part's panic in `end_shots` leaves the run in one of two ways,
    /// by who ended the shots: in an end a part above calls (the driver in
    /// `drive` or `drive_each`, or an adapter in one of its `adapt`
    /// methods), it unwinds through that part's call, and the run fails
    /// naming that part; in the runner's end, it is raised again from the
    /// run.
    pub fn run_async(
        &mut self,
        shots: &ShotSet,
        seed: impl Into<Option<u64>>,
    ) -> impl Future<Output = Result<Vec<D::Output>, RunError>> {
        self.run_with(shots, seed.into(), AtTheEnd)
    }

    /// Runs `shots` with `seed`, as [`run`](Self::run) does, but gives
    /// each shot's output to `each` as the driver gives it, as
    /// [`run_each_async`](Self::run_each_async) says. It blocks until the
    /// run ends. Its errors and panics are
    /// [`run_each_async`](Self::run_each_async)'s. Not on wasm32: there,
    /// use `run_each_async`.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn run_each(
        &mut self,
        shots: &ShotSet,
        seed: impl Into<Option<u64>>,
        each: &mut impl FnMut(ShotId, D::Output),
    ) -> Result<(), RunError> {
        wait::wait(self.run_each_async(shots, seed, each))
    }

    /// Runs `shots` with `seed`, as [`run_async`](Self::run_async) does,
    /// with its refusals, failures and end of the shots, but gives each
    /// shot's output to `each` as the driver gives it
    /// ([`Drive::drive_each`]): all at the end by default; a driver that
    /// overrides `drive_each` may give each earlier, as its shot ends.
    /// After an error, the outputs already given stand, and the shots not
    /// given have none.
    ///
    /// The future is `Send` exactly when every part and every part's
    /// future, `drive_each`'s included, is, and `each` is `Send`.
    ///
    /// A panic in `each` is the caller's, not the driver's: `each` is not
    /// called again, the run goes on to its end, the shots are ended on
    /// every part, and the panic is then raised again from the run, after
    /// an `Ok` or an error alike; unless a part panics in the runner's end
    /// of the shots, whose panic is then raised instead, as the Panics
    /// section of `run_async` says, or unless the run's future is dropped
    /// first.
    pub fn run_each_async(
        &mut self,
        shots: &ShotSet,
        seed: impl Into<Option<u64>>,
        each: &mut impl FnMut(ShotId, D::Output),
    ) -> impl Future<Output = Result<(), RunError>> {
        self.run_with(
            shots,
            seed.into(),
            AsTheDriverGives(CallersCallback::new(each)),
        )
    }

    /// `run_async`'s body, shared with `run_each_async`, the driver called
    /// as `delivery` says.
    async fn run_with<V: Delivery<D>>(
        &mut self,
        shots: &ShotSet,
        seed: Option<u64>,
        mut delivery: V,
    ) -> Result<V::Output, RunError> {
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
        await_part(self.executor.ready(&self.executor_needs))
            .await
            .map_err(|not_ready| {
                RunError::Refused(RunRefusal::part_not_ready(not_ready, &self.executor_part))
            })?;
        await_part(self.driver.ready()).await.map_err(|not_ready| {
            RunError::Refused(RunRefusal::part_not_ready(not_ready, &self.driver_part))
        })?;
        let seed = Seed::new(seed.unwrap_or_else(drawn_seed));
        let unended = UnendedShots {
            driver: &mut self.driver,
            executor: &mut self.executor,
            shots,
        };
        let mut below = BoundaryCrossing::new(
            &mut *unended.executor,
            &self.executor_part,
            &self.executor_offer,
            &self.batch_limits,
        );
        let driver = &mut *unended.driver;
        let failure = match catch_panic(|| delivery.drive(driver, shots, seed, &mut below)).await {
            Ok(Ok(outputs)) => {
                // The shots are ended before the caller's panic is
                // raised, so a panic in this end is raised instead of
                // the caller's, which is dropped.
                drop(unended);
                delivery.raise_callers_panic();
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
        delivery.raise_callers_panic();
        Err(RunError::Failed(failure))
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
