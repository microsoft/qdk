//! The truncation and resource policy of one engine.
//!
//! [`ExecutionPolicy`] fixes the device, precision, SVD cutoffs, SVD
//! algorithm, gauge, workspace budget, the workspace size each compute asks
//! for, whether each compute is captured as the start of the next one, and
//! how often the native state is restarted from its computed MPS, and on
//! which cuTensorNet handle.
//! `validate` accepts only fp64, whose other variant exists for tests, and a
//! budget of at most 64 GiB. It accepts every [`SvdAlgorithm`] and
//! [`MpsGauge`]: cuTensorNet 2.13 accepts each with the cutoffs set here
//! (see [`SvdAlgorithm`]); the defaults, GESVD and SIMPLE, are M0's, and
//! the others are there for the A100 to compare time and accuracy with
//! them (`EngineVariant`). It refuses a restart without capture, whose
//! computes would not leave the MPS a restart starts from, and a fresh
//! handle at restart without a restart period, which would never take
//! effect. It does not check capture itself: turning it off is not
//! reachable outside tests and the `qualification` feature
//! (`without_capture`), and gives the same values without truncation, only
//! more slowly.
//! `base_qualification` is the engine's default: it restarts each state
//! every [`RESTART_PERIOD`] computes on a fresh handle. Tests and the
//! `qualification` feature may change the workspace preference, the
//! restart period, the fresh handle, the SVD algorithm and the gauge, to
//! measure what each changes on the A100; a policy that does not restart
//! says so (`with_restart_period(None)`,
//! `with_fresh_handle_at_restart(false)`). Qualifying another precision or
//! budget takes an A100 qualification case and a change to `validate`;
//! adopting another SVD algorithm or gauge as the default takes the A100
//! A/B and a change to `base_qualification`.
//!
//! The bond cap χ is not here: each state's capacity, a `tensornet::MpsShape`
//! given to `prepare`, carries a bond for every cut.

#[cfg(any(test, feature = "qualification"))]
use super::profile::EngineVariant;
use super::{EngineError, memory_workspace::WorkspacePreference};
use std::num::NonZeroUsize;

pub(super) const MAXIMUM_WORKSPACE_BYTES: usize = 68_719_476_736;

/// The computes after which the engine's policy restarts a state from its
/// computed MPS, on a cuTensorNet handle of its own. Needed because the
/// workspace cuTensorNet recommends for a compute grows by about 16.06 MiB
/// with each compute done on a long-lived handle, restarts on the same
/// handle included: about 44 GiB at the end of an H₂ shot (two computes per
/// measurement, 1,404 measurements), more than the A100 could give: the
/// first H₂ shot failed at its 1,252nd measurement, asking for 42 GB. On
/// the A100 (`history_growth`, 300 steps of H₂'s pattern, one compute
/// each), a restart every 10 computes on a fresh handle kept the
/// recommended workspace flat at 193 MiB, where every variant on the
/// engine's handle reached 4,831 MiB after 300 computes, for about 15 ms
/// per restart, 1% of the time, with P(One) identical. A larger period
/// raises that bound by about 16 MiB per compute; a smaller one pays more
/// restarts. Why the handle grows is not known: a new state on the old
/// handle asks for 48 MiB, so the growth needs computes on a long-lived
/// handle.
pub(super) const RESTART_PERIOD: usize = 10;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Precision {
    #[cfg(test)]
    F32,
    F64,
}

