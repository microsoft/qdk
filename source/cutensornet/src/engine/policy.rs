//! The truncation and resource policy of one engine.
//!
//! [`ExecutionPolicy`] fixes the device, precision, SVD cutoffs, SVD
//! algorithm, gauge and workspace budget. `validate` accepts only the
//! qualified configuration, fp64 with GESVD and the SIMPLE gauge, and a budget
//! of at most 64 GiB; the other variants exist for tests.
//! `base_qualification` is the engine's default. Qualifying another
//! configuration takes an A100 qualification case and a change to `validate`.
//!
//! The bond cap χ is not here: each state's capacity, a `tensornet::MpsShape`
//! given to `prepare`, carries a bond for every cut.

use super::EngineError;

pub(super) const MAXIMUM_WORKSPACE_BYTES: usize = 68_719_476_736;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Precision {
    #[cfg(test)]
    F32,
    F64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SvdAlgorithm {
    Gesvd,
    #[cfg(test)]
    Gesvdj,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Gauge {
    Simple,
    #[cfg(test)]
    Free,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ExecutionPolicy {
    pub(crate) device_ordinal: i32,
    pub(super) precision: Precision,
    pub(super) absolute_cutoff: f64,
    pub(super) relative_cutoff: f64,
    pub(super) svd_algorithm: SvdAlgorithm,
    pub(super) gauge: Gauge,
    pub(super) maximum_workspace_bytes: usize,
}

impl ExecutionPolicy {
    pub(crate) const fn base_qualification() -> Self {
        Self {
            device_ordinal: 0,
            precision: Precision::F64,
            absolute_cutoff: 1.0e-10,
            relative_cutoff: 1.0e-16,
            svd_algorithm: SvdAlgorithm::Gesvd,
            gauge: Gauge::Simple,
            maximum_workspace_bytes: MAXIMUM_WORKSPACE_BYTES,
        }
    }

    pub(crate) fn validate(self) -> Result<Self, EngineError> {
        if self.device_ordinal < 0 {
            return Err(invalid("device ordinal must be nonnegative"));
        }
        if self.precision != Precision::F64 {
            return Err(invalid("only fp64 is qualified"));
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

fn validate_cutoff(label: &'static str, value: f64) -> Result<(), EngineError> {
    if !value.is_finite() || value < 0.0 {
        Err(invalid(label))
    } else {
        Ok(())
    }
}

fn invalid(reason: &'static str) -> EngineError {
    EngineError::InvalidExecutionPolicy { reason }
}

#[cfg(test)]
mod tests {
    use super::{ExecutionPolicy, Gauge, Precision, SvdAlgorithm};
    use crate::engine::EngineError;

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
    fn base_policy_matches_the_approved_b1_values() {
        let policy = ExecutionPolicy::base_qualification();

        assert_eq!(policy.validate().expect("policy should be valid"), policy);
        assert_eq!(policy.device_ordinal, 0);
        assert_eq!(policy.precision, Precision::F64);
        assert_relative_close(policy.absolute_cutoff, 1.0e-10);
        assert_relative_close(policy.relative_cutoff, 1.0e-16);
        assert_eq!(policy.svd_algorithm, SvdAlgorithm::Gesvd);
        assert_eq!(policy.gauge, Gauge::Simple);
        assert_eq!(policy.maximum_workspace_bytes, 68_719_476_736);
    }

    #[test]
    fn rejects_every_unqualified_or_invalid_setting() {
        let valid = ExecutionPolicy::base_qualification();
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
                Err(EngineError::InvalidExecutionPolicy { .. })
            ));
        }
    }
}
