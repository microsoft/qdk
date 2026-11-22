// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The angle of a rotation step ([`Angle`]): a number, or a parameter.

/// The angle of a rotation: a number, or one of the instruction's `number`
/// parameters.
///
/// Angles compare bit for bit, so that meanings are `Eq`: `0.5` equals
/// `0.5`, a NaN equals itself, and `0.0` differs from `-0.0`; a loader
/// that reads angles from text writes each the same way.
#[derive(Clone, Copy, Debug)]
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