/// The cuSOLVER routine with which cuTensorNet factorizes each truncated
/// bond (`CUTENSORNET_STATE_CONFIG_MPS_SVD_ALGO`). Each is configured with
/// the library's default parameters (`..._SVD_ALGO_PARAMS` is not set).
///
/// The engine sets an absolute and a relative cutoff and no
/// discarded-weight cutoff, so cuTensorNet 2.13 accepts all four: its
/// header forbids only a discarded-weight cutoff with GESVDR. A policy that
/// adds one must refuse it with [`SvdAlgorithm::Gesvdr`] in `validate`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(
    not(any(test, feature = "qualification")),
    allow(dead_code, reason = "only the qualification picks another algorithm")
)]
pub enum SvdAlgorithm {
    /// `cusolverDnGesvd`, the QR-based SVD. The default because it is M0's
    /// choice and cuTensorNet's default, not because it was measured
    /// against the others: on the A100, H₂ costs about 38 ms per operator
    /// once its bonds reach χ = 32, and GESVD is expected to be slow on
    /// matrices that small. The A100 A/B of the algorithm and the gauge on
    /// H₂ decides whether another one replaces it.
    #[default]
    Gesvd,
    /// `cusolverDnGesvdj`, Jacobi sweeps, with the library's default
    /// tolerance and sweep limit. Expected to be faster than GESVD on small
    /// matrices; not yet measured here.
    Gesvdj,
    /// `cusolverDnXgesvdp`, through a polar decomposition. It may lose
    /// accuracy on a rank-deficient matrix, as a two-site update's often
    /// is; the A/B's readout check tells.
    Gesvdp,
    /// `cusolverDnXgesvdr`, a randomized SVD of the rank the bond allows,
    /// with the library's default oversampling and iterations. Its singular
    /// values are approximate, and it pays off when that rank is far below
    /// the matrix's; a two-site update at χ keeps χ of 2χ.
    Gesvdr,
}

