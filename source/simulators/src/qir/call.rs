// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The quantum calls a [`QirMachine`] yields in its batches ([`QirCall`]),
//! with the qubits of a call on any number of them ([`QirQubits`]).
//!
//! They are where the machine and a resumer meet: a resumer reads a
//! batch's calls, runs them on its quantum state, and answers each call
//! that writes a result ([`QirOutcome`]). They are the machine's own
//! types, not the bytecode's, so that a resumer depends on QIR's
//! instructions only ([`QirInstruction`]), not on how the bytecode encodes
//! them. They name the qubits and results of the program by their
//! indices, as its bytecode does.

use std::fmt;
use std::ops::Deref;

use qsc_data_structures::measurement::MeasurementBasis;

use super::{QirGate, QirInstruction};
#[cfg(doc)]
use super::{QirMachine, QirOutcome, QirProgram};

/// A call of a [`QirInstruction`], as a [`QirMachine`] yields it in a
/// batch. Its qubits and result are indices below the program's
/// [`qubits`](QirProgram::qubits) and [`results`](QirProgram::results).
/// A call that writes a result ([`result`](Self::result)) is answered by
/// one [`QirOutcome`].
///
/// It displays as its mnemonic and operands, as `cx(qubit 0, qubit 1)` or
/// `mz(qubit 0, result 0)`, so that a batch reads well when debugging.
#[derive(Clone, Debug, PartialEq)]
pub enum QirCall {
    /// A unitary gate on its qubits, in the order of its matrix's operands
    /// ([`QirGate::matrix_at`]): the second is `None` for a one-qubit gate.
    /// The two qubits of a two-qubit gate are not checked distinct. The
    /// angle is `Some` for a rotation and `None` for any other gate, as
    /// [`QirGate::matrix_at`] takes it.
    Gate {
        gate: QirGate,
        qubits: (u32, Option<u32>),
        angle: Option<f64>,
    },
    /// `mx`, `my` or `mz`, as `basis` says, of `qubit` into `result`.
    Measure {
        basis: MeasurementBasis,
        qubit: u32,
        result: u32,
    },
    /// `mresetz` of `qubit` into `result`.
    MeasureReset { qubit: u32, result: u32 },
    /// `reset` of `qubit`.
    Reset { qubit: u32 },
    /// `move` of `qubit`.
    Move { qubit: u32 },
    /// `correlated_noise` on `qubits`, in the program's order, drawn from
    /// the noise table `table`. It writes no result.
    CorrelatedNoise { qubits: QirQubits, table: u32 },
    /// `peek_loss` of `qubit` into `result`: `One` when it is lost.
    PeekLoss { qubit: u32, result: u32 },
    /// `readout_noise` on `result`: reads `Zero` as `One` with probability
    /// `zero_as_one`, and `One` as `Zero` with probability `one_as_zero`.
    ReadoutNoise {
        zero_as_one: f64,
        one_as_zero: f64,
        result: u32,
    },
}

impl QirCall {
    /// The instruction it calls.
    #[must_use]
    pub fn instruction(&self) -> QirInstruction {
        match self {
            Self::Gate { gate, .. } => QirInstruction::Gate(*gate),
            Self::Measure { basis, .. } => QirInstruction::Measure(*basis),
            Self::MeasureReset { .. } => QirInstruction::MeasureReset,
            Self::Reset { .. } => QirInstruction::Reset,
            Self::Move { .. } => QirInstruction::Move,
            Self::CorrelatedNoise { .. } => QirInstruction::CorrelatedNoise,
            Self::PeekLoss { .. } => QirInstruction::PeekLoss,
            Self::ReadoutNoise { .. } => QirInstruction::ReadoutNoise,
        }
    }

    /// The result it writes, whose outcome the next resume takes; `None`
    /// for a call that writes none.
    #[must_use]
    pub fn result(&self) -> Option<u32> {
        match self {
            Self::Measure { result, .. }
            | Self::MeasureReset { result, .. }
            | Self::PeekLoss { result, .. }
            | Self::ReadoutNoise { result, .. } => Some(*result),
            Self::Gate { .. }
            | Self::Reset { .. }
            | Self::Move { .. }
            | Self::CorrelatedNoise { .. } => None,
        }
    }
}

impl fmt::Display for QirCall {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}(", self.instruction())?;
        match self {
            Self::Gate {
                qubits: (first, second),
                angle,
                ..
            } => {
                write!(f, "qubit {first}")?;
                if let Some(second) = second {
                    write!(f, ", qubit {second}")?;
                }
                if let Some(angle) = angle {
                    write!(f, ", angle {angle}")?;
                }
            }
            Self::Measure { qubit, result, .. }
            | Self::MeasureReset { qubit, result }
            | Self::PeekLoss { qubit, result } => write!(f, "qubit {qubit}, result {result}")?,
            Self::Reset { qubit } | Self::Move { qubit } => write!(f, "qubit {qubit}")?,
            Self::CorrelatedNoise { qubits, table } => {
                write!(f, "qubits {:?}, table {table}", &**qubits)?;
            }
            Self::ReadoutNoise {
                zero_as_one,
                one_as_zero,
                result,
            } => write!(
                f,
                "zero_as_one {zero_as_one}, one_as_zero {one_as_zero}, result {result}"
            )?,
        }
        f.write_str(")")
    }
}

/// The qubits of a call on any number of them
/// ([`QirCall::CorrelatedNoise`]), in order. Up to four are held in place,
/// so that a call on few qubits allocates nothing.
#[derive(Clone)]
pub struct QirQubits(Qubits);

/// The qubits that fit in place, at most: four keep a [`QirCall`] at 32
/// bytes, a gate's size.
const IN_PLACE: usize = 4;

#[derive(Clone)]
enum Qubits {
    InPlace { len: u8, qubits: [u32; IN_PLACE] },
    Allocated(Box<[u32]>),
}

impl Deref for QirQubits {
    type Target = [u32];

    fn deref(&self) -> &[u32] {
        match &self.0 {
            Qubits::InPlace { len, qubits } => &qubits[..usize::from(*len)],
            Qubits::Allocated(qubits) => qubits,
        }
    }
}

impl FromIterator<u32> for QirQubits {
    fn from_iter<I: IntoIterator<Item = u32>>(iter: I) -> Self {
        let mut iter = iter.into_iter();
        let mut qubits = [0; IN_PLACE];
        let mut len = 0;
        while let Some(qubit) = iter.next() {
            if len == IN_PLACE {
                let all = qubits.into_iter().chain([qubit]).chain(iter).collect();
                return Self(Qubits::Allocated(all));
            }
            qubits[len] = qubit;
            len += 1;
        }
        #[allow(clippy::cast_possible_truncation)] // At most `IN_PLACE`.
        Self(Qubits::InPlace {
            len: len as u8,
            qubits,
        })
    }
}

impl PartialEq for QirQubits {
    fn eq(&self, other: &Self) -> bool {
        **self == **other
    }
}

impl Eq for QirQubits {}

impl fmt::Debug for QirQubits {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}
