// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The fake parts on [`Resettable`], whose capabilities are qubits and a
//! reset: drivers needing qubits, one of them per shot of the run, an
//! adapter needing a reset below, and a translating adapter onto it from
//! [`QubitLimited`], for the tests of a fault naming the part that stated
//! each need. Its executor is [`Zeros`](super::Zeros), of any set.

use crate::prelude::*;
use crate::roles::lent_translating_adapter;
use crate::tests::example_sets::{QubitLimited, Qubits, QubitsAndReset, Resettable};
use crate::{
    Batch, BatchReadouts, Capabilities, DriveError, ExecuteError, RunRequirement, Seed, ShotSet,
    StatesLimits, TranslatingAdapter,
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

/// A driver running a run's shots side by side on one register, `width`
/// qubits each: it requires the run's resources and features, with
/// `width` qubits per shot of the run and no reset, so a run that states
/// no shots, as the one `on` checks a chain with, needs no qubit. It
/// sends no batch.
pub(crate) struct SideBySideProgram {
    width: u32,
}

impl SideBySideProgram {
    pub(crate) fn new(width: u32) -> StatesLimits<Self> {
        Self { width }.stating_limits()
    }
}

impl DriverRequirements for SideBySideProgram {
    /// The run's, with `width` qubits per shot.
    fn requires(&self, run: &RunRequirement) -> Capabilities<Resettable> {
        let needs = QubitsAndReset {
            qubits: self.width.saturating_mul(run.resources.shots.unwrap_or(0)),
            reset: false,
        };
        Capabilities::new(run.resources, run.features, needs)
    }
}

impl Driver for SideBySideProgram {
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

/// An adapter reusing the qubits that arrive as ancillas, which resets
/// them between uses: it requires what arrives below, with a reset if any
/// qubit arrives, as with none it has none to reuse, and passes each batch
/// as it came.
pub(crate) struct AncillaReuse;

impl AncillaReuse {
    pub(crate) fn new() -> StatesLimits<Self> {
        Self.stating_limits()
    }
}

impl AdapterLimits for AncillaReuse {
    /// What arrives, with a reset if any qubit arrives.
    fn requires(&self, above: &Capabilities<Resettable>) -> Capabilities<Resettable> {
        let arriving = *above.set_capabilities();
        let needs = QubitsAndReset {
            reset: arriving.reset || arriving.qubits > 0,
            ..arriving
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

/// A translating adapter of [`QubitLimited`] to [`Resettable`], as a
/// compiler pass inserting mid-circuit resets: it requires below the
/// qubits that arrive, with a reset. The tests compose it only above parts
/// with no reset, which refuse every run at `ready`, so it never adapts:
/// its `adapt` panics.
pub(crate) struct ResetInsertion;

impl TranslatingAdapter for ResetInsertion {
    type Accepts = QubitLimited;
    type Sends = Resettable;

    fn accepts(&self) -> &QubitLimited {
        &QubitLimited
    }

    fn sends(&self) -> &Resettable {
        &Resettable
    }

    /// What arrives, its qubits with a reset.
    fn requires(&self, above: &Capabilities<QubitLimited>) -> Capabilities<Resettable> {
        let Qubits(qubits) = *above.set_capabilities();
        let needs = QubitsAndReset {
            qubits,
            reset: true,
        };
        Capabilities::new(*above.resources(), *above.features(), needs)
    }

    fn bind(&mut self, (): ()) {}

    async fn adapt(
        &mut self,
        _: &[Batch<'_, QubitLimited>],
        _: Seed,
        _: &mut impl Executor<Accepts = Resettable>,
        _: &mut [BatchReadouts<QubitLimited>],
    ) -> Result<(), ExecuteError> {
        unreachable!("a reset insertion is composed only above parts with no reset, so none adapts")
    }
}

lent_translating_adapter!([] ResetInsertion);
