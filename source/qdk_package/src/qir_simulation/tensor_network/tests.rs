// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Which Python exception a run's failure raises, and with what message,
//! for the failures the package's Python tests cannot cause: an engine's
//! errors and a stopped run ([`RunFailure::of`]), a GPU engine that was not
//! made ([`engine_not_made`]), the line every GPU setup failure ends with,
//! and a run thread that panicked ([`lost_run`]).

use std::fmt;

use qdk_cutensornet::{GpuSetup, PackagedLibraries};
use qdk_simulators::mps_simulator::MpsResumerError;
use qdk_simulators::qir::{QirResumeError, QirShotId, QirTrap};
use tensornet::{MpsEngineError, Refusal};

use super::detached_run::lost_run;
use super::run_failure::{RunFailure, engine_not_made, gpu_not_set_up};

/// An engine's error, as any engine's may be: a refusal, or a failure of
/// the engine's own.
#[derive(Debug)]
enum StandInError {
    Refused(Refusal),
    OutOfMemory,
}

impl fmt::Display for StandInError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused(refusal) => refusal.fmt(f),
            Self::OutOfMemory => f.write_str("the device ran out of memory"),
        }
    }
}

impl std::error::Error for StandInError {}

impl From<Refusal> for StandInError {
    fn from(refusal: Refusal) -> Self {
        Self::Refused(refusal)
    }
}

impl MpsEngineError for StandInError {
    fn refusal(&self) -> Option<&Refusal> {
        match self {
            Self::Refused(refusal) => Some(refusal),
            Self::OutOfMemory => None,
        }
    }
}

/// The failure of a run whose shot 0 met `error` of the MPS simulation
/// method.
fn shot_0_failure(error: MpsResumerError<StandInError>) -> RunFailure {
    RunFailure::of(QirResumeError::Resumer {
        shot: QirShotId(0),
        error,
    })
}

/// An engine that fails during a run (a device out of memory) raises
/// `OSError` with the failure's message, as a setup problem does.
#[test]
fn an_engine_failure_is_an_os_error() {
    let failure = shot_0_failure(MpsResumerError::Engine(StandInError::OutOfMemory));
    assert!(
        matches!(&failure, RunFailure::Os(message) if message.contains("ran out of memory")),
        "an engine failure is an OSError: {failure:?}"
    );
}

/// An engine that refuses a call (an operator on no site) raises
/// `ValueError`, as the program's other faults do.
#[test]
fn an_engine_refusal_is_a_value_error() {
    let failure = shot_0_failure(MpsResumerError::Engine(StandInError::from(
        Refusal::NoSites,
    )));
    assert!(
        matches!(&failure, RunFailure::Value(message) if message.contains("at least one site")),
        "an engine refusal is a ValueError: {failure:?}"
    );
}

/// A run stopped by its stop flag (the caller's Ctrl-C, whose own exception
/// is raised instead) raises `RuntimeError` saying so.
#[test]
fn a_stopped_run_is_a_runtime_error() {
    let failure = shot_0_failure(MpsResumerError::Stopped);
    assert!(
        matches!(&failure, RunFailure::Runtime(message) if message == "the run was stopped"),
        "a stopped run is a RuntimeError: {failure:?}"
    );
}

/// A shot that traps (an instruction on a qubit past the program's) raises
/// `RuntimeError` naming the shot.
#[test]
fn a_trap_is_a_runtime_error_naming_the_shot() {
    let failure = RunFailure::of::<StandInError>(QirResumeError::Trapped {
        shot: QirShotId(3),
        trap: QirTrap::NoSuchQubit {
            instruction: 0,
            qubit: 5,
            qubits: 2,
        },
    });
    assert!(
        matches!(&failure, RunFailure::Runtime(message) if message.contains(&QirShotId(3).to_string())),
        "a trap is a RuntimeError naming the shot: {failure:?}"
    );
}

/// A GPU engine that is not made on CUDA device 0 (no device, say) raises
/// `OSError` whose message gives the engine's error before the setup
/// check's report, which may not show the cause. The rule holds whatever
/// this host's setup is, so the report is the check's here.
#[test]
fn an_engine_not_made_gives_its_error_then_the_setup_report() {
    let setup = GpuSetup::check(&PackagedLibraries::default());
    let error = "no CUDA device is visible";
    let RunFailure::Os(message) = engine_not_made(&error, &setup) else {
        panic!("an engine not made is an OSError");
    };
    let error_at = message
        .find(error)
        .expect("the message gives the engine's error");
    let report_at = message
        .find(&setup.to_string())
        .expect("the message gives the setup check's report");
    assert!(
        error_at < report_at,
        "the error comes before the report: {message}"
    );
}

/// A GPU setup that fails, whether discovery failed or the engine was not
/// made, raises a message whose last line offers the engine that needs no
/// GPU, so a user without one knows the choice that runs.
#[test]
fn a_gpu_setup_failure_ends_offering_the_cpu_engine() {
    let setup = GpuSetup::check(&PackagedLibraries::default());
    let failures = [
        ("discovery failed", gpu_not_set_up(&setup)),
        (
            "no engine made",
            engine_not_made(&"no CUDA device is visible", &setup),
        ),
    ];
    for (case, failure) in failures {
        let RunFailure::Os(message) = failure else {
            panic!("{case}: a GPU setup failure is an OSError");
        };
        let last_line = message.lines().last().expect("the message has lines");
        assert!(
            last_line.contains("engine=\"cpu\"") && last_line.contains("without a GPU"),
            "{case}: the last line offers the CPU engine: {message}"
        );
    }
}

/// A run thread that panics (a defect) raises `RuntimeError` with the
/// panic's message, rather than a `PanicException` from the thread scope.
#[test]
fn a_run_that_panics_is_a_runtime_error_with_its_message() {
    let failure = std::thread::scope(|scope| {
        let run_thread = scope.spawn(|| panic!("a defect of the run"));
        lost_run(run_thread)
    });
    assert!(
        matches!(&failure, RunFailure::Runtime(message) if message.contains("a defect of the run")),
        "a run that panics is a RuntimeError with the panic's message: {failure:?}"
    );
}

/// A run thread that panics with a formatted message (an `expect` on an
/// error, an index out of bounds, a `panic!` with arguments: most defects)
/// raises `RuntimeError` with that message too.
#[test]
fn a_run_that_panics_with_a_formatted_message_is_a_runtime_error_with_it() {
    let shot = 3;
    let failure = std::thread::scope(|scope| {
        let run_thread = scope.spawn(|| panic!("a defect of shot {shot}"));
        lost_run(run_thread)
    });
    assert!(
        matches!(&failure, RunFailure::Runtime(message) if message.contains("a defect of shot 3")),
        "a run that panics is a RuntimeError with the panic's formatted message: {failure:?}"
    );
}
