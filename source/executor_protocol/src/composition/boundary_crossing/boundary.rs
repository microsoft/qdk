// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! One boundary of a chain ([`Boundary`]): the part below it and what
//! crosses it, as the chain's check found it, kept by the part that holds
//! the boundary, a joined part or the runner, so that every call across it
//! is checked, and every run gets the part below ready, with the values the
//! admission checked; or, for a joined part the check did not reach, read
//! from its halves on each run.

use super::BatchLimits;
use crate::{InstructionSet, Needs, Offer, PartId};

/// One boundary of a chain: the part below it, what crosses it, and the
/// limits of the batches sent across it.
pub(crate) struct Boundary {
    /// The top part below the boundary, the one that receives its calls,
    /// at its position in the chain, when the chain's check found the
    /// boundary: what a panic below the boundary names, and what a refusal
    /// to get ready names when no part below was named. `None` for a
    /// boundary read on a run, whose part's position is not known: a panic
    /// or a refusal then passes up unnamed, to the boundary above, which
    /// names the part at its position.
    pub(crate) part: Option<PartId>,
    /// The needs crossing the boundary down, as the admission checked
    /// them, or as read on the run: what the part below gets ready for.
    pub(crate) needs: Needs,
    /// The offer crossing the boundary up, as the admission checked it, or
    /// as read on the run: what the part above is answered when it asks
    /// the part below.
    pub(crate) offer: Offer,
    /// The limits of the batches crossing the boundary, from the set the
    /// part below accepts, that offer and those needs.
    pub(crate) batch_limits: BatchLimits,
}

impl Boundary {
    /// `needs` crossing down to `part`, if known, a receiver of
    /// `receiver`'s instructions, and `offer` crossing up, with the limits
    /// of the batches sent it.
    pub(crate) fn new(
        part: Option<PartId>,
        needs: Needs,
        offer: Offer,
        receiver: InstructionSet,
    ) -> Self {
        Self {
            batch_limits: BatchLimits::new(receiver, &offer, &needs),
            part,
            needs,
            offer,
        }
    }
}
