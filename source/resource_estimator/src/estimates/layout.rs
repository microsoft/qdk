// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use serde::Serialize;

use super::Error;
use super::{ErrorBudget, ErrorBudgetStrategy};

/// Trait to model post-layout logical overhead
pub trait Overhead {
    /// The number of logical qubits to execute the algorithm after mapping
    ///
    /// This number does not include qubit used to produce magic states.
    fn logical_qubits(&self) -> Result<u64, String>;

    /// The number of logical unit cycles to execute the algorithm
    ///
    /// This number is a lower bound for the execution time of the algorithm,
    /// and might be extended by assuming no-ops.
    fn logical_depth(&self, budget: &ErrorBudget) -> Result<u64, String>;

    /// The number of magic states
    ///
    /// The index is used to indicate the type of magic states and must be
    /// supported by available factory builders in the physical estimation.
    fn num_magic_states(&self, budget: &ErrorBudget, index: usize) -> Result<u64, String>;

    /// When implemented, prunes the error budget with respect to the provided
    /// strategy
    #[allow(unused_variables)]
    fn prune_error_budget(
        &self,
        budget: &mut ErrorBudget,
        strategy: ErrorBudgetStrategy,
    ) -> Result<(), String> {
        Ok(())
    }
}

/// This is the realized logical overhead after applying an error budget.  This
/// structure has two purposes: 1) it is used to store the realized logical
/// overhead, once the error budget partition is decided into the resource
/// estimation result; 2) it can be used to pass a logical overhead to the
/// resource estimation API, if it does not depend on the error budget, since it
/// also implements the [`Overhead`] trait.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RealizedOverhead {
    logical_qubits: u64,
    logical_depth: u64,
    num_magic_states: Vec<u64>,
}

impl RealizedOverhead {
    pub fn from_overhead(
        overhead: &impl Overhead,
        budget: &ErrorBudget,
        num_magic_state_types: usize,
    ) -> Result<Self, Error> {
        let logical_qubits = overhead
            .logical_qubits()
            .map_err(Error::AlgorithmicLogicalQubitsComputationFailed)?;
        let logical_depth = overhead
            .logical_depth(budget)
            .map_err(Error::AlgorithmicLogicalDepthComputationFailed)?;
        let num_magic_states = (0..num_magic_state_types)
            .map(|index| overhead.num_magic_states(budget, index))
            .collect::<Result<_, _>>()
            .map_err(Error::NumberOfMagicStatesComputationFailed)?;

        Ok(Self {
            logical_qubits,
            logical_depth,
            num_magic_states,
        })
    }

    #[must_use]
    pub fn logical_qubits(&self) -> u64 {
        self.logical_qubits
    }

    #[must_use]
    pub fn logical_depth(&self) -> u64 {
        self.logical_depth
    }

    #[must_use]
    pub fn num_magic_states(&self) -> &[u64] {
        &self.num_magic_states
    }
}

impl Overhead for RealizedOverhead {
    fn logical_qubits(&self) -> Result<u64, String> {
        Ok(self.logical_qubits)
    }

    fn logical_depth(&self, _budget: &ErrorBudget) -> Result<u64, String> {
        Ok(self.logical_depth)
    }

    fn num_magic_states(&self, _budget: &ErrorBudget, index: usize) -> Result<u64, String> {
        Ok(self.num_magic_states[index])
    }
}
