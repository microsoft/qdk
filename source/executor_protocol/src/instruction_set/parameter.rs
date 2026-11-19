// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! An instruction's classical inputs ([`Parameter`]) and their kinds
//! ([`ParameterKind`]), as Qodec declares them (`parameters: {theta:
//! number}`): what an instruction's arguments are checked against.

use std::fmt;

/// One classical input of an instruction: its name, by which an argument
/// is given when an instruction is built, and its kind. An instruction's
/// parameters are ordered, and its arguments follow that order.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Parameter {
    /// Its name, by which an argument is given.
    pub name: String,
    /// Its kind, which its argument must have.
    pub kind: ParameterKind,
}

impl Parameter {
    /// The parameter `name`, of kind `kind`.
    #[must_use]
    pub fn new(name: impl Into<String>, kind: ParameterKind) -> Self {
        Self {
            name: name.into(),
            kind,
        }
    }
}

/// The kind of a parameter, and of the argument that fills it: Qodec's
/// parameter types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ParameterKind {
    /// A classical bit known when the shot runs, Qodec's `bit`: a value,
    /// or a parity of the shot's earlier readouts.
    Bit,
    /// A floating-point number, such as a rotation angle; Qodec's
    /// `number`.
    Number,
    /// An integer, Qodec's `integer`.
    Integer,
    /// A boolean known when the instruction is written, Qodec's
    /// `boolean`; unlike a [`Bit`](Self::Bit), never a readout.
    Boolean,
    /// A string, Qodec's `string`.
    Text,
    /// A Pauli operator over the instruction's qubits (not checked),
    /// Qodec's `pauli`.
    Pauli,
}

impl fmt::Display for ParameterKind {
    /// Qodec's name of the kind: `bit`, `number`, `integer`, `boolean`,
    /// `string`, `pauli`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ParameterKind::Bit => "bit",
            ParameterKind::Number => "number",
            ParameterKind::Integer => "integer",
            ParameterKind::Boolean => "boolean",
            ParameterKind::Text => "string",
            ParameterKind::Pauli => "pauli",
        })
    }
}
