// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The GPU setup check: it tells a user setting up the GPU for the MPS
//! simulation method what was found and, when something is missing, what
//! to fix. [`GpuSetup`] holds the answer.
//!
//! The module is private, so its contract (the steps and their order) is
//! on [`GpuSetup`], which the crate exports. Each library declares what the
//! check shows and advises of it, the driver and compute capability it
//! needs included, in its `LibrarySetupGuide` (`libraries::setup_guide`).
//! `advice` holds the advice shared by every library and that of the steps
//! after discovery. `device_report` reads the devices. A user's guide to
//! the check, its steps and its advice, is `docs/gpu_setup.md`.

mod advice;
mod device_report;

use crate::discovery::Discovered;
use crate::engine::cuda_algebra::settings::DEVICE_ORDINAL;
use crate::libraries::setup_guide::{SetupGuide, path_or_unread};
use crate::{
    AvailabilityError, AvailabilityReport, CudaAlgebraError, LibraryReport, PackagedLibraries,
};
use advice::{availability_advice, device_advice, other_copy_advice, trial_run_advice};
use canonical_mps::CanonicalMpsError;
use device_report::{DeviceReport, EngineDevice};
use std::error::Error;
use std::fmt;

/// The state of this machine's GPU setup, as [`GpuSetup::check`] found it:
/// what each step found and, for the first problem met, what to fix. Its
/// [`Display`](fmt::Display) is the report a user reads.
///
/// A GPU run needs several things outside the QDK: Linux x86-64, an NVIDIA
/// driver, a CUDA device, and NVIDIA's libraries at the audited versions,
/// today cuTensorNet and the CUDA Runtime. Some libraries also need a
/// driver, or a device of a compute capability, of some version or later.
/// When one is missing or wrong, the user who meets the failure is often
/// not the one who knows CUDA. So `check` checks each and answers in one
/// report, and never fails. The report says whether the setup is ready,
/// what was found, and the other copies of a library visible beside the
/// one loaded, which another process can load instead. It advises on the
/// first problem met in the order of the steps, since a later problem is
/// often the earlier one's consequence: with a driver too old for the CUDA
/// Runtime, the process sees no device.
///
/// The steps, in order. Those after discovery run whenever it succeeds.
///
/// 1. The libraries: [`discover_with`](crate::discover_with), which runs
///    no GPU work. An invalid override is advised first, since an override
///    decides what is loaded. Then the first fault of the first library at
///    fault, in the order they are opened.
/// 2. The host: a problem of the host that a library's report reveals.
///    Today the CUDA Runtime's: no NVIDIA driver, or one that supports no
///    CUDA 12.
/// 3. The devices: how many CUDA devices the process sees, and the compute
///    capability and memory of the one the GPU engine runs on, the only
///    one read. A problem is met when they cannot be read, when there is
///    none, or when that device's compute capability is below a library's
///    floor.
/// 4. The trial run: a [`CudaMpsEngine`](crate::CudaMpsEngine) on that
///    device prepares two qubits, entangles them and reads the second's
///    probability of One, which must be 1/2 up to a margin far wider than
///    rounding. It is the smallest run that makes cuTensorNet compute on
///    the device (the entangling gate's split) and checks what it computes.
///
/// It holds no library, device or engine. The check runs in the process
/// that will use the GPU, so its answer holds for that process.
#[derive(Debug)]
pub struct GpuSetup {
    /// What the steps found once the libraries loaded. Otherwise, why
    /// discovery found no usable set; then no later step ran.
    loaded: Result<LoadedSetup, AvailabilityError>,
    /// The setup guide of each library discovery opened, in the order
    /// opened. Each is found by its library's name
    /// ([`SetupGuide::for_library`]). These are the libraries the report
    /// shows and the advice addresses.
    guides: Vec<SetupGuide>,
    /// The packager's command that installs the libraries, from
    /// [`PackagedLibraries`]. The advice gives it as an example.
    install_command: Option<String>,
}

/// The setup of a machine whose libraries loaded: their report, and the
/// outcome of each step run on them.
#[derive(Debug)]
struct LoadedSetup {
    /// The libraries' paths and versions.
    libraries: AvailabilityReport,
    /// How many CUDA devices there are, and the one the GPU engine runs on,
    /// or why they could not be read.
    devices: Result<DeviceReport, CudaAlgebraError>,
    /// The probability of One the trial run read, or why it failed.
    trial_run: Result<f64, CanonicalMpsError<CudaAlgebraError>>,
}

impl GpuSetup {
    /// Checks this machine's GPU setup. It discovers the libraries, trying
    /// `packaged` as [`discover_with`](crate::discover_with) does. When
    /// they load, it reads the devices and makes the trial run on the GPU
    /// engine's device. Never fails: a problem is part of the
    /// [`GpuSetup`].
    #[must_use]
    pub fn check(packaged: &PackagedLibraries) -> Self {
        Self::from_discovery(crate::discovery::discover_with_guides(packaged), packaged)
    }

