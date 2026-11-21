// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The fake part that panics as a driver or as an executor.

use super::coin_gates;
use crate::prelude::*;
use crate::{
    Batch, BatchReadouts, DriveError, ExecuteError, InstructionSet, Needs, Offer, OpcodeMap, Seed,
    ShotSet,
};

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
    fn sends(&self) -> &InstructionSet {
        coin_gates()
    }

    fn bind(&mut self, _below: OpcodeMap) {}

    fn needs(&self) -> Needs {
        Needs::qubits(1)
    }

    type Output = ();

    fn drive(
        &mut self,
        _shots: &ShotSet,
        _seed: Seed,
        _below: &mut impl Executor,
    ) -> impl Future<Output = Result<Vec<()>, DriveError>> {
        assert!(
            matches!(self.0, PanicsWhen::Polled),
            "the part broke while making its future"
        );
        async { broken() }
    }
}

impl Executor for Panics {
    fn accepts(&self) -> &InstructionSet {
        Driver::sends(self)
    }

    fn offers(&self) -> Offer {
        Offer::unlimited()
    }

    fn execute(
        &mut self,
        _batches: &[Batch<'_>],
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
