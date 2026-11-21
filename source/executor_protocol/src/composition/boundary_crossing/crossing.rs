// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What crosses one boundary of a chain ([`Crossing`]), as the chain's
//! check found it: kept by the part that holds the boundary, a joined part
//! or the runner, so that every call across it is checked, and every run
//! gets the part below ready, with the values the admission checked.

use super::BatchLimits;
use crate::{InstructionSet, Needs, Offer, PartId};

/// What crosses one boundary of a checked chain, and the limits of the
/// batches sent across it.
pub(crate) struct Crossing {
    /// The top part below the boundary, the one that receives its calls:
    /// what a panic below the boundary names, and what a refusal to get
    /// ready names when no part below was named.
    pub(crate) part: PartId,
    /// The needs crossing the boundary down, as the admission checked
    /// them: what the part below gets ready for on every run.
    pub(crate) needs: Needs,
    /// The offer crossing the boundary up, as the admission checked it:
    /// what the part above is answered when it asks the part below.
    pub(crate) offer: Offer,
    /// The limits of the batches crossing the boundary, from the set the
    /// part below accepts, that offer and those needs.
    pub(crate) batch_limits: BatchLimits,
}

impl Crossing {
    /// `needs` crossing down to `part`, a receiver of `receiver`'s
    /// instructions, and `offer` crossing up, with the limits of the
    /// batches sent it.
    pub(crate) fn new(part: PartId, needs: Needs, offer: Offer, receiver: InstructionSet) -> Self {
        Self {
            batch_limits: BatchLimits::new(receiver, &offer, &needs),
            part,
            needs,
            offer,
        }
    }
}
