// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The bench's adapter, which forwards.

use block_instruction_set::{BlockInstructionSet, OpcodeMap};
use executor_protocol_core::prelude::*;
use executor_protocol_core::{Batch, BatchReadouts, ExecuteError, Seed};

use super::MixGates;

/// An adapter that forwards every batch as it is: the cost of a boundary.
/// It states only its instructions: it neither needs nor limits the rest.
pub(crate) struct Forward {
    set: BlockInstructionSet,
    identity: OpcodeMap,
}

impl Forward {
    pub(crate) fn new() -> Self {
        let set = MixGates::set();
        let identity = set.fits(&set).expect("a set fits itself");
        Self { set, identity }
    }
}

impl Adapter for Forward {
    type Set = BlockInstructionSet;

    fn accepts(&self) -> &BlockInstructionSet {
        &self.set
    }

    fn sends(&self) -> &BlockInstructionSet {
        &self.set
    }

    fn bind(&mut self, below: OpcodeMap) {
        assert!(
            below == self.identity,
            "the receiver's set is the adapter's"
        );
    }

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
