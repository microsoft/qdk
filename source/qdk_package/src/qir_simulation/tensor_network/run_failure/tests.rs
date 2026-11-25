// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Which Python exception a run's failure raises, and with what message,
//! for the failures the package's Python tests cannot cause: an engine's
//! failure and a stopped run ([`RunFailure::of`]), and a GPU engine that
//! was not made ([`engine_not_made`]). And that a failure's message shows
//! each error of its chain once, which needs the chain the Python side
//! cannot see. A trap, an engine's refusal and a GPU not set up, which a
//! program or a setup causes, are pinned by the Python tests alone.

use std::error::Error;
use std::fmt;

use canonical_mps::CanonicalMpsError;
use qdk_cutensornet::{CudaAlgebraError, GpuSetup, PackagedLibraries};
use qdk_simulators::mps_simulator::MpsResumerError;
use qdk_simulators::qir::{QirResumeError, QirShotId};
use tensornet::{MpsEngineError, MpsShapeError, Refusal};

use super::{RunFailure, engine_not_made};

/// An engine's error, as any engine's may be: a refusal, or a failure of
/// the engine's own.
#[derive(Debug)]
enum FakeEngineError {
    /// The engine refused a call: the program's or the settings' fault.
    Refused(Refusal),
    /// The engine failed on its own: the machine's fault.
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

impl Error for FakeEngineError {}

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
fn shot_0_failure<E: MpsEngineError>(error: MpsResumerError<E>) -> RunFailure {
    RunFailure::of(QirResumeError::Resumer {
        shot: QirShotId(0),
        error,
    })
}

/// The failure of a run whose shot 0 met `error`, with the message of each
/// error in `error`'s chain: `error`'s own, then each source's.
fn shot_0_failure_and_its_chain<E: MpsEngineError + 'static>(
    error: MpsResumerError<E>,
) -> (RunFailure, Vec<String>) {
    let mut chain = vec![error.to_string()];
    let mut source = error.source();
    while let Some(cause) = source {
        chain.push(cause.to_string());
        source = cause.source();
    }
    (shot_0_failure(error), chain)
}

/// The Python exception `failure` raises, by name, and its message.
fn raised(failure: &RunFailure) -> (&'static str, &str) {
    match failure {
        RunFailure::Os(message) => ("OSError", message),
        RunFailure::Value(message) => ("ValueError", message),
        RunFailure::Runtime(message) => ("RuntimeError", message),
    }
}

/// Each failure the Python tests cannot cause raises the exception of the
/// party that must act: an engine failing during a run (a device out of
/// memory) is an `OSError`, as a setup problem is, the machine must act; a
/// run stopped by its stop flag (the caller's Ctrl-C, whose own exception
/// is raised instead) is a `RuntimeError` saying it stopped.
#[test]
fn each_run_failure_raises_the_exception_of_who_must_act() {
    let cases = [
        (
            "an engine failure",
            shot_0_failure(MpsResumerError::Engine(FakeEngineError::OutOfMemory)),
            "OSError",
            "out of memory",
        ),
        (
            "a stopped run",
            shot_0_failure(MpsResumerError::<FakeEngineError>::Stopped),
            "RuntimeError",
            "stopped",
        ),
    ];
    for (case, failure, exception, says) in cases {
        let (exception_raised, message) = raised(&failure);
        assert_eq!(exception_raised, exception, "{case}: {failure:?}");
        assert!(message.contains(says), "{case} says {says:?}: {message}");
    }
}

/// A failed shot's message shows each error of its chain once, as an
/// `OSError` or a `ValueError`. Each row is a chain the run's error, naming
/// the shot, heads: it shows its source in its message and gives it as its
/// source too. An `OSError`: the MPS simulation method's engine failure,
/// whose source is the canonical MPS engine's algebra failure, which says
/// only that the site algebra failed and keeps the CUDA call that failed
/// as its source. A `ValueError`: the method's capacity failure, which
/// keeps why the shape is no chain as its source.
#[test]
fn a_failures_message_shows_each_cause_once() {
    let cuda_call_failed = CudaAlgebraError::NativeCallFailed {
        component: "CUDA Runtime",
        operation: "cudaMalloc",
        status: 2,
        message: "out of memory".to_string(),
    };
    let cases = [
        (
            "the canonical engine's algebra failure on the GPU",
            shot_0_failure_and_its_chain(MpsResumerError::Engine(CanonicalMpsError::Algebra(
                cuda_call_failed,
            ))),
        ),
        (
            "a max bond dimension too large for the program's qubits",
            shot_0_failure_and_its_chain(MpsResumerError::<FakeEngineError>::Capacity {
                qubits: 70,
                max_bond_dimension: 1 << 40,
                source: MpsShapeError::ElementCountOverflow { site: 35 },
            }),
        ),
    ];
    for (case, (failure, chain)) in cases {
        assert!(chain.len() >= 2, "{case}: the failure wraps a cause");
        let (_, message) = raised(&failure);
        for error in &chain {
            assert_eq!(
                message.matches(error.as_str()).count(),
                1,
                "{case}: the message gives {error:?} once: {message}"
            );
        }
    }
}

/// A GPU engine that is not made on CUDA device 0 (no device, say) raises
/// `OSError` whose message gives the engine's error, then the setup
/// check's report, which may not show the cause, and ends offering the
/// engine that needs no GPU, so a user without one knows the choice that
/// runs. The rule holds whatever this host's setup is, so the report is the
/// check's here. (A GPU not set up, which a setup causes, ends the same
/// way: the Python tests pin it.)
#[test]
fn an_engine_not_made_gives_its_error_then_the_setup_report_then_the_cpu_engine() {
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
    let last_line = message.lines().last().expect("the message has lines");
    assert!(
        last_line.contains("engine=\"cpu\""),
        "the last line offers the CPU engine: {message}"
    );
}
