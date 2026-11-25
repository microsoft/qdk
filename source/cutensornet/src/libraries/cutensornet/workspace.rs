// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! cuTensorNet's workspace: the descriptor of the scratch memory its other
//! calls ask for, the size they need, and the memory given to them. Each type
//! is the pointer type of the C function of its name.

use super::v2_13;
use std::ffi::c_void;

pub(crate) type cutensornetCreateWorkspaceDescriptor =
    unsafe extern "C" fn(
        v2_13::cutensornetHandle_t,
        *mut v2_13::cutensornetWorkspaceDescriptor_t,
    ) -> v2_13::cutensornetStatus_t;

pub(crate) type cutensornetDestroyWorkspaceDescriptor =
    unsafe extern "C" fn(v2_13::cutensornetWorkspaceDescriptor_t) -> v2_13::cutensornetStatus_t;

pub(crate) type cutensornetWorkspaceGetMemorySize =
    unsafe extern "C" fn(
        v2_13::cutensornetHandle_t,
        v2_13::cutensornetWorkspaceDescriptor_t,
        v2_13::cutensornetWorksizePref_t,
        v2_13::cutensornetMemspace_t,
        v2_13::cutensornetWorkspaceKind_t,
        *mut i64,
    ) -> v2_13::cutensornetStatus_t;

pub(crate) type cutensornetWorkspaceSetMemory = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    v2_13::cutensornetWorkspaceDescriptor_t,
    v2_13::cutensornetMemspace_t,
    v2_13::cutensornetWorkspaceKind_t,
    *mut c_void,
    i64,
) -> v2_13::cutensornetStatus_t;
