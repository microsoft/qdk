// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Full-pipeline snapshot tests that stress the cross-package FIR transforms.
//!
//! Each test compiles a sample (pulled in from the `samples/` tree via
//! [`include_str!`]), runs the complete FIR transform pipeline, and snapshots
//! the result. `shor` additionally pins the one simulation-only intrinsic
//! (`DrawRandomInt`) to a constant so the rest of the algorithm stays intact.
//!
//! Unlike most snapshot tests in this crate — which render a single (user)
//! package — these render **every item reachable from the entry expression
//! across all packages** via
//! [`crate::pretty::write_reachable_qsharp_parseable`], so the transformed
//! library/dependency callables appear in the snapshot alongside the user
//! callables.

mod grover;
mod shor;
