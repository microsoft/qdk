// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/// Optimistic hardware target, also used for raw T-state preparation.
/// Litinski, <https://arxiv.org/abs/1808.02892>, considers both 1e-3 and 1e-4 physical errors.
pub(super) const PHYSICAL_ERROR_RATE: f64 = 1e-4;

/// Physical syndrome-extraction round assumed by the demo.
pub(super) const CODE_CYCLE_NANOS: usize = 1_000;

/// Returns the duration of one distance-dependent logical time step.
pub(super) fn logical_step_nanos(ecc_distance: usize) -> usize {
    assert!(ecc_distance > 0, "ECC distance must be positive");
    ecc_distance * CODE_CYCLE_NANOS
}

/// Returns the logical-step failure budget from <https://arxiv.org/abs/2211.07629>, Appendix B.
pub(super) fn logical_error_per_step(ecc_distance: usize) -> f64 {
    assert!(ecc_distance > 0, "ECC distance must be positive");
    #[allow(clippy::cast_precision_loss)]
    let distance = ecc_distance as f64;
    0.03 * (PHYSICAL_ERROR_RATE / 0.01).powf(distance.midpoint(1.0))
}

/// Converts the logical-step budget to the demo's linear time-accumulation rate.
pub(super) fn logical_error_per_nanosecond(ecc_distance: usize) -> f64 {
    #[allow(clippy::cast_precision_loss)]
    let step_nanos = logical_step_nanos(ecc_distance) as f64;
    logical_error_per_step(ecc_distance) / step_nanos
}
