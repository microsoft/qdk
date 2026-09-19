#![allow(
    dead_code,
    reason = "the private simulation core becomes live in the consumer integration iteration"
)]

#[path = "library/simulation/branch.rs"]
mod branch;
#[path = "library/simulation/circuit.rs"]
mod circuit;
#[path = "library/simulation/consumer.rs"]
mod consumer;
#[path = "library/simulation/contraction.rs"]
pub(crate) mod contraction;
#[path = "library/simulation/error.rs"]
mod error;
#[path = "library/simulation/ffi.rs"]
mod ffi;
#[path = "library/simulation/memory_workspace.rs"]
pub(crate) mod memory_workspace;
#[path = "library/simulation/mps_execution.rs"]
mod mps_execution;
#[path = "library/simulation/policy.rs"]
mod policy;
#[path = "library/simulation/query.rs"]
mod query;
#[path = "library/simulation/resources.rs"]
pub(crate) mod resources;
#[path = "library/simulation/sampler.rs"]
mod sampler;

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub(crate) use contraction::ContractionApi;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub(crate) use sampler::SamplerApi;

pub(super) use circuit::{Circuit, Gate, SimulationResult, UnitaryOperationConversionError};
#[allow(
    unused_imports,
    reason = "crate-private integration surface for the MPS shot loop"
)]
pub(super) use consumer::{
    CuTensorNetMpsConsumer, CuTensorNetMpsConsumerError, CuTensorNetSampleMatrix,
    collect_sampled_shots,
};
pub(super) use error::SimulationError;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub(super) use ffi::Complex64Abi;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub(super) use mps_execution::{
    MpsExecutionApi, MpsTarget, OutputMetadata, StateF64Attribute, StateU32Configuration,
};
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub(super) use policy::ExecutionPolicy;
pub(super) use sampler::SamplingRequest;

use std::{ffi::c_void, ptr::NonNull};

pub(super) type OpaqueHandle = NonNull<c_void>;
pub(super) type Stream = NonNull<c_void>;
