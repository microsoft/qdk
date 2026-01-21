// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! This crate contains the noisy simulator backend for the Q# language.
//!
//! It includes two simulators:
//!  - A density matrix simulator.
//!  - A state vector simulator.
//!
//! # Density matrix simulator
//! The density matrix simulator is faster, since it evolves the entire probability space
//! of the system as a whole, so you only need to run the simulation once, and take all
//! the samples you need from the final density matrix. However, it is more memory intensive.
//!
//! A density matrix has 2 ^ (2 * `number_of_qubits`) complex numbers entries.
//! If each complex number is represented as two 64-bits floating point numbers,
//! the density matrix will be 2 ^ (2 * `number_of_qubits`) * 16 bytes. E.g., a density
//! matrix representing a 20 qubits system will be 17592186044416 bytes, or 16.4 TB.
//!
//! # State vector simulator
//! The state vector simulator allocates less memory, however if you want 1,000,000 shots
//! of the circuit, you need to run the simulation 1,000,000 times.
//! A state vector has 2 ^ (`number_of_qubits`) complex entries. So, a state vector of a 20
//! qubits system will be 16777216 bytes, or 16 MB.
//!
//! # Which one should I use?
//! If you are interested in running many shots of the circuit it is better to use the
//! density matrix simulator, as long as you have enough memory in your system (13 qubits or less).
//!
//! However if you are interested in a single or very few shots, you should use the state
//! vector simulator.

#![deny(missing_docs)]

pub(crate) mod density_matrix_simulator;
pub(crate) mod instrument;
pub(crate) mod kernel;
pub(crate) mod operation;
pub(crate) mod state_vector_simulator;
#[cfg(test)]
pub(crate) mod tests;

use nalgebra::{DMatrix, DVector};
use num_complex::Complex;
use thiserror::Error;

// Re-exports.
pub use {
    density_matrix_simulator::{DensityMatrix, DensityMatrixSimulator},
    instrument::Instrument,
    operation::Operation,
    state_vector_simulator::{StateVector, StateVectorSimulator},
};

/// A square matrix of `Complex<f64>`.
pub type SquareMatrix = DMatrix<Complex<f64>>;
/// A complex vector.
pub type ComplexVector = DVector<Complex<f64>>;
/// Error tolerance used in the simulators.
pub(crate) const TOLERANCE: f64 = 1e-12;

/// A trait representing a noisy quantum circuit simulator.
pub trait NoisySimulator {
    /// State of the noisy simulator. Depending on the simulation method the state will be
    /// a `DensityMatrix` or a `StateVector`.
    type State;

    /// Creates a new `NoisySimulator`.
    fn new(number_of_qubits: usize) -> Self;

    /// Creates a new `NoisySimulator` with a given seed for its random number generator.
    fn new_with_seed(number_of_qubits: usize, seed: u64) -> Self;

    /// Apply an operation to the given qubit ids.
    fn apply_operation(&mut self, operation: &Operation, qubits: &[usize]) -> Result<(), Error>;

    /// Performs selective evolution under the given instrument.
    /// Returns the index of the observed outcome.
    ///
    /// Use this method to perform measurements on the quantum system.
    fn sample_instrument(
        &mut self,
        instrument: &Instrument,
        qubits: &[usize],
    ) -> Result<usize, Error>;

    /// Performs selective evolution under the given instrument using a custom random distribution.
    /// Returns the index of the observed outcome.
    ///
    /// This method is used for testing and debugging purposes.
    fn sample_instrument_with_distribution(
        &mut self,
        instrument: &Instrument,
        qubits: &[usize],
        random_sample: f64,
    ) -> Result<usize, Error>;

    /// Returns the `State` if the simulator is in a valid state.
    fn state(&self) -> Result<&Self::State, &Error>;

    /// Set state of the quantum system.
    fn set_state(&mut self, new_state: Self::State) -> Result<(), Error>;

    /// Return theoretical change in trace due to operations that have been applied so far
    /// In reality, the density matrix is always renormalized after instruments/operations
    /// have been applied.
    fn trace_change(&self) -> Result<f64, Error>;

    /// Set the trace of the quantum system.
    fn set_trace(&mut self, trace: f64) -> Result<(), Error>;
}

/// A noisy simulation error.
#[derive(Clone, Debug, Error, PartialEq)]
pub enum Error {
    /// Failure when building a `DensityMatrix` from raw data.
    #[error("error when building `DensityMatrix` from raw_data: {0}")]
    DensityMatrixTryFromError(String),
    /// Failure when building an instrument.
    #[error("error when building instrument: {0}")]
    FailedToConstructInstrument(String),
    /// Failure when building an operation.
    #[error("error when building operation: {0}")]
    FailedToConstructOperation(String),
    /// Failure when sampling instrument outcome.
    #[error("numerical error: no outcome found when sampling instrument")]
    FailedToSampleInstrumentOutcome,
    /// Failure when sampling Kraus operators.
    #[error("numerical error: no outcome found when sampling Kraus operators")]
    FailedToSampleKrausOperators,
    /// Provided an invalid state when creating or setting the state of the simulator.
    #[error("provided an invalid state when creating or setting the state of the simulator: {0}")]
    InvalidState(String),
    /// `Matrix` ⋅ `Vector` multiplication mismatch.
    #[error(
        "matrix ⋅ vector multiplication mismatch; matrix is of dimension ({nrows}, {ncols}) but vector has {vec_dim} entries"
    )]
    MatrixVecDimensionMismatch {
        /// Number of rows in the matrix.
        nrows: usize,
        /// Number of columns in the matrix.
        ncols: usize,
        /// Number of elements in the vector.
        vec_dim: usize,
    },
    /// State is not normalized.
    #[error("numerical error: trace should be between 0 and 1, but it is {0}")]
    NotNormalized(f64),
    /// A numerical error, such as a probability-0 event.
    #[error("numerical error: probability-0 event")]
    ProbabilityZeroEvent,
    /// A qubit-id is greater than the number of qubits the simulation supports.
    #[error("qubit id out of bounds: {0}")]
    QubitIdOutOfBounds(usize),
    /// Failure when building a `StateVector` from raw data.
    #[error("error when building `StateVector` from raw_data: {0}")]
    StateVectorTryFromError(String),
    /// Trace is not real
    #[error(
        "state trace should be real since it represents a probability, but its imaginary part is: {0}"
    )]
    TraceIsNotReal(f64),
}

impl Error {
    const fn is_unrecoverable(&self) -> bool {
        matches!(
            self,
            Error::ProbabilityZeroEvent
                | Error::FailedToSampleInstrumentOutcome
                | Error::FailedToSampleKrausOperators
        )
    }
}

impl From<&Error> for Error {
    fn from(value: &Error) -> Self {
        value.clone()
    }
}

impl From<&mut Error> for Error {
    fn from(value: &mut Error) -> Self {
        value.clone()
    }
}

/// Handles errors in the simulator.
/// If an error is unrecoverable, it will set the state of the simulator to that error,
/// invalidating any further evolution of the quantum system.
macro_rules! handle_error {
    ($self:expr, $err:expr) => {{
        if $err.is_unrecoverable() {
            $self.state = Err($err.clone());
        }
        return Err($err);
    }};
}

pub(crate) use handle_error;
