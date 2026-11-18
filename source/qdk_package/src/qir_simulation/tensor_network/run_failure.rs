// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Why a tensor-network run gave no records, as the Python exception its
//! caller handles.
//!
//! A user's handler decides by who must act, so each exception names one
//! party:
//!
//! - `OSError`, the machine: the GPU engine cannot be made (its libraries
//!   not usable, no device), with the message `gpu_not_set_up` or
//!   `engine_not_made` composes; or the engine failing during the run.
//! - `ValueError`, the program or the settings: a call the MPS simulation
//!   method does not run, a max bond dimension that makes no chain, or a
//!   call the engine refuses.
//! - `RuntimeError`, the run: a shot that trapped, named; a run that was
//!   stopped; or a run thread that panicked, a defect, with its message.
//!
//! A failed shot's message shows the errors that caused the failure, each
//! once (`with_causes`), whichever exception it raises.
//!
//! An interrupt (Ctrl-C) raises its own exception, `KeyboardInterrupt`,
//! which `detached_run.rs` raises.

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
#[derive(Debug)]
pub(super) enum RunFailure {
    /// The machine: the GPU setup, or the engine failing. `OSError`.
    Os(String),
    /// The program or the settings. `ValueError`.
    Value(String),
    /// The run: a trap, a stop, or a defect. `RuntimeError`.
    Runtime(String),
}

/// The last line of a GPU setup failure's message: the choice that needs
/// no GPU.
const WITHOUT_A_GPU: &str = "engine=\"cpu\" runs without a GPU.";

impl RunFailure {
    /// The failure a run's `error` is.
    #[expect(
        clippy::match_same_arms,
        reason = "a trap is matched by name, so that its exception is chosen, not a default"
    )]
    pub(super) fn of<E: MpsEngineError>(error: QirResumeError<MpsResumerError<E>>) -> Self {
        let message_with_causes = with_causes(&error);
        match error {
            QirResumeError::Resumer { error, .. } => match error {
                MpsResumerError::Stopped => Self::Runtime(error.to_string()),
                MpsResumerError::Engine(engine) if engine.refusal().is_none() => {
                    Self::Os(message_with_causes)
                }
                MpsResumerError::Engine(_)
                | MpsResumerError::Unsupported { .. }
                | MpsResumerError::RepeatedQubit { .. }
                | MpsResumerError::Capacity { .. } => Self::Value(message_with_causes),
                // `MpsResumerError` is `#[non_exhaustive]`: a variant added
                // later raises `RuntimeError` until it is given its own
                // exception.
                _ => Self::Runtime(message_with_causes),
            },
            QirResumeError::Trapped { .. } => Self::Runtime(message_with_causes),
            // `QirResumeError` is `#[non_exhaustive]`: a variant added later
            // raises `RuntimeError` until it is given its own exception.
            _ => Self::Runtime(message_with_causes),
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
/// cause already shown is not repeated: the run's and the MPS simulation
/// method's errors show their source in their message too.
fn with_causes(error: &dyn Error) -> String {
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
