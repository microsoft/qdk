// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`GpuSetup`]: for a user setting up the GPU for the MPS simulation
//! method, what was found and, when something is missing, what to fix.
//!
//! A GPU run needs several things outside the QDK: Linux x86-64, an NVIDIA
//! driver, a CUDA device, and NVIDIA's libraries, today cuTensorNet and the
//! CUDA Runtime, at the audited versions (the crate's README says where
//! each was verified); a library may also need a driver, or a device of a
//! compute capability, of some version or later. Each can be missing or
//! wrong, and the user who meets the failure is often not the one who
//! knows CUDA. So [`GpuSetup::check`] checks each and answers in one
//! report, and never fails: whether the setup is ready, what was found,
//! the other copies of a library that could be loaded instead, and what to
//! do about the first problem met in the order of the steps.
//!
//! The steps, in order; those after discovery run whenever it succeeds:
//!
//! 1. the libraries: [`discover_with`](crate::discover_with), which runs no
//!    GPU work; an invalid override is advised first, since an override
//!    decides what is loaded, then the first fault of the first library at
//!    fault, in the order they are opened;
//! 2. the host: a problem of the host a library's report reveals, as its
//!    `LibrarySetupGuide` (in `libraries::setup_guide`) says, today the
//!    CUDA Runtime's driver check;
//! 3. the devices: how many CUDA devices the process sees, and the compute
//!    capability and memory of the one the GPU engine runs on, the only
//!    one read (`device_report`); a problem when they cannot be read, when
//!    there is none, or when its compute capability is below a library's
//!    floor;
//! 4. the trial run: a [`CudaMpsEngine`](crate::CudaMpsEngine) on that
//!    device prepares one qubit, applies a Hadamard and reads the
//!    probability of One, which must be 1/2: the smallest run that touches
//!    the device and checks what it computes.
//!
//! What the check shows and advises of a library, the driver and the
//! compute capability it needs included, the library declares in its
//! `LibrarySetupGuide`, so a library bound later brings its own; `advice`
//! holds the advice shared by every library and that of the steps after
//! discovery.

mod advice;
pub(crate) mod device_report;

use crate::discovery::Discovered;
use crate::engine::cuda_algebra::policy::DEVICE_ORDINAL;
use crate::libraries::setup_guide::{SetupGuide, or_unread};
use crate::{AvailabilityError, AvailabilityReport, CudaAlgebraError, PackagedLibraries};
use advice::{availability_advice, device_advice, other_copy_advice, trial_run_advice};
use canonical_mps::CanonicalMpsError;
use device_report::DeviceReport;
use std::error::Error;
use std::fmt;

/// The state of this machine's GPU setup, as [`GpuSetup::check`] found it:
/// each step's outcome. Its [`Display`](fmt::Display) is the report a user
/// reads.
///
/// It holds no library, device or engine. The check runs in the process
/// that will use the GPU, so its answer holds for that process.
#[derive(Debug)]
pub struct GpuSetup {
    /// What the steps found once the libraries loaded, or why discovery
    /// found no usable set, in which case no later step ran.
    pub(crate) loaded: Result<LoadedSetup, AvailabilityError>,
    /// The setup guide of each library discovery opened, in the order
    /// opened: the libraries the report shows and the advice addresses.
    pub(crate) guides: Vec<SetupGuide>,
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
    /// The probability of One the trial run read, or why it failed.
    pub(crate) trial_run: Result<f64, CanonicalMpsError<CudaAlgebraError>>,
}

impl GpuSetup {
    /// Checks this machine's GPU setup: discovers the libraries, trying
    /// `packaged` as [`discover_with`](crate::discover_with) does, and when
    /// they load, reads the devices and runs the trial run on the GPU
    /// engine's device. Never fails: a problem is part of the
    /// [`GpuSetup`].
    #[must_use]
    pub fn check(packaged: &PackagedLibraries) -> Self {
        Self::from_discovery(crate::discovery::discover_with_guides(packaged), packaged)
    }

