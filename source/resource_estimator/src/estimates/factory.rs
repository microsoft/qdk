// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::{borrow::Cow, rc::Rc};

use crate::estimates::ErrorCorrection;

/// Helper structs when dispatching multiple magic states to different factories
mod dispatch;
pub use dispatch::{BuilderDispatch2, FactoryDispatch2};

/// Helper structs for when no factories are used
mod empty;
pub use empty::NoFactories;

/// Generic factory model based on multiple rounds of distillation
mod round_based;
pub use round_based::{
    DistillationRound, DistillationUnit, FactoryBuildError, PhysicalQubitCalculation,
    RoundBasedFactory,
};

pub trait FactoryBuilder<E: ErrorCorrection>
where
    Self::Factory: Clone,
{
    type Factory;

    fn find_factories(
        &self,
        ftp: &E,
        qubit: &Rc<E::Qubit>,
        magic_state_type: usize,
        output_error_rate: f64,
        max_code_parameter: &E::Parameter,
    ) -> Result<Vec<Cow<'_, Self::Factory>>, String>;

    fn num_magic_state_types(&self) -> usize {
        1
    }
}

pub trait Factory
where
    Self::Parameter: Clone,
{
    type Parameter;

    fn physical_qubits(&self) -> u64;
    fn duration(&self) -> u64;
    /// The number of magic states produced by the factory
    fn num_output_states(&self) -> u64;
    fn normalized_volume(&self) -> f64 {
        ((self.physical_qubits() * self.duration()) as f64) / (self.num_output_states() as f64)
    }
    /// The maximum code parameter setting for a magic state factory. This is
    /// used to constrain the search space, when looking for magic state
    /// factories.
    fn max_code_parameter(&self) -> Option<Cow<'_, Self::Parameter>>;
}
