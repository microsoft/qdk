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
#[derive(Clone)]
pub(super) struct TFactory {
    logical_qubits: usize,
    distance: usize,
    pub(super) probability_success: f64,
    distillation_round_duration_nanos: usize,
    input_state_error: f64,
    output_state_error: f64,
}

impl TFactory {
    /// Creates a five-qubit factory with a 50% distillation success rate.
    pub fn new() -> Self {
        Self {
            logical_qubits: 5,
            distance: 5,
            probability_success: 0.5,
            distillation_round_duration_nanos: 11,
            input_state_error: 1e-2,
            output_state_error: 1e-2,
        }
    }

    /// Returns the duration of one distillation attempt.
    pub fn distillation_round_duration_nanos(&self) -> usize {
        self.distillation_round_duration_nanos
    }
}
