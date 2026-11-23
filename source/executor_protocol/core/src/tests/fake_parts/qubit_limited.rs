// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The fake parts on [`QubitLimited`], whose capabilities are a count of
//! qubits: a driver, a pass-through adapter, an adapter requiring fewer
//! qubits below than it sends on, and an executor, for the tests of the
//! set's own capabilities and checks across a chain.

use super::Log;
use crate::prelude::*;
use crate::tests::example_sets::{OnQubit, QubitLimited, Qubits, on_qubits};
use crate::{
    Batch, BatchReadouts, Capabilities, DriveError, ExecuteError, NotReady, RunRequirement, Seed,
    ShotSet, StatesLimits,
};

/// What `part` logs once ready for `above`: the qubits it states.
fn ready_for(part: &str, above: &Capabilities<QubitLimited>) -> String {
    let Qubits(n) = above.set_capabilities();
    format!("{part} ready for {n} qubits")
}

/// A driver sending `gates`, one checked batch of every shot, and
/// requiring the run's capabilities on `qubits` qubits.
pub(crate) struct QubitProgram {
    gates: Vec<OnQubit>,
    qubits: u32,
}

impl QubitProgram {
    pub(crate) fn new(gates: &[u32], qubits: u32) -> StatesLimits<Self> {
        Self {
            gates: gates.iter().copied().map(OnQubit).collect(),
            qubits,
        }
        .stating_limits()
    }
}

impl DriverRequirements for QubitProgram {
    /// The run's, on `qubits` qubits.
    fn requires(&self, run: &RunRequirement) -> Capabilities<QubitLimited> {
        Capabilities::new(run.resources, run.features, Qubits(self.qubits))
    }
}

impl Driver for QubitProgram {
    type Sends = QubitLimited;
    type Output = ();

    fn sends(&self) -> &QubitLimited {
        &QubitLimited
    }

    fn bind(&mut self, (): ()) {}

    async fn drive(
        &mut self,
        shots: &ShotSet,
        seed: Seed,
        below: &mut impl Executor<Accepts = QubitLimited>,
    ) -> Result<Vec<()>, DriveError> {
        let batches = [Batch::new(shots, &self.gates, 0).checked()];
        let mut readouts = [BatchReadouts::new(0, 0)];
        below.execute(&batches, seed, &mut readouts).await?;
        Ok(vec![(); shots.len()])
    }
}

/// An adapter that keeps every default but `ready_for`, which logs what
/// arrives: it passes what arrives below, the set's qubits included, and
/// each batch as it came.
pub(crate) struct QubitPassThrough {
    log: Log,
}

impl QubitPassThrough {
    pub(crate) fn new(log: &Log) -> StatesLimits<Self> {
        Self { log: log.clone() }.stating_limits()
    }
}

impl AdapterLimits for QubitPassThrough {
    async fn ready_for(&mut self, above: &Capabilities<QubitLimited>) -> Result<(), NotReady> {
        self.log.push(ready_for("QubitPassThrough", above));
        Ok(())
    }
}

impl Adapter for QubitPassThrough {
    type Set = QubitLimited;

    fn accepts(&self) -> &QubitLimited {
        &QubitLimited
    }

    fn sends(&self) -> &QubitLimited {
        &QubitLimited
    }

    fn bind(&mut self, (): ()) {}

    async fn adapt(
        &mut self,
        batches: &[Batch<'_, QubitLimited>],
        seed: Seed,
        below: &mut impl Executor<Accepts = QubitLimited>,
        readouts: &mut [BatchReadouts<QubitLimited>],
    ) -> Result<(), ExecuteError> {
        below.execute(batches, seed, readouts).await
    }
}

/// An adapter that passes each batch as it came, but requires `qubits`
/// qubits below whatever arrives, as a compiler that says it maps a
/// program onto fewer qubits would, were it to send the program unmapped.
pub(crate) struct QubitNarrowing {
    qubits: u32,
}

impl QubitNarrowing {
    pub(crate) fn to(qubits: u32) -> StatesLimits<Self> {
        Self { qubits }.stating_limits()
    }
}

impl AdapterLimits for QubitNarrowing {
    /// What arrives, on `qubits` qubits.
    fn requires(&self, above: &Capabilities<QubitLimited>) -> Capabilities<QubitLimited> {
        Capabilities::new(*above.resources(), *above.features(), Qubits(self.qubits))
    }
}

impl Adapter for QubitNarrowing {
    type Set = QubitLimited;

    fn accepts(&self) -> &QubitLimited {
        &QubitLimited
    }

    fn sends(&self) -> &QubitLimited {
        &QubitLimited
    }

    fn bind(&mut self, (): ()) {}

    async fn adapt(
        &mut self,
        batches: &[Batch<'_, QubitLimited>],
        seed: Seed,
        below: &mut impl Executor<Accepts = QubitLimited>,
        readouts: &mut [BatchReadouts<QubitLimited>],
    ) -> Result<(), ExecuteError> {
        below.execute(batches, seed, readouts).await
    }
}

/// An executor holding `qubits` qubits, which runs nothing: it logs what
/// arrives once ready, and `"QubitRegister execute"` for each `execute`.
pub(crate) struct QubitRegister {
    provides: Capabilities<QubitLimited>,
    log: Log,
}

impl QubitRegister {
    pub(crate) fn new(qubits: u32, log: &Log) -> StatesLimits<Self> {
        Self {
            provides: on_qubits(qubits),
            log: log.clone(),
        }
        .stating_limits()
    }
}

impl ExecutorLimits for QubitRegister {
    fn provides(&self) -> Option<&Capabilities<QubitLimited>> {
        Some(&self.provides)
    }

    async fn ready_for(&mut self, above: &Capabilities<QubitLimited>) -> Result<(), NotReady> {
        self.log.push(ready_for("QubitRegister", above));
        Ok(())
    }
}

impl Executor for QubitRegister {
    type Accepts = QubitLimited;

    fn accepts(&self) -> &QubitLimited {
        &QubitLimited
    }

    async fn execute(
        &mut self,
        _batches: &[Batch<'_, QubitLimited>],
        _seed: Seed,
        _readouts: &mut [BatchReadouts<QubitLimited>],
    ) -> Result<(), ExecuteError> {
        self.log.push("QubitRegister execute");
        Ok(())
    }
}
