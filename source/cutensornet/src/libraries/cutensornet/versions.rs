// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! cuTensorNet's versions: its own, and the CUDA Runtime's it was built
//! against. Each type is the pointer type of the C function of its name.

pub(crate) type cutensornetGetVersion = unsafe extern "C" fn() -> usize;

pub(crate) type cutensornetGetCudartVersion = unsafe extern "C" fn() -> usize;
