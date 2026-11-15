// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Tests of the native calls' own code, which runs without a library. The
//! calls themselves are tested through the engine, on the fake libraries
//! (`src/tests/engine.rs`), except the tensor API, which nothing calls yet:
//! the fake's is tested directly (`fake_libraries.rs`).

mod complex;
mod error;
mod fake_libraries;