/// How cuTensorNet keeps the MPS gauge between its two-site updates
/// (`CUTENSORNET_STATE_CONFIG_MPS_GAUGE_OPTION`).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(
    not(any(test, feature = "qualification")),
    allow(dead_code, reason = "only the qualification picks another gauge")
)]
pub enum MpsGauge {
    /// The simple-update gauge, which cuTensorNet documents as improving
    /// accuracy, for some work per gate. The default because it is M0's
    /// choice, not because its cost or gain was measured; the A100 A/B of
    /// the algorithm and the gauge on H₂ decides whether
    /// [`MpsGauge::Free`] replaces it.
    #[default]
    Simple,
    /// No gauge, cuTensorNet's own default.
    Free,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ExecutionPolicy {
    pub(crate) device_ordinal: i32,
    pub(super) precision: Precision,
    pub(super) absolute_cutoff: f64,
    pub(super) relative_cutoff: f64,
    pub(super) svd_algorithm: SvdAlgorithm,
    pub(super) gauge: MpsGauge,
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
    /// tensors are released. [`RESTART_PERIOD`] by default; `None` never
    /// restarts. Needs capture.
    pub(super) restart_period: Option<NonZeroUsize>,
    /// Whether each restart also moves the state to a cuTensorNet handle of
    /// its own, created for the new state, and destroys the handle the old
    /// state was on once nothing uses it (never the session's). On by
    /// default: what grows with a shot's history is kept by the handle,
    /// not by the state ([`RESTART_PERIOD`]). Needs a restart period.
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
            gauge: MpsGauge::Simple,
            maximum_workspace_bytes: MAXIMUM_WORKSPACE_BYTES,
            capture_after_compute: true,
            compute_workspace: WorkspacePreference::Recommended,
            restart_period: NonZeroUsize::new(RESTART_PERIOD),
            fresh_handle_at_restart: true,
        }
    }

    /// The same policy, with every compute starting again from |0…0⟩: the
    /// path the A100 qualification compares capture with, not a supported
    /// configuration. A restart needs capture, so the policy must also not
    /// restart (`with_restart_period(None)` and
    /// `with_fresh_handle_at_restart(false)`).
    #[cfg(any(test, feature = "qualification"))]
    pub(crate) const fn without_capture(self) -> Self {
        Self {
            capture_after_compute: false,
            ..self
        }
    }

    /// The same policy, with each state compute given the minimum scratch
    /// cuTensorNet accepts instead of the recommended one.
    #[cfg(test)]
    pub(crate) const fn with_minimum_compute_workspace(self) -> Self {
        Self {
            compute_workspace: WorkspacePreference::Min,
            ..self
        }
    }

    /// The same policy, with the native state restarted from its computed
    /// MPS every `period` computes, or never with `None`.
    #[cfg(any(test, feature = "qualification"))]
    pub(crate) const fn with_restart_period(self, period: Option<NonZeroUsize>) -> Self {
        Self {
            restart_period: period,
            ..self
        }
    }

    /// The same policy, with each restart moving the state to a new
    /// cuTensorNet handle, or keeping it on its handle with `false`.
    #[cfg(any(test, feature = "qualification"))]
    pub(crate) const fn with_fresh_handle_at_restart(self, fresh: bool) -> Self {
        Self {
            fresh_handle_at_restart: fresh,
            ..self
        }
    }

    /// The same policy, with each bond factorized by `algorithm`. The
    /// qualification sets it through `with_variant`.
    #[cfg(test)]
    pub(crate) const fn with_svd_algorithm(self, algorithm: SvdAlgorithm) -> Self {
        Self {
            svd_algorithm: algorithm,
            ..self
        }
    }

    /// The same policy, with the MPS kept in `gauge`. The qualification
    /// sets it through `with_variant`.
    #[cfg(test)]
    pub(crate) const fn with_gauge(self, gauge: MpsGauge) -> Self {
        Self { gauge, ..self }
    }

    /// The same policy, changed as the qualification's `variant` says:
    /// `EngineVariant::default()` leaves the engine's policy as it is.
    #[cfg(any(test, feature = "qualification"))]
    pub(crate) const fn with_variant(self, variant: EngineVariant) -> Self {
        Self {
            svd_algorithm: variant.svd_algorithm,
            gauge: variant.gauge,
            compute_workspace: if variant.minimum_workspace {
                WorkspacePreference::Min
            } else {
                WorkspacePreference::Recommended
            },
            restart_period: variant.restart_period,
            fresh_handle_at_restart: variant.fresh_handle_at_restart,
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
        ExecutionPolicy, MpsGauge, NonZeroUsize, Precision, RESTART_PERIOD, SvdAlgorithm,
        WorkspacePreference,
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
        assert_eq!(policy.gauge, MpsGauge::Simple);
        assert_eq!(policy.svd_algorithm, SvdAlgorithm::default());
        assert_eq!(policy.gauge, MpsGauge::default());
        assert_eq!(policy.maximum_workspace_bytes, 68_719_476_736);
        assert!(policy.capture_after_compute);
        assert_eq!(policy.compute_workspace, WorkspacePreference::Recommended);
        assert_eq!(policy.restart_period, NonZeroUsize::new(10));
        assert_eq!(policy.restart_period, NonZeroUsize::new(RESTART_PERIOD));
        assert!(policy.fresh_handle_at_restart);
        let variant = policy
            .with_minimum_compute_workspace()
            .with_restart_period(NonZeroUsize::new(50))
            .with_fresh_handle_at_restart(false);
        assert_eq!(variant.validate().expect("policy should be valid"), variant);
        let without_restart = policy
            .with_restart_period(None)
            .with_fresh_handle_at_restart(false);
        assert_eq!(
            without_restart.validate().expect("policy should be valid"),
            without_restart
        );
        let uncaptured = without_restart.without_capture();
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
                maximum_workspace_bytes: 0,
                ..valid
            },
            ExecutionPolicy {
                maximum_workspace_bytes: super::MAXIMUM_WORKSPACE_BYTES + 1,
                ..valid
            },
            valid.without_capture(),
            valid.with_fresh_handle_at_restart(false).without_capture(),
            valid.with_restart_period(None),
        ];

        for policy in invalid {
            assert!(matches!(
                policy.validate(),
                Err(EngineError::InvalidExecutionPolicy { .. })
            ));
        }
    }

    #[test]
    fn accepts_every_svd_algorithm_with_every_gauge() {
        let valid = ExecutionPolicy::base_qualification();
        for algorithm in [
            SvdAlgorithm::Gesvd,
            SvdAlgorithm::Gesvdj,
            SvdAlgorithm::Gesvdp,
            SvdAlgorithm::Gesvdr,
        ] {
            for gauge in [MpsGauge::Simple, MpsGauge::Free] {
                let policy = valid.with_svd_algorithm(algorithm).with_gauge(gauge);
                assert_eq!((policy.svd_algorithm, policy.gauge), (algorithm, gauge));
                assert_eq!(policy.validate().expect("policy should be valid"), policy);
            }
        }
    }
}
