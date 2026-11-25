// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`MpsResumerError`]: why an [`MpsResumer`] failed a shot, one variant
//! per cause, each naming the item at fault.

use std::fmt;

#[cfg(doc)]
use super::MpsResumer;
use crate::qir::QirInstruction;
use tensornet::MpsShapeError;

/// Why an [`MpsResumer`] failed a shot, apart from a trap of the program
/// ([`QirResumeError::Trapped`](crate::qir::QirResumeError::Trapped)). The
/// run's error names the failing shot
/// ([`QirResumeError::Resumer`](crate::qir::QirResumeError::Resumer)); this
/// says what about the shot failed. `#[non_exhaustive]`: variants are
/// added as the MPS simulation method needs them.
///
/// Its message says what failed and names the item. An error it wraps
/// (the engine's, or why the shape is no chain) is its [`source`], not
/// repeated in its message, as Rust's convention for wrapped errors asks.
///
/// [`source`]: std::error::Error::source
#[derive(Debug)]
#[non_exhaustive]
pub enum MpsResumerError<E> {
    /// A call of an instruction the MPS simulation method does not run:
    /// correlated noise, readout noise and a qubit's loss check
    /// (`peek_loss`). They need noise and
    /// loss on matrix product states, which are not built
    /// (`src/mps_simulator/design.md`, "What is deliberately not here").
    /// The shot fails when it reaches the call, so a program whose noise is
    /// on a path no shot takes runs.
    Unsupported {
        /// The instruction called.
        instruction: QirInstruction,
    },

    /// A two-qubit gate called on one qubit twice. No matrix on two
    /// distinct qubits applies it, and the machine does not check it.
    RepeatedQubit {
        /// The gate called.
        instruction: QirInstruction,
        /// The qubit named twice.
        qubit: u32,
    },

    /// The program's qubits and the max bond dimension make no capacity.
    /// For instance, on a 64-bit machine, a chain of more than 63 sites
    /// whose bonds are unbounded.
    Capacity {
        /// The program's qubits.
        qubits: u32,
        /// The max bond dimension χ.
        max_bond_dimension: usize,
        /// Why the shape is no chain.
        source: MpsShapeError,
    },

    /// The engine refused a call, or failed in it. A refusal says the call
    /// broke the engine contract: for instance, a gate whose angle is not
    /// finite has a matrix that is not unitary. Any other error is the
    /// engine's own failure.
    /// [`MpsEngineError::refusal`](tensornet::MpsEngineError::refusal)
    /// tells the two apart. The engine's error is the
    /// [`source`](std::error::Error::source).
    ///
    /// The resumer retries neither. Every shot that shares the state fails:
    /// none can go on past the call, and after the engine's own failure the
    /// engine contract leaves the state unspecified.
    Engine(E),

    /// The run was stopped: its stop flag was read set
    /// ([`MpsResumer::with_stop_flag`]).
    Stopped,
}

impl<E> fmt::Display for MpsResumerError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported { instruction } => write!(
                f,
                "`{instruction}` is not run on matrix product states, which are simulated without noise or loss"
            ),
            Self::RepeatedQubit { instruction, qubit } => {
                write!(f, "`{instruction}` is called on qubit {qubit} twice")
            }
            Self::Capacity {
                qubits,
                max_bond_dimension,
                ..
            } => write!(
                f,
                "{qubits} qubits with bonds of at most {max_bond_dimension} make no chain"
            ),
            Self::Engine(_) => write!(f, "the MPS engine failed or refused a call"),
            Self::Stopped => write!(f, "the run was stopped: its stop flag was set"),
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for MpsResumerError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Capacity { source, .. } => Some(source),
            Self::Engine(error) => Some(error),
            Self::Unsupported { .. } | Self::RepeatedQubit { .. } | Self::Stopped => None,
        }
    }
}

#[cfg(test)]
mod tests;
