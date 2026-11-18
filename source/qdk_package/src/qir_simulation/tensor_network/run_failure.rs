// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Why a tensor-network run gave no records, as the Python exception its
//! caller handles.
//!
//! A user's handler decides by who must act, so each exception names one
//! party:
//!
//! - `OSError`, the machine: the GPU engine cannot be made (its libraries
//!   not usable, no device), with the setup check's report as the message,
//!   after the engine's error when discovery succeeded but no engine was
//!   made, and a last line saying that `engine="cpu"` runs without a GPU;
//!   or the engine failing during the run, with the failure's message.
//! - `ValueError`, the program or the settings: the program's faults, a
//!   call the MPS simulation method does not run, a max bond dimension that
//!   makes no chain, or a call the engine refuses.
//! - `RuntimeError`, the run: a shot that trapped, named; a run that was
//!   stopped; or a run thread that panicked, a defect, with its message.
//!
//! An interrupt (Ctrl-C) raises its own exception, `KeyboardInterrupt`,
//! which `detached_run.rs` raises.

use std::fmt::Display;

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
        let message = error.to_string();
        match error {
            QirResumeError::Resumer {
                error: stopped @ MpsResumerError::Stopped,
                ..
            } => Self::Runtime(stopped.to_string()),
            QirResumeError::Resumer {
                error: MpsResumerError::Engine(engine),
                ..
            } if engine.refusal().is_none() => Self::Os(message),
            QirResumeError::Resumer { .. } => Self::Value(message),
            QirResumeError::Trapped { .. } => Self::Runtime(message),
            // `QirResumeError` is `#[non_exhaustive]`: a variant added later
            // raises `RuntimeError` until it is given its own exception.
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
