//! The truncation and resource policy of one engine.
//!
//! [`ExecutionPolicy`] fixes the device, precision, SVD cutoffs, SVD
//! algorithm, gauge, workspace budget, the workspace size each compute asks
//! for, whether each compute is captured as the start of the next one, and
//! how often the native state is restarted from its computed MPS, and on
//! which cuTensorNet handle.
//! `validate` accepts only the qualified configuration, fp64 with GESVD and
//! the SIMPLE gauge, and a budget of at most 64 GiB; the other variants exist
//! for tests. It refuses a restart without capture, whose computes would
//! not leave the MPS a restart starts from, and a fresh handle at restart
//! without a restart period, which would never take effect. It does not
//! check capture itself: turning it off is not reachable outside tests and
//! the `qualification` feature (`without_capture`), and gives the same
//! values without truncation, only more slowly. The minimum workspace, the
//! restart and its fresh handle are reachable there too, until the A100's
//! `history_growth` shows which pays.
//! `base_qualification` is the engine's default. Qualifying another
//! configuration takes an A100 qualification case and a change to `validate`.
//!
//! The bond cap χ is not here: each state's capacity, a `tensornet::MpsShape`
//! given to `prepare`, carries a bond for every cut.

use super::{EngineError, memory_workspace::WorkspacePreference};
use std::num::NonZeroUsize;

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
    /// Whether each compute is followed by `cutensornetStateCaptureMPS`, so
    /// that the computed MPS becomes the state's start and the next compute
    /// applies only the operators applied since. Needed because cuTensorNet
    /// otherwise computes every time from |0…0⟩ with every operator: a
    /// terminal that reads after each of `M` measurements of a `G`-gate shot
    /// would pay `O(M·G)` instead of `O(G)` (H₂: M ≈ 1,404, G ≈ 48,000).
    /// Off only to measure that cost (DECISIONS G1).
    pub(super) capture_after_compute: bool,
    /// The scratch size each state compute asks for: cuTensorNet's
    /// recommended size, or its minimum (`CUTENSORNET_WORKSIZE_PREF_MIN`).
    /// Queries and the sampler always ask for the recommended size.
    pub(super) compute_workspace: WorkspacePreference,
    /// Every this many computes, the native state is replaced, before the
    /// next operator, by a new one initialized from the last computed MPS
    /// (`cutensornetStateInitializeMPS`), and the old state and its operator
    /// tensors are released. `None` never restarts. Needs capture.
    pub(super) restart_period: Option<NonZeroUsize>,
    /// Whether each restart also moves the state to a cuTensorNet handle of
    /// its own, created for the new state, and destroys the handle the old
    /// state was on once nothing uses it (never the session's). For the
    /// A100's `history_growth` only: it tests whether what grows with a
    /// shot's history is kept by the handle rather than by the state.
    /// Needs a restart period.
    pub(super) fresh_handle_at_restart: bool,
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
            capture_after_compute: true,
            compute_workspace: WorkspacePreference::Recommended,
            restart_period: None,
            fresh_handle_at_restart: false,
        }
    }

    /// The same policy, with every compute starting again from |0…0⟩: the
    /// path the A100 qualification compares capture with, not a supported
    /// configuration.
    #[cfg(any(test, feature = "qualification"))]
    pub(crate) const fn without_capture(self) -> Self {
        Self {
            capture_after_compute: false,
            ..self
        }
    }

    /// The same policy, with each state compute given the minimum scratch
    /// cuTensorNet accepts instead of the recommended one.
    #[cfg(any(test, feature = "qualification"))]
    pub(crate) const fn with_minimum_compute_workspace(self) -> Self {
        Self {
            compute_workspace: WorkspacePreference::Min,
            ..self
        }
    }

    /// The same policy, with the native state restarted from its computed
    /// MPS every `period` computes.
    #[cfg(any(test, feature = "qualification"))]
    pub(crate) const fn with_restart_period(self, period: Option<NonZeroUsize>) -> Self {
        Self {
            restart_period: period,
            ..self
        }
    }

    /// The same policy, with each restart moving the state to a new
    /// cuTensorNet handle.
    #[cfg(any(test, feature = "qualification"))]
    pub(crate) const fn with_fresh_handle_at_restart(self, fresh: bool) -> Self {
        Self {
            fresh_handle_at_restart: fresh,
            ..self
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
        if self.restart_period.is_some() && !self.capture_after_compute {
            return Err(invalid("a restart needs capture"));
        }
        if self.fresh_handle_at_restart && self.restart_period.is_none() {
            return Err(invalid("a fresh handle at restart needs a restart period"));
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
    use super::{
        ExecutionPolicy, Gauge, NonZeroUsize, Precision, SvdAlgorithm, WorkspacePreference,
    };
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
        assert!(policy.capture_after_compute);
        assert_eq!(policy.compute_workspace, WorkspacePreference::Recommended);
        assert_eq!(policy.restart_period, None);
        assert!(!policy.fresh_handle_at_restart);
        let variant = policy
            .with_minimum_compute_workspace()
            .with_restart_period(NonZeroUsize::new(10))
            .with_fresh_handle_at_restart(true);
        assert_eq!(variant.validate().expect("policy should be valid"), variant);
        let uncaptured = policy.without_capture();
        assert!(!uncaptured.capture_after_compute);
        assert_eq!(
            uncaptured.validate().expect("policy should be valid"),
            uncaptured
        );
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
            valid
                .without_capture()
                .with_restart_period(NonZeroUsize::new(1)),
            valid.with_fresh_handle_at_restart(true),
        ];

        for policy in invalid {
            assert!(matches!(
                policy.validate(),
                Err(EngineError::InvalidExecutionPolicy { .. })
            ));
        }
    }
}
