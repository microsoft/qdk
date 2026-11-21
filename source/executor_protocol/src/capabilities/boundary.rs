// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Where two parts meet: the [`Boundary`], what
//! [`Capabilities`]' `fits` gives when the part above's requirement fits
//! the part below's provision.

mod batch_limits;

pub(crate) use batch_limits::{BatchFault, BatchLimits};

use crate::{Capabilities, OpcodeMap, PartId};

/// Where two parts meet, as the check of the part above's requirement
/// against the part below's provision found it: the opcode map, which the
/// part above binds; the capabilities required, which the part below gets
/// ready for; and those provided, which each batch sent across is checked
/// against.
///
/// Only [`fits`](crate::BoundaryCompatible::fits) builds it. The protocol
/// keeps one for each boundary of a chain, through which every call across
/// it goes.
#[derive(Clone, Debug)]
pub struct Boundary {
    opcode_map: OpcodeMap,
    required: Capabilities,
    provided: Capabilities,
    /// The limits of each batch sent across, from `required` and
    /// `provided`: compiled once per run, compared per batch.
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
    /// Where `required` meets `provided`, with the opcode map their fit
    /// gave.
    pub(super) fn new(
        opcode_map: OpcodeMap,
        required: &Capabilities,
        provided: &Capabilities,
    ) -> Self {
        Self {
            opcode_map,
            batch_limits: required.batch_limits(provided),
            required: required.clone(),
            provided: provided.clone(),
            part: None,
        }
    }

    /// The receiver's opcode for each instruction sent across.
    #[must_use]
    pub fn opcode_map(&self) -> &OpcodeMap {
        &self.opcode_map
    }

    /// The capabilities required across: what the part below gets ready
    /// for.
    #[must_use]
    pub fn required(&self) -> &Capabilities {
        &self.required
    }

    /// The capabilities provided across: what the part above is answered
    /// when it asks the part below.
    #[must_use]
    pub fn provided(&self) -> &Capabilities {
        &self.provided
    }

    /// This boundary on a run, as it starts: with the resources and
    /// features `required` states, once forwarded across the parts that do
    /// not state them, and the instructions its fit matched.
    pub(crate) fn require(&mut self, required: &Capabilities) {
        self.required = self.required.with_kinds_of(required);
    }

    /// This boundary on a run, at its first call: with the resources and
    /// features `provided` states, once forwarded across the parts that do
    /// not state them, and the instructions its fit matched; and the
    /// limits of each batch sent across, from what is required and
    /// provided.
    pub(crate) fn provide(&mut self, provided: &Capabilities) {
        self.provided = self.provided.with_kinds_of(provided);
        self.batch_limits = self.required.batch_limits(&self.provided);
    }
}
