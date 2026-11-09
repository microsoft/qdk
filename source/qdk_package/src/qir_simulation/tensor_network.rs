// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The native side of `qdk.simulation.tensornetwork_qir` and
//! `check_tensornetwork_setup`: a QIR program's shots on matrix product
//! states, with the run's report, and the setup check of the GPU.
//!
//! A chemistry user runs an Adaptive QIR program (H₂'s phase estimation)
//! on cuTensorNet from Python; this module joins the pieces that do it, so
//! that no Python code knows engines or resumers: the program, decoded from
//! the dict `AdaptiveProfilePass(Bytecode.Bit64)` writes, is built as a
//! `QirProgram` (every fault reported), the libraries are discovered (with
//! the paths of NVIDIA's wheels that Python found), and `MpsResumer` runs
//! the shots on the engine, on a thread of its own with the GIL released.
//! The caller's thread checks for Python signals every 100 ms and, on
//! Ctrl-C, sets the resumer's stop flag, so the run ends within one batch
//! and frees the device. A later `run_qir(type="mps")` calls the same
//! function.
//!
//! Errors keep the split users' handlers rely on: the setup (no library, no
//! device, a device failure) is `OSError`, its message the report
//! `check_tensornetwork_setup` shows; the program (faults, a call the method
//! does not run, a max bond dimension that makes no chain, an engine's
//! refusal) is `ValueError`; a trap is `RuntimeError` naming the shot; an
//! interrupt is `KeyboardInterrupt`.
//!
//! The engine is chosen by name: `"cutensornet"`, the only one the public
//! API passes, or `"reference"`, the exact reference, for the package's
//! tests on hosts with no GPU.

use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use pyo3::{
    exceptions::{PyOSError, PyRuntimeError, PyValueError},
    prelude::*,
    types::PyDict,
};
use qdk_cutensornet::{EngineError, PackagedLibraries, diagnose, discover_with};
use qdk_simulators::OutputRecord;
use qdk_simulators::mps_simulator::{MpsResumer, MpsResumerError, MpsRunReport};
use qdk_simulators::qir::{
    QirMachine, QirProgram, QirResumeError, QirResumer, QirShotId, expect_ready,
};
use tensornet::reference::ExactReference;
use tensornet::{MpsEngine, MpsEngineError};

use super::{adaptive_program_from_pydict, cpu_simulators::output_records_to_pylist};

/// How often the caller's thread checks for a Python signal while a run is
/// under way.
const SIGNAL_CHECK: Duration = Duration::from_millis(100);

/// Runs `shots` shots (ids `0..shots`) of `program`, a dict as
/// `AdaptiveProfilePass(Bytecode.Bit64).as_dict()` gives it, on the MPS
/// method with bonds of at most `max_bond_dimension`, drawing from `seed`,
/// on the engine named `engine`. Gives each shot's output records and the
/// run's report: a dict of `largest_bond`, `discarded_weight`,
/// `max_bond_dimension` and `truncation_warning` (the warning's text, or
/// `None`).
///
/// `packaged` holds the paths of `libcutensornet.so.2` and
/// `libcudart.so.12` that Python packages installed, if any.
#[pyfunction]
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn _tensor_network_sample<'py>(
    py: Python<'py>,
    program: &Bound<'py, PyDict>,
    shots: u32,
    seed: u32,
    max_bond_dimension: usize,
    packaged: (Option<PathBuf>, Option<PathBuf>),
    engine: &str,
) -> PyResult<(Py<PyAny>, Bound<'py, PyDict>)> {
    let bytecode = adaptive_program_from_pydict::<u64>(program)?;
    let program =
        QirProgram::new(bytecode).map_err(|faults| PyValueError::new_err(faults.to_string()))?;
    let max_bond_dimension = NonZeroUsize::new(max_bond_dimension)
        .ok_or_else(|| PyValueError::new_err("max_bond_dimension must be at least 1"))?;
    let settings = Settings {
        program: &program,
        shots,
        seed: u64::from(seed),
        max_bond_dimension,
    };
    let (records, report) = match engine {
        "cutensornet" => {
            // A setup that fails raises the setup check's report: the check is
            // run again for it, so the text is the check's by construction.
            let packaged = packaged_libraries(packaged);
            let availability = discover_with(&packaged)
                .map_err(|_| PyOSError::new_err(diagnose(&packaged).to_string()))?;
            run_detached(py, &settings, move || {
                cutensornet_engine(&availability)
                    .map_err(|_| Failure::Os(diagnose(&packaged).to_string()))
            })?
        }
        "reference" => run_detached(py, &settings, || Ok(ExactReference::new()))?,
        other => {
            return Err(PyValueError::new_err(format!(
                "unknown tensor-network engine {other:?}"
            )));
        }
    };
    Ok((
        output_records_to_pylist(py, records)?,
        report_dict(py, &report)?,
    ))
}

/// The setup check: the diagnosis of the GPU setup, as `diagnose` gives it
/// with the libraries `packaged` names: whether the method is ready, the
/// report a user reads, the fix for the first problem, if any, and the
/// warnings (other copies of a library). Never fails.
#[pyfunction]
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn _check_tensornetwork_setup(
    py: Python<'_>,
    packaged: (Option<PathBuf>, Option<PathBuf>),
) -> (bool, String, Option<String>, Vec<String>) {
    let packaged = packaged_libraries(packaged);
    let diagnosis = py.detach(move || diagnose(&packaged));
    (
        diagnosis.is_ready(),
        diagnosis.to_string(),
        diagnosis.advice(),
        diagnosis.warnings(),
    )
}

