// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Target-neutral quantum evolution regions and their consumer contract.

use super::UnitaryOperation;

/// A target-executable unit of quantum-state evolution between host-visible
/// semantic boundaries.
///
/// A region contains work that a consumer can prepare and execute without
/// returning to the control executor for a measurement result, stochastic
/// choice, query result, output action, or classical-control decision. It does
/// not own the evolving quantum state, control state, randomness, or outputs.
///
/// The current adaptive representation contains only [`UnitaryOperation`]s.
/// A future concrete consumer may justify extending the payload with
/// nonunitary state evolution whose host-visible decisions are already
/// resolved. Measurements, branch selection, queries, and output recording
/// remain separate commands or events even when they do not all mutate the
/// state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct QuantumEvolutionRegion {
    operations: Box<[UnitaryOperation]>,
}

impl QuantumEvolutionRegion {
    #[must_use]
    pub fn new(operations: impl Into<Box<[UnitaryOperation]>>) -> Self {
        Self {
            operations: operations.into(),
        }
    }

    #[must_use]
    pub fn operations(&self) -> &[UnitaryOperation] {
        &self.operations
    }
}

/// Prepares and executes reached quantum evolution regions against continuing
/// target state.
pub trait RegionConsumer {
    type PreparedRegion<'region>;
    type RegionReport;
    type ExecutionReport;
    type Error;

    fn prepare_region<'region>(
        &mut self,
        region: &'region QuantumEvolutionRegion,
    ) -> Result<Self::PreparedRegion<'region>, Self::Error>;

    fn execute_region(
        &mut self,
        region: Self::PreparedRegion<'_>,
    ) -> Result<Self::RegionReport, Self::Error>;

    fn finish_execution(&mut self) -> Result<Self::ExecutionReport, Self::Error>;

    fn close(&mut self) -> Result<(), Self::Error>;
}