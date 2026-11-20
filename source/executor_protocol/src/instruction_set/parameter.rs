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

impl ParameterKind {
    /// Every kind, so that one is read back by the name `Display` writes,
    /// each at its [`place`](Self::place).
    #[cfg(feature = "serde")]
    const ALL: [ParameterKind; 6] = [
        ParameterKind::Bit,
        ParameterKind::Number,
        ParameterKind::Integer,
        ParameterKind::Boolean,
        ParameterKind::Text,
        ParameterKind::Pauli,
    ];

    /// Its place in [`ALL`](Self::ALL). The match is exhaustive, so a new
    /// kind does not build until it is given a place, and the check below
    /// fails the build unless each kind of `ALL` is at its place.
    #[cfg(feature = "serde")]
    const fn place(self) -> usize {
        match self {
            ParameterKind::Bit => 0,
            ParameterKind::Number => 1,
            ParameterKind::Integer => 2,
            ParameterKind::Boolean => 3,
            ParameterKind::Text => 4,
            ParameterKind::Pauli => 5,
        }
    }
}

/// Each kind of `ParameterKind::ALL` is at its place, checked when the
/// crate builds.
#[cfg(feature = "serde")]
const _: () = {
    let mut place = 0;
    while place < ParameterKind::ALL.len() {
        assert!(
            ParameterKind::ALL[place].place() == place,
            "each kind of `ParameterKind::ALL` is at its place"
        );
        place += 1;
    }
};

/// Its stable form is its name, Qodec's, as [`Display`](fmt::Display)
/// writes it.
#[cfg(feature = "serde")]
impl serde::Serialize for ParameterKind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

/// Any text that is not a kind's name is refused.
#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for ParameterKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        ParameterKind::ALL
            .into_iter()
            .find(|kind| kind.to_string() == text)
            .ok_or_else(|| {
                serde::de::Error::custom(format_args!("`{text}` is not a parameter kind"))
            })
    }
}
