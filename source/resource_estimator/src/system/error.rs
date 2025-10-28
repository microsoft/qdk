// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use miette::Diagnostic;
use thiserror::Error;

#[derive(Debug, Error, Diagnostic)]
pub enum IO {
    /// Cannot open a filename that is passed by string
    ///
    /// ✅ This does not contain user data and can be logged
    /// 🐞 We want this to be tracked as error and investigate
    ///
    /// Note that in the service, we are creating all filenames.
    /// It's not relevant to log this data.
    #[error("cannot open file: '{0}'")]
    #[diagnostic(code("Qsc.Estimates.IOError.CannotOpenFile"))]
    CannotOpenFile(String),
    /// Captures various reasons that JSON cannot be parsed
    ///
    /// ❌ This may contain user data and cannot be logged
    /// 🧑‍💻 This indicates a user error
    #[error("cannot parse JSON: '{0}'")]
    #[diagnostic(code("Qsc.Estimates.IOError.CannotParseJSON"))]
    CannotParseJSON(serde_json::error::Error),
}

#[derive(Debug, Error, Diagnostic)]
pub enum TFactory {
    /// Cannot compute the inverse binomial distribution
    ///
    /// ✅ This does not contain user data and can be logged
    /// 🐞 We want this to be tracked as error and investigate
    #[error("cannot compute inverse binomial distribution for n = {0}, p1 = {1}, and p2 = {2}")]
    #[diagnostic(code("Qsc.Estimates.TFactoryError.CannotComputeInverseBinomial"))]
    CannotComputeInverseBinomial(usize, f64, f64),
}

#[derive(Debug, Error, Diagnostic)]
pub enum InvalidInput {
    /// Fault-tolerance protocol is not compatible with instruction set
    ///
    /// ✅ This does not contain user data and can be logged
    /// 🧑‍💻 This indicates a user error
    #[error("fault tolerance protocol does not support gate type of qubit")]
    #[diagnostic(code("Qsc.Estimates.InvalidInputError.InvalidFaultToleranceProtocol"))]
    InvalidFaultToleranceProtocol,
    /// Logical cycle is non-positive for some code distance value
    ///
    /// ✅ This does not contain user data and can be logged
    /// 🧑‍💻 This indicates a user error
    #[error("logicalCycleTime formula yields non-positive value for code distance = {0}")]
    #[diagnostic(code("Qsc.Estimates.InvalidInputError.NonPositiveLogicalCycleTime"))]
    NonPositiveLogicalCycleTime(u64),
    /// Number of physical qubits per logical qubits is non-positive for some code distance value
    ///
    /// ✅ This does not contain user data and can be logged
    /// 🧑‍💻 This indicates a user error
    #[error(
        "physicalQubitsPerLogicalQubit formula yields non-positive value for code distance = {0}"
    )]
    #[diagnostic(code("Qsc.Estimates.InvalidInputError.NonPositivePhysicalQubitsPerLogicalQubit"))]
    NonPositivePhysicalQubitsPerLogicalQubit(u64),
    /// Invalid error budget (<= 0.0 or >= 1.0)
    ///
    /// ✅ This does not contain user data and can be logged
    /// 🧑‍💻 This indicates a user error
    #[error("The error budget must be between 0.0 and 1.0, provided input was `{0}`")]
    #[diagnostic(code("Qsc.Estimates.InvalidInputError.InvalidErrorBudget"))]
    InvalidErrorBudget(f64),
    /// Constraints provided for frontier estimation
    /// (maximal time, maximal number of qubits, maximal number of T factories)
    /// are not supported for frontier estimation.
    ///
    /// ✅ This does not contain user data and can be logged
    /// 🧑‍💻 This indicates a user error
    #[error("Unsupported constraints provided for Frontier Estimation.")]
    #[diagnostic(code("Qsc.Estimates.InvalidInputError.ConstraintsProvidedForFrontierEstimation"))]
    ConstraintsProvidedForFrontierEstimation,
}

#[derive(Debug, Error, Diagnostic)]
pub enum Error {
    /// Handles various types of I/O errors
    ///
    /// ❌ This may contain user data and cannot be logged
    #[error(transparent)]
    #[diagnostic(transparent)]
    IO(IO),
    /// An error that happens when evaluating an expression
    ///
    /// ❌ This may contain user data and cannot be logged
    /// 🧑‍💻 This indicates a user error
    #[error("cannot evaluate expression: '{0}'")]
    #[diagnostic(code("Qsc.Estimates.EvaluationError.CannotEvaluateExpression"))]
    Evaluation(String),
    /// Invalid value for some variable, allowed range is specified via lower
    /// and upper bound
    ///
    /// ✅ This does not contain user data and can be logged
    /// 🧑‍💻 This indicates a user error
    #[error("invalid value for '{0}', expected value between {1} and {2}")]
    #[diagnostic(code("Qsc.Estimates.InvalidValueError.InvalidValue"))]
    InvalidValue(String, f64, f64),
    /// Handles various types of invalid input
    ///
    /// ✅ This does not contain user data and can be logged
    /// (mostly user error, but check `InvalidInput` for more details)
    #[error(transparent)]
    #[diagnostic(transparent)]
    InvalidInput(InvalidInput),
    /// Handles various types of T-factory problems
    ///
    /// ✅ This does not contain user data and can be logged
    /// 🧑‍💻 This indicates a user error
    #[error(transparent)]
    #[diagnostic(transparent)]
    TFactory(TFactory),
    /// Handles various errors from physical resource estimation
    ///
    /// ✅ This does not contain user data and can be logged
    /// 🧑‍💻 This indicates a user error
    #[error(transparent)]
    #[diagnostic(transparent)]
    Estimation(#[from] crate::estimates::Error),
}

impl From<fasteval::Error> for Error {
    fn from(error: fasteval::Error) -> Self {
        Self::Evaluation(error.to_string())
    }
}

impl From<IO> for Error {
    fn from(error: IO) -> Self {
        Self::IO(error)
    }
}

impl From<TFactory> for Error {
    fn from(error: TFactory) -> Self {
        Self::TFactory(error)
    }
}

impl From<InvalidInput> for Error {
    fn from(error: InvalidInput) -> Self {
        Self::InvalidInput(error)
    }
}
