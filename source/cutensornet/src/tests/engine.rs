// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests of the engine's GPU code, without a GPU: the CUDA algebra (with
//! the device session it opens and the cleanups its error's rules govern,
//! through the algebra's methods), and the native calls' own code and the
//! fake's rules, on fake libraries that simulate the native API on the CPU
//! (`src/engine/native_calls/fake_libraries.rs`) and enforce cuTensorNet's
//! call order. The engine itself is `canonical_mps`'s, tested there; its
//! circuits run on a GPU (`tests/cuda_mps_engine_on_the_gpu.rs`).

mod cuda_algebra;
mod native_calls;
