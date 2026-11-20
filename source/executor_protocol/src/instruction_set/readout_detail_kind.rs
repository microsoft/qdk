// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The kind of detail an instruction's outcomes carry beside their bits
//! ([`ReadoutDetailKind`]), as its set declares it.

use std::fmt;

/// The kind of detail each outcome of an instruction carries beside its
/// hard [`ReadoutValue`](crate::ReadoutValue), declared in its
/// [`InstructionDef`](crate::InstructionDef): what a part above that reads
/// soft information, such as a decoder, needs from the parts below, and
/// what the fit checks at `>>`. Non-exhaustive, so that another kind, such
/// as raw I/Q, can be added.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ReadoutDetailKind {
    /// A floating-point number, such as a confidence or a probability.
    Number,
    /// An integer, such as a leakage level.
    Integer,
}

impl fmt::Display for ReadoutDetailKind {
    /// `number` or `integer`, as Qodec names its parameters' kinds.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ReadoutDetailKind::Number => "number",
            ReadoutDetailKind::Integer => "integer",
        })
    }
}
