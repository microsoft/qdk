// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Diagnosis`]: for a user setting up the GPU for the MPS method, what was
//! found and, when something is missing, what to fix.
//!
//! The MPS method runs on the GPU only when several things outside the QDK are
//! in place: Linux x86-64, an NVIDIA driver for CUDA 12, a device of compute
//! capability [`MINIMUM_COMPUTE_CAPABILITY`] or later, and NVIDIA's libraries,
//! today cuTensorNet and the CUDA Runtime, at the audited versions (the crate's
//! README says where each was verified). Each can be missing or wrong, and the
//! user who meets the failure is often not the one who knows CUDA. So
//! [`diagnose`] checks each and answers in one report, and never fails: the
//! libraries found and their versions, other copies of them that could be
//! loaded instead (a warning), the driver's CUDA version, each device's compute
//! capability and memory, whether an MPS engine could run a gate on device 0,
//! and for the first problem met, what to do ([`Diagnosis::advice`]). A caller
//! shows it to the user on request (the setup check), and its text with the
//! error of a GPU run that cannot start.
//!
//! How: [`discover_with`](crate::discover_with) (no GPU work), then, when it
//! succeeds, the devices ([`DeviceReport`]), and an engine on device 0 with
//! one state of one qubit, a Hadamard and a probability read: the smallest
//! run that touches the device. Each library's module declares what the
//! check shows of the library, what to install, and the problems of the
//! host its report reveals (`SetupGuide`), the CUDA Runtime's driver floor
//! among them; so a library bound later brings its own.

use std::fmt;

use crate::{
    AvailabilityError, AvailabilityReport, DeviceReport, EngineError, LibraryFault, LibraryReport,
    MINIMUM_COMPUTE_CAPABILITY, PackagedLibraries,
};

/// Discovers the libraries, trying `packaged` as
/// [`discover_with`](crate::discover_with) does, and when they load, reads
/// the devices, makes an MPS engine on device 0 and runs one gate on one
/// qubit. Never fails: a problem is part of the [`Diagnosis`].
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

