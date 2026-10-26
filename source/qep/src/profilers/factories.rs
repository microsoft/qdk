// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/// T factory consists of 5 logical qubits.
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
/// For the demo, I am using 15-1 factory from fig.15 in https://arxiv.org/pdf/1808.02892
#[derive(Clone)]
pub(super) struct TFactory {
    pub(super) logical_qubits: usize,
    distance: usize,
    pub(super) probability_success: f64,
    distillation_round_duration_nanos: usize,
    input_state_error: f64,
    output_state_error: f64,
}

impl TFactory {
    /// Creates a distance-5 demo factory with a 98.5% distillation success rate.
    pub fn new() -> Self {
        // Formulas from https://arxiv.org/abs/2211.07629

        // Input T-state error rate (TODO: take as input parameter).
        let input_state_error: f64 = 1e-3;

        // Average error rate for clifford operations.
        // TODO: use formula epsilon ≈ 0.03 * (p_phys / 0.01)^((d + 1) / 2)
        let epsilon = 1e-5;

        // Error of resulting T state.
        let output_state_error = 35.0 * input_state_error.powf(3.0) + 7.1 * epsilon;

        let discard_probability = 15.0 * input_state_error + 356.0 * epsilon;

        Self {
            logical_qubits: 5,
            distance: 5,
            probability_success: 1.0 - discard_probability,
            // Litinski, arXiv:1808.02892, Section 3.3 / Fig. 17c: the 15-to-1
            // protocol takes 11 logical steps. Section 1 identifies each step
            // with roughly d physical code cycles; Section 4 assumes 1 us per
            // code cycle. At the fixed demo distance 5 this gives
            // 11 * 5 * 1_000 = 55_000 ns per attempt, not 11 ns.
            // This leading-order estimate assumes state preparation and
            // classical processing do not add latency (Section 3.3's caveat).
            distillation_round_duration_nanos: 11 * 5 * 1_000,
            input_state_error,
            output_state_error,
        }
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
        assert_eq!(TFactory::new().distillation_round_duration_nanos(), 55_000);
    }
}
