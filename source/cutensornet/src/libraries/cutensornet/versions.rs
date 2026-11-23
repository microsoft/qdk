// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! cuTensorNet's versions: its own, and the CUDA Runtime's it was built
//! against. Each `XFn` is the type of `cutensornetX`.

pub(crate) type GetVersionFn = unsafe extern "C" fn() -> usize;

pub(crate) type GetCudartVersionFn = unsafe extern "C" fn() -> usize;
