// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Finding and loading NVIDIA cuTensorNet and the CUDA Runtime at run time.
//!
//! The crate never links them, so the QDK builds and runs without them.
//! When a GPU run is asked for, [`discover`] opens the libraries installed
//! on the host. It looks up the functions of a table per library, and
//! checks the libraries' versions against the audited ones. It returns an
//! [`Availability`] that keeps the libraries loaded, or an
//! [`AvailabilityError`] listing every fault of each library. A caller can
//! then report the faults or run on the CPU instead.
//!
//! A later change adds the MPS engine, which calls cuTensorNet through
//! these tables.
//!
//! The crate's README says why. Its `docs/loading.md` shows the steps of
//! discovery and the faults each one can find.

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
mod test_utils;

pub use discovery::{Availability, discover, error::AvailabilityError, report::AvailabilityReport};
pub use libraries::report::{LibraryFault, LibraryReport, Version};
