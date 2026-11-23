// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The fake parts on the block set, for the tests of a composition fault
//! naming the part that stated each need: a driver needing qubits, an
//! adapter needing readout parities below, and an executor of a few
//! qubits taking none. They are composed, never run: each sends, adapts
//! or executes nothing.

use executor_protocol_core::prelude::*;
use executor_protocol_core::{
    Batch, BatchReadouts, Capabilities, DriveError, ExecuteError, ExecutorLimits, Features,
    Resources, RunRequirement, Seed, ShotSet, StatesLimits,
};

use crate::{BlockCapabilities, BlockInstructionSet, OpcodeMap};
use gates::Gates;

#[expect(dead_code, reason = "the parts are composed, never sent a gate")]
mod gates {
    use crate::instruction_set;

    instruction_set! {
        /// The gates every fake part sends and accepts.
        pub(crate) Gates {
            H = h(qubit),
        }
    }
}

/// A driver whose program needs `qubits` qubits.
pub(crate) struct QubitProgram {
    sends: BlockInstructionSet,
    qubits: u32,
}

impl QubitProgram {
    pub(crate) fn new(qubits: u32) -> StatesLimits<Self> {
        let sends = Gates::set();
        Self { sends, qubits }.stating_limits()
    }
}

impl DriverRequirements for QubitProgram {
    /// The run's, with its qubits.
    fn requires(&self, run: &RunRequirement) -> Capabilities<BlockInstructionSet> {
        let needs = BlockCapabilities::NONE.with_qubits(self.qubits);
        Capabilities::new(run.resources, run.features, needs)
    }
}

impl Driver for QubitProgram {
    type Sends = BlockInstructionSet;
    type Output = ();

    fn sends(&self) -> &BlockInstructionSet {
        &self.sends
    }

    fn bind(&mut self, _below: OpcodeMap) {}

    async fn drive(
        &mut self,
        shots: &ShotSet,
        _seed: Seed,
        _below: &mut impl Executor<Accepts = BlockInstructionSet>,
    ) -> Result<Vec<()>, DriveError> {
        Ok(vec![(); shots.len()])
    }
}

/// An adapter measuring each logical qubit as the parity of its block's
/// outcomes: it requires what arrives, with readout parities, below.
pub(crate) struct ParityMeasurement {
    set: BlockInstructionSet,
}

impl ParityMeasurement {
    pub(crate) fn new() -> StatesLimits<Self> {
        let set = Gates::set();
        Self { set }.stating_limits()
    }
}

impl AdapterLimits for ParityMeasurement {
    /// What arrives, with readout parities.
    fn requires(
        &self,
        above: &Capabilities<BlockInstructionSet>,
    ) -> Capabilities<BlockInstructionSet> {
        let needs = above.set_capabilities().clone().with_readout_parity(true);
        Capabilities::new(*above.resources(), *above.features(), needs)
    }
}

impl Adapter for ParityMeasurement {
    type Set = BlockInstructionSet;

    fn accepts(&self) -> &BlockInstructionSet {
        &self.set
    }

    fn sends(&self) -> &BlockInstructionSet {
        &self.set
    }

    fn bind(&mut self, _below: OpcodeMap) {}

    async fn adapt(
        &mut self,
        batches: &[Batch<'_, BlockInstructionSet>],
        seed: Seed,
        below: &mut impl Executor<Accepts = BlockInstructionSet>,
        readouts: &mut [BatchReadouts<BlockInstructionSet>],
    ) -> Result<(), ExecuteError> {
        below.execute(batches, seed, readouts).await
    }
}

/// An executor holding `qubits` qubits, which takes no readout parity.
pub(crate) struct QubitRegister {
    accepts: BlockInstructionSet,
    provides: Capabilities<BlockInstructionSet>,
}

impl QubitRegister {
    pub(crate) fn new(qubits: u32) -> StatesLimits<Self> {
        let limits = BlockCapabilities::NONE
            .with_qubits(qubits)
            .with_readout_parity(false);
        Self {
            accepts: Gates::set(),
            provides: Capabilities::new(
                Resources::nothing_stated(),
                Features::nothing_stated(),
                limits,
            ),
        }
        .stating_limits()
    }
}

impl ExecutorLimits for QubitRegister {
    fn provides(&self) -> Option<&Capabilities<BlockInstructionSet>> {
        Some(&self.provides)
    }
}

impl Executor for QubitRegister {
    type Accepts = BlockInstructionSet;

    fn accepts(&self) -> &BlockInstructionSet {
        &self.accepts
    }

    async fn execute(
        &mut self,
        _batches: &[Batch<'_, BlockInstructionSet>],
        _seed: Seed,
        _readouts: &mut [BatchReadouts<BlockInstructionSet>],
    ) -> Result<(), ExecuteError> {
        Ok(())
    }
}
