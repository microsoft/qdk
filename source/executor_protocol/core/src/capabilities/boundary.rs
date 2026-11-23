// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Where two parts meet: the [`Boundary`] the join of two parts keeps,
//! through which every call across it goes.

mod batch_limits;

pub use batch_limits::BatchFault;
pub(crate) use batch_limits::BatchLimits;

use crate::{Binding, Capabilities, InstructionSet, PartId};

/// Where two parts meet, as the join found it: the set the part below
/// accepts and what it provides, which each batch sent across is checked
/// against; and, on each run, what arrives at the part below, which it
/// gets ready for.
///
/// A batch crosses only once a run has started at the boundary
/// ([`require`](Self::require)): its limits are built from what arrives
/// on that run, never from the join's requirement of nothing, which a
/// set's check could read as allowing no instruction.
///
/// The protocol keeps one for each boundary of a chain, through which
/// every call across it goes.
pub(crate) struct Boundary<S: InstructionSet> {
    /// The instructions the part below accepts, as the join read them.
    pub(crate) accepted: S,
    /// What arrives at the part below on this run: what it gets ready for.
    /// Before any run, the requirement of nothing.
    pub(crate) required: Capabilities<S>,
    /// What the part below provides, as the join read it: what the part
    /// above is answered when it asks the part below. `None` limits
    /// nothing.
    pub(crate) provided: Option<Capabilities<S>>,
    /// The limits of each batch sent across, from `required`, `provided`
    /// and `accepted`: prepared once per run, compared per batch. `None`
    /// until a run starts.
    pub(crate) batch_limits: Option<BatchLimits<S>>,
    /// The top part below the boundary, the one that receives its calls,
    /// at its position in the chain, when the run's `ready` gave it one:
    /// what a panic below the boundary names. `None` for a joined part
    /// readied without a position, inside a part of the user's: a panic
    /// then passes up unnamed, to the boundary above, which names the
    /// part at its position.
    pub(crate) part: Option<PartId>,
}

impl<S: InstructionSet> Boundary<S> {
    /// Where a part that sends `sends` meets one that accepts `accepted`
    /// and provides `provided`, before any run, if the instructions fit:
    /// the one check between neighbours. Gives the boundary, and the
    /// fit's binding, which the part above is given.
    ///
    /// # Errors
    ///
    /// The set's mismatch, when `sends` does not fit `accepted`.
    pub(crate) fn fit(
        sends: &S,
        accepted: &S,
        provided: Option<&Capabilities<S>>,
    ) -> Result<(Self, Binding<S>), S::Mismatch> {
        let binding = sends.fits(accepted)?;
        let boundary = Self {
            batch_limits: None,
            accepted: accepted.clone(),
            required: Capabilities::needs_nothing(),
            provided: provided.cloned(),
            part: None,
        };
        Ok((boundary, binding))
    }

    /// This boundary on a run, as it starts: with `required`, what arrives
    /// at the part below, and the limits of each batch sent across.
    // Once per run: kept out of line, so that it does not grow the
    // per-call code of the run it would be inlined into.
    #[inline(never)]
    pub(crate) fn require(&mut self, required: Capabilities<S>) {
        self.batch_limits = Some(BatchLimits::new(
            &required,
            self.provided.as_ref(),
            &self.accepted,
        ));
        self.required = required;
    }

    /// The limits of each batch sent across on this run.
    ///
    /// # Panics
    ///
    /// Before any run has started at this boundary: a part calls another
    /// only once a run got it ready, as its role says.
    pub(crate) fn batch_limits(&self) -> &BatchLimits<S> {
        self.batch_limits.as_ref().expect(READY_FIRST)
    }
}

/// Why a boundary is crossed only on a run.
pub(crate) const READY_FIRST: &str = "each run gets a part ready before calling it, as its \
                                      role says, and a joined part whose halves do not fit \
                                      refuses every run then";
