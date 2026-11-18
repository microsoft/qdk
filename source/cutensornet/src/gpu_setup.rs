// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`GpuSetup`]: for a user setting up the GPU for the MPS simulation
//! method, what was found and, when something is missing, what to fix.
//!
//! A GPU run needs several things outside the QDK: Linux x86-64, an NVIDIA
//! driver for CUDA 12, a CUDA device of compute capability 7.5 or later
//! (`devices::MINIMUM_COMPUTE_CAPABILITY`), and NVIDIA's libraries, today
//! cuTensorNet and the CUDA Runtime, at the audited versions (the crate's
//! README says where each was verified). Each can be missing or wrong, and
//! the user who meets the failure is often not the one who knows CUDA. So
//! [`GpuSetup::check`] checks each and answers in one report, and never
//! fails: a caller shows the report to the user on request, and its text
//! with the error of a GPU run that cannot start. The report lists what was
//! found, warns of other copies of a library that could be loaded instead,
//! and, for the first problem met in the order of the steps, says what to
//! do ([`GpuSetup::advice`]).
//!
//! The steps, each run only when the one before it succeeded:
//!
//! 1. the libraries: [`discover_with`](crate::discover_with), which runs no
//!    GPU work;
//! 2. the host: each library's report may reveal a problem of the host,
//!    today a driver that supports no CUDA 12 (its `SetupGuide`, in
//!    `libraries::setup_guide`);
//! 3. the devices: each CUDA device's compute capability and memory
//!    (`devices`);
//! 4. the trial run: a [`CudaMpsEngine`](crate::CudaMpsEngine) on CUDA
//!    device 0 prepares one qubit, applies a Hadamard and reads a
//!    probability, the smallest run that touches the device.
//!
//! What the report says of each library, and what it advises about one, is
//! the library's to declare, so a library bound later brings its own; the
//! advice's words are in `advice`.

mod advice;
pub(crate) mod devices;

use crate::libraries::setup_guide::or_unread;
use crate::{AvailabilityError, AvailabilityReport, CudaAlgebraError, PackagedLibraries};
use advice::{availability_advice, gibibytes, setup_guides};
use canonical_mps::CanonicalMpsError;
use devices::{DeviceReport, MINIMUM_COMPUTE_CAPABILITY};
use std::{error::Error, fmt};

/// The state of this machine's GPU setup, as [`GpuSetup::check`] found it:
/// each step's outcome. Its [`Display`](fmt::Display) is the report a user
/// reads.
#[derive(Debug)]
pub struct GpuSetup {
    /// The libraries' paths and versions, or why discovery found no usable
    /// set.
    pub(crate) availability: Result<AvailabilityReport, AvailabilityError>,
    /// Every CUDA device, or why they could not be read: `None` when
    /// discovery failed, so none were read.
    pub(crate) devices: Option<Result<Vec<DeviceReport>, CudaAlgebraError>>,
    /// Whether the trial run on CUDA device 0 succeeded: `None` when
    /// discovery failed, so it was not tried.
    pub(crate) trial_run: Option<Result<(), CanonicalMpsError<CudaAlgebraError>>>,
    /// The command that installs the libraries, which the advice gives as
    /// an example: the packager's, from [`PackagedLibraries`].
    pub(crate) install_command: Option<String>,
}

