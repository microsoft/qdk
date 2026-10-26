//! Where a state's time and device memory go, call by call, for the A100
//! qualification's `history_growth`, which looks for what grows with a
//! shot's history (DECISIONS P1–P8 of the perf slice).
//!
//! A state always keeps its [`StateProfile`]: a clock read around each
//! native call costs nanoseconds against the microseconds of the call. Only
//! the `qualification` feature exposes it, through
//! `CuTensorNetEngine::profile`, with the engine variants it is compared on
//! (`Availability::mps_engine_variant`).

use std::{num::NonZeroUsize, time::Duration};

/// The totals of one state's native calls since it was prepared, and the
/// workspace sizes cuTensorNet asked for at the last compute.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(
    not(any(test, feature = "qualification")),
    allow(dead_code, reason = "read by the qualification only")
)]
pub struct StateProfile {
    /// Operators applied: each one's device upload and
    /// `cutensornetStateApplyTensorOperator`.
    pub apply: Duration,
    /// `cutensornetStatePrepare` before each compute.
    pub prepare: Duration,
    /// `cutensornetWorkspaceGetMemorySize` after each prepare.
    pub workspace_query: Duration,
    /// The compute's scratch and output allocations.
    pub allocate: Duration,
    /// `cutensornetStateCompute` and the stream synchronization after it.
    pub compute: Duration,
    /// `cutensornetStateCaptureMPS`.
    pub capture: Duration,
    /// The frees after a compute (its scratch, the previous outputs) and
    /// the bookkeeping of which buffers are live.
    pub free: Duration,
    /// Expectations and samples, from the first native call to the last
    /// release.
    pub reads: Duration,
    /// Restarts: the new state, its initialization, the old state's and
    /// its operator tensors' release.
    pub restart: Duration,
    pub computes: usize,
    pub restarts: usize,
    /// cuTensorNet's minimum scratch for the last compute
    /// (`CUTENSORNET_WORKSIZE_PREF_MIN`), in bytes.
    pub minimum_workspace_bytes: i64,
    /// cuTensorNet's recommended scratch for the last compute, in bytes.
    pub recommended_workspace_bytes: i64,
    /// The device allocations the state holds now, operator tensors
    /// included.
    pub live_allocations: usize,
    /// The device tensors of the operators applied to the native state.
    pub operator_tensors: usize,
}

/// A variant of the engine `Availability::mps_engine` creates, for the A100
/// qualification to compare: not a supported configuration.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(
    not(feature = "qualification"),
    allow(dead_code, reason = "built by the qualification only")
)]
pub struct EngineVariant {
    /// Each compute gets cuTensorNet's minimum scratch instead of its
    /// recommended one.
    pub minimum_workspace: bool,
    /// Restart the native state from its computed MPS every this many
    /// computes.
    pub restart_period: Option<NonZeroUsize>,
}
