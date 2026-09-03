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
#[path = "library/simulation/error.rs"]
mod error;
#[path = "library/simulation/ffi.rs"]
mod ffi;
#[path = "library/simulation/policy.rs"]
mod policy;
#[path = "library/simulation/query.rs"]
mod query;
#[path = "library/simulation/replay.rs"]
mod replay;

pub(super) use circuit::{Circuit, Gate, SimulationResult};
#[allow(
    unused_imports,
    reason = "crate-private integration surface for the MPS shot loop"
)]
pub(super) use consumer::{CuTensorNetMpsConsumer, CuTensorNetSampleMatrix};
pub(super) use error::SimulationError;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub(super) use ffi::Complex64Abi;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub(super) use policy::ExecutionPolicy;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub(super) use replay::{
    MpsTarget, OutputMetadata, ReplayApi, StateF64Attribute, StateU32Configuration,
};

use std::{ffi::c_void, ptr::NonNull};

pub(super) type OpaqueHandle = NonNull<c_void>;
pub(super) type Stream = NonNull<c_void>;
