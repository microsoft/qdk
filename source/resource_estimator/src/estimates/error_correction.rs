// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::cmp::Ordering;

mod code_with_threshold_and_distance;
pub use code_with_threshold_and_distance::{
    CodeWithThresholdAndDistance, CodeWithThresholdAndDistanceEvaluator,
};

/// Trait to model quantum error correction.
///
/// This trait models one quantum error correction code that encodes k logical
/// qubits using n physical qubits.  The physical qubits are of type
/// `Self::Qubit`.  Each code is parameterized by assignments to parameters of
/// type `Self::Parameter`.  Implementers of this trait need to specify values
/// for k, n, the logical cycle time, and the logical error rate, given an
/// assignment to the code parameter.
///
/// In order to define the space of possible code parameters, implementers of
/// this trait need to provide a range of code parameters as well as a
/// comparison function that orders all possible code parameter assignments.
pub trait ErrorCorrection {
    /// The underlying physical qubit type for the code
    type Qubit;
    /// The type for the code parameter
    ///
    /// This could be a numeric type in case the code parameter is the code
    /// distance, or a tuple type, if the code is parameterized over multiple
    /// values.
    type Parameter;

    /// The total number of physical qubits required by the code
    fn physical_qubits(&self, code_parameter: &Self::Parameter) -> Result<u64, String>;

    /// The number of logical qubits provided by the code
    fn logical_qubits(&self, code_parameter: &Self::Parameter) -> Result<u64, String>;

    /// The logical cycle time in nano seconds
    fn logical_cycle_time(
        &self,
        qubit: &Self::Qubit,
        code_parameter: &Self::Parameter,
    ) -> Result<u64, String>;

    /// The logical error rate
    fn logical_error_rate(
        &self,
        qubit: &Self::Qubit,
        code_parameter: &Self::Parameter,
    ) -> Result<f64, String>;

    /// Computes a code parameter assignment for a provided required logical
    /// error rate
    ///
    /// The default implementation iterates through all code parameters using
    /// the `Self::code_parameter_range` method and returns the first parameter
    /// for which the logical error rate is less or equal the required logical
    /// error rate.
    ///
    /// This method assumes that the code parameters that are returned from
    /// `Self::code_parameter_range` are ordered by the logical error rate per
    /// qubit, starting from the largest one.
    fn compute_code_parameter(
        &self,
        qubit: &Self::Qubit,
        required_logical_error_rate: f64,
    ) -> Result<Self::Parameter, String> {
        self.compute_smallest_code_parameter(qubit, required_logical_error_rate)
    }

    /// Default implementation for `Self::compute_code_parameter` in which the
    /// smallest code parameter that satisfies the required logical error rate
    /// is returned.
    ///
    /// This method assumes that the code parameters that are returned from
    /// `Self::code_parameter_range` are ordered by the logical error rate per
    /// qubit, starting from the largest one.
    fn compute_smallest_code_parameter(
        &self,
        qubit: &Self::Qubit,
        required_logical_error_rate: f64,
    ) -> Result<Self::Parameter, String> {
        for parameter in self.code_parameter_range(None) {
            let probability = self.logical_error_rate(qubit, &parameter)?;
            let logical_qubits = self.logical_qubits(&parameter)?;

            if probability / (logical_qubits as f64) <= required_logical_error_rate {
                return Ok(parameter);
            }
        }

        Err(format!(
            "No code parameter achieves required logical error rate {required_logical_error_rate:.3e}"
        ))
    }

    /// Computes the code parameter assignment that requires the fewest number
    /// of physical qubits
    ///
    /// Compared to the default implementation `Self::compute_code_parameter`,
    /// this method evaluates _all_ possible parameters, filters those which
    /// fulfill the required logical error rate, and then chooses the one among
    /// them, which requires the smallest number of physical qubits.
    fn compute_code_parameter_for_smallest_size(
        &self,
        qubit: &Self::Qubit,
        required_logical_error_rate: f64,
    ) -> Result<Self::Parameter, String> {
        let mut best: Option<(Self::Parameter, f64)> = None;

        for parameter in self.code_parameter_range(None) {
            let probability = self.logical_error_rate(qubit, &parameter)?;
            let logical_qubits = self.logical_qubits(&parameter)?;
            let physical_qubits = self.physical_qubits(&parameter)?;

            let physical_qubits_per_logical_qubits = physical_qubits as f64 / logical_qubits as f64;
            if (probability / (logical_qubits as f64) <= required_logical_error_rate)
                && best
                    .as_ref()
                    .is_none_or(|&(_, pq)| physical_qubits_per_logical_qubits < pq)
            {
                best = Some((parameter, physical_qubits_per_logical_qubits));
            }
        }

        best.map(|(p, _)| p)
            .ok_or_else(|| format!("No code parameter achieves required logical error rate {required_logical_error_rate:.3e}"))
    }

    /// Computes the code parameter assignment that provides the fastest logical
    /// cycle time
    ///
    /// Compared to the default implementation `Self::compute_code_parameter`,
    /// this method evaluates _all_ possible parameters, filters those which
    /// fulfill the required logical error rate, and then chooses the one among
    /// them, which provides the fastest logical cycle time.
    fn compute_code_parameter_for_smallest_runtime(
        &self,
        qubit: &Self::Qubit,
        required_logical_error_rate: f64,
    ) -> Result<Self::Parameter, String> {
        let mut best: Option<(Self::Parameter, u64)> = None;

        for parameter in self.code_parameter_range(None) {
            let probability = self.logical_error_rate(qubit, &parameter)?;
            let logical_qubits = self.logical_qubits(&parameter)?;
            let logical_cycle_time = self.logical_cycle_time(qubit, &parameter)?;

            if (probability / (logical_qubits as f64) <= required_logical_error_rate)
                && best.as_ref().is_none_or(|&(_, t)| logical_cycle_time < t)
            {
                best = Some((parameter, logical_cycle_time));
            }
        }

        best.map(|(p, _)| p)
            .ok_or_else(|| format!("No code parameter achieves required logical error rate {required_logical_error_rate:.3e}"))
    }

    /// Adjusts code parameter after computing it
    ///
    /// This function is called after the initial code parameter has been
    /// computed.  The initial code parameter will fit the required logical
    /// error rate, but it's possible to further refine it, if necessary.
    ///
    /// Note, that when implementing the `ErrorCorrection` trait and providing a
    /// custom implementation for this function, it may return a code parameter
    /// that will not fit the required logical error rate any longer.
    ///
    /// The default implementation does not update the code parameter.
    fn adjust_code_parameter(&self, parameter: Self::Parameter) -> Result<Self::Parameter, String> {
        // Default implementation does not update the code parameter
        Ok(parameter)
    }

    /// Returns an iterator of all possible code parameters
    ///
    /// Implementers of this method should sort the code parameters such that
    /// the least costly parameters appear first.  Least costly may be defined
    /// in terms of physical qubits, the logical cycle time, or a combination of
    /// both.
    fn code_parameter_range(
        &self,
        lower_bound: Option<&Self::Parameter>,
    ) -> impl Iterator<Item = Self::Parameter>;

    /// Compares to code parameters
    ///
    /// A code parameter is less than another code parameter, if it requires
    /// less cost in the implementation.  The cost may be defined in terms of
    /// physical qubits, the logical cycle time, or a combination of both.
    fn code_parameter_cmp(
        &self,
        qubit: &Self::Qubit,
        p1: &Self::Parameter,
        p2: &Self::Parameter,
    ) -> Ordering;
}
