// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Which Python exception a run's failure raises, and with what message,
//! for the failures the package's Python tests cannot cause: the engine's
//! errors ([`Failure::of`]), an engine that was not made
//! ([`engine_not_made`]), and a run thread that panicked ([`lost_run`]).

use qdk_cutensornet::{AvailabilityError, Diagnosis, EngineError};
use qdk_simulators::mps_simulator::MpsResumerError;
use qdk_simulators::qir::{QirResumeError, QirShotId, QirTrap};
use tensornet::Refusal;
use tensornet::reference::ReferenceError;

use super::{Failure, engine_not_made, lost_run};

/// The failure of a run whose shot 0 met `error` of the engine.
fn engine_error(error: ReferenceError) -> Failure {
    Failure::of(QirResumeError::Resumer {
        shot: QirShotId(0),
        error: MpsResumerError::Engine(error),
    })
}

/// An engine that fails during a run (a device out of memory, say; here
/// the reference asked for more sites than it holds) raises `OSError`
/// with the failure's message, as a setup problem does.
#[test]
fn an_engine_failure_is_an_os_error() {
    let failure = engine_error(ReferenceError::ChainTooLong {
        requested: 21,
        max: 20,
    });
    assert!(
        matches!(&failure, Failure::Os(message) if message.contains("at most 20 sites")),
        "an engine failure is an OSError"
    );
}

/// An engine that refuses a call (an operator on no site) raises
/// `ValueError`, as the program's other faults do.
#[test]
fn an_engine_refusal_is_a_value_error() {
    let failure = engine_error(ReferenceError::Refused(Refusal::NoSites));
    assert!(
        matches!(&failure, Failure::Value(message) if message.contains("at least one site")),
        "an engine refusal is a ValueError"
    );
}

/// A shot that traps (an instruction on a qubit past the program's) raises
/// `RuntimeError` naming the shot.
#[test]
fn a_trap_is_a_runtime_error_naming_the_shot() {
    let failure = Failure::of::<ReferenceError>(QirResumeError::Trapped {
        shot: QirShotId(3),
        trap: QirTrap::NoSuchQubit {
            instruction: 0,
            qubit: 5,
            qubits: 2,
        },
    });
    assert!(
        matches!(&failure, Failure::Runtime(message) if message.contains(&QirShotId(3).to_string())),
        "a trap is a RuntimeError naming the shot"
    );
}

/// An engine that is not made on device 0 (no device, here) raises
/// `OSError` whose message gives the engine's error before the setup
/// check's report, which may not show the cause.
#[test]
fn an_engine_not_made_gives_its_error_then_the_setup_report() {
    let diagnosis = Diagnosis {
        availability: Err(AvailabilityError::UnsupportedPlatform {
            platform: "a test platform".to_string(),
        }),
        devices: None,
        engine: None,
    };
    let failure = engine_not_made(&EngineError::NoDevice, &diagnosis);
    let Failure::Os(message) = failure else {
        panic!("an engine not made is an OSError");
    };
    let error = message
        .find(&EngineError::NoDevice.to_string())
        .expect("the message gives the engine's error");
    let report = message
        .find(&diagnosis.to_string())
        .expect("the message gives the setup check's report");
    assert!(
        error < report,
        "the error comes before the report: {message}"
    );
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
        matches!(&failure, Failure::Runtime(message) if message.contains("a defect of the run")),
        "a run that panics is a RuntimeError with the panic's message"
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
        matches!(&failure, Failure::Runtime(message) if message.contains("a defect of shot 3")),
        "a run that panics is a RuntimeError with the panic's formatted message"
    );
}