fn packaged_libraries(
    (cutensornet, cuda_runtime): (Option<PathBuf>, Option<PathBuf>),
) -> PackagedLibraries {
    PackagedLibraries {
        cutensornet,
        cuda_runtime,
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn cutensornet_engine(
    availability: &qdk_cutensornet::Availability,
) -> Result<qdk_cutensornet::CuTensorNetEngine, EngineError> {
    availability.mps_engine()
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn cutensornet_engine(
    _availability: &qdk_cutensornet::Availability,
) -> Result<ExactReference, EngineError> {
    unreachable!("discovery fails off Linux x86-64")
}

/// What a run needs besides its engine.
struct Settings<'p> {
    program: &'p QirProgram,
    shots: u32,
    seed: u64,
    max_bond_dimension: NonZeroUsize,
}

/// Why a run gave no records, as the Python exception it raises.
enum Failure {
    /// The setup or the device: `OSError`.
    Os(String),
    /// The program or the settings: `ValueError`.
    Value(String),
    /// A shot trapped: `RuntimeError`.
    Runtime(String),
    /// The stop flag ended the run.
    Stopped,
}

impl Failure {
    /// The failure a run's `error` is.
    fn of<E: MpsEngineError>(error: QirResumeError<MpsResumerError<E>>) -> Self {
        let message = error.to_string();
        match error {
            QirResumeError::Resumer {
                error: MpsResumerError::Stopped,
                ..
            } => Self::Stopped,
            QirResumeError::Resumer {
                error: MpsResumerError::Engine(engine),
                ..
            } if engine.refusal().is_none() => Self::Os(message),
            QirResumeError::Resumer { .. } => Self::Value(message),
            _ => Self::Runtime(message),
        }
    }

    fn into_py_err(self) -> PyErr {
        match self {
            Self::Os(message) => PyOSError::new_err(message),
            Self::Value(message) => PyValueError::new_err(message),
            Self::Runtime(message) => PyRuntimeError::new_err(message),
            Self::Stopped => PyRuntimeError::new_err("the run was stopped"),
        }
    }
}

/// Makes the engine with `make_engine` and runs `settings` on it, on a
/// thread of its own (the engine never leaves it: cuTensorNet's states are
/// not `Send`), with the GIL released. The caller's thread checks for
/// signals every [`SIGNAL_CHECK`]; on one (Ctrl-C) it sets the stop flag,
/// waits for the run to end, and raises the signal's exception.
fn run_detached<E, F>(
    py: Python<'_>,
    settings: &Settings<'_>,
    make_engine: F,
) -> PyResult<(Vec<Vec<OutputRecord>>, MpsRunReport)>
where
    E: MpsEngine,
    E::Error: MpsEngineError,
    F: FnOnce() -> Result<E, Failure> + Send,
{
    let stop = Arc::new(AtomicBool::new(false));
    let (sender, mut outcomes) = mpsc::channel();
    std::thread::scope(|scope| {
        let stop_flag = Arc::clone(&stop);
        scope.spawn(move || {
            // The receiver may be gone only if the caller's thread panicked.
            let _ = sender.send(run(settings, make_engine, stop_flag));
        });
        let mut interrupt = None;
        loop {
            let (waited, returned) = py.detach(move || {
                let waited = outcomes.recv_timeout(SIGNAL_CHECK);
                (waited, outcomes)
            });
            outcomes = returned;
            match waited {
                Ok(outcome) => {
                    return match (interrupt, outcome) {
                        (Some(interrupt), _) => Err(interrupt),
                        (None, outcome) => outcome.map_err(Failure::into_py_err),
                    };
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if interrupt.is_none()
                        && let Err(signal) = py.check_signals()
                    {
                        stop.store(true, Ordering::Relaxed);
                        interrupt = Some(signal);
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(PyRuntimeError::new_err(
                        "the tensor-network run ended without an outcome",
                    ));
                }
            }
        }
    })
}

/// The run itself, on its own thread.
fn run<E, F>(
    settings: &Settings<'_>,
    make_engine: F,
    stop: Arc<AtomicBool>,
) -> Result<(Vec<Vec<OutputRecord>>, MpsRunReport), Failure>
where
    E: MpsEngine,
    E::Error: MpsEngineError,
    F: FnOnce() -> Result<E, Failure>,
{
    let engine = make_engine()?;
    let mut resumer = MpsResumer::new(engine, settings.seed)
        .with_max_bond_dimension(settings.max_bond_dimension)
        .with_stop(stop);
    let records = expect_ready(resumer.run(
        &QirMachine::new(settings.program),
        &QirShotId::first(settings.shots),
    ))
    .map_err(Failure::of)?;
    Ok((records, *resumer.last_run()))
}

fn report_dict<'py>(py: Python<'py>, report: &MpsRunReport) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new(py);
    dict.set_item("largest_bond", report.largest_bond)?;
    dict.set_item("discarded_weight", 1.0 - report.least_retained_weight)?;
    dict.set_item("max_bond_dimension", report.max_bond_dimension)?;
    dict.set_item(
        "truncation_warning",
        report
            .truncation_warning()
            .map(|warning| warning.to_string()),
    )?;
    Ok(dict)
}
