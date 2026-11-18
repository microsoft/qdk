// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The native side of `qdk.simulation.tensornetwork_qir` and
//! `check_tensornetwork_setup`: a QIR program's shots on matrix product
//! states (MPS), on the GPU or the CPU, with the run's report; and the check
//! of the GPU's setup.
//!
//! Python calls it to run an Adaptive QIR program's shots, for minutes to
//! hours. This module joins the pieces that run it, so that no Python code
//! knows engines or resumers and no Rust crate knows Python: the program,
//! decoded from the dict `AdaptiveProfilePass(Bytecode.Bit64)` writes, is
//! built as a `QirProgram` (every fault reported), and `MpsResumer` runs
//! its shots on the engine the caller named, off the Python thread.
//!
//! Two kinds of method meet here. An _MPS simulation method_ is the
//! algorithm that runs a program's gates, measurements and shots on an
//! MPS; this module runs `qdk_simulators`' (`MpsResumer`). It calls the
//! _`MpsEngine` methods_ (`apply`, `project`, `probability_of_one`, …), the
//! operations the trait declares and each engine implements.
//!
//! The engine is chosen by name: `"gpu"`, the canonical MPS engine on CUDA
//! device 0 (`CudaMpsEngine`, on cuTensorNet), whose libraries discovery
//! finds, with the files of NVIDIA's wheels that Python found; or `"cpu"`,
//! the same engine on the host CPU (`CpuMpsEngine`), which needs no setup.
//! Which Python exception each failure raises, and with what message (a
//! GPU not set up among them), is in `tensor_network/run_failure.rs`; how
//! the run leaves the Python thread, and stops on Ctrl-C, in
//! `tensor_network/detached_run.rs`.

mod detached_run;
mod run_failure;
#[cfg(test)]
mod tests;

use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use canonical_mps::{CpuAlgebra, CpuMpsEngine};
use pyo3::{exceptions::PyValueError, prelude::*, types::PyDict};
use qdk_cutensornet::{GpuSetup, PackagedLibraries};
use qdk_simulators::OutputRecord;
use qdk_simulators::mps_simulator::{MpsResumer, MpsRunReport};
use qdk_simulators::qir::{QirMachine, QirProgram, QirResumer, QirShotId, expect_ready};
use tensornet::MpsEngine;

use super::{adaptive_program_from_pydict, cpu_simulators::output_records_to_pylist};
use detached_run::run_detached;
use run_failure::{RunFailure, gpu_not_set_up};

/// Runs `shots` shots (ids `0..shots`) of `program`, a dict as
/// `AdaptiveProfilePass(Bytecode.Bit64).as_dict()` gives it, by the MPS
/// simulation method, with bonds of at most `max_bond_dimension`, drawing
/// from `seed`, on the engine named `engine`: `"gpu"` or `"cpu"`. Gives
/// each shot's output records and the run's report: a dict of
/// `largest_bond`, `discarded_weight`, `max_bond_dimension` and
/// `truncation_warning` (the warning's text, or `None`).
///
/// `packaged` holds the library files that Python packages installed, and
/// `install_command` the command that installs them; the GPU engine's
/// discovery tries those files, and its setup report gives that command.
#[pyfunction]
#[allow(
    clippy::needless_pass_by_value,
    reason = "pyo3 extracts arguments by value"
)]
#[allow(
    clippy::too_many_arguments,
    reason = "a Python function's arguments, passed on from `tensornetwork_qir`"
)]
pub(crate) fn _tensor_network_sample<'py>(
    py: Python<'py>,
    program: &Bound<'py, PyDict>,
    shots: u32,
    seed: u32,
    max_bond_dimension: usize,
    packaged: Vec<PathBuf>,
    install_command: Option<String>,
    engine: &str,
) -> PyResult<(Py<PyAny>, Bound<'py, PyDict>)> {
    let bytecode = adaptive_program_from_pydict::<u64>(program)?;
    let program =
        QirProgram::new(bytecode).map_err(|faults| PyValueError::new_err(faults.to_string()))?;
    let max_bond_dimension = NonZeroUsize::new(max_bond_dimension)
        .ok_or_else(|| PyValueError::new_err("max_bond_dimension must be at least 1"))?;
    let settings = RunSettings {
        program: &program,
        shots,
        seed: u64::from(seed),
        max_bond_dimension,
    };
    let (records, report) = match engine {
        "gpu" => {
            let packaged = PackagedLibraries {
                files: packaged,
                install_command,
            };
            gpu_run(py, &settings, &packaged)?
        }
        "cpu" => run_detached(py, |stop_flag| {
            run(&settings, || Ok(CpuMpsEngine::new(CpuAlgebra)), stop_flag)
        })?,
        other => {
            return Err(PyValueError::new_err(format!(
                "Invalid engine: '{other}'. Use \"gpu\" or \"cpu\"."
            )));
        }
    };
    Ok((
        output_records_to_pylist(py, records)?,
        report_dict(py, &report)?,
    ))
}

