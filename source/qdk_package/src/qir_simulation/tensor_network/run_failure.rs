// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Why a tensor-network run gave no records, as the Python exception its
//! caller handles: [`RunFailure`], and the messages of a GPU engine that
//! is not set up or not made.

#[cfg(test)]
mod tests;

use std::{error::Error, fmt::Display};

use pyo3::{
    PyErr,
    exceptions::{PyOSError, PyRuntimeError, PyValueError},
};
use qdk_cutensornet::GpuSetup;
use qdk_simulators::mps_simulator::MpsResumerError;
use qdk_simulators::qir::QirResumeError;
use tensornet::MpsEngineError;

/// Why a run gave no records, as the Python exception it raises.
///
/// A user's handler decides by who must act, so each exception names one
/// party, whose side the failure is on:
///
/// - [`Os`](Self::Os), `OSError`: the machine. The GPU engine cannot be
///   made (its libraries not usable, no device), or it fails during the
///   run (a CUDA or cuTensorNet call, say). An engine's failure that is
///   not a refusal is taken to be the machine's, though a defect in the
///   engine's own code raises it too; either way, the program and the
///   settings cannot avoid it, so it is reported with the errors that
///   caused it. A setup problem also gives the setup check's report, which
///   says what to fix when the check still finds the problem, and a last
///   line saying that `engine="cpu"` runs without a GPU.
/// - [`Value`](Self::Value), `ValueError`: the program or the settings. A
///   call the MPS simulation method does not run, a max bond dimension too
///   large for the program's qubits, or a call the engine refuses. The
///   caller must change the program or the settings.
/// - [`Runtime`](Self::Runtime), `RuntimeError`: the run. A shot that
///   trapped (a division by zero, say), named; a run stopped by its stop
///   flag (unreachable from Python today, where an interrupt raises its
///   own exception); or a run thread that panicked, a defect on our side,
///   with its message.
///
/// A failed shot's message shows the errors that caused the failure, each
/// once ([`message_with_causes`]), whichever exception it raises. An
/// interrupt (Ctrl-C) is no `RunFailure`: it raises its own exception,
/// `KeyboardInterrupt` ([`run_detached`](super::detached_run::run_detached)).
#[derive(Debug)]
pub(super) enum RunFailure {
    /// Made by [`gpu_not_set_up`] and [`engine_not_made`] for the GPU's
    /// setup, and by [`of`](Self::of) for an engine failing.
    Os(String),
    /// Made by [`of`](Self::of).
    Value(String),
    /// Made by [`of`](Self::of) for a trap or a stop, and by
    /// [`panicked_run`](super::detached_run::panicked_run) for a defect.
    Runtime(String),
}

/// The last line of a GPU setup failure's message: the choice that needs
/// no GPU.
const WITHOUT_A_GPU: &str = "engine=\"cpu\" runs without a GPU.";

impl RunFailure {
    /// The failure `error` is: the engine failing is `Os`; the engine
    /// refusing a call, or the program or the settings at fault, `Value`;
    /// anything else, a stop or a trap among them, `Runtime`.
    #[expect(
        clippy::match_same_arms,
        reason = "a trap is matched by name, so that its exception is chosen, not a default"
    )]
    pub(super) fn of<E: MpsEngineError>(error: QirResumeError<MpsResumerError<E>>) -> Self {
        let message = message_with_causes(&error);
        match error {
            QirResumeError::Resumer { error, .. } => Self::of_method_error(error, message),
            QirResumeError::Trapped { .. } => Self::Runtime(message),
            // `QirResumeError` is `#[non_exhaustive]`: a variant added later
            // raises `RuntimeError` until it is given its own exception.
            _ => Self::Runtime(message),
        }
    }

    /// The failure the MPS simulation method's `error` is, where `message`
    /// is the run's error's, with its causes. A stop says only that the run
    /// stopped.
    fn of_method_error<E: MpsEngineError>(error: MpsResumerError<E>, message: String) -> Self {
        match error {
            MpsResumerError::Stopped => Self::Runtime(error.to_string()),
            MpsResumerError::Engine(engine) if engine.refusal().is_none() => Self::Os(message),
            MpsResumerError::Engine(_)
            | MpsResumerError::Unsupported { .. }
            | MpsResumerError::RepeatedQubit { .. }
            | MpsResumerError::Capacity { .. } => Self::Value(message),
            // `MpsResumerError` is `#[non_exhaustive]`: a variant added
            // later raises `RuntimeError` until it is given its own
            // exception.
            _ => Self::Runtime(message),
        }
    }

    /// The Python exception this failure raises.
    pub(super) fn into_py_err(self) -> PyErr {
        match self {
            Self::Os(message) => PyOSError::new_err(message),
            Self::Value(message) => PyValueError::new_err(message),
            Self::Runtime(message) => PyRuntimeError::new_err(message),
        }
    }
}

/// `error`'s message, then, after `: `, the message of each error that
/// caused it and that the message does not already show. A wrapping error
/// then does not hide its cause: the canonical MPS engine's algebra failure
/// says only that the site algebra failed, and its cause (on the GPU, for
/// instance, the CUDA call that failed and its status) is its source. A
/// cause already shown is not repeated: the run's error, which names the
/// failing shot, shows its source in its message too.
fn message_with_causes(error: &dyn Error) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        let cause_message = cause.to_string();
        if !message.contains(&cause_message) {
            message.push_str(": ");
            message.push_str(&cause_message);
        }
        source = cause.source();
    }
    message
}

/// The failure of a run whose GPU libraries discovery did not find usable:
/// the setup check's report, `setup`, then [`WITHOUT_A_GPU`].
pub(super) fn gpu_not_set_up(setup: &GpuSetup) -> RunFailure {
    RunFailure::Os(format!("{setup}\n{WITHOUT_A_GPU}"))
}

/// The failure of a run whose GPU engine was not made: the engine's
/// `error`, then the setup check's report, `setup`, then
/// [`WITHOUT_A_GPU`]. The error comes first, as the check, made again, may
/// find the setup ready (CUDA device 0 was busy, say) and then gives
/// neither the cause nor a fix.
#[cfg_attr(
    not(all(target_os = "linux", target_arch = "x86_64")),
    allow(
        dead_code,
        reason = "off Linux x86-64 discovery fails, so no GPU engine is made; the tests still pin the message"
    )
)]
pub(super) fn engine_not_made(error: &impl Display, setup: &GpuSetup) -> RunFailure {
    RunFailure::Os(format!(
        "no MPS engine was made on CUDA device 0: {error}\n{setup}\n{WITHOUT_A_GPU}"
    ))
}
