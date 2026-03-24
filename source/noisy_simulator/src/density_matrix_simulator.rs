// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! This module contains two structs: the `DensityMatrixSimulator` and its
//! internal `DensityMatrix` state.

#[cfg(test)]
mod tests;

use crate::{
    ComplexVector, Error, NoisySimulator, SquareMatrix, TOLERANCE, handle_error,
    instrument::Instrument, kernel::apply_kernel, operation::Operation,
};
use num_complex::Complex;
use rand::{Rng, SeedableRng, rngs::StdRng};

/// A vectorized density matrix.
#[derive(Debug, Clone)]
pub struct DensityMatrix {
    /// Dimension of the matrix. E.g.: If the matrix is 5 x 5, then dimension is 5.
    dimension: usize,
    /// Number of qubits in the system.
    number_of_qubits: usize,
    /// Theoretical change in trace due to operations that have been applied so far.
    trace_change: f64,
    /// Vector storing the entries of the density matrix.
    data: ComplexVector,
}

impl DensityMatrix {
    fn new(number_of_qubits: usize) -> Self {
        let dimension = 1 << number_of_qubits;
        let mut data = ComplexVector::zeros(dimension * dimension);
        data[0].re = 1.0;
        Self {
            dimension,
            number_of_qubits,
            trace_change: 1.0,
            data,
        }
    }

    /// Builds a `DensityMatrix` from its raw fields. Returns `None` if
    ///  the provided args don't represent a valid `DensityMatrix`.
    ///
    /// This method is to be used from the `PyO3` wrapper.
    pub fn try_from(
        dimension: usize,
        number_of_qubits: usize,
        trace_change: f64,
        data: ComplexVector,
    ) -> Result<Self, Error> {
        if 1 << number_of_qubits != dimension {
            return Err(Error::DensityMatrixTryFromError(format!(
                "the system has {number_of_qubits} qubits and density matrix has dimension {dimension} but 2 ^ {number_of_qubits} != {dimension}"
            )));
        }

        if data.len() != dimension * dimension {
            return Err(Error::DensityMatrixTryFromError(format!(
                "matrix dimension is {} but data has {} entries, {} x {} != {}",
                dimension,
                data.len(),
                dimension,
                dimension,
                data.len()
            )));
        }

        Ok(Self {
            dimension,
            number_of_qubits,
            trace_change,
            data,
        })
    }

    /// Returns a reference to the vector containing the density matrix's data.
    #[must_use]
    pub fn data(&self) -> &ComplexVector {
        &self.data
    }

    /// Returns dimension of the matrix. E.g.: If the matrix is 5 x 5, then dimension is 5.
    #[must_use]
    pub fn dimension(&self) -> usize {
        self.dimension
    }

    /// Returns the number of qubits in the system.
    #[must_use]
    pub fn number_of_qubits(&self) -> usize {
        self.number_of_qubits
    }

    /// Returns `true` if the matrix is Hermitian.
    fn is_hermitian(&self) -> bool {
        for row in 0..self.dimension {
            for col in 0..self.dimension {
                let elt = self.data[self.dimension * row + col];
                let mirror_elt = self.data[self.dimension * col + row];
                if (elt.re - mirror_elt.re).abs() > TOLERANCE
                    || (elt.im + mirror_elt.im).abs() > TOLERANCE
                {
                    return false;
                }
            }
        }
        true
    }

    /// Returns `true` if the trace of the matrix is 1.
    fn is_normalized(&self) -> Result<bool, Error> {
        Ok((self.trace()? - 1.0).abs() <= TOLERANCE)
    }

    /// Returns the trace of the matrix. The trace is the sum of the diagonal entries of a matrix.
    fn trace(&self) -> Result<f64, Error> {
        let mut trace: Complex<f64> = Complex::ZERO;
        for idx in 0..self.dimension {
            trace += self.data[(self.dimension + 1) * idx];
        }

        if trace.im > TOLERANCE {
            Err(Error::TraceIsNotReal(trace.im))
        } else {
            Ok(trace.re)
        }
    }

