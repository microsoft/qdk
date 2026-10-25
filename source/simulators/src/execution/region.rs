// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Quantum evolution regions: the unit of work of the region ISA.

use super::UnitaryOperation;

/// A target-executable unit of quantum-state evolution between host-visible
/// semantic boundaries.
///
/// A region contains work that a terminal can prepare and execute without
/// returning to the control executor for a measurement result, stochastic
/// choice, query result, output action, or classical-control decision. It does
/// not own the evolving quantum state, control state, randomness, or outputs.
///
/// The current adaptive representation contains only [`UnitaryOperation`]s.
/// A future concrete terminal may justify extending the payload with
/// nonunitary state evolution whose host-visible decisions are already
/// resolved. Measurements, branch selection, queries, and output recording
/// remain separate commands or events even when they do not all mutate the
/// state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct QuantumEvolutionRegion {
    operations: Box<[UnitaryOperation]>,
}

impl QuantumEvolutionRegion {
    /// A region that applies `operations` in order.
    #[must_use]
    pub fn new(operations: impl Into<Box<[UnitaryOperation]>>) -> Self {
        Self {
            operations: operations.into(),
        }
    }

    /// Its operations, in the order they apply.
    #[must_use]
    pub fn operations(&self) -> &[UnitaryOperation] {
        &self.operations
    }
}
