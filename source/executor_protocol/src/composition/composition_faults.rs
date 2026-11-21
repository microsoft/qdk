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

/// One part whose requirement a part below it does not provide: the part
/// below it, for instructions, or the part the requirement arrives at,
/// passed down unchanged by the parts between them.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CompositionFault {
    /// The part whose requirement is not provided: for a value that
    /// passed parts unchanged, the first part that required it.
    pub requirer: PartId,
    /// The part that does not provide it.
    pub provider: PartId,
    /// What it falls short of.
    pub mismatch: BoundaryMismatch,
}

/// `` `d` (part 0) requires what `e` (part 2) does not provide: … ``.
impl fmt::Display for CompositionFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} requires what {} does not provide: {}",
            self.requirer, self.provider, self.mismatch
        )
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
        for (i, mismatch) in self.instructions.iter().enumerate() {
            if i > 0 {
                f.write_str("; ")?;
            }
            write!(f, "{mismatch}")?;
        }
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
