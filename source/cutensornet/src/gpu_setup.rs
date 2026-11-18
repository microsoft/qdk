// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`GpuSetup`]: for a user setting up the GPU for the MPS simulation
//! method, what was found and, when something is missing, what to fix.
//!
//! A GPU run needs several things outside the QDK: Linux x86-64, an NVIDIA
//! driver for CUDA 12, a CUDA device of a compute capability cuTensorNet
//! supports (`devices::MINIMUM_COMPUTE_CAPABILITY`), and NVIDIA's
//! libraries, today cuTensorNet and the CUDA Runtime, at the audited
//! versions (the crate's README says where each was verified). Each can be
//! missing or wrong, and the user who meets the failure is often not the
//! one who knows CUDA. So [`GpuSetup::check`] checks each and answers in
//! one report, and never fails: a caller shows the report to the user on
//! request, and its text with the error of a GPU run that cannot start.
//! The report lists what was found, warns of other copies of a library
//! that could be loaded instead, and, for the first problem met in the
//! order of the steps, says what to do ([`GpuSetup::advice`]).
//!
//! The steps, in order; those after discovery run whenever it succeeds:
//!
//! 1. the libraries: [`discover_with`](crate::discover_with), which runs no
//!    GPU work;
//! 2. the host: a problem of the host a library's report reveals, as its
//!    `SetupGuide` (in `libraries::setup_guide`) says, today the CUDA
//!    Runtime's driver check;
//! 3. the devices: how many CUDA devices the process sees, and the compute
//!    capability and memory of the one the GPU engine runs on, the only
//!    one read (`devices`);
//! 4. the trial run: a [`CudaMpsEngine`](crate::CudaMpsEngine) on that
//!    device prepares one qubit, applies a Hadamard and reads a
//!    probability, the smallest run that touches the device.
//!
//! What the library and host steps show and advise of a library, the
//! library declares in its `SetupGuide`, so a library bound later brings
//! its own; `advice` holds the advice shared by every library and that of
//! the steps after discovery. The device step is the exception: the check
//! holds its facts, cuTensorNet's compute-capability floor and the CUDA
//! Runtime's driver advice.

mod advice;
pub(crate) mod devices;

use crate::engine::cuda_algebra::policy::DEVICE_ORDINAL;
use crate::libraries::setup_guide::{or_unread, setup_guides};
use crate::{AvailabilityError, AvailabilityReport, CudaAlgebraError, PackagedLibraries};
use advice::{availability_advice, device_advice, other_copy_advice, trial_run_advice};
use canonical_mps::CanonicalMpsError;
use devices::DeviceReport;
use std::error::Error;
use std::fmt;

/// The state of this machine's GPU setup, as [`GpuSetup::check`] found it:
/// each step's outcome. Its [`Display`](fmt::Display) is the report a user
/// reads.
#[derive(Debug)]
pub struct GpuSetup {
    /// What the steps found once the libraries loaded, or why discovery
    /// found no usable set, in which case no later step ran.
    pub(crate) loaded: Result<LoadedSetup, AvailabilityError>,
    /// The command that installs the libraries, which the advice gives as
    /// an example: the packager's, from [`PackagedLibraries`].
    pub(crate) install_command: Option<String>,
}

/// The setup of a machine whose libraries loaded: their report, and the
/// outcome of each step run on them.
#[derive(Debug)]
pub(crate) struct LoadedSetup {
    /// The libraries' paths and versions.
    pub(crate) libraries: AvailabilityReport,
    /// How many CUDA devices there are, and the one the GPU engine runs on,
    /// or why they could not be read.
    pub(crate) devices: Result<DeviceReport, CudaAlgebraError>,
    /// Whether the trial run on the GPU engine's device succeeded.
    pub(crate) trial_run: Result<(), CanonicalMpsError<CudaAlgebraError>>,
}

