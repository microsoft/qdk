// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Diagnosis`]: for a user setting up the GPU for the MPS method, what was
//! found and, when something is missing, what to fix.
//!
//! The MPS method runs on the GPU only when several things outside the QDK
//! are in place: Linux x86-64, an NVIDIA driver for CUDA 12, a device of
//! compute capability 7.5 or later, and NVIDIA's cuTensorNet and CUDA
//! Runtime at the audited versions. Each can be missing or wrong, and the
//! user who meets the failure is often not the one who knows CUDA. So
//! [`diagnose`] answers in one report, and never fails: the libraries found
//! and their versions, other copies of them that could be loaded instead
//! (a warning), the driver's CUDA version, each device's compute capability
//! and memory, whether an MPS engine could run a gate on device 0, and for
//! the first problem met, what to do ([`Diagnosis::advice`]). A caller shows
//! it to the user on request (the setup check), and its text with the
//! error of a GPU run that cannot start.
//!
//! How: [`discover_with`](crate::discover_with) (no GPU work), then, when it
//! succeeds, the devices ([`DeviceReport`]), and an engine on device 0 with
//! one state of one qubit, a Hadamard and a probability read: the smallest
//! run that touches the device. The driver floor is NVIDIA's for CUDA 12
//! (525.60.13); the QDK adds none.

use std::fmt;

use crate::{
    AvailabilityError, AvailabilityReport, DeviceReport, EngineError, MINIMUM_COMPUTE_CAPABILITY,
    PackagedLibraries,
};

/// The oldest CUDA a driver must support, as CUDA encodes it: 12.0.
const MINIMUM_DRIVER_CUDA: i32 = 12_000;

/// What [`diagnose`] found: discovery's report or error, the devices, and
/// the engine's smoke test.
#[derive(Debug)]
pub struct Diagnosis {
    /// The libraries' paths and versions, or why discovery found no usable
    /// pair.
    pub availability: Result<AvailabilityReport, AvailabilityError>,
    /// Every CUDA device, or why they could not be read: `None` when
    /// discovery failed, so none were read.
    pub devices: Option<Result<Vec<DeviceReport>, EngineError>>,
    /// Whether an MPS engine was made on device 0 and ran a gate: `None`
    /// when discovery failed, so no engine was tried.
    pub engine: Option<Result<(), EngineError>>,
}

