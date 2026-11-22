// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The value of a `bit` argument ([`Bit`]): known when it is written, or
//! resolved per shot from its earlier readouts.

use super::ReadoutParity;

/// The value of a `bit` argument.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Bit {
    /// A bit its sender knows, such as an earlier `execute`'s readout.
    Value(bool),
    /// A bit resolved per shot by the executor, as the shot runs.
    Parity(ReadoutParity),
}
