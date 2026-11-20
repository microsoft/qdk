// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Chains of the crate's parts on Qodec's example qodecs, each checked
//! against Qodec's runtime driven directly, as Qodec's own tests drive it:
//!
//! ```text
//! TopIsaDriver >> QodecLayerAdapter(s) >> ActionSimulatorExecutor
//! ```
//!
//! The driver and the direct runtime are written here, on the protocol's
//! roles and on Qodec's runtime; they read one program, in Qodec's REPL
//! syntax.

mod action_simulator_executor;
mod direct_runtime;
mod programs;
mod qodec_layer_adapter;
mod qodecs;
mod top_isa_driver;

/// The seed of every run.
const SEED: u64 = 0x5EED;
