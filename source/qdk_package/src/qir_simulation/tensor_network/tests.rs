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
enum FakeEngineError {
    Refused(Refusal),
    OutOfMemory,
}

impl fmt::Display for FakeEngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused(refusal) => refusal.fmt(f),
            Self::OutOfMemory => f.write_str("the device ran out of memory"),
        }
    }
}

impl std::error::Error for FakeEngineError {}

impl From<Refusal> for FakeEngineError {
    fn from(refusal: Refusal) -> Self {
        Self::Refused(refusal)
    }
}

impl MpsEngineError for FakeEngineError {
    fn refusal(&self) -> Option<&Refusal> {
        match self {
            Self::Refused(refusal) => Some(refusal),
            Self::OutOfMemory => None,
        }
    }
}

/// The failure of a run whose shot 0 met `error` of the MPS simulation
/// method.
fn shot_0_failure(error: MpsResumerError<FakeEngineError>) -> RunFailure {
    RunFailure::of(QirResumeError::Resumer {
        shot: QirShotId(0),
        error,
    })
}

/// An engine that fails during a run (a device out of memory) raises
/// `OSError` with the failure's message, as a setup problem does.
#[test]
fn an_engine_failure_is_an_os_error() {
    let failure = shot_0_failure(MpsResumerError::Engine(FakeEngineError::OutOfMemory));
    assert!(
        matches!(&failure, RunFailure::Os(message) if message.contains("ran out of memory")),
        "an engine failure is an OSError: {failure:?}"
    );
}

/// An engine that refuses a call (an operator on no site) raises
/// `ValueError`, as the program's other faults do.
#[test]
fn an_engine_refusal_is_a_value_error() {
    let failure = shot_0_failure(MpsResumerError::Engine(FakeEngineError::from(
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
    let failure = RunFailure::of::<FakeEngineError>(QirResumeError::Trapped {
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
/// panic's message, rather than a `PanicException` from the thread scope,
/// whether the message is a literal or formatted (an `expect` on an error,
/// an index out of bounds, a `panic!` with arguments: most defects), which
/// reach the thread's join as a `&str` and a `String` payload.
#[test]
fn a_run_that_panics_is_a_runtime_error_with_its_message() {
    fn literal() {
        panic!("a defect of the run");
    }
    fn formatted() {
        let shot = 3;
        panic!("a defect of shot {shot}");
    }
    let cases: [(&str, fn(), &str); 2] = [
        ("literal", literal, "a defect of the run"),
        ("formatted", formatted, "a defect of shot 3"),
    ];
    for (case, defect, expected) in cases {
        let failure = std::thread::scope(|scope| lost_run(scope.spawn(defect)));
        assert!(
            matches!(&failure, RunFailure::Runtime(message) if message.contains(expected)),
            "{case}: a run that panics is a RuntimeError with the panic's message: {failure:?}"
        );
    }
}
