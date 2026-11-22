// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Why the [`on`](crate::DriverJoins::on) that makes a runner refused its
//! chain: [`CompositionFaults`], every fault of the chain at once, each a
//! [`CompositionFault`], whose [`BoundaryMismatch`] says what falls short.

use std::error::Error;
use std::fmt;

use crate::{CapabilitiesMismatch, PartId};

/// Every fault of a chain, reported at once by the
/// [`on`](crate::DriverJoins::on) that makes its runner, before any shot
/// runs: each part whose instructions the part below does not accept, and
/// each part whose requirement arrives at a part that does not provide it,
/// top to bottom.
#[derive(Debug)]
pub struct CompositionFaults {
    /// At least one.
    pub(super) faults: Vec<CompositionFault>,
}

impl CompositionFaults {
    /// Each fault, by its requirer top to bottom, then by its provider.
    #[must_use]
    pub fn faults(&self) -> &[CompositionFault] {
        &self.faults
    }
}

/// `the chain does not compose: ` then each fault, joined by `; `.
impl fmt::Display for CompositionFaults {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the chain does not compose: ")?;
        for (i, fault) in self.faults.iter().enumerate() {
            if i > 0 {
                f.write_str("; ")?;
            }
            write!(f, "{fault}")?;
        }
        Ok(())
    }
}

impl Error for CompositionFaults {}

/// One part whose instructions the part directly below does not accept,
/// or whose requirement, passed down unchanged by the parts between, a
/// part below it does not provide.
#[derive(Debug)]
#[non_exhaustive]
pub struct CompositionFault {
    /// The part whose instructions are not accepted, or whose requirement
    /// is not provided: for a value that passed parts unchanged, the
    /// first part that required it.
    pub requirer: PartId,
    /// The part that does not accept or provide it.
    pub provider: PartId,
    /// What it falls short of.
    pub mismatch: BoundaryMismatch,
}

/// `` `d` (part 0) sends what `a` (part 1) does not accept: … ``, for
/// instructions, `` `d` (part 0) requires what `e` (part 2) does not
/// provide: … ``, for capabilities, or both, joined by `; and`.
impl fmt::Display for CompositionFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let BoundaryMismatch {
            instructions,
            capabilities,
        } = &self.mismatch;
        write!(f, "{}", self.requirer)?;
        if let Some(instructions) = instructions {
            write!(
                f,
                " sends what {} does not accept: {instructions}",
                self.provider
            )?;
        }
        if !capabilities.is_empty() {
            if instructions.is_some() {
                f.write_str("; and")?;
            }
            write!(
                f,
                " requires what {} does not provide: {capabilities}",
                self.provider
            )?;
        }
        Ok(())
    }
}

/// Every way what one part sends or requires falls short at a part below
/// it: how the set it sends does not fit the set the part below accepts,
/// and the capabilities it does not provide. In a mismatch a failed check
/// returns, at least one is there.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct BoundaryMismatch {
    /// How the set sent does not fit the set accepted: the set's own
    /// [`fits`](crate::BoundaryCompatible::fits) mismatch, which
    /// `downcast_ref` reads as its type.
    pub instructions: Option<Box<dyn Error + Send + Sync>>,
    /// Each capability not provided.
    pub capabilities: CapabilitiesMismatch,
}

/// Each mismatch, the instruction set's first, joined by `; `.
impl fmt::Display for BoundaryMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(instructions) = &self.instructions {
            write!(f, "{instructions}")?;
        }
        if !self.capabilities.is_empty() {
            if self.instructions.is_some() {
                f.write_str("; ")?;
            }
            write!(f, "{}", self.capabilities)?;
        }
        Ok(())
    }
}

impl Error for BoundaryMismatch {}