    /// [`check`](Self::check), with the override variables read by
    /// `read_variable`.
    #[cfg(all(test, unix))]
    fn check_from(
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

    /// What to do about the first problem met, in words for the user, or
    /// `None` when the setup is ready. The steps are taken in order: the
    /// libraries, the host, the devices, the trial run.
    ///
    /// ```text
    /// libraries -ok-> host -ok-> devices -ok-> trial run -ok-> None
    ///     |            |           |              |
    ///     +------------+-----------+--------------+--> the step's advice
    /// ```
    #[must_use]
    pub fn advice(&self) -> Option<String> {
        let install_command = self.install_command.as_deref();
        let loaded = match &self.loaded {
            Err(error) => {
                return Some(availability_advice(error, &self.guides, install_command));
            }
            Ok(loaded) => loaded,
        };
        self.host_advice(&loaded.libraries)
            .or_else(|| device_advice(&loaded.devices, &self.guides))
            .or_else(|| trial_run_advice(&loaded.trial_run, &self.guides, install_command))
    }

    /// The host step's advice: the first problem of the host that a
    /// library's report reveals, as its guide words it.
    fn host_advice(&self, libraries: &AvailabilityReport) -> Option<String> {
        libraries.libraries.iter().find_map(|library| {
            (SetupGuide::for_library(&self.guides, library).host_problem)(library)
        })
    }

    /// Each other copy of a library visible beside the one loaded, with
    /// what to do about it. They are given whether or not the setup is
    /// ready, since a stray copy can be what makes the one loaded fail.
    #[must_use]
    pub fn warnings(&self) -> Vec<String> {
        let Some(report) = self.libraries_found() else {
            return Vec::new();
        };
        report
            .libraries
            .iter()
            .flat_map(|library| self.other_copy_warnings(library))
            .collect()
    }

    /// What discovery found of the libraries, whether or not they are
    /// usable. `None` when it ran on no platform it supports.
    fn libraries_found(&self) -> Option<&AvailabilityReport> {
        match &self.loaded {
            Ok(LoadedSetup { libraries, .. }) | Err(AvailabilityError::Unusable(libraries)) => {
                Some(libraries)
            }
            Err(AvailabilityError::UnsupportedPlatform { .. }) => None,
        }
    }

    /// One warning for each other copy of `library`, each naming the copy
    /// loaded and what to do.
    fn other_copy_warnings<'a>(
        &self,
        library: &'a LibraryReport,
    ) -> impl Iterator<Item = String> + 'a {
        let loaded = path_or_unread(library);
        let fix = other_copy_advice(SetupGuide::for_library(&self.guides, library), library);
        library.other_copies.iter().map(move |other| {
            format!(
                "another copy of {} is at `{}`, beside the one loaded, {loaded}: {fix}",
                library.name,
                other.display(),
            )
        })
    }
}

/// Lists what was found, then the warnings, then the problem and its fix,
/// one item a line.
impl fmt::Display for GpuSetup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_found(f)?;
        for warning in self.warnings() {
            write!(f, "\nwarning: {warning}")?;
        }
        if let Some(advice) = self.advice() {
            write!(f, "\nto fix: {advice}")?;
        }
        Ok(())
    }
}

impl GpuSetup {
    /// Lists what each step found, one item a line, the last with no line
    /// break. When discovery failed, the later steps are shown not run.
    fn write_found(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.loaded {
            Ok(loaded) => {
                write_libraries(f, &self.guides, &loaded.libraries)?;
                write!(f, "{loaded}")
            }
            Err(AvailabilityError::Unusable(libraries)) => {
                write_libraries(f, &self.guides, libraries)?;
                write_steps_not_run(f)
            }
            Err(error @ AvailabilityError::UnsupportedPlatform { .. }) => {
                writeln!(f, "libraries: {error}")?;
                write_steps_not_run(f)
            }
        }
    }
}

/// The lines of the steps after discovery, when it failed: they did not
/// run.
fn write_steps_not_run(f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(
        f,
        "CUDA devices: not read\ntrial run on CUDA device {DEVICE_ORDINAL}: not tried"
    )
}

/// Lists what the steps after discovery found, one item a line: the
/// devices, the trial run.
impl fmt::Display for LoadedSetup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_devices(f, &self.devices)?;
        write_trial_run(f, &self.trial_run)
    }
}

/// The devices step's line: how many devices, and the engine's device, or
/// why they could not be read.
fn write_devices(
    f: &mut fmt::Formatter<'_>,
    devices: &Result<DeviceReport, CudaAlgebraError>,
) -> fmt::Result {
    match devices {
        Ok(DeviceReport { count: 0, .. }) => writeln!(f, "CUDA devices: none"),
        Ok(DeviceReport {
            count,
            engine_device,
        }) => {
            write!(f, "CUDA devices: {count}")?;
            if let Some(device) = engine_device {
                write_engine_device(f, device)?;
            }
            writeln!(f)
        }
        Err(error) => writeln!(f, "CUDA devices: {}", with_sources(error)),
    }
}

