// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Why the [`on`](crate::DriverJoins::on) that makes a runner refused its
//! chain: [`CompositionFaults`], every fault of the chain at once, each a
//! [`CompositionFault`].

use std::error::Error;
use std::fmt;

use crate::{CapabilitiesMismatch, PartId};

/// Every fault of a chain, reported at once by the
/// [`on`](crate::DriverJoins::on) that makes its runner, before any shot
/// runs: each part whose requirement a part it reaches does not provide,
/// top to bottom.
#[derive(Clone, Debug)]
pub struct CompositionFaults {
    faults: Vec<CompositionFault>,
}

impl CompositionFaults {
    /// The faults `faults`, at least one.
    pub(super) fn new(faults: Vec<CompositionFault>) -> Self {
        Self { faults }
    }

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
/// below it, or, for what no adapter between them states again, a part
/// further below.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CompositionFault {
    /// The part whose requirement is not provided.
    pub requirer: PartId,
    /// The part that does not provide it.
    pub provider: PartId,
    /// What it falls short of.
    pub mismatch: CapabilitiesMismatch,
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
