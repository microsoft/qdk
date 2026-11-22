// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The one relation between the two sides of a boundary:
//! [`BoundaryCompatible`], whether what the part above requires is
//! provided by the part below.

/// A statement made in one type on both sides of a boundary: what the part
/// above requires, and what the part below provides. [`fits`](Self::fits)
/// checks the first against the second.
///
/// Every check between parts is this one relation: an instruction set's
/// ([`InstructionSet`](crate::InstructionSet)), and the capabilities'
/// ([`Capabilities`](crate::Capabilities)), which the joins check at each
/// boundary of a chain, and whose fit compares their
/// [`Resources`](crate::Resources) and [`Features`](crate::Features) too.
pub trait BoundaryCompatible {
    /// What the check gives when `self` fits `provided`.
    type Fit;
    /// What the check gives when it does not: every shortfall at once.
    type Mismatch;

    /// Whether `self`, the part above's statement, fits `provided`, the
    /// part below's.
    ///
    /// # Errors
    ///
    /// Every way `provided` falls short of `self`.
    fn fits(&self, provided: &Self) -> Result<Self::Fit, Self::Mismatch>;
}