/// The devices line's part on `device`, the GPU engine's: its compute
/// capability, which a library's floor applies to, and its memory, which
/// bounds a run's max bond dimension.
fn write_engine_device(f: &mut fmt::Formatter<'_>, device: &EngineDevice) -> fmt::Result {
    write!(
        f,
        "; CUDA device {DEVICE_ORDINAL}, where the GPU engine runs: compute capability {}, {} \
         of {} GiB free",
        device.compute_capability,
        gibibytes(device.free_bytes),
        gibibytes(device.total_bytes)
    )
}

/// The trial run's line, with no line break: `ready`, the wrong
/// probability it read, or why it failed.
fn write_trial_run(
    f: &mut fmt::Formatter<'_>,
    trial_run: &Result<f64, CanonicalMpsError<CudaAlgebraError>>,
) -> fmt::Result {
    write!(f, "trial run on CUDA device {DEVICE_ORDINAL}: ")?;
    match trial_run {
        Ok(probability) if is_right_trial_run(*probability) => write!(f, "ready"),
        Ok(probability) => write!(f, "{}", wrong_probability(*probability)),
        Err(error) => write!(f, "{}", with_sources(error)),
    }
}

/// What the trial run read when it read `probability`, a wrong one, set
/// against the exact one. The report's line and the advice both say it.
fn wrong_probability(probability: f64) -> String {
    format!("read the probability {probability} of One, where {TRIAL_RUN_PROBABILITY} is exact")
}

/// Lists each library of `report` as its guide in `guides` shows it, then
/// each fault found in it, one item a line. It shows what discovery found,
/// whether or not it succeeded. A library with no path shows only its
/// faults, which say why.
fn write_libraries(
    f: &mut fmt::Formatter<'_>,
    guides: &[SetupGuide],
    report: &AvailabilityReport,
) -> fmt::Result {
    for library in &report.libraries {
        if library.path.is_some() {
            for line in (SetupGuide::for_library(guides, library).describe)(library) {
                writeln!(f, "{line}")?;
            }
        }
        for fault in &library.faults {
            writeln!(f, "{} at fault: {fault}", library.name)?;
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

/// The probability of One the trial run must read: the Bell state it
/// prepares gives exactly 1/2 on either qubit.
const TRIAL_RUN_PROBABILITY: f64 = 0.5;

/// The largest distance the check accepts between the trial run's
/// probability and [`TRIAL_RUN_PROBABILITY`]. The rounding error of a
/// two-qubit state after one gate split is near 1e-15; this margin is far
/// wider, so that rounding never fails the check.
const TRIAL_RUN_TOLERANCE: f64 = 1e-6;

/// How many qubits the trial run prepares: two, the fewest an entangling
/// gate needs.
const TRIAL_RUN_QUBITS: usize = 2;

/// The trial run's max bond dimension χ: 2, the Schmidt rank of a Bell
/// state, so no truncation changes the probability it reads.
const TRIAL_RUN_MAX_BOND_DIMENSION: usize = 2;

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

/// The trial run on `engine`: prepares two qubits, applies a Hadamard to
/// the first and a CNOT from it to the second, and returns the second's
/// probability of One, [`TRIAL_RUN_PROBABILITY`] when the engine computes
/// right.
///
/// ```text
/// qubit 0  |0> --H--*------
///                   |
/// qubit 1  |0> -----X------ probability of One: 1/2
/// ```
///
/// The CNOT is there for the GPU. The engine applies a one-qubit gate and
/// reads a probability on the host, so a one-qubit run would only copy
/// sites to and from the device. It applies an entangling gate by the
/// algebra's gate split, the cuTensorNet computation every entangling gate
/// of a run needs. A max bond dimension of
/// [`TRIAL_RUN_MAX_BOND_DIMENSION`] keeps the whole Bell state.
#[cfg(any(test, all(target_os = "linux", target_arch = "x86_64")))]
fn run_trial<E: tensornet::MpsEngine>(engine: &E) -> Result<f64, E::Error> {
    use num_complex::Complex64;

    let capacity = tensornet::MpsShape::qubit_chain(TRIAL_RUN_QUBITS, TRIAL_RUN_MAX_BOND_DIMENSION)
        .expect("two qubits are a chain");
    let mut state = engine.prepare(&capacity)?;
    let (zero, one) = (Complex64::new(0.0, 0.0), Complex64::new(1.0, 0.0));
    let amplitude = Complex64::new(std::f64::consts::FRAC_1_SQRT_2, 0.0);
    let hadamard = [amplitude, amplitude, amplitude, -amplitude];
    #[rustfmt::skip]
    let cnot = [
        one, zero, zero, zero,
        zero, one, zero, zero,
        zero, zero, zero, one,
        zero, zero, one, zero,
    ];
    engine.apply(&mut state, &[0], &hadamard)?;
    engine.apply(&mut state, &[0, 1], &cnot)?;
    engine.probability_of_one(&state, 1)
}

/// `error`, then each error it was caused by, separated by `: `. So a
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
    format!("{:.1}", bytes as f64 / f64::from(BYTES_PER_GIB))
}

/// The bytes in a GiB, 2^30.
const BYTES_PER_GIB: u32 = 1 << 30;

#[cfg(test)]
mod tests;
