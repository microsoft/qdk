// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The fake part that panics as a driver or as an executor.

use super::coin_gates;
use crate::prelude::*;
use crate::tests::example_sets::Measuring;
use crate::{Batch, BatchReadouts, Capabilities, DriveError, ExecuteError, Seed, ShotSet};

/// When a [`Panics`] part panics.
#[derive(Clone, Copy)]
pub(crate) enum PanicsWhen {
    /// While its method makes the future it returns.
    MakingItsFuture,
    /// While its future is polled.
    Polled,
}

/// A part that panics, as a driver in `drive` and as an executor in
/// `execute`.
pub(crate) struct Panics(pub(crate) PanicsWhen);

/// A part's panic while its future is polled.
fn broken<T>() -> T {
    panic!("the part broke while it was polled")
}

impl Driver for Panics {
    type Sends = Measuring;

    fn sends(&self) -> &Measuring {
        coin_gates()
    }

    /// The run's, in batches of 1 instruction.
    fn requires(&self, run: &Capabilities<Measuring>) -> Capabilities<Measuring> {
        run.clone()
            .map_resources(|resources| resources.with_instructions_per_batch(1))
    }

    fn bind(&mut self, _below: Vec<u32>) {}

    type Output = ();

    fn drive(
        &mut self,
        _shots: &ShotSet,
        _seed: Seed,
        _below: &mut impl Executor<Accepts = Measuring>,
    ) -> impl Future<Output = Result<Vec<()>, DriveError>> {
        assert!(
            matches!(self.0, PanicsWhen::Polled),
            "the part broke while making its future"
        );
        async { broken() }
    }
}

impl Executor for Panics {
    type Accepts = Measuring;

    fn accepts(&self) -> &Measuring {
        coin_gates()
    }

    fn execute(
        &mut self,
        _batches: &[Batch<'_, Measuring>],
        _seed: Seed,
        _readouts: &mut [BatchReadouts],
    ) -> impl Future<Output = Result<(), ExecuteError>> {
        assert!(
            matches!(self.0, PanicsWhen::Polled),
            "the part broke while making its future"
        );
        async { broken() }
    }
}