    /// Return theoretical change in trace due to operations that have been applied so far.
    /// In reality, the density matrix is always renormalized after instruments / operations
    /// have been applied.
    #[must_use]
    pub fn trace_change(&self) -> f64 {
        self.trace_change
    }

    /// Renormalizes the matrix such that the trace is 1.
    fn renormalize(&mut self) -> Result<(), Error> {
        self.renormalize_with_trace(self.trace()?)
    }

    /// Renormalizes the matrix such that the trace is 1. Uses a precomputed `trace`.
    fn renormalize_with_trace(&mut self, trace: f64) -> Result<(), Error> {
        if trace < TOLERANCE {
            return Err(Error::ProbabilityZeroEvent);
        }
        self.trace_change *= trace;
        let renormalization_factor = 1.0 / trace;
        self.data.scale_mut(renormalization_factor);
        Ok(())
    }

    /// Applies the operation matrix to the target qubits.
    fn apply_operation_matrix(
        &mut self,
        operation_matrix: &SquareMatrix,
        qubits: &[usize],
    ) -> Result<(), Error> {
        // TODO [Research]: Figure out why they do this qubits_expanded thing.
        let mut qubits_expanded = Vec::with_capacity(2 * qubits.len());
        for id in qubits {
            qubits_expanded.push(*id);
        }
        for id in qubits {
            qubits_expanded.push(*id + self.number_of_qubits());
        }
        apply_kernel(&mut self.data, operation_matrix, &qubits_expanded)
    }
}

/// A quantum circuit simulator using a density matrix.
///
/// All the simulator methods return a `Result<_, Error>`. If the simulator reaches an
/// invalid state due to a numerical error, it will return that last error from there on.
pub struct DensityMatrixSimulator {
    /// A `DensityMatrix` representing the current state of the quantum system.
    state: Result<DensityMatrix, Error>,
    /// Dimension of the density matrix. We need this field to verify the size of the
    /// quantum system in the `set_state` method in the case that `self.state == Err(...)`.
    dimension: usize,
    /// Random number generator used for probabilistic operations.
    rng: StdRng,
}

impl DensityMatrixSimulator {
    fn check_out_of_bounds_qubits(&self, qubits: &[usize]) -> Result<(), Error> {
        let number_of_qubits = self.state.as_ref()?.number_of_qubits;
        if let Some(id) = qubits.iter().find(|id| **id >= number_of_qubits) {
            Err(Error::QubitIdOutOfBounds(*id))
        } else {
            Ok(())
        }
    }

    /// Apply non selective evolution to the given qubit ids.
    pub fn apply_instrument(
        &mut self,
        instrument: &Instrument,
        qubits: &[usize],
    ) -> Result<(), Error> {
        self.check_out_of_bounds_qubits(qubits)?;
        self.state
            .as_mut()?
            .apply_operation_matrix(instrument.non_selective_operation_matrix(), qubits)?;
        if let Err(err) = self.state.as_mut()?.renormalize() {
            handle_error!(self, err);
        }
        Ok(())
    }
}

impl NoisySimulator for DensityMatrixSimulator {
    type State = DensityMatrix;

    /// Creates a new `DensityMatrixSimulator`.
    fn new(number_of_qubits: usize) -> Self {
        let density_matrix = DensityMatrix::new(number_of_qubits);
        let dimension = density_matrix.dimension();
        Self {
            state: Ok(density_matrix),
            dimension,
            rng: StdRng::from_entropy(),
        }
    }

    /// Creates a new `DensityMatrixSimulator` with a given seed for its random number generator.
    fn new_with_seed(number_of_qubits: usize, seed: u64) -> Self {
        let density_matrix = DensityMatrix::new(number_of_qubits);
        let dimension = density_matrix.dimension();
        Self {
            state: Ok(density_matrix),
            dimension,
            rng: StdRng::seed_from_u64(seed),
        }
    }

    /// Apply an operation to the given qubit ids.
    fn apply_operation(&mut self, operation: &Operation, qubits: &[usize]) -> Result<(), Error> {
        self.check_out_of_bounds_qubits(qubits)?;

        self.state
            .as_mut()?
            .apply_operation_matrix(operation.matrix(), qubits)?;
        if let Err(err) = self.state.as_mut()?.renormalize() {
            handle_error!(self, err);
        }
        Ok(())
    }

