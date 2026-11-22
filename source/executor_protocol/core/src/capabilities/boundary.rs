// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Where two parts meet: the [`Boundary`] the join of two parts keeps,
//! through which every call across it goes.

mod batch_limits;

pub use batch_limits::BatchFault;
pub(crate) use batch_limits::BatchLimits;

use crate::{
    BoundaryCompatible, Capabilities, InstructionMismatch, InstructionSet, OpcodeMap, PartId,
};

/// Where two parts meet, as the join found it: the opcode map of the
/// instructions sent across, which the part above binds; the set the part
/// below accepts and what it provides, which each batch sent across is
/// checked against; and, on each run, what arrives at the part below,
/// which it gets ready for.
///
/// The protocol keeps one for each boundary of a chain, through which
/// every call across it goes.
#[derive(Clone, Debug)]
pub(crate) struct Boundary {
    opcode_map: OpcodeMap,
    /// The instructions the part below accepts, as the join read them.
    pub(crate) accepted: InstructionSet,
    /// What arrives at the part below on this run: what it gets ready for.
    pub(crate) required: Capabilities,
    /// What the part below provides, as the join read it: what the part
    /// above is answered when it asks the part below.
    pub(crate) provided: Capabilities,
    /// The limits of each batch sent across, from `required`, `provided`
    /// and `accepted`: compiled once per run, compared per batch.
    pub(crate) batch_limits: BatchLimits,
    /// The top part below the boundary, the one that receives its calls,
    /// at its position in the chain, when the run's `ready` gave it one:
    /// what a panic below the boundary names. `None` for a joined part
    /// readied without a position, inside a part of the user's: a panic
    /// then passes up unnamed, to the boundary above, which names the
    /// part at its position.
    pub(crate) part: Option<PartId>,
}

impl Boundary {
    /// Where a part that sends `sends` meets one that accepts `accepted`
    /// and provides `provided`, before any run, if the instructions fit:
    /// the one check between neighbours.
    ///
    /// # Errors
    ///
    /// Every instruction of `sends` that does not fit `accepted`.
    pub(crate) fn fit(
        sends: &InstructionSet,
        accepted: &InstructionSet,
        provided: &Capabilities,
    ) -> Result<Self, Vec<InstructionMismatch>> {
        let opcode_map = sends.fits(accepted)?;
        let required = Capabilities::UNCONSTRAINED;
        Ok(Self {
            opcode_map,
            batch_limits: BatchLimits::new(&required, provided, accepted),
            accepted: accepted.clone(),
            required,
            provided: provided.clone(),
            part: None,
        })
    }

    /// The receiver's opcode for each instruction sent across.
    pub(crate) fn opcode_map(&self) -> &OpcodeMap {
        &self.opcode_map
    }

    /// This boundary on a run, as it starts: with `required`, what arrives
    /// at the part below, and the limits of each batch sent across.
    // Once per run: kept out of line, so that it does not grow the
    // per-call code of the run it would be inlined into.
    #[inline(never)]
    pub(crate) fn require(&mut self, required: Capabilities) {
        self.batch_limits = BatchLimits::new(&required, &self.provided, &self.accepted);
        self.required = required;
    }
}
