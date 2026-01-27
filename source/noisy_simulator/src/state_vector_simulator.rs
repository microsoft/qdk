// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! This module contains two structs: the `StateVectorSimulator` and its
//! internal `StateVector` state.

#[cfg(test)]
mod tests;

use rand::{Rng, SeedableRng, rngs::StdRng};

use crate::{
    ComplexVector, Error, NoisySimulator, SquareMatrix, TOLERANCE, handle_error,
    instrument::Instrument, kernel::apply_kernel, operation::Operation,
};

/// A vector representing the state of a quantum system.
pub struct StateVector {
    /// Dimension of the vector.
    dimension: usize,
    /// Number of qubits in the system.
    number_of_qubits: usize,
    /// Theoretical change in trace due to operations that have been applied so far.
    trace_change: f64,
    /// Vector storing the entries of the density matrix.
    data: ComplexVector,
}

impl StateVector {
    fn new(number_of_qubits: usize) -> Self {
        let dimension = 1 << number_of_qubits;
        let mut state_vector = ComplexVector::zeros(dimension);
        state_vector[0].re = 1.0;
        Self {
            dimension,
            number_of_qubits,
            trace_change: 1.0,
            data: state_vector,
        }
    }

    /// Builds a `StateVector` from its raw fields. Returns `None` if
    ///  the provided args don't represent a valid `StateVector`.
    ///
    /// This method is to be used from the `PyO3` wrapper.
    pub fn try_from(
        dimension: usize,
        number_of_qubits: usize,
        trace_change: f64,
        data: ComplexVector,
    ) -> Result<Self, Error> {
        if 1 << number_of_qubits != dimension {
            return Err(Error::StateVectorTryFromError(format!(
                "the system has {number_of_qubits} qubits and the state vector has dimension {dimension} but 2 ^ {number_of_qubits} != {dimension}"
            )));
        }

        if data.len() != dimension {
            return Err(Error::StateVectorTryFromError(format!(
                "state vector dimension is {} but data has {} entries, {} != {}",
                dimension,
                data.len(),
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

    /// Returns `true` if the squared L2 norm of the matrix is 1.
    fn is_normalized(&self) -> bool {
        (self.norm_squared() - 1.0).abs() <= TOLERANCE
    }

    /// Returns the squared L2 norm of the matrix.
    fn norm_squared(&self) -> f64 {
        self.data.norm_squared()
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
        self.renormalize_with_norm_squared(self.norm_squared())
    }

    /// Renormalizes the matrix such that the trace is 1. Uses a precomputed `norm_squared`.
    fn renormalize_with_norm_squared(&mut self, norm_squared: f64) -> Result<(), Error> {
        if norm_squared < TOLERANCE {
            return Err(Error::ProbabilityZeroEvent);
        }
        let renormalization_factor = 1.0 / norm_squared.sqrt();
        self.data.scale_mut(renormalization_factor);
        Ok(())
    }

    /// Return the probability of a given effect.
    fn effect_probability(
        &self,
        effect_matrix: &SquareMatrix,
        qubits: &[usize],
    ) -> Result<f64, Error> {
        let mut state_copy = self.data.clone();
        apply_kernel(&mut state_copy, effect_matrix, qubits)?;
        Ok(self.data.dotc(&state_copy).re)
    }

    fn sample_kraus_operators(
        &mut self,
        kraus_operators: &[SquareMatrix],
        qubits: &[usize],
        renormalization_factor: f64,
        random_sample: f64,
    ) -> Result<usize, Error> {
        let mut summed_probability = 0.0;
        let mut last_non_zero_probability = 0.0;
        let mut last_non_zero_probability_index = 0;

        for (i, kraus_operator) in kraus_operators.iter().enumerate() {
            let mut state_copy = self.data.clone();
            apply_kernel(&mut state_copy, kraus_operator, qubits)?;
            let norm_squared = state_copy.norm_squared();
            let p = norm_squared / renormalization_factor;
            summed_probability += p;
            if p >= TOLERANCE {
                last_non_zero_probability = p;
                last_non_zero_probability_index = i;
                if summed_probability > random_sample {
                    self.data = state_copy;
                    self.renormalize_with_norm_squared(norm_squared)?;
                    return Ok(last_non_zero_probability_index);
                }
            }
        }

        if summed_probability + TOLERANCE > random_sample && last_non_zero_probability >= TOLERANCE
        {
            return Err(Error::FailedToSampleKrausOperators);
        }

        apply_kernel(
            &mut self.data,
            &kraus_operators[last_non_zero_probability_index],
            qubits,
        )?;

        self.renormalize()?;

        Ok(last_non_zero_probability_index)
    }
}

/// A quantum circuit simulator using a state vector.
pub struct StateVectorSimulator {
    /// A `StateVector` representing the current state of the quantum system.
    state: Result<StateVector, Error>,
    /// Dimension of the density matrix. We need this field to verify the size of the
    /// quantum system in the `set_state` method in the case that `self.state == Err(...)`.
    dimension: usize,
    /// Random number generator used for probabilistic operations.
    rng: StdRng,
}

impl StateVectorSimulator {
    fn check_out_of_bounds_qubits(&self, qubits: &[usize]) -> Result<(), Error> {
        let number_of_qubits = self.state.as_ref()?.number_of_qubits;
        if let Some(id) = qubits.iter().find(|id| **id >= number_of_qubits) {
            Err(Error::QubitIdOutOfBounds(*id))
        } else {
            Ok(())
        }
    }

    /// Apply non selective evolution.
    pub fn apply_instrument(
        &mut self,
        instrument: &Instrument,
        qubits: &[usize],
    ) -> Result<usize, Error> {
        self.check_out_of_bounds_qubits(qubits)?;

        let renormalization_factor = self
            .state
            .as_mut()?
            .effect_probability(instrument.total_effect(), qubits)?;
        self.state.as_mut()?.trace_change *= renormalization_factor;

        match self.state.as_mut()?.sample_kraus_operators(
            instrument.non_selective_kraus_operators(),
            qubits,
            renormalization_factor,
            self.rng.r#gen(),
        ) {
            Ok(choice) => Ok(choice),
            Err(err) => handle_error!(self, err),
        }
    }
}

impl NoisySimulator for StateVectorSimulator {
    type State = StateVector;

    /// Creates a new `StateVectorSimulator`.
    fn new(number_of_qubits: usize) -> Self {
        let state_vector = StateVector::new(number_of_qubits);
        let dimension = state_vector.dimension();
        Self {
            state: Ok(state_vector),
            dimension,
            rng: StdRng::from_entropy(),
        }
    }

    /// Creates a new `StateVectorSimulator` with a given seed for its random number generator.
    fn new_with_seed(number_of_qubits: usize, seed: u64) -> Self {
        let state_vector = StateVector::new(number_of_qubits);
        let dimension = state_vector.dimension();
        Self {
            state: Ok(state_vector),
            dimension,
            rng: StdRng::seed_from_u64(seed),
        }
    }

    /// Apply an operation to given qubit ids.
    fn apply_operation(&mut self, operation: &Operation, qubits: &[usize]) -> Result<(), Error> {
        self.check_out_of_bounds_qubits(qubits)?;

        let renormalization_factor = self
            .state
            .as_mut()?
            .effect_probability(operation.effect_matrix(), qubits)?;
        self.state.as_mut()?.trace_change *= renormalization_factor;

        if let Err(err) = self.state.as_mut()?.sample_kraus_operators(
            operation.kraus_operators(),
            qubits,
            renormalization_factor,
            self.rng.r#gen(),
        ) {
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

        let renormalization_factor = self
            .state
            .as_mut()?
            .effect_probability(instrument.total_effect(), qubits)?;
        let mut last_non_zero_norm_squared = 0.0;
        let mut summed_probability = 0.0;
        let mut last_non_zero_outcome = 0;

        for outcome in 0..instrument.num_operations() {
            let norm_squared = self
                .state
                .as_mut()?
                .effect_probability(instrument.operation(outcome).effect_matrix(), qubits)?;
            let p = norm_squared / renormalization_factor;
            if p >= TOLERANCE {
                last_non_zero_outcome = outcome;
                last_non_zero_norm_squared = norm_squared;
            }
            summed_probability += p;
            if summed_probability > random_sample {
                break;
            }
        }

        if summed_probability + TOLERANCE <= random_sample || last_non_zero_norm_squared < TOLERANCE
        {
            let err = Error::FailedToSampleInstrumentOutcome;
            handle_error!(self, err);
        }

        self.state.as_mut()?.trace_change *= last_non_zero_norm_squared;
        let rescaled_random_sample = ((summed_probability - random_sample)
            / last_non_zero_norm_squared
            * renormalization_factor)
            .max(0.0);

        if let Err(err) = self.state.as_mut()?.sample_kraus_operators(
            instrument
                .operation(last_non_zero_outcome)
                .kraus_operators(),
            qubits,
            last_non_zero_norm_squared,
            rescaled_random_sample,
        ) {
            handle_error!(self, err);
        }
        Ok(last_non_zero_outcome)
    }

    /// Returns the `StateVector` if the simulator is in a valid state.
    fn state(&self) -> Result<&StateVector, &Error> {
        self.state.as_ref()
    }

    /// Set state of the quantum system.
    fn set_state(&mut self, new_state: StateVector) -> Result<(), Error> {
        if self.dimension != new_state.dimension() {
            return Err(Error::InvalidState(format!(
                "the provided state should have the same dimensions as the quantum system's state, {} != {}",
                self.dimension,
                new_state.dimension(),
            )));
        }
        if !new_state.is_normalized() {
            return Err(Error::InvalidState(format!(
                "`state` is not normalized, norm_squared is {}",
                new_state.norm_squared()
            )));
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
