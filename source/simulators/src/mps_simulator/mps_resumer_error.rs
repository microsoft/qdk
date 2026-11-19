// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`MpsResumerError`]: why an [`MpsResumer`] failed a shot, apart from a
//! trap of the program, which the run reports itself
//! ([`QirResumeError::Trapped`](crate::qir::QirResumeError::Trapped)).
//!
//! A run that fails names the failing shot
//! ([`QirResumeError::Resumer`](crate::qir::QirResumeError::Resumer));
//! this says what about the shot failed: a call the MPS simulation method
//! does not run, a call it cannot run, a capacity that is no chain, or the
//! engine; or the run was stopped (no shot failed: the shot named is the
//! first that had not ended).

use std::fmt;

#[cfg(doc)]
use super::MpsResumer;
use crate::qir::QirInstruction;
use tensornet::MpsShapeError;

/// Why an [`MpsResumer`] failed a shot. Variants are added as the MPS
/// simulation method needs them, hence `#[non_exhaustive]`.
///
/// Its message says what failed and names the item; an error it wraps (the
/// engine's, or why the shape is no chain) is its [`source`], not repeated
/// in its message (Rust's convention for a wrapped error): a caller that
/// prints the message alone walks `source()` for it, and one that shows
/// this error's chain shows each cause once. The run's error around it does
/// not follow that convention: [`QirResumeError`](crate::qir::QirResumeError)
/// repeats this error in its message and also gives it as its source, so a
/// caller that walks a run's chain shows each cause once only if it skips a
/// cause its message already holds.
///
/// [`source`]: std::error::Error::source
#[derive(Debug)]
#[non_exhaustive]
pub enum MpsResumerError<E> {
    /// A call of an instruction the MPS simulation method does not run yet:
    /// correlated noise, readout noise and loss checks, which noise and
    /// loss on matrix product states (planned) will run. Failed when a shot
    /// reaches the call, so a program whose noise is on a path no shot
    /// takes runs.
    Unsupported {
        /// The instruction called.
        instruction: QirInstruction,
    },

    /// A two-qubit gate called on one qubit twice, which no matrix on two
    /// distinct qubits applies (the machine does not check it).
    RepeatedQubit {
        /// The gate called.
        instruction: QirInstruction,
        /// The qubit named twice.
        qubit: u32,
    },

    /// The program's qubits and the max bond dimension make no capacity: a
    /// chain of more than 63 sites whose bonds are unbounded, say.
    Capacity {
        /// The program's qubits.
        qubits: u32,
        /// The max bond dimension χ.
        max_bond_dimension: usize,
        /// Why the shape is no chain.
        source: MpsShapeError,
    },

    /// The engine failed, or refused a call (a gate whose angle is not
    /// finite has a matrix that is not unitary, say).
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
                "`{instruction}` is not run on matrix product states: noise and loss on them are planned"
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
