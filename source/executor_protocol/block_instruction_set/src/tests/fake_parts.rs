// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The fake parts on the block set, for the tests of a composition fault
//! naming the part that stated each need: a driver needing blocks, an
//! adapter adding a need of readout parities or of an ancilla qubit below,
//! and an executor of a few blocks. They are composed, never run: the
//! driver sends nothing, the adapter passes on what it is sent, and the
//! executor runs nothing.

use executor_protocol_core::prelude::*;
use executor_protocol_core::{
    Batch, BatchReadouts, Capabilities, DriveError, ExecuteError, ExecutorLimits, Features,
    Resources, RunRequirement, Seed, ShotSet, StatesLimits,
};

use crate::{BlockCapabilities, BlockInstructionSet, BlockType, OpcodeMap};
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

/// A driver whose program needs `needs`: blocks of each type, such as
/// qubits.
pub(crate) struct BlockProgram {
    sends: BlockInstructionSet,
    needs: BlockCapabilities,
}

impl BlockProgram {
    pub(crate) fn new(needs: BlockCapabilities) -> StatesLimits<Self> {
        let sends = Gates::set();
        Self { sends, needs }.stating_limits()
    }
}

impl DriverRequirements for BlockProgram {
    /// The run's, with its blocks.
    fn requires(&self, run: &RunRequirement) -> Capabilities<BlockInstructionSet> {
        Capabilities::new(run.resources, run.features, self.needs.clone())
    }
}

impl Driver for BlockProgram {
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

/// An adapter measuring each logical qubit in a way that adds a need
/// below: it requires what arrives, with that need.
pub(crate) struct Measurement {
    set: BlockInstructionSet,
    adds: fn(BlockCapabilities) -> BlockCapabilities,
}

impl Measurement {
    /// Measuring each logical qubit as the parity of its block's outcomes,
    /// which needs readout parities.
    pub(crate) fn by_readout_parity() -> StatesLimits<Self> {
        Self::adding(|needs| needs.with_readout_parity(true))
    }

    /// Measuring each logical qubit through an ancilla qubit, which needs
    /// one more qubit.
    pub(crate) fn through_an_ancilla() -> StatesLimits<Self> {
        Self::adding(|needs| {
            let qubits = needs.blocks(&BlockType::qubit()).unwrap_or(0);
            needs.with_qubits(qubits + 1)
        })
    }

    fn adding(adds: fn(BlockCapabilities) -> BlockCapabilities) -> StatesLimits<Self> {
        let set = Gates::set();
        Self { set, adds }.stating_limits()
    }
}

impl AdapterLimits for Measurement {
    /// What arrives, with the need its measurement adds.
    fn requires(
        &self,
        above: &Capabilities<BlockInstructionSet>,
    ) -> Capabilities<BlockInstructionSet> {
        let needs = (self.adds)(above.set_capabilities().clone());
        Capabilities::new(*above.resources(), *above.features(), needs)
    }
}

impl Adapter for Measurement {
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

/// An executor holding the blocks `limits` states, such as a few qubits.
pub(crate) struct BlockRegister {
    accepts: BlockInstructionSet,
    provides: Capabilities<BlockInstructionSet>,
}

impl BlockRegister {
    pub(crate) fn new(limits: BlockCapabilities) -> StatesLimits<Self> {
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

impl ExecutorLimits for BlockRegister {
    fn provides(&self) -> Option<&Capabilities<BlockInstructionSet>> {
        Some(&self.provides)
    }
}

impl Executor for BlockRegister {
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
