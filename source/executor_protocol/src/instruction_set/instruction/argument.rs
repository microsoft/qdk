// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! An instruction's argument ([`Argument`]), one per parameter of its
//! kind; and a `bit` argument's value ([`Bit`]), known when it is written
//! or resolved as the shot runs, from the shot's earlier readouts
//! ([`ReadoutParity`]).

use super::super::{ParameterKind, Pauli};
use super::InstructionFault;
use smallvec::SmallVec;
use std::sync::Arc;

/// The value of one of an instruction's parameters, of the parameter's
/// kind. Text and Paulis are shared, so that copying an instruction
/// allocates nothing.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Argument {
    /// For a `bit` parameter.
    Bit(Bit),
    Number(f64),
    Integer(i64),
    Boolean(bool),
    Text(Arc<str>),
    /// In Qodec's sparse form over the instruction's qubits, `Z_0 Z_2`.
    Pauli(Arc<Pauli>),
}

impl Argument {
    /// The shot's `k`-th latest readout, Stim's `rec[-k]`: `recent(1)` is
    /// the latest. `k` counts from 1; an instruction with `recent(0)` is
    /// refused when it is checked
    /// ([`ZeroLookback`](InstructionFault::ZeroLookback)).
    #[must_use]
    pub fn recent(k: u32) -> Self {
        Self::parity([k], false)
    }

    /// The XOR of `flip` and of the shot's readouts counted back by each of
    /// `back`, as [`ReadoutParity`] reads it.
    #[must_use]
    pub fn parity(back: impl IntoIterator<Item = u32>, flip: bool) -> Self {
        Argument::Bit(Bit::Parity(ReadoutParity {
            flip,
            back: back.into_iter().collect(),
        }))
    }

    /// The Pauli of Qodec's text: an optional sign (`-`, `+`), an optional
    /// `i`, then factors `X_k`, `Y_k`, `Z_k` (or `I`), separated by spaces,
    /// each qubit at most once: `-X_0 Z_2`, `i Y_1`, `-I`.
    ///
    /// # Errors
    ///
    /// [`MalformedPauli`](InstructionFault::MalformedPauli) for any other
    /// text.
    pub fn pauli(text: &str) -> Result<Self, InstructionFault> {
        Pauli::parse(text)
            .map(|pauli| Argument::Pauli(Arc::new(pauli)))
            .ok_or_else(|| InstructionFault::MalformedPauli(text.to_owned()))
    }

    /// The kind of parameter it fills.
    #[must_use]
    pub fn kind(&self) -> ParameterKind {
        match self {
            Argument::Bit(_) => ParameterKind::Bit,
            Argument::Number(_) => ParameterKind::Number,
            Argument::Integer(_) => ParameterKind::Integer,
            Argument::Boolean(_) => ParameterKind::Boolean,
            Argument::Text(_) => ParameterKind::Text,
            Argument::Pauli(_) => ParameterKind::Pauli,
        }
    }
}

impl From<f64> for Argument {
    fn from(value: f64) -> Self {
        Argument::Number(value)
    }
}

impl From<i64> for Argument {
    fn from(value: i64) -> Self {
        Argument::Integer(value)
    }
}

impl From<bool> for Argument {
    /// A `boolean` argument; a known `bit` is `Argument::Bit(Bit::Value(_))`.
    fn from(value: bool) -> Self {
        Argument::Boolean(value)
    }
}

impl From<&str> for Argument {
    /// A `string` argument; a Pauli is parsed by [`Argument::pauli`].
    fn from(value: &str) -> Self {
        Argument::Text(value.into())
    }
}

/// The value of a `bit` argument.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Bit {
    /// A bit its sender knows, such as an earlier `execute`'s readout.
    Value(bool),
    /// A bit resolved per shot by the executor, as the shot runs.
    Parity(ReadoutParity),
}

/// The XOR of `flip` and of the shot's readouts counted back from its
/// latest: `back: [1, 3]` is Stim's `rec[-1] ^ rec[-3]`. It counts over
/// the shot's whole history, across `execute`s, so one instruction means
/// the same on every shot of a batch, and splitting instructions into
/// smaller batches changes nothing. A parity, as Qodec's `if:
/// [correction, outcomes[0]]`, because below a code one logical readout is
/// the XOR of several physical ones.
///
/// Physicists call this feed-forward: a shot's readouts steer its later
/// instructions, which a device does with real-time classical control.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadoutParity {
    flip: bool,
    /// Each at least 1 once checked; two inline.
    back: SmallVec<[u32; 2]>,
}

impl ReadoutParity {
    /// Whether the parity is flipped: the XOR's constant term.
    #[must_use]
    pub fn flip(&self) -> bool {
        self.flip
    }

    /// The readouts it XORs, each counted back from the shot's latest, 1
    /// for the latest.
    #[must_use]
    pub fn back(&self) -> &[u32] {
        &self.back
    }
}
