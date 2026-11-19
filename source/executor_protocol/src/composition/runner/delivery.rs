// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! How a run calls its driver ([`Delivery`]): for its outputs at the end
//! ([`AtTheEnd`]), or for each output as the driver gives it
//! ([`AsTheDriverGives`]).

use crate::composition::part_panics::CallersCallback;
use crate::roles::await_part;
use crate::{Drive, DriveError, Execute, Seed, ShotId, ShotSet};

/// How a run calls its driver, and what it returns when the driver
/// answers.
pub(super) trait Delivery<D: Drive> {
    /// What the run returns when the driver answers.
    type Output;

    /// Calls `driver` on `shots`, with `below` as its chain below.
    fn drive<B: Execute>(
        &mut self,
        driver: &mut D,
        shots: &ShotSet,
        seed: Seed,
        below: &mut B,
    ) -> impl Future<Output = Result<Self::Output, DriveError>>;

    /// Raises again a panic of the caller's own code that the call caught,
    /// once the run's shots are ended; none by default.
    fn raise_callers_panic(&mut self) {}
}

/// `drive`'s outputs at the end.
pub(super) struct AtTheEnd;

impl<D: Drive> Delivery<D> for AtTheEnd {
    type Output = Vec<D::Output>;

    fn drive<B: Execute>(
        &mut self,
        driver: &mut D,
        shots: &ShotSet,
        seed: Seed,
        below: &mut B,
    ) -> impl Future<Output = Result<Self::Output, DriveError>> {
        driver.drive(shots, seed, below)
    }
}

/// Each output to the caller's `each`, as `drive_each` gives it, a panic of
/// `each` kept to be raised again from the run.
pub(super) struct AsTheDriverGives<'f, F>(pub(super) CallersCallback<&'f mut F>);

impl<D: Drive, F: FnMut(ShotId, D::Output)> Delivery<D> for AsTheDriverGives<'_, F> {
    type Output = ();

    fn drive<B: Execute>(
        &mut self,
        driver: &mut D,
        shots: &ShotSet,
        seed: Seed,
        below: &mut B,
    ) -> impl Future<Output = Result<(), DriveError>> {
        let callback = &mut self.0;
        async move {
            let mut give = |id, output| callback.call(|each| each(id, output));
            await_part(driver.drive_each(shots, seed, below, &mut give)).await
        }
    }

    fn raise_callers_panic(&mut self) {
        self.0.raise_panic();
    }
}
