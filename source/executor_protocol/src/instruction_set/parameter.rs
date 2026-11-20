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
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(deny_unknown_fields)
)]
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
///
/// With the `serde` feature, its stable form is its name, Qodec's, as
/// [`Display`](fmt::Display) writes it; any other text is refused, naming
/// it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "lowercase")
)]
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
    #[cfg_attr(feature = "serde", serde(rename = "string"))]
    Text,
    /// A Pauli operator over the instruction's qubits (not checked),
    /// Qodec's `pauli`.
    Pauli,
}

/// Its stable form spells each name again, in its `serde` attributes: a
/// test checks that each kind is written as it is displayed.
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
