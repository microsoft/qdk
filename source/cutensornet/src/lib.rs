// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Finding and loading NVIDIA cuTensorNet and the CUDA Runtime at run time.
//!
//! This crate is a step towards running the QDK's tensor-network
//! simulations, the MPS simulator first, on an NVIDIA GPU through
//! cuTensorNet, NVIDIA's tensor-network library, which runs on the CUDA
//! Runtime. Most users have neither, so the QDK must build, install and run
//! without them. The crate therefore never links them: [`discover`] finds
//! and opens the two shared libraries installed on the host, when a GPU run
//! is asked for, looks up the functions of a table per library, and checks
//! the libraries' versions against the audited ones. It returns an
//! [`Availability`] that keeps the libraries loaded, or an
//! [`AvailabilityError`] listing every fault of each library, so a caller
//! can report them or run on the CPU instead. Discovery runs no GPU work.
//! The MPS engine, added in a later change, calls cuTensorNet through these
//! tables.
//!
//! The crate's `README.md` shows the steps of discovery and the faults each
//! one can find.

mod discovery;
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
pub use libraries::report::{LibraryFault, LibraryReport, Version};
