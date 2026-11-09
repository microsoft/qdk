// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Running the QDK's tensor-network simulations, the MPS (matrix product
//! state) simulator first, on an NVIDIA GPU, through cuTensorNet, NVIDIA's
//! tensor-network library, without making NVIDIA software part of the QDK's
//! build.
//!
//! An MPS simulation method written against `tensornet::MpsEngine` (gates
//! as matrices on sites; probabilities, projections, forks, samples and
//! expectations back) runs on any engine. This crate gives it one on the
//! GPU, in two halves:
//!
//! - **Finding and loading.** cuTensorNet runs on the CUDA Runtime, and most
//!   users have neither, so the QDK must build, install and run without
//!   them: the crate never links them. [`discover`] finds and opens the two
//!   shared libraries installed on the host, when a GPU run is asked for,
//!   looks up the functions of a table per library, and checks the
//!   libraries' versions against the audited ones. It returns an
//!   [`Availability`] that keeps the libraries loaded, or an
//!   [`AvailabilityError`] listing every fault of each library, so a caller
//!   can report them or run on the CPU instead. Discovery runs no GPU work.
//! - **The MPS engine.** `CuTensorNetEngine::new` (Linux x86-64) opens a
//!   [`CuTensorNetEngine`] on an `Availability`, the `tensornet::MpsEngine`
//!   on the loaded libraries: it holds each state as a cuTensorNet state,
//!   computed only when a read needs it, and refuses a call that breaks the
//!   contract as every engine does ([`EngineError`]).
//!
//! The crate's `docs/loading.md` shows the steps of discovery and the faults
//! each one can find, and its `docs/engine.md` how the engine calls
//! cuTensorNet.

mod discovery;
#[cfg_attr(
    not(all(target_os = "linux", target_arch = "x86_64")),
    allow(
        dead_code,
        reason = "off Linux x86-64 nothing calls the native API; the engine stays compiled so its host tests run"
    )
)]
mod engine;
#[cfg_attr(
    not(all(target_os = "linux", target_arch = "x86_64")),
    allow(
        dead_code,
        reason = "off Linux x86-64 the libraries' loader, tables and probes are compiled for their tests only"
    )
)]
mod libraries;
#[cfg(test)]
mod tests;

pub use discovery::{Availability, discover, error::AvailabilityError, report::AvailabilityReport};
pub use engine::{CuTensorNetEngine, CuTensorNetState, EngineError};
pub use libraries::report::{LibraryFault, LibraryReport, Version};
