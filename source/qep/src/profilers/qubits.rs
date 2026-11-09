pub(crate) struct LogicalQubit {
    /// Idling error rate.
    error_per_nanosecond: f64,

    /// Not counting idling error.
    pub accumulated_error: f64,

    /// Time of last reset, for error computation.
    /// We assume that error at reset is 0.
    pub last_reset_nanos: usize,
}

impl LogicalQubit {
    /// Creates a qubit using the demo's distance-dependent surface-code error model.
    pub fn new(ecc_distance: usize) -> Self {
        assert!(ecc_distance > 0, "ECC distance must be positive");
        #[allow(clippy::cast_precision_loss)]
        let distance = ecc_distance as f64;
        let physical_error: f64 = 1e-3;
        let code_cycle_nanos = 1_000.0;
        // arXiv:2211.07629, Appendix B: error per logical time step.
        // Each step takes d physical code cycles, as in the factory timing model.
        let logical_step_error = 0.03 * (physical_error / 0.01).powf((distance + 1.0) / 2.0);
        Self {
            error_per_nanosecond: logical_step_error / (distance * code_cycle_nanos),
            accumulated_error: 0.0,
            last_reset_nanos: 0,
        }
    }

    pub fn current_error(&self, now_nanos: usize) -> f64 {
        assert!(now_nanos >= self.last_reset_nanos);
        #[allow(clippy::cast_precision_loss)]
        let dt = (now_nanos - self.last_reset_nanos) as f64;
        self.error_per_nanosecond * dt + self.accumulated_error
    }

    pub fn reset(&mut self, now_nanos: usize) {
        self.accumulated_error = 0.0;
        self.last_reset_nanos = now_nanos;
    }
}

#[cfg(test)]
mod tests {
    use super::LogicalQubit;

    /// Checks logical-step normalization and exponential suppression with distance.
    #[test]
    fn error_rate_depends_on_distance() {
        for (distance, expected_rate) in [
            (3, 1e-7),
            (5, 6e-9),
            (7, 3e-6 / 7_000.0),
            (9, 3e-7 / 9_000.0),
            (11, 3e-8 / 11_000.0),
        ] {
            let qubit = LogicalQubit::new(distance);
            assert!(
                (qubit.error_per_nanosecond - expected_rate).abs() <= expected_rate * 1e-12,
                "unexpected error rate at distance {distance}"
            );
            let expected_error = expected_rate * 5_000.0;
            assert!((qubit.current_error(5_000) - expected_error).abs() <= expected_error * 1e-12);
        }
    }
}
