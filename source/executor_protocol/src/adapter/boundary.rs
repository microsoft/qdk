// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Where an adapter meets what is downstream of it ([`AdapterBoundary`]): what
//! [`compose`](fn@crate::compose) gives an adapter when it builds it
//! ([`Adapter::build`]), and takes back from it.

use std::error::Error;

#[cfg(doc)]
use crate::{Adapter, Apply, BuildError, ShotContext};
use crate::{AdapterSeeds, Capabilities, ComponentId, OpcodeMap};

/// Where an adapter meets what is downstream of it, as
/// [`compose`](fn@crate::compose) builds it ([`Adapter::build`]): what
/// downstream offers, the adapter's seeds if it draws, where the set the
/// adapter sends is in downstream's, what the adapter offers upstream, and
/// its own faults.
///
/// [`compose`](fn@crate::compose) matches the set an adapter sends in the set
/// downstream once, here, and renumbers the adapter's calls on their way
/// down where the two number them otherwise, unless the adapter adopts the
/// downstream numbering.
pub struct AdapterBoundary<'a> {
    downstream: &'a Capabilities,
    /// The id the adapter draws with ([`Adapter::draws`]).
    drawing: Option<ComponentId>,
    /// Where the set the adapter sends is in downstream's; `None` when it
    /// sends none of its own, or it does not fit.
    numbering: Option<&'a OpcodeMap>,
    adopted: bool,
    offered: Capabilities,
    faults: Vec<Box<dyn Error + Send + Sync>>,
}

impl<'a> AdapterBoundary<'a> {
    /// The boundary of an adapter upstream of what offers `downstream`,
    /// which draws with `drawing`, and whose set `numbering` matches what
    /// the adapter sends in; the adapter offers `offered` until it changes
    /// it.
    pub(crate) fn new(
        downstream: &'a Capabilities,
        drawing: Option<ComponentId>,
        numbering: Option<&'a OpcodeMap>,
        offered: Capabilities,
    ) -> Self {
        Self {
            downstream,
            drawing,
            numbering,
            adopted: false,
            offered,
            faults: Vec::new(),
        }
    }

    /// What downstream offers.
    #[must_use]
    pub fn downstream(&self) -> &'a Capabilities {
        self.downstream
    }

    /// The adapter's seeds, if it draws ([`Adapter::draws`]): the run's
    /// seed, the one the last executor downstream is given
    /// ([`ExecutorFn::seed`](crate::ExecutorFn::seed)), passed up through
    /// the adapters between whatever they offer upstream
    /// ([`offered_upstream`](Self::offered_upstream)), bound to the id the
    /// adapter draws with. An adapter written by hand that draws keeps them
    /// when built, and draws for shot `j` from the
    /// [`seed`](ShotContext::seed) of [`AdapterSeeds::shot`]`(j)`: the seed
    /// an [`adapter_fn`](fn@crate::adapter_fn) drawing with that id makes
    /// shot `j`'s state from. So one seed replays the whole chain, each
    /// adapter that draws under an id of its own.
    ///
    /// `None` when the adapter draws nothing. One that draws where no run's
    /// seed reaches it is refused ([`BuildError::NoRunSeed`]), and then gets
    /// `None` too. One refused for another reason, such as
    /// [`BuildError::RngInUse`], may still get them; it is never applied.
    /// `build` runs even when the adapter is refused, so that every fault is
    /// reported: an adapter keeps the `Option`, and takes its seeds in
    /// [`apply`](crate::Apply::apply), which runs only once the chain is built.
    #[must_use]
    pub fn drawing_seeds(&self) -> Option<AdapterSeeds> {
        Some(AdapterSeeds::new(self.downstream.run_seed?, self.drawing?))
    }

    /// Where each instruction of the set the adapter sends is in the set
    /// downstream; `None` when it sends no set ([`Adapter::sends`]: it
    /// states neither a set it sends nor one it accepts, or only the one it
    /// accepts and sends instructions of another kind), or the set does not
    /// fit.
    ///
    /// Taking it, the adapter undertakes to send its calls in the
    /// downstream numbering, so they reach downstream as they are: no
    /// batch is renumbered on its way down. An adapter that does not take
    /// it sends in its own numbering, and its calls are renumbered where
    /// the two differ, each batch copied.
    pub fn adopt_downstream_numbering(&mut self) -> Option<&'a OpcodeMap> {
        self.adopted |= self.numbering.is_some();
        self.numbering
    }

    /// What the adapter offers upstream: what downstream offers, the qubit
    /// count included, with the instruction set the adapter accepts and the ids
    /// it draws with, and no `log_probability` if it draws
    /// ([`Adapter::draws`]). An adapter changes what it changes: a code offers
    /// a qubit per block of qubits downstream. [`compose`](fn@crate::compose)
    /// keeps the fields it derives as derived, whatever the adapter offers,
    /// even an offer it assigns whole: the run's seed
    /// ([`drawing_seeds`](Self::drawing_seeds)); the instruction set, the one
    /// the adapter states in [`Adapter::accepts`], or downstream's for one that
    /// states none and passes its calls through as they are
    /// ([`Capabilities::instruction_set`]); and the ids drawn with, those below
    /// the adapter and its own ([`Capabilities::rng_ids`]). The offer may turn
    /// `log_probability` off, never on ([`Capabilities::log_probability`]).
    ///
    /// An offer assigned whole states every limit, the shots at once, the
    /// resumable shots and the resets included, so an adapter that assigns one
    /// starts it from `boundary.downstream().clone()`, and changes what it
    /// changes: one made from [`Capabilities::default`] states no limit of
    /// qubits or shots, so it would offer upstream more than downstream
    /// holds, and none of downstream's resumable shots, replayable shots or
    /// resets.
    pub fn offered_upstream(&mut self) -> &mut Capabilities {
        &mut self.offered
    }

    /// Reports `fault`, a fault of the adapter's own, of a type it defines:
    /// for example a code whose block needs more qubits than downstream
    /// holds. It is reported as a [`BuildError::Adapter`] with every other
    /// fault of the parts built together, so that all are fixed at once; a
    /// part composed with the executor first is built on its own, before
    /// this adapter, so its faults come back without this one
    /// (`docs/composition.md`, §Composing a run).
    pub fn fault(&mut self, fault: impl Into<Box<dyn Error + Send + Sync>>) {
        self.faults.push(fault.into());
    }

    /// Whether the adapter adopted the downstream numbering, what it
    /// offers upstream, and its own faults.
    pub(crate) fn into_parts(self) -> (bool, Capabilities, Vec<Box<dyn Error + Send + Sync>>) {
        (self.adopted, self.offered, self.faults)
    }
}