/// The setup check of the GPU engine, with the library files `packaged`
/// names and the packager's `install_command`, as `GpuSetup::check` makes
/// it: whether the GPU engine is ready, the report a user reads, the fix
/// for the first problem, if any, and the warnings. Never fails.
#[pyfunction]
#[allow(
    clippy::needless_pass_by_value,
    reason = "pyo3 extracts arguments by value"
)]
pub(crate) fn _check_tensornetwork_setup(
    py: Python<'_>,
    packaged: Vec<PathBuf>,
    install_command: Option<String>,
) -> (bool, String, Option<String>, Vec<String>) {
    let packaged = PackagedLibraries {
        files: packaged,
        install_command,
    };
    // The check loads the libraries and runs on the GPU, so it runs with
    // the GIL released.
    let setup = py.detach(move || GpuSetup::check(&packaged));
    (
        setup.is_ready(),
        setup.to_string(),
        setup.advice(),
        setup.warnings(),
    )
}

/// What a run needs besides its engine.
struct RunSettings<'p> {
    program: &'p QirProgram,
    shots: u32,
    seed: u64,
    max_bond_dimension: NonZeroUsize,
}

/// What a run gives: each shot's output records, and the run's report.
type RunResults = (Vec<Vec<OutputRecord>>, MpsRunReport);

/// The run on the GPU engine, on CUDA device 0, with the libraries
/// discovery finds (trying the `packaged` files). When discovery fails, the
/// run raises the setup check's report: the check is made again for it, so
/// the text is the check's by construction. Discovery loads the libraries,
/// so it runs with the GIL released.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn gpu_run(
    py: Python<'_>,
    settings: &RunSettings<'_>,
    packaged: &PackagedLibraries,
) -> PyResult<RunResults> {
    let availability = py
        .detach(|| qdk_cutensornet::discover_with(packaged))
        .map_err(|_| gpu_not_set_up(&py.detach(|| GpuSetup::check(packaged))).into_py_err())?;
    run_detached(py, move |stop_flag| {
        run(settings, || gpu_engine(&availability, packaged), stop_flag)
    })
}

/// The run on the GPU engine, off Linux x86-64, where discovery always
/// fails: it raises the setup check's report, as the Linux x86-64 version
/// does when discovery fails.
#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn gpu_run(
    py: Python<'_>,
    _settings: &RunSettings<'_>,
    packaged: &PackagedLibraries,
) -> PyResult<RunResults> {
    Err(gpu_not_set_up(&py.detach(|| GpuSetup::check(packaged))).into_py_err())
}

/// The GPU engine, on CUDA device 0, on the libraries `availability`
/// holds. When it cannot be made, the failure gives why, then the setup
/// check's report with the library files `packaged` names.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn gpu_engine(
    availability: &qdk_cutensornet::Availability,
    packaged: &PackagedLibraries,
) -> Result<qdk_cutensornet::CudaMpsEngine, RunFailure> {
    qdk_cutensornet::CudaMpsEngine::try_from(availability)
        .map_err(|error| run_failure::engine_not_made(&error, &GpuSetup::check(packaged)))
}

/// The run itself, on the run thread: makes the engine with `make_engine`,
/// then runs the shots on it, stopping once `stop_flag` is set.
fn run<E: MpsEngine>(
    settings: &RunSettings<'_>,
    make_engine: impl FnOnce() -> Result<E, RunFailure>,
    stop_flag: Arc<AtomicBool>,
) -> Result<RunResults, RunFailure> {
    let engine = make_engine()?;
    let mut resumer = MpsResumer::new(engine, settings.seed)
        .with_max_bond_dimension(settings.max_bond_dimension)
        .with_stop_flag(stop_flag);
    let records = expect_ready(resumer.run(
        &QirMachine::new(settings.program),
        &QirShotId::first(settings.shots),
    ))
    .map_err(RunFailure::of)?;
    Ok((records, *resumer.last_run()))
}

/// The run's `report` as the dict Python reads: `discarded_weight` is the
/// most weight a shot lost, `1 - least_retained_weight`.
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
