// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What an observation step measures ([`Observable`]): a Pauli, or a
//! parameter.

use crate::Pauli;

/// What an observation measures: a Pauli, or the instruction's `pauli`
/// parameter at a position, as Stim's `MPP` measures the product it is
/// given (Qodec's `observe: p`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Observable {
    /// This Pauli, fixed by the instruction.
    Pauli(Pauli),
    /// The instruction's `pauli` parameter at this position, from 0.
    Parameter(usize),
}

impl From<Pauli> for Observable {
    fn from(pauli: Pauli) -> Self {
        Observable::Pauli(pauli)
    }
}