impl GpuSetup {
    /// Checks this machine's GPU setup: discovers the libraries, trying
    /// `packaged` as [`discover_with`](crate::discover_with) does, and when
    /// they load, reads the devices and runs the trial run on the GPU
    /// engine's device. Never fails: a problem is part of the
    /// [`GpuSetup`].
    #[must_use]
    pub fn check(packaged: &PackagedLibraries) -> Self {
        let loaded = crate::discover_with(packaged).map(|availability| LoadedSetup {
            devices: device_report(&availability),
            trial_run: trial_run(&availability),
            libraries: availability.report().clone(),
        });
        Self {
            loaded,
            install_command: packaged.install_command.clone(),
        }
    }

    /// Whether the MPS simulation method can run on the GPU: no problem was
    /// met.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.advice().is_none()
    }

    /// What to do about the first problem met, in words for the user;
    /// `None` when the setup is ready. The problems, in the order they are
    /// looked for: discovery failed (an invalid override first, or else the
    /// first fault of the first library at fault, in the order they are
    /// opened); a library's report reveals a problem of the host (today the
    /// CUDA Runtime's driver check); the devices could not be
    /// read, or the GPU engine's device is missing, or of a compute
    /// capability cuTensorNet does not support; the trial run failed.
    #[must_use]
    pub fn advice(&self) -> Option<String> {
        let loaded = match &self.loaded {
            Err(error) => {
                return Some(availability_advice(error, self.install_command.as_deref()));
            }
            Ok(loaded) => loaded,
        };
        setup_guides()
            .iter()
            .find_map(|guide| (guide.host_problem)(loaded.libraries.library(guide.library)?))
            .or_else(|| device_advice(&loaded.devices))
            .or_else(|| loaded.trial_run.as_ref().err().map(trial_run_advice))
    }

    /// Warnings, whether or not the setup is ready: each other copy of a
    /// library the setup makes visible beside the one loaded, since a
    /// stray copy can be what makes the one loaded fail, with what to do
    /// about it, which depends on whether the one loaded is at fault
    /// (`advice::other_copy_advice`).
    #[must_use]
    pub fn warnings(&self) -> Vec<String> {
        let report = match &self.loaded {
            Ok(LoadedSetup { libraries, .. }) | Err(AvailabilityError::Unusable(libraries)) => {
                libraries
            }
            Err(AvailabilityError::UnsupportedPlatform { .. }) => return Vec::new(),
        };
        setup_guides()
            .iter()
            .filter_map(|guide| Some((guide, report.library(guide.library)?)))
            .flat_map(|(guide, library)| {
                let loaded = or_unread(library.path.as_ref().map(|path| path.display()));
                let fix = other_copy_advice(guide, library);
                library.other_copies.iter().map(move |other| {
                    format!(
                        "another copy of {} is at {}, beside the one loaded, {loaded}: {fix}",
                        guide.library,
                        other.display(),
                    )
                })
            })
            .collect()
    }
}

/// Lists what was found, then the warnings, then the problem and its fix,
/// one item a line.
impl fmt::Display for GpuSetup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.loaded {
            Ok(loaded) => write!(f, "{loaded}")?,
            Err(error) => {
                match error {
                    AvailabilityError::Unusable(libraries) => write_libraries(f, libraries)?,
                    AvailabilityError::UnsupportedPlatform { .. } => {
                        writeln!(f, "libraries: {error}")?;
                    }
                }
                write!(
                    f,
                    "CUDA devices: not read\ntrial run on CUDA device {DEVICE_ORDINAL}: not tried"
                )?;
            }
        }
        for warning in self.warnings() {
            write!(f, "\nwarning: {warning}")?;
        }
        if let Some(advice) = self.advice() {
            write!(f, "\nto fix: {advice}")?;
        }
        Ok(())
    }
}

