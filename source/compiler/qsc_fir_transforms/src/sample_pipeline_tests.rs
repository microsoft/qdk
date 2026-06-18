// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Full-pipeline snapshot tests over the `Dynamics.qs` and `Grover.qs`
//! samples.
//!
//! Each test compiles the corresponding sample (pulled in directly from the
//! `samples/` tree via [`include_str!`]), runs the complete FIR transform
//! pipeline, and snapshots the result.
//!
//! Unlike most snapshot tests in this crate — which render a single (user)
//! package — these render **every item reachable from the entry expression
//! across all packages** via
//! [`crate::pretty::write_reachable_qsharp_parseable`], so the transformed
//! library/dependency callables appear in the snapshot alongside the user
//! callables.

mod dynamics;
mod grover;