impl Diagnosis {
    /// Whether the MPS method can run on the GPU: no problem was met.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.advice().is_none()
    }

    /// What to do about the first problem met, in words for the user;
    /// `None` when the method is ready. The problems, in the order they
    /// are looked for: discovery failed; the driver supports no CUDA 12;
    /// the devices could not be read, or there are none; device 0, where
    /// the engine runs, has a compute capability below 7.5; the engine did
    /// not run its gate.
    #[must_use]
    pub fn advice(&self) -> Option<String> {
        const DRIVER: &str = "check that `nvidia-smi` lists a GPU, and that the NVIDIA driver \
                              supports CUDA 12 (525.60.13 or later)";
        let report = match &self.availability {
            Err(error) => return Some(availability_advice(error)),
            Ok(report) => report,
        };
        if report.cuda_driver_version < MINIMUM_DRIVER_CUDA {
            return Some(format!(
                "the NVIDIA driver supports CUDA {} only: install a driver for CUDA 12 \
                 (525.60.13 or later)",
                cuda_version(report.cuda_driver_version.into())
            ));
        }
        match &self.devices {
            Some(Ok(devices)) => match devices.first() {
                None => return Some(format!("no CUDA device was found: {DRIVER}")),
                Some(device) if !device.is_supported() => {
                    let (major, minor) = device.compute_capability;
                    let (needed_major, needed_minor) = MINIMUM_COMPUTE_CAPABILITY;
                    return Some(format!(
                        "CUDA device 0, where the engine runs, has compute capability \
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
        match &self.engine {
            Some(Ok(())) => None,
            Some(Err(_)) | None => Some(format!(
                "the libraries load, but no MPS engine could run on CUDA device 0: {DRIVER}"
            )),
        }
    }

    /// Warnings that leave the method ready: each other copy of a library
    /// the setup makes visible beside the one loaded.
    #[must_use]
    pub fn warnings(&self) -> Vec<String> {
        let Ok(report) = &self.availability else {
            return Vec::new();
        };
        let copies = [
            (
                "cuTensorNet",
                &report.cutensornet_library,
                &report.other_cutensornet_copies,
            ),
            (
                "CUDA Runtime",
                &report.cuda_runtime_library,
                &report.other_cuda_runtime_copies,
            ),
        ];
        copies
            .into_iter()
            .flat_map(|(library, loaded, others)| {
                others.iter().map(move |other| {
                    format!(
                        "another copy of {library} is at {}, beside the one loaded, {}: \
                         remove one, or set {} to the one to load",
                        other.display(),
                        loaded.display(),
                        override_variable(library)
                    )
                })
            })
            .collect()
    }
}

/// Lists what was found, then the warnings, then the problem and its fix,
/// one item a line.
impl fmt::Display for Diagnosis {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.availability {
            Ok(report) => {
                writeln!(
                    f,
                    "cuTensorNet: {} ({}, built for CUDA Runtime {})",
                    report.cutensornet_library.display(),
                    cutensornet_version(report.cutensornet_version),
                    cuda_version(report.cutensornet_cuda_runtime_version as i128),
                )?;
                writeln!(
                    f,
                    "CUDA Runtime: {} ({})",
                    report.cuda_runtime_library.display(),
                    cuda_version(report.cuda_runtime_version.into()),
                )?;
                writeln!(
                    f,
                    "CUDA driver: supports CUDA {}",
                    cuda_version(report.cuda_driver_version.into())
                )?;
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
            Some(Err(error)) => writeln!(f, "CUDA devices: {error}")?,
            None => writeln!(f, "CUDA devices: not read")?,
        }
        match &self.engine {
            Some(Ok(())) => write!(f, "MPS engine on CUDA device 0: ready")?,
            Some(Err(error)) => write!(f, "MPS engine on CUDA device 0: {error}")?,
            None => write!(f, "MPS engine: not tried")?,
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

/// `bytes` in GiB, to one decimal.
#[allow(clippy::cast_precision_loss)] // A device's memory, shown to 0.1 GiB.
fn gibibytes(bytes: usize) -> String {
    format!("{:.1}", bytes as f64 / f64::from(1u32 << 30))
}

/// What to do about `error`.
fn availability_advice(error: &AvailabilityError) -> String {
    const INSTALL: &str = "install cuTensorNet 2.13 and the CUDA Runtime 12.9 (from Python: \
                           `pip install \"qdk[tensornetwork]\"`)";
    match error {
        AvailabilityError::UnsupportedPlatform { .. } => {
            "the MPS method's GPU engine runs on Linux x86-64 only".to_string()
        }
        AvailabilityError::InvalidOverride { variable, .. } => {
            format!("set {variable} to the absolute path of the library file, or unset it")
        }
        AvailabilityError::LibraryNotFound { library, .. } => format!(
            "{INSTALL}, or set {} to the absolute path of {library}",
            override_variable(library)
        ),
        AvailabilityError::LoadFailed { library, .. } => format!(
            "{library} was found but did not load, often because a library it needs is \
             missing: {INSTALL}, which brings its dependencies"
        ),
        AvailabilityError::MissingRequiredSymbol { .. }
        | AvailabilityError::UnsupportedVersion { .. } => {
            format!("only the audited versions are supported: {INSTALL}")
        }
        AvailabilityError::VersionProbeFailed { .. } => "CUDA could not report its version, \
             usually because the NVIDIA driver is missing or older than CUDA 12 needs \
             (525.60.13): check `nvidia-smi`"
            .to_string(),
    }
}

/// The variable that overrides `library`'s path.
fn override_variable(library: &str) -> &'static str {
    if library == "cuTensorNet" {
        "QDK_CUTENSORNET_LIBRARY"
    } else {
        "QDK_CUDART_LIBRARY"
    }
}

/// cuTensorNet's `10000·major + 100·minor + patch` as `major.minor.patch`.
fn cutensornet_version(version: usize) -> String {
    format!(
        "{}.{}.{}",
        version / 10_000,
        version / 100 % 100,
        version % 100
    )
}

/// CUDA's `1000·major + 10·minor` as `major.minor`.
fn cuda_version(version: i128) -> String {
    format!("{}.{}", version / 1000, version % 1000 / 10)
}

/// Discovers the libraries, trying `packaged` as
/// [`discover_with`](crate::discover_with) does, and when they load, makes an
/// MPS engine on device 0 and runs one gate on one qubit. Never fails: a
/// problem is part of the [`Diagnosis`].
#[must_use]
pub fn diagnose(packaged: &PackagedLibraries) -> Diagnosis {
    match crate::discover_with(packaged) {
        Err(error) => Diagnosis {
            availability: Err(error),
            devices: None,
            engine: None,
        },
        Ok(availability) => Diagnosis {
            devices: Some(devices(&availability)),
            engine: Some(smoke_test(&availability)),
            availability: Ok(availability.report().clone()),
        },
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn devices(availability: &crate::Availability) -> Result<Vec<DeviceReport>, EngineError> {
    availability.devices()
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn devices(_availability: &crate::Availability) -> Result<Vec<DeviceReport>, EngineError> {
    unreachable!("discovery fails off Linux x86-64")
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn smoke_test(availability: &crate::Availability) -> Result<(), EngineError> {
    use tensornet::{MpsEngine, MpsShape};

    let engine = availability.mps_engine()?;
    let capacity = MpsShape::qubit_chain(1, 1).expect("one qubit is a chain");
    let mut state = engine.prepare(&capacity)?;
    let h = std::f64::consts::FRAC_1_SQRT_2;
    let hadamard = [h, h, h, -h].map(|x| num_complex::Complex64::new(x, 0.0));
    engine.apply(&mut state, &[0], &hadamard)?;
    engine.probability_of_one(&state, 0)?;
    Ok(())
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn smoke_test(_availability: &crate::Availability) -> Result<(), EngineError> {
    unreachable!("discovery fails off Linux x86-64")
}
