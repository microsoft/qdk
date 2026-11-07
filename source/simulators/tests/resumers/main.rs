// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The tests of the resumers of the example `qir_resumers`
//! (`examples/qir_resumers/`), written as any implementer of
//! `qdk_simulators::qir::QirResumer` would write one, each run through the
//! conformance kit (`qir::conformance`) and its own tests: a resumer that
//! runs each shot on a simulator of the crate
//! ([`simulator_resumer::QirSimulatorResumer`]), and one that runs its
//! shots together, grouped by their next batch
//! ([`grouping_resumer::QirGroupingResumer`]).

#[path = "../../examples/qir_resumers/grouping_resumer.rs"]
mod grouping_resumer;
mod programs;
#[path = "../../examples/qir_resumers/simulator_resumer.rs"]
mod simulator_resumer;
mod tests;

/// The bytecode's encoding, which the crate keeps to itself: the tests'
/// programs are written in it.
#[allow(dead_code)]
#[path = "../../src/bytecode/encoding.rs"]
mod encoding;
