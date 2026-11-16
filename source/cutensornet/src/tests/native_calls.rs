// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests of the native calls' own code, which runs without a library. The
//! calls themselves are tested through their callers, on the fake
//! libraries: the state API through the engine (`src/tests/engine.rs`), the
//! tensor API through the CUDA algebra (`src/tests/cuda_algebra.rs`). The
//! fake's rules on the tensor API, and on the streams it shares with the
//! state API, are also tested directly (`fake_libraries.rs`), as only a
//! direct caller can make the misuse they refuse.

mod complex;
mod error;
mod fake_libraries;
