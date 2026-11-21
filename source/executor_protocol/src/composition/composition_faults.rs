// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Why the [`on`](crate::DriverJoins::on) that makes a runner refused its
//! chain: [`CompositionFaults`], every fault of the chain at once, each a
//! [`CompositionFault`], whose [`BoundaryMismatch`] says what falls short.

use std::error::Error;
use std::fmt;

use crate::{CapabilitiesMismatch, InstructionMismatch, PartId};

/// Every fault of a chain, reported at once by the
/// [`on`](crate::DriverJoins::on) that makes its runner, before any shot
/// runs: each part whose instructions the part below does not accept, and
/// each part whose requirement arrives at a part that does not provide it,
/// top to bottom.
#[derive(Clone, Debug)]
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
#[derive(Clone, Debug, PartialEq, Eq)]
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
/// provide: … ``, for resources and features, or both, joined by `; and`.
impl fmt::Display for CompositionFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let BoundaryMismatch {
            instructions,
            capabilities,
        } = &self.mismatch;
        write!(f, "{}", self.requirer)?;
        if !instructions.is_empty() {
            write!(f, " sends what {} does not accept: ", self.provider)?;
            write_joined(f, instructions)?;
        }
        if !capabilities.is_empty() {
            if !instructions.is_empty() {
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
/// it: the instructions the part below does not accept, and the resources
/// and features it does not provide. At least one list is not empty.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct BoundaryMismatch {
    /// Each instruction sent that does not fit, as
    /// [`InstructionSet`](crate::InstructionSet)'s `fits` finds them.
    pub instructions: Vec<InstructionMismatch>,
    /// Each resource and feature not provided.
    pub capabilities: CapabilitiesMismatch,
}

/// Each mismatch, the instructions first, joined by `; `.
impl fmt::Display for BoundaryMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_joined(f, &self.instructions)?;
        if !self.capabilities.is_empty() {
            if !self.instructions.is_empty() {
                f.write_str("; ")?;
            }
            write!(f, "{}", self.capabilities)?;
        }
        Ok(())
    }
}

impl Error for BoundaryMismatch {}

/// Each instruction mismatch of `instructions`, joined by `; `.
fn write_joined(f: &mut fmt::Formatter<'_>, instructions: &[InstructionMismatch]) -> fmt::Result {
    for (i, mismatch) in instructions.iter().enumerate() {
        if i > 0 {
            f.write_str("; ")?;
        }
        write!(f, "{mismatch}")?;
    }
    Ok(())
}
