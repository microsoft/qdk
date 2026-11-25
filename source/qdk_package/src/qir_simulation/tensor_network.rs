// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The native side of `qdk.simulation.tensornetwork_qir` and
//! `check_tensornetwork_setup`.
//!
//! Python hands it a lowered QIR program; it runs the program's shots by
//! the MPS simulation method and gives the records and the run's report.
//! It joins the pieces, so that no Python code knows engines and no Rust
//! crate knows Python. A run leaves the Python thread, so that other
//! Python threads run and Ctrl-C stops it
//! (`tensor_network/detached_run.rs`); a failed run raises the exception
//! of who must act (`tensor_network/run_failure.rs`).

mod detached_run;
mod run_failure;

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

use super::{adaptive_program_from_pydict, output_records_to_pylist};
use detached_run::run_detached;
use run_failure::{RunFailure, gpu_not_set_up};

/// Runs shots `0..shots` of `program` by the MPS simulation method, on the
/// engine named `engine` (`"gpu"` or `"cpu"`), drawing from `seed`, with
/// bonds of at most `max_bond_dimension`. Gives each shot's output records
/// and the run's report.
///
/// `program` is the dict
/// `AdaptiveProfilePass(Bytecode.Bit64).run(module).as_dict()` gives. The
/// report is a dict of `largest_bond_dimension`, `discarded_weight`,
/// `max_bond_dimension` and `truncation_warning` (its text, or `None`).
/// `packaged` lists the library files NVIDIA's wheels installed, which GPU
/// discovery tries; `install_command` installs them, and the setup report
/// gives it as the fix.
///
/// A run takes three steps, each with its own failures:
///
/// 1. The program is built and the settings checked: `ValueError` for a
///    faulty program or a max bond dimension of 0.
/// 2. The engine is chosen by name: `"gpu"`, the canonical MPS engine on
///    cuTensorNet, on CUDA device 0, with the libraries discovery finds;
///    or `"cpu"`, the same engine on the host CPU, which needs no setup.
///    `ValueError` for any other name.
/// 3. On the GPU, discovery finds the libraries. Then, off the Python
///    thread, the engine is made and `MpsResumer` runs the shots on it.
///    Each failure of this step raises as [`RunFailure`] says: the
///    libraries not set up, the engine not made, or the run failing, the
///    method's own refusals among them (a readout-noise call, a max bond
///    dimension too large).
#[pyfunction(name = "_sample_mps")]
#[allow(
    clippy::needless_pass_by_value,
    reason = "pyo3 extracts arguments by value"
)]
#[allow(
    clippy::too_many_arguments,
    reason = "a Python function's arguments, passed on from `tensornetwork_qir`"
)]
pub(crate) fn sample_mps<'py>(
    py: Python<'py>,
    program: &Bound<'py, PyDict>,
    shots: u32,
    seed: u32,
    max_bond_dimension: usize,
    packaged: Vec<PathBuf>,
    install_command: String,
    engine: &str,
) -> PyResult<(Py<PyAny>, Bound<'py, PyDict>)> {
    let program = qir_program(program)?;
    let settings = RunSettings::new(&program, shots, seed, max_bond_dimension)?;
    let (records, report) = match EngineChoice::named(engine)? {
        EngineChoice::Gpu => gpu_run(
            py,
            &settings,
            &packaged_libraries(packaged, install_command),
        )?,
        EngineChoice::Cpu => cpu_run(py, &settings)?,
    };
    Ok((
        output_records_to_pylist(py, records)?,
        report_dict(py, &report)?,
    ))
}

/// The GPU engine's setup check, with the library files `packaged` and the
/// fix's `install_command`: whether the engine is ready, the report, the
/// advice for the first problem, and the warnings. Never fails.
#[pyfunction(name = "_check_tensornetwork_setup")]
#[allow(
    clippy::needless_pass_by_value,
    reason = "pyo3 extracts arguments by value"
)]
pub(crate) fn check_tensornetwork_setup(
    py: Python<'_>,
    packaged: Vec<PathBuf>,
    install_command: String,
) -> (bool, String, Option<String>, Vec<String>) {
    let setup = setup_check(py, &packaged_libraries(packaged, install_command));
    (
        setup.is_ready(),
        setup.to_string(),
        setup.advice(),
        setup.warnings(),
    )
}

/// The program `program` describes, a dict as
/// `AdaptiveProfilePass(Bytecode.Bit64).run(module).as_dict()` gives it.
/// Fails with `ValueError`, giving every fault the program has.
fn qir_program(program: &Bound<'_, PyDict>) -> PyResult<QirProgram> {
    let bytecode = adaptive_program_from_pydict::<u64>(program)?;
    QirProgram::new(bytecode).map_err(|faults| PyValueError::new_err(faults.to_string()))
}

/// What a run needs besides its engine.
struct RunSettings<'p> {
    /// The program whose shots run.
    program: &'p QirProgram,
    /// The number of shots, whose ids are `0..shots`.
    shots: u32,
    /// The seed every shot's draws come from, widened to the resumer's
    /// `u64`.
    seed: u64,
    /// The max bond dimension χ: every bond of a state is at most χ.
    max_bond_dimension: NonZeroUsize,
}

