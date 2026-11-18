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
//! 2. the host: each library's report may reveal a problem of the host,
//!    today a driver that supports no CUDA 12 (its `SetupGuide`, in
//!    `libraries::setup_guide`);
//! 3. the devices: how many CUDA devices the process sees, and the compute
//!    capability and memory of the one the GPU engine runs on, the only
//!    one read (`devices`);
//! 4. the trial run: a [`CudaMpsEngine`](crate::CudaMpsEngine) on that
//!    device prepares one qubit, applies a Hadamard and reads a
//!    probability, the smallest run that touches the device.
//!
//! What is particular to a library, in the report and in the advice, the
//! library declares in its `SetupGuide`, so a library bound later brings
//! its own; the words of every step's advice are in `advice`.

mod advice;
pub(crate) mod devices;

use crate::engine::cuda_algebra::policy::DEVICE_ORDINAL;
use crate::libraries::setup_guide::or_unread;
use crate::{AvailabilityError, AvailabilityReport, CudaAlgebraError, PackagedLibraries};
use advice::{
    availability_advice, device_advice, gibibytes, setup_guides, trial_run_advice, with_sources,
};
use canonical_mps::CanonicalMpsError;
use devices::DeviceReport;
use std::fmt;

/// The state of this machine's GPU setup, as [`GpuSetup::check`] found it:
/// each step's outcome. Its [`Display`](fmt::Display) is the report a user
/// reads.
#[derive(Debug)]
pub struct GpuSetup {
    /// The libraries' paths and versions, or why discovery found no usable
    /// set.
    pub(crate) availability: Result<AvailabilityReport, AvailabilityError>,
    /// How many CUDA devices there are, and the one the GPU engine runs on,
    /// or why they could not be read: `None` when discovery failed, so none
    /// were read.
    pub(crate) devices: Option<Result<DeviceReport, CudaAlgebraError>>,
    /// Whether the trial run on the GPU engine's device succeeded: `None`
    /// when discovery failed, so it was not tried.
    pub(crate) trial_run: Option<Result<(), CanonicalMpsError<CudaAlgebraError>>>,
    /// The command that installs the libraries, which the advice gives as
    /// an example: the packager's, from [`PackagedLibraries`].
    pub(crate) install_command: Option<String>,
}

impl GpuSetup {
    /// Checks this machine's GPU setup: discovers the libraries, trying
    /// `packaged` as [`discover_with`](crate::discover_with) does, and when
    /// they load, reads the devices and runs the trial run on the GPU
    /// engine's device. Never fails: a problem is part of the
    /// [`GpuSetup`].
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
                devices: Some(device_report(&availability)),
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
    /// that supports no CUDA 12); the devices could not be read, or the
    /// GPU engine's device is missing, or of a compute capability
    /// cuTensorNet does not support; the trial run failed.
    #[must_use]
    pub fn advice(&self) -> Option<String> {
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
            Some(devices) => {
                if let Some(advice) = device_advice(devices) {
                    return Some(advice);
                }
            }
            None => return Some("the CUDA devices were not read".to_string()),
        }
        match &self.trial_run {
            Some(Ok(())) => None,
            Some(Err(error)) => Some(trial_run_advice(error)),
            None => Some("the trial run was not tried".to_string()),
        }
    }

    /// Warnings, whether or not the setup is ready: each other copy of a
    /// library the setup makes visible beside the one loaded, since a
    /// stray copy can be what makes the one loaded fail, with how to keep
    /// only one: when the loaded copy is the override's, remove the other
    /// or unset the variable; else remove one or set the variable.
    #[must_use]
    pub fn warnings(&self) -> Vec<String> {
        let report = match &self.availability {
            Ok(report) | Err(AvailabilityError::Unusable(report)) => report,
            Err(AvailabilityError::UnsupportedPlatform { .. }) => return Vec::new(),
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
            Some(Ok(DeviceReport { count: 0, .. })) => writeln!(f, "CUDA devices: none")?,
            Some(Ok(DeviceReport {
                count,
                engine_device,
            })) => {
                write!(f, "CUDA devices: {count}")?;
                if let Some(device) = engine_device {
                    let (major, minor) = device.compute_capability;
                    write!(
                        f,
                        "; device {DEVICE_ORDINAL}, where the GPU engine runs: compute \
                         capability {major}.{minor}, {} of {} GiB free",
                        gibibytes(device.free_bytes),
                        gibibytes(device.total_bytes)
                    )?;
                }
                writeln!(f)?;
            }
            Some(Err(error)) => writeln!(f, "CUDA devices: {}", with_sources(error))?,
            None => writeln!(f, "CUDA devices: not read")?,
        }
        match &self.trial_run {
            Some(Ok(())) => write!(f, "trial run on CUDA device {DEVICE_ORDINAL}: ready")?,
            Some(Err(error)) => write!(
                f,
                "trial run on CUDA device {DEVICE_ORDINAL}: {}",
                with_sources(error)
            )?,
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