impl GpuSetup {
    /// Checks this machine's GPU setup: discovers the libraries, trying
    /// `packaged` as [`discover_with`](crate::discover_with) does, and when
    /// they load, reads the devices and runs the trial run on CUDA device
    /// 0. Never fails: a problem is part of the [`GpuSetup`].
    #[must_use]
    pub fn check(packaged: &PackagedLibraries) -> Self {
        let install_command = packaged.install_command.clone();
        match crate::discover_with(packaged) {
            Err(error) => Self {
                availability: Err(error),
                devices: None,
                trial_run: None,
                install_command,
            },
            Ok(availability) => Self {
                devices: Some(read_devices(&availability)),
                trial_run: Some(trial_run(&availability)),
                availability: Ok(availability.report().clone()),
                install_command,
            },
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
    /// opened); a library reveals a problem of the host (today, a driver
    /// that supports no CUDA 12); the devices could not be read, or there
    /// are none; CUDA device 0, where the GPU engine runs, has a compute
    /// capability below 7.5; the trial run failed.
    #[must_use]
    pub fn advice(&self) -> Option<String> {
        const DRIVER: &str = concat!(
            "check that `nvidia-smi` lists a GPU, and that the NVIDIA driver supports CUDA 12 (",
            crate::libraries::cuda_runtime::minimum_driver!(),
            " or later)"
        );
        let report = match &self.availability {
            Err(error) => {
                return Some(availability_advice(error, self.install_command.as_deref()));
            }
            Ok(report) => report,
        };
        let host_problem = setup_guides().iter().find_map(|guide| {
            let library = report.library(guide.library)?;
            (guide.host_problem)(library)
        });
        if host_problem.is_some() {
            return host_problem;
        }
        match &self.devices {
            Some(Ok(devices)) => match devices.first() {
                None => return Some(format!("no CUDA device was found: {DRIVER}")),
                Some(device) if !device.is_supported() => {
                    let (major, minor) = device.compute_capability;
                    let (needed_major, needed_minor) = MINIMUM_COMPUTE_CAPABILITY;
                    return Some(format!(
                        "CUDA device 0, where the GPU engine runs, has compute capability \
                         {major}.{minor}; cuTensorNet needs {needed_major}.{needed_minor} or \
                         later"
                    ));
                }
                Some(_) => {}
            },
            Some(Err(_)) | None => {
                return Some(format!("the CUDA devices could not be read: {DRIVER}"));
            }
        }
        match &self.trial_run {
            Some(Ok(())) => None,
            Some(Err(_)) | None => Some(format!(
                "the libraries load, but the trial run failed on CUDA device 0: {DRIVER}"
            )),
        }
    }

    /// Warnings that leave the setup ready: each other copy of a library
    /// the setup makes visible beside the one loaded, with how to keep only
    /// one: when the loaded copy is the override's, remove the other or
    /// unset the variable; else remove one or set the variable.
    #[must_use]
    pub fn warnings(&self) -> Vec<String> {
        let Ok(report) = &self.availability else {
            return Vec::new();
        };
        setup_guides()
            .iter()
            .filter_map(|guide| Some((guide, report.library(guide.library)?)))
            .flat_map(|(guide, library)| {
                let loaded = or_unread(library.path.as_ref().map(|path| path.display()));
                let fix = if library.loaded_from_override {
                    format!("remove the other, or unset {}", guide.variable)
                } else {
                    format!("remove one, or set {} to the one to load", guide.variable)
                };
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
        match &self.availability {
            Ok(report) => {
                for guide in setup_guides() {
                    let Some(library) = report.library(guide.library) else {
                        continue;
                    };
                    for line in (guide.describe)(library) {
                        writeln!(f, "{line}")?;
                    }
                }
            }
            Err(error) => writeln!(f, "libraries: {error}")?,
        }
        match &self.devices {
            Some(Ok(devices)) if devices.is_empty() => writeln!(f, "CUDA devices: none")?,
            Some(Ok(devices)) => {
                for device in devices {
                    let (major, minor) = device.compute_capability;
                    writeln!(
                        f,
                        "CUDA device {}: compute capability {major}.{minor}, {} of {} GiB free",
                        device.ordinal,
                        gibibytes(device.free_bytes),
                        gibibytes(device.total_bytes)
                    )?;
                }
            }
            Some(Err(error)) => writeln!(f, "CUDA devices: {}", with_sources(error))?,
            None => writeln!(f, "CUDA devices: not read")?,
        }
        match &self.trial_run {
            Some(Ok(())) => write!(f, "trial run on CUDA device 0: ready")?,
            Some(Err(error)) => write!(f, "trial run on CUDA device 0: {}", with_sources(error))?,
            None => write!(f, "trial run: not tried")?,
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

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn read_devices(availability: &crate::Availability) -> Result<Vec<DeviceReport>, CudaAlgebraError> {
    DeviceReport::read_all(availability)
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn read_devices(
    _availability: &crate::Availability,
) -> Result<Vec<DeviceReport>, CudaAlgebraError> {
    unreachable!("discovery fails off Linux x86-64")
}

/// The trial run: a [`CudaMpsEngine`](crate::CudaMpsEngine) on CUDA device
/// 0 prepares one qubit, applies a Hadamard and reads the probability of
/// One.
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