/// Lists what each step found, one item a line: each library, the
/// devices, the trial run.
impl fmt::Display for LoadedSetup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_libraries(f, &self.libraries)?;
        match &self.devices {
            Ok(DeviceReport { count: 0, .. }) => writeln!(f, "CUDA devices: none")?,
            Ok(DeviceReport {
                count,
                engine_device,
            }) => {
                write!(f, "CUDA devices: {count}")?;
                if let Some(device) = engine_device {
                    let (major, minor) = device.compute_capability;
                    write!(
                        f,
                        "; CUDA device {DEVICE_ORDINAL}, where the GPU engine runs: compute \
                         capability {major}.{minor}, {} of {} GiB free",
                        gibibytes(device.free_bytes),
                        gibibytes(device.total_bytes)
                    )?;
                }
                writeln!(f)?;
            }
            Err(error) => writeln!(f, "CUDA devices: {}", with_sources(error))?,
        }
        match &self.trial_run {
            Ok(()) => write!(f, "trial run on CUDA device {DEVICE_ORDINAL}: ready"),
            Err(error) => write!(
                f,
                "trial run on CUDA device {DEVICE_ORDINAL}: {}",
                with_sources(error)
            ),
        }
    }
}

/// Lists each library of `report` as its guide shows it, then each fault
/// found in it, one item a line: what discovery found, whether or not it
/// succeeded. A library with no path shows only its faults, which say why.
fn write_libraries(f: &mut fmt::Formatter<'_>, report: &AvailabilityReport) -> fmt::Result {
    for guide in setup_guides() {
        let Some(library) = report.library(guide.library) else {
            continue;
        };
        if library.path.is_some() {
            for line in (guide.describe)(library) {
                writeln!(f, "{line}")?;
            }
        }
        for fault in &library.faults {
            writeln!(f, "{} at fault: {fault}", guide.library)?;
        }
    }
    Ok(())
}

/// The devices step: how many devices the CUDA Runtime `availability`
/// loaded sees, and the GPU engine's device.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn device_report(availability: &crate::Availability) -> Result<DeviceReport, CudaAlgebraError> {
    devices::read_devices(availability.libraries().as_ref())
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn device_report(_availability: &crate::Availability) -> Result<DeviceReport, CudaAlgebraError> {
    unreachable!("discovery fails off Linux x86-64")
}

/// The trial run: a [`CudaMpsEngine`](crate::CudaMpsEngine) on its CUDA
/// device prepares one qubit, applies a Hadamard and reads the probability
/// of One.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn trial_run(
    availability: &crate::Availability,
) -> Result<(), CanonicalMpsError<CudaAlgebraError>> {
    use num_complex::Complex64;
    use tensornet::{MpsEngine, MpsShape};

    let engine =
        crate::CudaMpsEngine::try_from(availability).map_err(CanonicalMpsError::Algebra)?;
    let capacity = MpsShape::qubit_chain(1, 1).expect("one qubit is a chain");
    let mut state = engine.prepare(&capacity)?;
    let h = std::f64::consts::FRAC_1_SQRT_2;
    let hadamard = [h, h, h, -h].map(|x| Complex64::new(x, 0.0));
    engine.apply(&mut state, &[0], &hadamard)?;
    engine.probability_of_one(&state, 0)?;
    Ok(())
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn trial_run(
    _availability: &crate::Availability,
) -> Result<(), CanonicalMpsError<CudaAlgebraError>> {
    unreachable!("discovery fails off Linux x86-64")
}

/// `error`, then each error it was caused by, separated by `: `, so a
/// wrapping error does not hide its cause: the engine's error says only
/// that its algebra failed.
fn with_sources(error: &dyn Error) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        text.push_str(": ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text
}

/// `bytes` in GiB, to one decimal.
#[expect(
    clippy::cast_precision_loss,
    reason = "a device's memory, shown to 0.1 GiB"
)]
fn gibibytes(bytes: usize) -> String {
    format!("{:.1}", bytes as f64 / f64::from(1u32 << 30))
}
