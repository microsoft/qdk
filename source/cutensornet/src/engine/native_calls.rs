// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`NativeCalls`]: the calls to the CUDA Runtime and cuTensorNet that a
//! canonical MPS on the GPU needs.
//!
//! The code that makes these calls must run where it is tested, and CI has
//! no GPU. So it makes each native call through these traits, and is the
//! same whichever implementation answers them.
//!
//! ```text
//! NativeCalls            all three traits, which each implementation has
//!   ├─ session_calls.rs  SessionCalls: the devices, stream and handle
//!   ├─ memory_calls.rs   MemoryCalls: device buffers, copies, workspaces
//!   └─ tensor_calls.rs   TensorCalls: tensor descriptors, gate splits,
//!                        QRs, pairwise contractions
//! the values passed      OpaqueHandle, Stream, complex.rs (Complex64Abi)
//! ```
//!
//! Each implementation has one file per trait. A new native call is a
//! method of the trait of its kind, implemented in that trait's file of
//! each implementation.

pub(crate) mod complex;
#[cfg(test)]
pub(crate) mod fake_libraries;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod loaded_libraries;
pub(crate) mod memory_calls;
pub(crate) mod session_calls;
pub(super) mod tensor_calls;

// What the tests under `src/tests` name of these private modules: the size
// of the algebra's buffers, and the traits and arguments of the calls they
// make on the fake libraries and on the loaded ones.
#[cfg(test)]
pub(crate) use self::{
    complex::{Complex64Abi, complex_bytes},
    memory_calls::{MemoryCalls, WorkspaceSlot},
    session_calls::SessionCalls,
    tensor_calls::{
        GateSplitDescriptors, GateSplitOperands, NativeContractionNetwork, SvdSetting,
        SvdTruncation, TensorCalls, TensorModes, TensorOperand,
    },
};

use std::{ffi::c_void, ptr::NonNull};

/// The calls to the CUDA Runtime and cuTensorNet: the session's, the
/// memory's and the tensor API's, in one trait.
pub(crate) trait NativeCalls:
    session_calls::SessionCalls + tensor_calls::TensorCalls
{
}

impl<Calls> NativeCalls for Calls where
    Calls: session_calls::SessionCalls + tensor_calls::TensorCalls + ?Sized
{
}

/// A cuTensorNet handle or object, or a device buffer, as the libraries
/// return it.
pub(crate) type OpaqueHandle = NonNull<c_void>;
/// A CUDA stream.
pub(crate) type Stream = NonNull<c_void>;
