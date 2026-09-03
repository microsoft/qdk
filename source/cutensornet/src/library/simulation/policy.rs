use super::SimulationError;

pub(super) const MAXIMUM_WORKSPACE_BYTES: usize = 68_719_476_736;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Precision {
    F32,
    F64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SvdAlgorithm {
    Gesvd,
    Gesvdj,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Gauge {
    Simple,
    Free,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ExecutionPolicy {
    pub(crate) device_ordinal: i32,
    pub(super) precision: Precision,
    pub(super) bond_cap: i64,
    pub(super) absolute_cutoff: f64,
    pub(super) relative_cutoff: f64,
    pub(super) svd_algorithm: SvdAlgorithm,
    pub(super) gauge: Gauge,
    pub(super) maximum_workspace_bytes: usize,
}

impl ExecutionPolicy {
    pub(super) const fn bell_regression() -> Self {
        Self {
            device_ordinal: 0,
            precision: Precision::F64,
            bond_cap: 2,
            absolute_cutoff: 1.0e-10,
            relative_cutoff: 1.0e-10,
            svd_algorithm: SvdAlgorithm::Gesvd,
            gauge: Gauge::Simple,
            maximum_workspace_bytes: MAXIMUM_WORKSPACE_BYTES,
        }
    }

    pub(super) const fn base_qualification() -> Self {
        Self {
            device_ordinal: 0,
            precision: Precision::F64,
            bond_cap: 128,
            absolute_cutoff: 1.0e-10,
            relative_cutoff: 1.0e-16,
            svd_algorithm: SvdAlgorithm::Gesvd,
            gauge: Gauge::Simple,
            maximum_workspace_bytes: MAXIMUM_WORKSPACE_BYTES,
        }
    }

    pub(super) const fn b3_matched_bond_qualification() -> Self {
        Self {
            absolute_cutoff: 1.0e-12,
            ..Self::base_qualification()
        }
    }

    pub(super) const fn b4_convergence_qualification(bond_cap: i64) -> Self {
        Self {
            bond_cap,
            ..Self::b3_matched_bond_qualification()
        }
    }

    pub(crate) fn validate(self) -> Result<Self, SimulationError> {
        if self.device_ordinal < 0 {
            return Err(invalid("device ordinal must be nonnegative"));
        }
        if self.precision != Precision::F64 {
            return Err(invalid("only fp64 is qualified"));
        }
        if self.bond_cap <= 0 {
            return Err(invalid("bond cap must be positive"));
        }
        validate_cutoff("absolute cutoff", self.absolute_cutoff)?;
        validate_cutoff("relative cutoff", self.relative_cutoff)?;
        if self.svd_algorithm != SvdAlgorithm::Gesvd {
            return Err(invalid("only GESVD is qualified"));
        }
        if self.gauge != Gauge::Simple {
            return Err(invalid("only SIMPLE gauge is qualified"));
        }
        if !(1..=MAXIMUM_WORKSPACE_BYTES).contains(&self.maximum_workspace_bytes) {
            return Err(invalid("workspace budget is outside the approved range"));
        }
        Ok(self)
    }
}

fn validate_cutoff(label: &'static str, value: f64) -> Result<(), SimulationError> {
    if !value.is_finite() || value < 0.0 {
        Err(invalid(label))
    } else {
        Ok(())
    }
}

fn invalid(reason: &'static str) -> SimulationError {
    SimulationError::InvalidExecutionPolicy { reason }
}

#[cfg(test)]
mod tests {
    use super::{ExecutionPolicy, Gauge, Precision, SvdAlgorithm};
    use crate::simulation::SimulationError;

    fn assert_relative_close(actual: f64, expected: f64) {
        assert!(actual.is_finite());
        assert!(expected.is_finite() && expected != 0.0);
        let relative_error = ((actual - expected) / expected).abs();
        assert!(
            relative_error <= 8.0 * f64::EPSILON,
            "expected {expected:e}, got {actual:e}, relative error {relative_error:e}"
        );
    }

    #[test]
    fn bell_policy_is_explicit_and_valid() {
        let policy = ExecutionPolicy::bell_regression();

        assert_eq!(policy.validate().expect("policy should be valid"), policy);
    }

    #[test]
    fn base_policy_matches_the_approved_b1_values() {
        let policy = ExecutionPolicy::base_qualification();

        assert_eq!(policy.validate().expect("policy should be valid"), policy);
        assert_eq!(policy.device_ordinal, 0);
        assert_eq!(policy.precision, Precision::F64);
        assert_eq!(policy.bond_cap, 128);
        assert_relative_close(policy.absolute_cutoff, 1.0e-10);
        assert_relative_close(policy.relative_cutoff, 1.0e-16);
        assert_eq!(policy.svd_algorithm, SvdAlgorithm::Gesvd);
        assert_eq!(policy.gauge, Gauge::Simple);
        assert_eq!(policy.maximum_workspace_bytes, 68_719_476_736);
    }

    #[test]
    fn b3_policy_uses_the_historical_matched_bond_cutoff() {
        let policy = ExecutionPolicy::b3_matched_bond_qualification();

        assert_eq!(policy.validate().expect("policy should be valid"), policy);
        assert_relative_close(policy.absolute_cutoff, 1.0e-12);
        assert_relative_close(policy.relative_cutoff, 1.0e-16);
        assert_eq!(policy.bond_cap, 128);
    }

    #[test]
    fn b4_policy_changes_only_the_bond_cap() {
        let b3 = ExecutionPolicy::b3_matched_bond_qualification();

        for bond_cap in [32, 64, 128, 256] {
            let policy = ExecutionPolicy::b4_convergence_qualification(bond_cap);
            assert_eq!(policy.validate().expect("policy should be valid"), policy);
            assert_eq!(policy.bond_cap, bond_cap);
            assert_eq!(policy, ExecutionPolicy { bond_cap, ..b3 });
        }
    }

    #[test]
    fn rejects_every_unqualified_or_invalid_setting() {
        let valid = ExecutionPolicy::bell_regression();
        let invalid = [
            ExecutionPolicy {
                device_ordinal: -1,
                ..valid
            },
            ExecutionPolicy {
                precision: Precision::F32,
                ..valid
            },
            ExecutionPolicy {
                bond_cap: 0,
                ..valid
            },
            ExecutionPolicy {
                absolute_cutoff: f64::NAN,
                ..valid
            },
            ExecutionPolicy {
                relative_cutoff: -1.0,
                ..valid
            },
            ExecutionPolicy {
                svd_algorithm: SvdAlgorithm::Gesvdj,
                ..valid
            },
            ExecutionPolicy {
                gauge: Gauge::Free,
                ..valid
            },
            ExecutionPolicy {
                maximum_workspace_bytes: 0,
                ..valid
            },
            ExecutionPolicy {
                maximum_workspace_bytes: super::MAXIMUM_WORKSPACE_BYTES + 1,
                ..valid
            },
        ];

        for policy in invalid {
            assert!(matches!(
                policy.validate(),
                Err(SimulationError::InvalidExecutionPolicy { .. })
            ));
        }
    }
}
