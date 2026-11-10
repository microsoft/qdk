use super::surface_code::logical_error_per_nanosecond;

pub(crate) struct LogicalQubit {
    /// Logical memory error rate, including time spent executing instructions.
    error_per_nanosecond: f64,

    /// Unreported injected-state error.
    pub accumulated_error: f64,

    /// Start of the interval not yet charged to the program's error budget.
    last_accounted_nanos: usize,
}

impl LogicalQubit {
    /// Creates a qubit using the demo's distance-dependent surface-code error model.
    pub fn new(ecc_distance: usize, now_nanos: usize) -> Self {
        Self {
            error_per_nanosecond: logical_error_per_nanosecond(ecc_distance),
            accumulated_error: 0.0,
            last_accounted_nanos: now_nanos,
        }
    }

    /// Returns memory and injected-state contributions not yet reported.
    pub fn current_error(&self, now_nanos: usize) -> f64 {
        assert!(now_nanos >= self.last_accounted_nanos);
        #[allow(clippy::cast_precision_loss)]
        let dt = (now_nanos - self.last_accounted_nanos) as f64;
        self.error_per_nanosecond * dt + self.accumulated_error
    }

    /// Charges each contribution once without treating measurement as physical error correction.
    pub fn take_error(&mut self, now_nanos: usize) -> f64 {
        let error = self.current_error(now_nanos);
        self.reset(now_nanos);
        error
    }

    /// Starts a fresh accounting interval with no unreported injected-state error.
    pub fn reset(&mut self, now_nanos: usize) {
        self.accumulated_error = 0.0;
        self.last_accounted_nanos = now_nanos;
    }
}

#[cfg(test)]
mod tests {
    use super::LogicalQubit;

    /// Checks logical-step normalization and exponential suppression with distance.
    #[test]
    fn error_rate_depends_on_distance() {
        for (distance, expected_rate) in [
            (3, 1e-9),
            (5, 6e-12),
            (7, 3e-10 / 7_000.0),
            (9, 3e-12 / 9_000.0),
            (11, 3e-14 / 11_000.0),
        ] {
            let qubit = LogicalQubit::new(distance, 0);
            assert!(
                (qubit.error_per_nanosecond - expected_rate).abs() <= expected_rate * 1e-12,
                "unexpected error rate at distance {distance}"
            );
            let expected_error = expected_rate * 5_000.0;
            assert!((qubit.current_error(5_000) - expected_error).abs() <= expected_error * 1e-12);
        }
    }
}
