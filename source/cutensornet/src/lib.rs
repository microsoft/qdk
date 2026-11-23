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
//! GPU, in three parts:
//!
//! - **The MPS engine.** [`CudaMpsEngine`] is `canonical_mps`'s
//!   canonical-form MPS engine on [`CudaAlgebra`], its `SiteAlgebra` on
//!   cuTensorNet's tensor API: the sites are [`DeviceTensor`]s in device
//!   memory, split and moved by cuTensorNet. `CudaMpsEngine::try_from`
//!   (Linux x86-64) opens one on the [`Availability`] discovery returns; a
//!   failure of the algebra is a [`CudaAlgebraError`].
//! - **Finding and loading.** cuTensorNet runs on the CUDA Runtime, and most
//!   users have neither, so the QDK must build, install and run without
//!   them: the crate never links them. When a GPU run is asked for,
//!   [`discover`] opens the libraries installed on the host, looks up the
//!   functions of a table per library, and checks the libraries' versions
//!   against the audited ones; [`discover_with`] also tries the files a
//!   package manager installed ([`PackagedLibraries`]). It returns an
//!   [`Availability`] that keeps the libraries loaded, or an
//!   [`AvailabilityError`] listing every fault of each library, so a caller
//!   can report them or run on the CPU instead.
//! - **The GPU setup check.** A GPU run needs a driver, a device and the
//!   libraries, each outside the QDK. [`GpuSetup::check`] checks each in
//!   turn, from discovery to a trial run on the engine's device, and never
//!   fails: its report lists what was found and, for the first problem
//!   met, what to fix.
//!
//! The crate's README says why; its `docs/engine.md` shows how the algebra
//! calls cuTensorNet, and its `docs/loading.md` the steps of discovery and
//! the faults each one can find.

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
        reason = "off Linux x86-64 discovery fails, so the check never reads a device"
    )
)]
mod gpu_setup;
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

pub use discovery::{
    Availability, discover, discover_with, error::AvailabilityError, report::AvailabilityReport,
};
pub use engine::{CudaAlgebra, CudaAlgebraError, CudaMpsEngine, DeviceTensor};
pub use gpu_setup::GpuSetup;
pub use libraries::{
    PackagedLibraries,
    report::{LibraryFault, LibraryReport, LibrarySource, Version},
};
