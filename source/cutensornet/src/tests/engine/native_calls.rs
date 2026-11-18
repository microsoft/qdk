// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests of the native calls' own code, which runs without a library. The
//! calls themselves are tested through their caller, the CUDA algebra, on
//! the fake libraries (`src/tests/engine/cuda_algebra.rs`). The fake's
//! rules on cuTensorNet and the CUDA Runtime are also tested directly
//! (`fake_libraries.rs`), as only a direct caller can make the misuse they
//! refuse.

mod complex;
mod fake_libraries;
