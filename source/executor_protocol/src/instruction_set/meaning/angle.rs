// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The angle of a rotation step ([`Angle`]): a number, or a parameter.

use std::hash::{Hash, Hasher};

/// The angle of a rotation: a number, or one of the instruction's `number`
/// parameters.
///
/// Angles compare and hash bit for bit, so that meanings are `Eq` and
/// `Hash`: `0.5` equals `0.5`, a NaN equals itself, and `0.0` differs from
/// `-0.0`; a loader that reads angles from text writes each the same way.
/// Its stable form writes a number as a 64-bit float, which a format that
/// writes its eight bytes as they are (bincode, for instance) keeps bit
/// for bit. A text format keeps it only when its reader rounds correctly
/// (`serde_json`'s does with its `float_roundtrip` feature), and JSON has
/// no NaN or infinity: such an angle written to JSON does not read back.
#[derive(Clone, Copy, Debug)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum Angle {
    /// An angle fixed by the instruction, in radians.
    Literal(f64),
    /// The instruction's parameter at this position, from 0.
    Parameter(usize),
}

impl PartialEq for Angle {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Angle::Literal(a), Angle::Literal(b)) => a.to_bits() == b.to_bits(),
            (Angle::Parameter(a), Angle::Parameter(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for Angle {}

impl Hash for Angle {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Angle::Literal(angle) => (0u8, angle.to_bits()).hash(state),
            Angle::Parameter(parameter) => (1u8, parameter).hash(state),
        }
    }
}
