// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The CUDA Runtime's streams, the queues the GPU runs its work from: their
//! creation, the wait for their work, and their destruction. Each type is the
//! pointer type of the C function of its name.

use super::cudart_12;
use std::ffi::c_uint;

pub(crate) type cudaStreamCreateWithFlags =
    unsafe extern "C" fn(*mut cudart_12::cudaStream_t, c_uint) -> cudart_12::cudaError_t;

pub(crate) type cudaStreamSynchronize =
    unsafe extern "C" fn(cudart_12::cudaStream_t) -> cudart_12::cudaError_t;

pub(crate) type cudaStreamDestroy =
    unsafe extern "C" fn(cudart_12::cudaStream_t) -> cudart_12::cudaError_t;