impl<'p> RunSettings<'p> {
    /// The settings of a run of `shots` shots of `program`, drawing from
    /// `seed`, with bonds of at most `max_bond_dimension`. Fails with
    /// `ValueError` when `max_bond_dimension` is 0.
    fn new(
        program: &'p QirProgram,
        shots: u32,
        seed: u32,
        max_bond_dimension: usize,
    ) -> PyResult<Self> {
        let max_bond_dimension = NonZeroUsize::new(max_bond_dimension)
            .ok_or_else(|| PyValueError::new_err("max_bond_dimension must be at least 1"))?;
        Ok(Self {
            program,
            shots,
            seed: u64::from(seed),
            max_bond_dimension,
        })
    }
}

/// The engine a caller names: the GPU's or the CPU's.
enum EngineChoice {
    /// `"gpu"`: the canonical MPS engine on cuTensorNet, on CUDA device 0.
    Gpu,
    /// `"cpu"`: the canonical MPS engine on the host CPU.
    Cpu,
}

impl EngineChoice {
    /// The engine `name` names, `"gpu"` or `"cpu"`. Fails with
    /// `ValueError` for any other name.
    fn named(name: &str) -> PyResult<Self> {
        match name {
            "gpu" => Ok(Self::Gpu),
            "cpu" => Ok(Self::Cpu),
            other => Err(PyValueError::new_err(format!(
                "Invalid engine: '{other}'. Use \"gpu\" or \"cpu\"."
            ))),
        }
    }
}

/// What a run gives: each shot's output records, and the run's report.
type RunResults = (Vec<Vec<OutputRecord>>, MpsRunReport);

/// The library files Python found in NVIDIA's wheels, `files`, with the
/// command that installs those wheels, `install_command`, which the setup
/// report gives as its fix.
fn packaged_libraries(files: Vec<PathBuf>, install_command: String) -> PackagedLibraries {
    PackagedLibraries {
        files,
        install_command: Some(install_command),
    }
}

/// The GPU engine's setup check, with the GIL released: it loads the
/// libraries and runs on the GPU, which can take seconds.
fn setup_check(py: Python<'_>, packaged: &PackagedLibraries) -> GpuSetup {
    py.detach(|| GpuSetup::check(packaged))
}

/// The run on the CPU engine, which needs no setup.
fn cpu_run(py: Python<'_>, settings: &RunSettings<'_>) -> PyResult<RunResults> {
    run_detached(py, |stop_flag| {
        run_on_engine(settings, || Ok(CpuMpsEngine::new(CpuAlgebra)), stop_flag)
    })
}

/// The run on the GPU engine, on CUDA device 0, with the libraries
/// [`qdk_cutensornet::discover_with`] finds, loads and checks (trying the
/// `packaged` files). Discovery loads the libraries, so it runs with the
/// GIL released.
///
/// Discovery fails when the machine's setup is at fault: a library
/// missing or of a version not supported, say
/// ([`LibraryFault`](qdk_cutensornet::LibraryFault) lists the cases). The
/// user must act, so the failure raises the setup check's report, which
/// says what to fix. The
/// check runs again, so that the text is the check's; it fails as
/// discovery did unless a file or a variable changed in between, or a load
/// failed for a passing reason (the process out of file handles, say).
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn gpu_run(
    py: Python<'_>,
    settings: &RunSettings<'_>,
    packaged: &PackagedLibraries,
) -> PyResult<RunResults> {
    let availability = py
        .detach(|| qdk_cutensornet::discover_with(packaged))
        .map_err(|_| gpu_not_set_up(&setup_check(py, packaged)).into_py_err())?;
    run_detached(py, move |stop_flag| {
        run_on_engine(settings, || gpu_engine(&availability, packaged), stop_flag)
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
    Err(gpu_not_set_up(&setup_check(py, packaged)).into_py_err())
}

/// The GPU engine, on CUDA device 0, on the libraries `availability`
/// holds: `CudaMpsEngine::try_from` (`CudaAlgebra::new`), which selects the
/// device and creates its stream, the cuTensorNet handle, the workspace
/// descriptor and the gate's buffer. That fails on the machine's side (no
/// device visible, the device busy or out of memory), or on a defect in
/// the engine's own code; the failure gives the engine's error, then the
/// setup check's report with the library files `packaged` names, so the
/// user sees both the cause and the fix.
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
fn run_on_engine<E: MpsEngine>(
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
/// most weight a shot lost, `1 - least_retained_weight`;
/// `truncation_warning` is [`MpsRunReport::truncation_warning`]'s text,
/// given when a shot lost more than the method's
/// [`TRUNCATION_TOLERANCE`][tolerance].
///
/// [tolerance]: qdk_simulators::mps_simulator::TRUNCATION_TOLERANCE
fn report_dict<'py>(py: Python<'py>, report: &MpsRunReport) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new(py);
    dict.set_item("largest_bond_dimension", report.largest_bond_dimension)?;
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
