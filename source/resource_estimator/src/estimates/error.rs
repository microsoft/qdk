// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use miette::Diagnostic;
use thiserror::Error;

#[derive(Debug, Error, Diagnostic)]
pub enum Error {
    /// Input algorithm has no resources
    ///
    /// ✅ This does not contain user data and can be logged
    /// 🧑‍💻 This indicates a user error
    #[error("Algorithm requires at least one magic state or measurement to estimate resources")]
    #[diagnostic(code("Qsc.Estimates.AlgorithmHasNoResources"))]
    AlgorithmHasNoResources,
    /// The number of algorithmic logical qubits cannot be computed.
    ///
    /// ✅ This does not contain user data and can be logged
    /// ✅ This error cannot be triggered by the system.
    #[error("Cannot compute the number of algorithmic logical qubits: {0}")]
    #[diagnostic(code("Qsc.Estimates.AlgorithmicLogicalQubitsComputationFailed"))]
    AlgorithmicLogicalQubitsComputationFailed(String),
    /// The algorithmic logical depth cannot be computed.
    ///
    /// ✅ This does not contain user data and can be logged
    /// ✅ This error cannot be triggered by the system.
    #[error("Cannot compute the algorithmic logical depth: {0}")]
    #[diagnostic(code("Qsc.Estimates.AlgorithmicLogicalDepthComputationFailed"))]
    AlgorithmicLogicalDepthComputationFailed(String),
    /// The number of required magic states cannot be computed.
    ///
    /// ✅ This does not contain user data and can be logged
    /// ✅ This error cannot be triggered by the system.
    #[error("Cannot compute the required number of magic states: {0}")]
    #[diagnostic(code("Qsc.Estimates.NumberOfMagicStatesComputationFailed"))]
    NumberOfMagicStatesComputationFailed(String),
    /// Error while pruning the error budget
    ///
    /// ✅ This does not contain user data and can be logged
    /// ✅ This error cannot be triggered by the system.
    #[error("Cannot prune the error budget: {0}")]
    #[diagnostic(code("Qsc.Estimates.PruneErrorBudgetFailed"))]
    PruneErrorBudgetFailed(String),
    /// Both constraints for maximal time and
    /// maximal number of qubits are provided
    ///
    /// ✅ This does not contain user data and can be logged
    /// 🧑‍💻 This indicates a user error
    #[error(
        "Both duration and number of physical qubits constraints are provided, but only one is allowed"
    )]
    #[diagnostic(code("Qsc.Estimates.BothDurationAndPhysicalQubitsProvided"))]
    BothDurationAndPhysicalQubitsProvided,
    /// No solution found for the provided maximum duration.
    ///
    /// ✅ This does not contain user data and can be logged
    /// 🧑‍💻 This indicates a user error
    #[error("No solution found for the provided maximum duration.")]
    #[diagnostic(code("Qsc.Estimates.MaxDurationTooSmall"))]
    MaxDurationTooSmall,
    /// No solution found for the provided maximum number of physical qubits
    ///
    /// ✅ This does not contain user data and can be logged
    /// 🧑‍💻 This indicates a user error
    #[error("No solution found for the provided maximum number of physical qubits.")]
    #[diagnostic(code("Qsc.Estimates.MaxPhysicalQubitsTooSmall"))]
    MaxPhysicalQubitsTooSmall,
    /// Logical depth scaling factor is too small
    ///
    /// 🧑‍💻 This indicates a user error
    #[error("Logical depth scaling factor is too small, it must be at least 1: {0}")]
    #[diagnostic(code("Qsc.Estimates.LogicalDepthScalingFactorTooSmall"))]
    LogicalDepthScalingFactorTooSmall(f64),
    /// Resource estimation failed to find factories
    ///
    /// ✅ This error cannot be triggered by the system.
    #[error("Resource estimation failed to find factories: {0}")]
    #[diagnostic(code("Qsc.Estimates.FactorySearchFailed"))]
    FactorySearchFailed(String),
    /// Constraint-based search only supports one magic state type.
    ///
    /// ✅ This error cannot be triggered by the system, since only one magic
    /// state type is supported.
    #[error("Constraint-based search only supports one magic state type.")]
    #[diagnostic(code("Qsc.Estimates.MultipleMagicStatesNotSupported"))]
    MultipleMagicStatesNotSupported,
    /// The number of physical qubits required for a code cannot be computed.
    ///
    /// ✅ This does not contain user data and can be logged
    /// 🧑‍💻 This indicates a user error
    #[error("The number of physical qubits required for a code cannot be computed: {0}")]
    #[diagnostic(code("Qsc.Estimates.PhysicalQubitComputationFailed"))]
    PhysicalQubitComputationFailed(String),
    /// The number of logical qubits provided by a code cannot be computed.
    ///
    /// ✅ This does not contain user data and can be logged
    /// 🧑‍💻 This indicates a user error
    #[error("The number of logical qubits provided by a code cannot be computed: {0}")]
    #[diagnostic(code("Qsc.Estimates.LogicalQubitComputationFailed"))]
    LogicalQubitComputationFailed(String),
    /// The logical cycle time cannot be computed.
    ///
    /// ✅ This does not contain user data and can be logged
    /// 🧑‍💻 This indicates a user error
    #[error("The logical cycle time cannot be computed: {0}")]
    #[diagnostic(code("Qsc.Estimates.LogicalCycleTimeComputationFailed"))]
    LogicalCycleTimeComputationFailed(String),
    /// The logical error rate cannot be computed.
    ///
    /// ✅ This does not contain user data and can be logged
    /// 🧑‍💻 This indicates a user error
    #[error("The logical error rate cannot be computed: {0}")]
    #[diagnostic(code("Qsc.Estimates.LogicalErrorRateComputationFailed"))]
    LogicalErrorRateComputationFailed(String),
    /// The code parameter cannot be computed.
    ///
    /// ✅ This does not contain user data and can be logged
    /// 🧑‍💻 This indicates a user error
    #[error("The code parameter cannot be computed: {0}")]
    #[diagnostic(code("Qsc.Estimates.CodeParameterComputationFailed"))]
    CodeParameterComputationFailed(String),
}
