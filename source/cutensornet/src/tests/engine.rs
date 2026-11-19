// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests of the engine's GPU code, on fake libraries that simulate the
//! native API on the CPU (`src/engine/native_calls/fake_libraries.rs`),
//! which enforce cuTensorNet's call order: the CUDA algebra and the error
//! it returns, the device session it runs in, and the native calls' own
//! code and the fake's rules.
//! The engine itself is `canonical_mps`'s, tested there; its circuits run
//! on a GPU (`tests/cuda_algebra_on_the_gpu.rs`).

mod cuda_algebra;
mod cuda_algebra_error;
mod device_session;
mod native_calls;
