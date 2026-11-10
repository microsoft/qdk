// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::surface_code::{
    PHYSICAL_ERROR_RATE, logical_error_per_nanosecond, logical_error_per_step, logical_step_nanos,
};

/// Single-round 15-to-1 RM-preparation factory using 31 logical patches.
/// At periodical intervals it produces a T state in last qubits. That T state can be then moved to
/// a memory buffer or be consumed by Inject-T (or Inject-T†) gate.
/// TODO: use `resource_estimator::system::TFactory` instead.
///
///
/// Factory lifecycle in Temporal Simulator:
///  * It starts distillation round.
///  * When distillation round ends:
///   - with probability 1-probability_success it fails, no state is produced, distillation round starts again.
///   - with probability `probability_success` it succeeds, now one of its qubits has T state.
/// * After this T state needs to be used:
///   - Either by "InjectT/InjectTAdjoint" gate on some of compute qubits.
///   - Or it gets moved into memory buffer (this is not implemented for now).
/// * After T state is used, distillation can start again.
///
/// Layout, duration, and fidelity follow arXiv:2211.07629, Appendix C / Table 6.
#[derive(Clone)]
pub(crate) struct TFactory {
    pub(super) logical_qubits: usize,
    ecc_distance: usize,
    pub(crate) probability_success: f64,
    distillation_round_duration_nanos: usize,
    #[expect(
        dead_code,
        reason = "The profiler does not yet report factory error rates."
    )]
    input_state_error: f64,
    pub(super) output_state_error: f64,
    pub(super) output_ready_nanos: Option<usize>,
}

impl TFactory {
    /// Creates a single-round factory using the shared physical and logical error model.
    pub fn new(ecc_distance: usize) -> Self {
        // Formulas from https://arxiv.org/abs/2211.07629

        let input_state_error = PHYSICAL_ERROR_RATE;

        let epsilon = logical_error_per_step(ecc_distance);

        // Error of resulting T state.
        let output_state_error = 35.0 * input_state_error.powf(3.0) + 7.1 * epsilon;

        let discard_probability = 15.0 * input_state_error + 356.0 * epsilon;

        Self {
            logical_qubits: 31,
            ecc_distance,
            probability_success: 1.0 - discard_probability,
            distillation_round_duration_nanos: 11 * logical_step_nanos(ecc_distance),
            input_state_error,
            output_state_error,
            output_ready_nanos: None,
        }
    }

    /// Consumes the output, including memory error while waiting and during injection.
    pub fn take_output_error(&mut self, now_nanos: usize) -> f64 {
        let ready_nanos = self
            .output_ready_nanos
            .take()
            .expect("consumed factory output must have been produced");
        assert!(now_nanos >= ready_nanos);
        #[allow(clippy::cast_precision_loss)]
        let storage_nanos = (now_nanos - ready_nanos) as f64;
        self.output_state_error + storage_nanos * logical_error_per_nanosecond(self.ecc_distance)
    }

    /// Counts data and syndrome qubits for the factory's rotated surface-code patches.
    pub fn physical_qubits(&self) -> usize {
        let d = self.ecc_distance;
        self.logical_qubits * (2 * d * d - 1)
    }

    /// Returns one attempt's duration, without averaging over failed attempts.
    ///
    /// The scheduler samples success and retries failed rounds itself, so
    /// dividing this duration by the success probability would count retries twice.
    pub fn distillation_round_duration_nanos(&self) -> usize {
        self.distillation_round_duration_nanos
    }
}

#[cfg(test)]
mod tests {
    use super::TFactory;

    /// Verifies that eleven distance-5 logical steps take 55 microseconds.
    #[test]
    fn distillation_duration_uses_logical_steps() {
        assert_eq!(TFactory::new(5).distillation_round_duration_nanos(), 55_000);
    }

    /// Checks that fidelity, acceptance, and footprint use the same published factory model.
    #[test]
    fn fidelity_and_space_depend_on_distance() {
        for (distance, expected_logical_error) in [(5, 3e-8), (9, 3e-12)] {
            let factory = TFactory::new(distance);
            let expected_error = 35.0 * 1e-12 + 7.1 * expected_logical_error;
            assert!((factory.output_state_error - expected_error).abs() < expected_error * 1e-12);
            let expected_success = 1.0 - 15.0 * 1e-4 - 356.0 * expected_logical_error;
            assert!((factory.probability_success - expected_success).abs() < 1e-15);
            assert_eq!(
                factory.physical_qubits(),
                31 * (2 * distance * distance - 1)
            );
        }
    }

    /// Charges memory error after production rather than assuming cached T states stay perfect.
    #[test]
    fn output_storage_accrues_error() {
        let mut factory = TFactory::new(5);
        factory.output_ready_nanos = Some(55_000);
        let expected_error = 35.0 * 1e-12 + 7.1 * 3e-8 + 20_000.0 * 6e-12;
        assert!((factory.take_output_error(75_000) - expected_error).abs() < 1e-18);
        assert_eq!(factory.output_ready_nanos, None);
    }
}