    /// Performs selective evolution under the given instrument.
    /// Returns the index of the observed outcome.
    ///
    /// Use this method to perform measurements on the quantum system.
    fn sample_instrument(
        &mut self,
        instrument: &Instrument,
        qubits: &[usize],
    ) -> Result<usize, Error> {
        let sample = self.rng.r#gen();
        self.sample_instrument_with_distribution(instrument, qubits, sample)
    }

    /// Performs selective evolution under the given instrument using a custom random distribution.
    /// Returns the index of the observed outcome.
    ///
    /// This method is used for testing and debugging purposes.
    fn sample_instrument_with_distribution(
        &mut self,
        instrument: &Instrument,
        qubits: &[usize],
        random_sample: f64,
    ) -> Result<usize, Error> {
        self.check_out_of_bounds_qubits(qubits)?;

        let mut tmp_state = self.state.clone()?;
        apply_kernel(
            &mut tmp_state.data,
            instrument.total_effect_transposed(),
            qubits,
        )?;
        let total_effect_trace = tmp_state.trace()?;
        if total_effect_trace < TOLERANCE {
            let err = Error::ProbabilityZeroEvent;
            handle_error!(self, err);
        }
        let mut last_non_zero_trace_outcome: usize = 0;
        let mut last_non_zero_trace: f64 = 0.0;
        let mut summed_probability: f64 = 0.0;

        for outcome in 0..instrument.num_operations() {
            if summed_probability > random_sample {
                break;
            }
            tmp_state = self.state.clone()?;
            apply_kernel(
                &mut tmp_state.data,
                instrument.operation(outcome).effect_matrix_transpose(),
                qubits,
            )?;
            let outcome_trace = tmp_state.trace()?;
            summed_probability += outcome_trace / total_effect_trace;
            if outcome_trace >= TOLERANCE {
                last_non_zero_trace_outcome = outcome;
                last_non_zero_trace = outcome_trace;
            }
        }

        if summed_probability + TOLERANCE <= random_sample || last_non_zero_trace < TOLERANCE {
            let err = Error::FailedToSampleInstrumentOutcome;
            handle_error!(self, err);
        }

        if let Err(err) = self.state.as_mut()?.apply_operation_matrix(
            instrument.operation(last_non_zero_trace_outcome).matrix(),
            qubits,
        ) {
            handle_error!(self, err);
        }
        if let Err(err) = self
            .state
            .as_mut()?
            .renormalize_with_trace(last_non_zero_trace)
        {
            handle_error!(self, err);
        }
        Ok(last_non_zero_trace_outcome)
    }

    /// Returns the `DensityMatrix` if the simulator is in a valid state.
    fn state(&self) -> Result<&DensityMatrix, &Error> {
        self.state.as_ref()
    }

    /// Set state of the quantum system.
    fn set_state(&mut self, new_state: DensityMatrix) -> Result<(), Error> {
        if self.dimension != new_state.dimension() {
            return Err(Error::InvalidState(format!(
                "the provided state should have the same dimensions as the quantum system's state, {} != {}",
                self.dimension,
                new_state.dimension(),
            )));
        }
        if !new_state.is_normalized()? {
            return Err(Error::InvalidState(format!(
                "`state` is not normalized, trace is {}",
                new_state.trace()?
            )));
        }
        if !new_state.is_hermitian() {
            return Err(Error::InvalidState("`state` is not Hermitian".to_string()));
        }
        self.state = Ok(new_state);
        Ok(())
    }

    /// Return theoretical change in trace due to operations that have been applied so far
    /// In reality, the density matrix is always renormalized after instruments/operations
    /// have been applied.
    fn trace_change(&self) -> Result<f64, Error> {
        Ok(self.state.as_ref()?.trace_change())
    }

    /// Set the trace of the quantum system.
    fn set_trace(&mut self, trace: f64) -> Result<(), Error> {
        if trace < TOLERANCE || (trace - 1.) > TOLERANCE {
            return Err(Error::NotNormalized(trace));
        }
        self.state.as_mut()?.trace_change = trace;
        Ok(())
    }
}
