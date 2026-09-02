use super::{SimulationError, SimulationResult, query::QueryResult};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct BranchRequest {
    pub(super) mode: u32,
    pub(super) selected: SelectedBranch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SelectedBranch {
    Zero,
    One,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct BranchMasses {
    pub(super) q0: f64,
    pub(super) q1: f64,
    pub(super) norm: f64,
    pub(super) p0: f64,
    pub(super) p1: f64,
}

impl BranchMasses {
    pub(super) fn from_expectations(
        raw_q0: num_complex::Complex64,
        norm_q0: num_complex::Complex64,
        raw_q1: num_complex::Complex64,
        norm_q1: num_complex::Complex64,
    ) -> Result<Self, SimulationError> {
        let q0 = validated_real("P0 mass", raw_q0)?;
        let q1 = validated_real("P1 mass", raw_q1)?;
        let norm_q0 = validated_real("P0 norm", norm_q0)?;
        let norm_q1 = validated_real("P1 norm", norm_q1)?;
        if q0 < 0.0 || q1 < 0.0 {
            return Err(invalid_masses("branch masses are negative"));
        }
        if norm_q0 <= 0.0 || norm_q1 <= 0.0 {
            return Err(invalid_masses("expectation norm is nonpositive"));
        }
        ensure_consistent("P0 and P1 expectation norms", norm_q0, norm_q1)?;
        let norm = f64::midpoint(norm_q0, norm_q1);
        ensure_consistent("branch mass sum and expectation norm", q0 + q1, norm)?;
        let p0 = q0 / norm;
        let p1 = q1 / norm;
        if !p0.is_finite() || !p1.is_finite() || p0 < 0.0 || p1 < 0.0 {
            return Err(invalid_masses(
                "normalized branch probabilities are invalid",
            ));
        }
        ensure_consistent("normalized branch probability sum and one", p0 + p1, 1.0)?;
        Ok(Self {
            q0,
            q1,
            norm,
            p0,
            p1,
        })
    }

    pub(super) fn probability(&self, selected: SelectedBranch) -> Result<f64, SimulationError> {
        Ok(match selected {
            SelectedBranch::Zero => self.p0,
            SelectedBranch::One => self.p1,
        })
    }

    pub(super) fn log_probability(&self, selected: SelectedBranch) -> Result<f64, SimulationError> {
        let p = self.probability(selected)?;
        if p <= 0.0 {
            return Err(SimulationError::InvalidCircuit {
                reason: "cannot compute log probability of zero-mass branch".to_string(),
            });
        }
        Ok(p.ln())
    }
}

fn validated_real(
    label: &'static str,
    value: num_complex::Complex64,
) -> Result<f64, SimulationError> {
    if !value.re.is_finite() || !value.im.is_finite() {
        return Err(invalid_masses(&format!("{label} is non-finite")));
    }
    let imaginary_tolerance = 1.0e-12 * value.re.abs().max(1.0);
    if value.im.abs() > imaginary_tolerance {
        return Err(invalid_masses(&format!(
            "{label} has a material imaginary component"
        )));
    }
    Ok(value.re)
}

fn ensure_consistent(label: &'static str, left: f64, right: f64) -> Result<(), SimulationError> {
    let tolerance = 1.0e-12 * left.abs().max(right.abs()).max(1.0);
    if (left - right).abs() > tolerance {
        return Err(invalid_masses(&format!(
            "{label} are inconsistent: {left} vs {right}"
        )));
    }
    Ok(())
}

fn invalid_masses(reason: &str) -> SimulationError {
    SimulationError::InvalidNativeResult {
        reason: reason.to_string(),
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[allow(
    clippy::struct_field_names,
    reason = "phase evidence keeps seconds explicit on every reported duration"
)]
pub(super) struct BranchPhaseTimings {
    pub(super) initial_execution_seconds: f64,
    pub(super) first_barrier_synchronization_seconds: f64,
    pub(super) first_capture_seconds: f64,
    pub(super) mass_computation_seconds: f64,
    pub(super) mass_synchronization_seconds: f64,
    pub(super) projection_registration_seconds: f64,
    pub(super) projection_preparation_compute_seconds: f64,
    pub(super) projection_barrier_synchronization_seconds: f64,
    pub(super) projection_capture_seconds: f64,
    pub(super) continuation_registration_seconds: f64,
    pub(super) continuation_preparation_compute_seconds: f64,
    pub(super) continuation_barrier_synchronization_seconds: f64,
    pub(super) query_seconds: f64,
    pub(super) cleanup_seconds: f64,
    pub(super) total_wall_seconds: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct BranchReport {
    pub(super) request: BranchRequest,
    pub(super) masses: BranchMasses,
    pub(super) probability: f64,
    pub(super) log_probability: f64,
    pub(super) timings: BranchPhaseTimings,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct BranchSimulationResult {
    pub(super) initial_state: SimulationResult,
    pub(super) post_projection_state: SimulationResult,
    pub(super) continuation_state: SimulationResult,
    pub(super) query: QueryResult,
    pub(super) report: BranchReport,
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_complex::Complex64;

    fn masses(q0: f64, q1: f64, norm: f64) -> Result<BranchMasses, SimulationError> {
        BranchMasses::from_expectations(
            Complex64::new(q0, 0.0),
            Complex64::new(norm, 0.0),
            Complex64::new(q1, 0.0),
            Complex64::new(norm, 0.0),
        )
    }

    #[test]
    fn branch_masses_validate_rejects_nonfinite() {
        assert!(masses(f64::NAN, 0.5, 1.0).is_err());
    }

    #[test]
    fn branch_masses_validate_rejects_negative() {
        assert!(masses(-0.1, 1.1, 1.0).is_err());
    }

    #[test]
    fn branch_masses_validate_rejects_nonpositive_norm() {
        assert!(masses(0.0, 0.0, 0.0).is_err());
    }

    #[test]
    fn branch_masses_validate_rejects_sum_mismatch() {
        assert!(masses(0.3, 0.5, 1.0).is_err());
    }

    #[test]
    fn branch_masses_validate_accepts_valid() {
        let masses = masses(0.3, 0.7, 1.0).expect("valid masses");
        assert!((masses.p0 - 0.3).abs() <= f64::EPSILON);
        assert!((masses.p1 - 0.7).abs() <= f64::EPSILON);
    }

    #[test]
    fn branch_masses_probability_computes_correctly() {
        let masses = masses(0.3, 0.7, 1.0).expect("valid masses");
        let p0 = masses
            .probability(SelectedBranch::Zero)
            .expect("zero probability should be available");
        let p1 = masses
            .probability(SelectedBranch::One)
            .expect("one probability should be available");
        assert!((p0 - 0.3).abs() < 1e-10);
        assert!((p1 - 0.7).abs() < 1e-10);
    }

    #[test]
    fn branch_masses_log_probability_rejects_zero_mass() {
        let masses = masses(0.0, 1.0, 1.0).expect("valid masses");
        assert!(masses.log_probability(SelectedBranch::Zero).is_err());
    }

    #[test]
    fn branch_masses_log_probability_computes_correctly() {
        let masses = masses(0.3, 0.7, 1.0).expect("valid masses");
        let log_p0 = masses
            .log_probability(SelectedBranch::Zero)
            .expect("zero log probability should be available");
        let log_p1 = masses
            .log_probability(SelectedBranch::One)
            .expect("one log probability should be available");
        assert!((log_p0 - 0.3_f64.ln()).abs() < 1e-10);
        assert!((log_p1 - 0.7_f64.ln()).abs() < 1e-10);
    }

    #[test]
    fn branch_masses_reject_material_imaginary_components_and_inconsistent_norms() {
        assert!(
            BranchMasses::from_expectations(
                Complex64::new(0.3, 1.1e-12),
                Complex64::new(1.0, 0.0),
                Complex64::new(0.7, 0.0),
                Complex64::new(1.0, 0.0),
            )
            .is_err()
        );
        assert!(
            BranchMasses::from_expectations(
                Complex64::new(0.3, 0.0),
                Complex64::new(1.0, 0.0),
                Complex64::new(0.7, 0.0),
                Complex64::new(1.1, 0.0),
            )
            .is_err()
        );
    }
}
