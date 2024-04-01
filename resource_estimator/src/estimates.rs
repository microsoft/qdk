// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod error;
pub use error::Error;
mod error_budget;
pub use error_budget::ErrorBudget;
mod factory;
pub use factory::{
    BuilderDispatch2, DistillationRound, DistillationUnit, FactoryBuildError, FactoryDispatch2,
    NoFactories, RoundBasedFactory,
};
mod physical_estimation;
pub use physical_estimation::{
    ErrorCorrection, Factory, FactoryBuilder, FactoryPart, PhysicalResourceEstimation,
    PhysicalResourceEstimationResult,
};
mod layout;
mod logical_qubit;
pub use layout::Overhead;
pub use logical_qubit::LogicalPatch;
pub mod optimization;
