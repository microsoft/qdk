// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The fake parts on [`Resettable`], whose capabilities are qubits and a
//! reset: a driver needing qubits, and an adapter needing a reset below,
//! for the tests of a fault naming the part that stated each need. Its
//! executor is [`Zeros`](super::Zeros), of any set.

use crate::prelude::*;
use crate::tests::example_sets::{QubitsAndReset, Resettable};
use crate::{
    Batch, BatchReadouts, Capabilities, DriveError, ExecuteError, RunRequirement, Seed, ShotSet,
    StatesLimits,
};

/// A driver requiring the run's capabilities on `needs`, qubits and a
/// reset or not, which sends no batch.
pub(crate) struct ResettableProgram {
    needs: QubitsAndReset,
}

impl ResettableProgram {
    pub(crate) fn new(qubits: u32, reset: bool) -> StatesLimits<Self> {
        let needs = QubitsAndReset { qubits, reset };
        Self { needs }.stating_limits()
    }
}

impl DriverRequirements for ResettableProgram {
    /// The run's, on `needs`.
    fn requires(&self, run: &RunRequirement) -> Capabilities<Resettable> {
        Capabilities::new(run.resources, run.features, self.needs)
    }
}

impl Driver for ResettableProgram {
    type Sends = Resettable;
    type Output = ();

    fn sends(&self) -> &Resettable {
        &Resettable
    }

    fn bind(&mut self, (): ()) {}

    async fn drive(
        &mut self,
        shots: &ShotSet,
        _seed: Seed,
        _below: &mut impl Executor<Accepts = Resettable>,
    ) -> Result<Vec<()>, DriveError> {
        Ok(vec![(); shots.len()])
    }
}

/// An adapter reusing its ancillas, which resets them between uses: it
/// requires what arrives, with a reset, below, and passes each batch as it
/// came.
pub(crate) struct AncillaReuse;

impl AncillaReuse {
    pub(crate) fn new() -> StatesLimits<Self> {
        Self.stating_limits()
    }
}

impl AdapterLimits for AncillaReuse {
    /// What arrives, with a reset.
    fn requires(&self, above: &Capabilities<Resettable>) -> Capabilities<Resettable> {
        let needs = QubitsAndReset {
            reset: true,
            ..*above.set_capabilities()
        };
        Capabilities::new(*above.resources(), *above.features(), needs)
    }
}

impl Adapter for AncillaReuse {
    type Set = Resettable;

    fn accepts(&self) -> &Resettable {
        &Resettable
    }

    fn sends(&self) -> &Resettable {
        &Resettable
    }

    fn bind(&mut self, (): ()) {}

    async fn adapt(
        &mut self,
        batches: &[Batch<'_, Resettable>],
        seed: Seed,
        below: &mut impl Executor<Accepts = Resettable>,
        readouts: &mut [BatchReadouts<Resettable>],
    ) -> Result<(), ExecuteError> {
        below.execute(batches, seed, readouts).await
    }
}