/// What [`diagnose`] found: discovery's report or error, the devices, and
/// the engine's smoke test.
#[derive(Debug)]
pub struct Diagnosis {
    /// The libraries' paths and versions, or why discovery found no usable
    /// set.
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
    /// are looked for: discovery failed (an invalid override first, or else
    /// the first fault of the first library at fault, in the order they are
    /// opened); a library reveals a problem of the host (today, a driver
    /// that supports no CUDA 12); the devices could not be read, or there
    /// are none; device 0, where the engine runs, has a compute capability
    /// below [`MINIMUM_COMPUTE_CAPABILITY`]; the engine did not run its
    /// gate.
    #[must_use]
    pub fn advice(&self) -> Option<String> {
        const DRIVER: &str = concat!(
            "check that `nvidia-smi` lists a GPU, and that the NVIDIA driver supports CUDA 12 (",
            crate::libraries::cuda_runtime::minimum_driver!(),
            " or later)"
        );
        let report = match &self.availability {
            Err(error) => return Some(availability_advice(error)),
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
impl fmt::Display for Diagnosis {
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

    let engine = crate::CuTensorNetEngine::new(availability)?;
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

/// What to do about `error`: when libraries are at fault, about an invalid
/// override if any, since an override decides what is loaded; or else
/// about the first fault of the first library at fault, in the order they
/// are opened.
fn availability_advice(error: &AvailabilityError) -> String {
    match error {
        AvailabilityError::UnsupportedPlatform { .. } => {
            "the MPS method's GPU engine runs on Linux x86-64 only".to_string()
        }
        AvailabilityError::Unusable(report) => {
            let faults = || {
                report
                    .libraries
                    .iter()
                    .flat_map(|library| library.faults.iter().map(|fault| (library.library, fault)))
            };
            faults()
                .find(|(_, fault)| matches!(fault, LibraryFault::InvalidOverride { .. }))
                .or_else(|| faults().next())
                .map_or_else(install, |(library, fault)| fault_advice(library, fault))
        }
    }
}

/// What to do about `fault`, of the library named `library`.
fn fault_advice(library: &str, fault: &LibraryFault) -> String {
    let guide = setup_guide(library);
    match fault {
        LibraryFault::InvalidOverride { variable, .. } => {
            format!("set {variable} to the absolute path of the library file, or unset it")
        }
        LibraryFault::NotFound { .. } => match guide {
            Some(guide) => format!(
                "{}, or set {} to the absolute path of {library}",
                install(),
                guide.variable
            ),
            None => install(),
        },
        LibraryFault::LoadFailed { .. } => format!(
            "{library} was found but did not load, often because a library it needs is \
             missing: {}, which brings its dependencies",
            install()
        ),
        LibraryFault::MissingRequiredSymbol { .. } | LibraryFault::UnsupportedVersion { .. } => {
            format!("only the audited versions are supported: {}", install())
        }
        LibraryFault::VersionProbeFailed { .. } => {
            match guide.and_then(|guide| guide.version_probe_advice) {
                Some(advice) => advice.to_string(),
                None => format!("{library} could not report its version: {}", install()),
            }
        }
    }
}

/// What to install: each library at its audited version, in the order the
/// check lists them, and how to get them from Python.
fn install() -> String {
    let libraries: Vec<&str> = setup_guides().iter().map(|guide| guide.install).collect();
    format!(
        "install {} (from Python: `pip install \"qdk[tensornetwork]\"`)",
        libraries.join(" and ")
    )
}

/// `value`, or `unread` when the report lacks it: a usable library's report
/// has every value, so the fallback only keeps a hand-made report shown.
pub(crate) fn or_unread(value: Option<impl fmt::Display>) -> String {
    value.map_or_else(|| "unread".to_string(), |value| value.to_string())
}

/// `bytes` in GiB, to one decimal.
#[allow(clippy::cast_precision_loss)] // A device's memory, shown to 0.1 GiB.
fn gibibytes(bytes: usize) -> String {
    format!("{:.1}", bytes as f64 / f64::from(1u32 << 30))
}

/// What the setup check tells a user about one library: what it shows of
/// the library and what it advises. Each library's module declares it, as
/// part of its `NativeLibrary`, and `LoadedLibraries::SETUP_GUIDES` lists
/// them.
pub(crate) struct SetupGuide {
    /// The library's name, as its report gives it.
    pub(crate) library: &'static str,
    /// The variable that overrides its path.
    variable: &'static str,
    /// What to install, for instance `cuTensorNet 2.13`.
    install: &'static str,
    /// What to do when a version probe fails, if one can.
    version_probe_advice: Option<&'static str>,
    /// The lines that show the usable library.
    describe: fn(&LibraryReport) -> Vec<String>,
    /// A problem of the host that the usable library reveals.
    host_problem: fn(&LibraryReport) -> Option<String>,
}

#[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
impl SetupGuide {
    /// The guide that `L`'s module declares.
    pub(crate) const fn of<L: crate::libraries::NativeLibrary>() -> Self {
        Self {
            library: L::NAME,
            variable: L::VARIABLE,
            install: L::INSTALL,
            version_probe_advice: L::VERSION_PROBE_ADVICE,
            describe: L::describe,
            host_problem: L::host_problem,
        }
    }
}

/// The guide of each library, in the order the check lists them; none off
/// Linux x86-64, where discovery fails before any library is looked at.
fn setup_guides() -> &'static [SetupGuide] {
    #[cfg(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64")))]
    {
        &crate::libraries::LoadedLibraries::SETUP_GUIDES
    }
    #[cfg(not(any(all(test, unix), all(target_os = "linux", target_arch = "x86_64"))))]
    {
        &[]
    }
}

/// The guide of the library named `library`.
fn setup_guide(library: &str) -> Option<&'static SetupGuide> {
    setup_guides().iter().find(|guide| guide.library == library)
}
