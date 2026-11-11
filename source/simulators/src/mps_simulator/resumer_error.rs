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
//! engine.

use std::fmt;

#[cfg(doc)]
use super::MpsResumer;
use crate::qir::QirInstruction;
use tensornet::MpsShapeError;

/// Why an [`MpsResumer`] failed a shot. Variants are added as the MPS
/// simulation method needs them, hence `#[non_exhaustive]`.
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
}

impl<E: fmt::Display> fmt::Display for MpsResumerError<E> {
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
                source,
            } => write!(
                f,
                "{qubits} qubits with bonds of at most {max_bond_dimension} make no chain: {source}"
            ),
            Self::Engine(error) => write!(f, "the MPS engine failed: {error}"),
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for MpsResumerError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Capacity { source, .. } => Some(source),
            Self::Engine(error) => Some(error),
            Self::Unsupported { .. } | Self::RepeatedQubit { .. } => None,
        }
    }
}
