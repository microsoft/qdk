// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The fake part that panics as a driver or as an executor.

use super::coin_gates;
use crate::{
    Batch, BatchReadouts, Drive, DriveError, DriverSetup, Execute, ExecuteError, ExecutorSetup,
    FinalRow, InstructionSet, Needs, Offer, OpcodeMap, Seed, ShotSet,
};

/// When a [`Panics`] part panics.
#[derive(Clone, Copy)]
pub(crate) enum PanicsWhen {
    /// While its method makes the future it returns.
    MakingItsFuture,
    /// While its future is polled.
    Polled,
    /// While its future is polled, after an `execute_each` gave its
    /// first row to `each`; as [`Polled`](Self::Polled) otherwise.
    PolledAfterARow,
}

/// A part that panics, as a driver in `drive` and as an executor in
/// `execute` and `execute_each`.
pub(crate) struct Panics(pub(crate) PanicsWhen);

/// A part's panic while its future is polled.
fn broken<T>() -> T {
    panic!("the part broke while it was polled")
}

impl DriverSetup for Panics {
    fn sends(&self) -> &InstructionSet {
        coin_gates()
    }

    fn bind(&mut self, _below: OpcodeMap) {}

    fn needs(&self) -> Needs {
        Needs::qubits(1)
    }
}

impl Drive for Panics {
    type Output = ();

    fn drive(
        &mut self,
        _shots: &ShotSet,
        _seed: Seed,
        _below: &mut impl Execute,
    ) -> impl Future<Output = Result<Vec<()>, DriveError>> {
        assert!(
            !matches!(self.0, PanicsWhen::MakingItsFuture),
            "the part broke while making its future"
        );
        async { broken() }
    }
}

impl ExecutorSetup for Panics {
    fn accepts(&self) -> &InstructionSet {
        DriverSetup::sends(self)
    }

    fn offers(&self) -> Offer {
        Offer::unlimited()
    }
}

impl Execute for Panics {
    fn execute(
        &mut self,
        _batches: &[Batch<'_>],
        _seed: Seed,
        _readouts: &mut [BatchReadouts],
    ) -> impl Future<Output = Result<(), ExecuteError>> {
        assert!(
            !matches!(self.0, PanicsWhen::MakingItsFuture),
            "the part broke while making its future"
        );
        async { broken() }
    }

    fn execute_each(
        &mut self,
        batches: &[Batch<'_>],
        seed: Seed,
        readouts: &mut [BatchReadouts],
        each: &mut impl FnMut(FinalRow<'_>),
    ) -> impl Future<Output = Result<(), ExecuteError>> {
        assert!(
            !matches!(self.0, PanicsWhen::MakingItsFuture),
            "the part broke while making its future"
        );
        async move {
            if matches!(self.0, PanicsWhen::PolledAfterARow) {
                each(FinalRow::of(0, 0, &readouts[0]));
            }
            self.execute(batches, seed, readouts).await
        }
    }
}