    /// [`check`](Self::check), with the override variables read by
    /// `read_variable`.
    #[cfg(all(test, unix))]
    pub(crate) fn check_from(
        read_variable: impl Fn(&'static str) -> Option<std::ffi::OsString>,
        packaged: &PackagedLibraries,
    ) -> Self {
        Self::from_discovery(
            crate::discovery::discover_from(read_variable, packaged),
            packaged,
        )
    }

    /// The steps after discovery, run on what `discovered` found.
    fn from_discovery(
        Discovered {
            availability,
            guides,
        }: Discovered,
        packaged: &PackagedLibraries,
    ) -> Self {
        let loaded = availability.map(|availability| LoadedSetup {
            devices: device_report(&availability),
            trial_run: trial_run(&availability),
            libraries: availability.report().clone(),
        });
        Self {
            loaded,
            guides,
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
    /// `None` when the setup is ready. The steps are taken in order: the
    /// libraries, the host, the devices, the trial run.
    #[must_use]
    pub fn advice(&self) -> Option<String> {
        let install_command = self.install_command.as_deref();
        let loaded = match &self.loaded {
            Err(error) => {
                return Some(availability_advice(error, &self.guides, install_command));
            }
            Ok(loaded) => loaded,
        };
        self.guides
            .iter()
            .zip(&loaded.libraries.libraries)
            .find_map(|(guide, library)| (guide.host_problem)(library))
            .or_else(|| device_advice(&loaded.devices, &self.guides))
            .or_else(|| trial_run_advice(&loaded.trial_run, &self.guides, install_command))
    }

    /// Each other copy of a library visible beside the one loaded, with
    /// what to do about it, whether or not the setup is ready: a stray copy
    /// can be what makes the one loaded fail.
    #[expect(
        clippy::unnecessary_debug_formatting,
        reason = "paths are quoted, as in the faults, so a line break adds no line"
    )]
    #[must_use]
    pub fn warnings(&self) -> Vec<String> {
        let report = match &self.loaded {
            Ok(LoadedSetup { libraries, .. }) | Err(AvailabilityError::Unusable(libraries)) => {
                libraries
            }
            Err(AvailabilityError::UnsupportedPlatform { .. }) => return Vec::new(),
        };
        self.guides
            .iter()
            .zip(&report.libraries)
            .flat_map(|(guide, library)| {
                let loaded = or_unread(library.path.as_ref().map(|path| format!("{path:?}")));
                let fix = other_copy_advice(guide, library);
                library.other_copies.iter().map(move |other| {
                    format!(
                        "another copy of {} is at {other:?}, beside the one loaded, {loaded}: \
                         {fix}",
                        guide.library,
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
            Ok(loaded) => {
                write_libraries(f, &self.guides, &loaded.libraries)?;
                write!(f, "{loaded}")?;
            }
            Err(error) => {
                match error {
                    AvailabilityError::Unusable(libraries) => {
                        write_libraries(f, &self.guides, libraries)?;
                    }
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

/// Lists what the steps after discovery found, one item a line: the
/// devices, the trial run.
impl fmt::Display for LoadedSetup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
        write!(f, "trial run on CUDA device {DEVICE_ORDINAL}: ")?;
        match &self.trial_run {
            Ok(probability) if is_right_trial_run(*probability) => write!(f, "ready"),
            Ok(probability) => write!(
                f,
                "read the probability {probability} of One, where {TRIAL_RUN_PROBABILITY} is \
                 exact"
            ),
            Err(error) => write!(f, "{}", with_sources(error)),
        }
    }
}

/// Lists each library of `report` as its guide in `guides` shows it, then
/// each fault found in it, one item a line: what discovery found, whether
/// or not it succeeded. A library with no path shows only its faults,
/// which say why.
fn write_libraries(
    f: &mut fmt::Formatter<'_>,
    guides: &[SetupGuide],
    report: &AvailabilityReport,
) -> fmt::Result {
    for (guide, library) in guides.iter().zip(&report.libraries) {
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
    device_report::read_devices(availability.libraries().as_ref())
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn device_report(_availability: &crate::Availability) -> Result<DeviceReport, CudaAlgebraError> {
    unreachable!("discovery fails off Linux x86-64")
}

/// The probability of One the trial run must read: a Hadamard on |0⟩
/// gives exactly 1/2.
const TRIAL_RUN_PROBABILITY: f64 = 0.5;

/// How far the trial run's probability may be from
/// [`TRIAL_RUN_PROBABILITY`]: the rounding error of a one-qubit state is
/// near 1e-16, so a value further off is a wrong result, not rounding.
const TRIAL_RUN_TOLERANCE: f64 = 1e-6;

/// Whether `probability`, read by the trial run, is
/// [`TRIAL_RUN_PROBABILITY`] within [`TRIAL_RUN_TOLERANCE`]; NaN is not.
fn is_right_trial_run(probability: f64) -> bool {
    (probability - TRIAL_RUN_PROBABILITY).abs() <= TRIAL_RUN_TOLERANCE
}

/// The trial run on a [`CudaMpsEngine`](crate::CudaMpsEngine) on its CUDA
/// device: the probability of One it reads ([`run_trial`]).
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn trial_run(
    availability: &crate::Availability,
) -> Result<f64, CanonicalMpsError<CudaAlgebraError>> {
    let engine =
        crate::CudaMpsEngine::try_from(availability).map_err(CanonicalMpsError::Algebra)?;
    run_trial(&engine)
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn trial_run(
    _availability: &crate::Availability,
) -> Result<f64, CanonicalMpsError<CudaAlgebraError>> {
    unreachable!("discovery fails off Linux x86-64")
}

/// The trial run on `engine`: prepares one qubit, applies a Hadamard and
/// returns the probability of One it reads, [`TRIAL_RUN_PROBABILITY`] when
/// the engine computes right.
#[cfg(any(test, all(target_os = "linux", target_arch = "x86_64")))]
pub(crate) fn run_trial<E: tensornet::MpsEngine>(engine: &E) -> Result<f64, E::Error> {
    use num_complex::Complex64;

    let capacity = tensornet::MpsShape::qubit_chain(1, 1).expect("one qubit is a chain");
    let mut state = engine.prepare(&capacity)?;
    let h = std::f64::consts::FRAC_1_SQRT_2;
    let hadamard = [h, h, h, -h].map(|x| Complex64::new(x, 0.0));
    engine.apply(&mut state, &[0], &hadamard)?;
    engine.probability_of_one(&state, 0)
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
