// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! An instruction's argument ([`Argument`]), one per parameter of its
//! kind; and a `bit` argument's value ([`Bit`]), known when it is written
//! or resolved as the shot runs, from the shot's earlier readouts
//! ([`ReadoutParity`], whose compact representation has its own file).

mod readout_parity;

pub use readout_parity::ReadoutParity;

use super::super::{ParameterKind, Pauli};
use super::InstructionFault;
use std::sync::Arc;

/// The value of one of an instruction's parameters, of the parameter's
/// kind. Text, Paulis and the rare readout parity of more than five
/// readouts are shared, so that copying one allocates nothing.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Argument {
    /// For a `bit` parameter.
    Bit(Bit),
    /// For a `number` parameter.
    Number(f64),
    /// For an `integer` parameter.
    Integer(i64),
    /// For a `boolean` parameter.
    Boolean(bool),
    /// For a `string` parameter.
    Text(Arc<str>),
    /// In Qodec's sparse form over the instruction's qubits (not checked),
    /// `Z_0 Z_2`.
    Pauli(Arc<Pauli>),
}

// An instruction holds up to three arguments inline, and its size sets how
// fast a batch runs, as the crate's bench measures.
#[cfg(target_pointer_width = "64")]
const _: () = assert!(size_of::<Argument>() == 24);

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
    /// `back`, as [`ReadoutParity`] reads it. An empty `back` is the
    /// constant `flip`, yet still a readout parity: its shots' needs must
    /// use readout parities
    /// ([`ParityNotProvisioned`](InstructionFault::ParityNotProvisioned)
    /// otherwise); [`Bit::Value`] is a constant that needs none.
    #[must_use]
    pub fn parity(back: impl IntoIterator<Item = u32>, flip: bool) -> Self {
        Argument::Bit(Bit::Parity(ReadoutParity::new(back, flip)))
    }

    /// The Pauli of Qodec's text: an optional sign (`-`, `+`), an optional
    /// `i`, then factors `X_k`, `Y_k`, `Z_k` or `I_k` (the identity on qubit
    /// `k`, which only names the qubit), or `I`, separated by spaces, each
    /// qubit at most once: `-X_0 Z_2`, `i Y_1`, `X_0 I_3`, `-I`.
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
