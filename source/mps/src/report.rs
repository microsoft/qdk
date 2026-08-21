// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::{num::NonZeroUsize, time::Duration};

use crate::ExecutionPolicy;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineDescriptor {
    pub name: String,
    pub version: String,
    pub backend: String,
    pub device: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceResolutionSource {
    Environment,
    ProcessVisible,
    InvalidConfigurationFallback,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceResolution {
    pub max_cpu_threads: NonZeroUsize,
    pub source: ResourceResolutionSource,
    pub caller_limit_honored: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineInfo {
    pub descriptor: EngineDescriptor,
    pub resources: ResourceResolution,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapabilityStatus {
    Available,
    Planned { reason: String },
    Unavailable { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MpsCapabilities {
    pub complex64: CapabilityStatus,
    pub maximum_gate_arity: usize,
    pub dynamic_allocation: CapabilityStatus,
    pub measurement_reset: CapabilityStatus,
    pub non_local_routing: CapabilityStatus,
    pub observables: CapabilityStatus,
    pub noise: CapabilityStatus,
    pub discarded_weight_diagnostics: CapabilityStatus,
    pub constrained_cpu_resources: CapabilityStatus,
    pub backend: String,
    pub device: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OperationCounts {
    pub one_qubit: u64,
    pub two_qubit: u64,
    pub measurement: u64,
    pub reset: u64,
    pub observable: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TimingReport {
    pub initialization: Duration,
    pub unitary: Duration,
    pub measurement_reset: Duration,
    pub observable: Duration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapStatus {
    NotConfigured,
    BelowCap,
    ReachedCapIndeterminate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReleaseOutcome {
    pub was_zero: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExecutionReport {
    pub requested_policy: ExecutionPolicy,
    pub engine: EngineInfo,
    pub capabilities: MpsCapabilities,
    pub resolved_seed: u64,
    pub operation_counts: OperationCounts,
    pub timings: TimingReport,
    pub state_norm: f64,
    pub norm_before_first_non_unitary: Option<f64>,
    pub reached_bond_dimension: usize,
    pub cap_status: CapStatus,
    pub local_threshold: Option<f64>,
    pub discarded_weight: CapabilityStatus,
}
