// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What components offer ([`Describe`], [`Capabilities`]), and the checks,
//! when a chain is built, of what a component upstream of them draws and
//! sends.

use std::fmt;
use std::sync::Arc;

#[cfg(doc)]
use crate::{AdapterBoundary, Rejected};
use crate::{BuildError, ComponentId, InstructionSet, OpcodeMap, RngIds};

/// What a component offers (an executor of several shots, an adapter), so
/// that a chain can check that a caller's needs fit it before anything runs,
/// instead of failing, or not, during the run.
pub trait Describe {
    /// Its capabilities. An adapter maps those of what it wraps; a component
    /// that only passes the work on reports them unchanged.
    fn capabilities(&self) -> Capabilities;
}

/// What a component offers.
///
/// New capabilities are added as fields whose value in
/// [`Capabilities::default`] keeps existing executors' meaning, hence
/// `#[non_exhaustive]`: build one from the default and its `with_` methods.
///
/// The instructions an executor accepts are first its message's instruction
/// type (see [`InstructionBatch`](crate::InstructionBatch)), whose shape the
/// compiler checks. It
/// may also describe, in [`instruction_set`](Capabilities::instruction_set),
/// which of them it runs, whatever their type;
/// an executor of [`InstructionCall`](crate::InstructionCall)s, whose set
/// is chosen at run time, describes it, since nothing else says which calls
/// it runs. The crate's `docs/instruction-sets.md` has the details.
#[derive(Clone, Eq)]
#[non_exhaustive]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each is an independent capability, matched one to one by a need"
)]
pub struct Capabilities {
    /// The most qubits a shot may use; `None` when it does not say.
    pub qubits: Option<u32>,
    /// Whether a later call may continue a shot where an earlier call left
    /// it, after a read or not: the executor keeps a state per shot between
    /// calls, which only a call on that shot changes (the conformance kit's
    /// `continues_a_shot_from_one_call_to_the_next` and
    /// `changes_no_shot_a_call_does_not_name`).
    pub feedback: bool,
    /// The most states it holds at once; `None` when it knows no limit. A
    /// limit it always has (its memory holds that many); a shortage now
    /// (another run holds the memory) is [`Rejected::Unavailable`]
    /// instead.
    pub states: Option<u64>,
    /// Whether each shot draws only from its own random numbers, so that a
    /// shot's outcomes do not depend on which shots share its executor (the
    /// conformance kit's `gives_each_shot_alone_what_it_gives_among_others`).
    pub replayable: bool,
    /// Whether it can prepare a qubit again during a shot (a reset). An adapter
    /// passes downstream's on by default, one that changes the instruction set
    /// included (it may change it, through
    /// `AdapterBoundary::offered_upstream`): where sets are described, the
    /// instructions matched are what guarantee a reset, since a caller that
    /// resets has a reset instruction in its set, which a chain whose set lacks
    /// one refuses. It informs only a caller that describes no set.
    pub resets: bool,
    /// Whether the report at close gives each shot's log probability, over
    /// every draw made for it. Only a simulator knows it; not hardware, and not
    /// a sampler that returns values only. An adapter may turn it off but never
    /// on, whatever it offers ([`AdapterBoundary::offered_upstream`]): its
    /// report passes up the log probability from below, so it is never offered
    /// upstream of an adapter that draws, not even by one above that draws
    /// nothing.
    pub probabilities: bool,
    /// The ids of the components that draw random numbers at or downstream of
    /// it, each with its own seed per shot: a component that draws is refused
    /// when built against one that already draws with its id
    /// ([`Capabilities::drawing`]). Derived by [`compose`](fn@crate::compose):
    /// an adapter offers those below it and its own
    /// ([`Adapter::draws`](crate::Adapter::draws)), whatever it offers
    /// ([`AdapterBoundary::offered_upstream`]).
    pub rng_ids: RngIds,
    /// The run's seed the components at or downstream of it draw from: the
    /// one the last executor is given
    /// ([`ExecutorFn::seed`](crate::ExecutorFn::seed)), passed up the
    /// chain as it is built, so that an adapter that draws derives its
    /// seeds from it, whatever an adapter offers
    /// (`an_adapter_reads_the_runs_seed_through_one_that_states_its_offer_whole`);
    /// `None` when none is given, and then an adapter that draws is refused
    /// ([`BuildError::NoRunSeed`]). Set by the facility only:
    /// planned, a way for an executor written by hand to state it. Not a
    /// capability, so neither compared nor shown: two simulators given
    /// other seeds offer the same.
    pub(crate) run_seed: Option<u64>,
    /// The instruction set it accepts, whatever the type of its instructions;
    /// `None` when it does not say, and then a caller that needs any
    /// instruction is refused. An adapter states the set it accepts in
    /// [`Adapter::accepts`](crate::Adapter::accepts), which
    /// [`compose`](fn@crate::compose) offers upstream of it whatever it offers
    /// ([`AdapterBoundary::offered_upstream`]); one that states none and passes
    /// its calls through as they are offers downstream's.
    /// [`resets`](Capabilities::resets) is not derived from it: a component
    /// need not describe a set.
    pub instruction_set: Option<Arc<InstructionSet>>,
}

impl PartialEq for Capabilities {
    fn eq(&self, other: &Self) -> bool {
        // Destructured, so that a new field is not left out unseen.
        let Self {
            qubits,
            feedback,
            states,
            replayable,
            resets,
            probabilities,
            rng_ids,
            run_seed: _,
            instruction_set,
        } = self;
        *qubits == other.qubits
            && *feedback == other.feedback
            && *states == other.states
            && *replayable == other.replayable
            && *resets == other.resets
            && *probabilities == other.probabilities
            && *rng_ids == other.rng_ids
            && *instruction_set == other.instruction_set
    }
}

impl fmt::Debug for Capabilities {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            qubits,
            feedback,
            states,
            replayable,
            resets,
            probabilities,
            rng_ids,
            run_seed: _,
            instruction_set,
        } = self;
        f.debug_struct("Capabilities")
            .field("qubits", qubits)
            .field("feedback", feedback)
            .field("states", states)
            .field("replayable", replayable)
            .field("resets", resets)
            .field("probabilities", probabilities)
            .field("rng_ids", rng_ids)
            .field("instruction_set", instruction_set)
            .finish()
    }
}

impl Default for Capabilities {
    /// An executor that states no limit: any number of qubits and states,
    /// feedback, replayable shots and resets, drawing with the last executor's
    /// id only. No probabilities, which an executor states only when it
    /// computes them, and no instruction set, which an executor states only
    /// when it describes one.
    ///
    /// Known issue (an open design question, with whether `feedback`
    /// requires a state per shot): an executor built from the default
    /// claims `feedback` until it states otherwise, so one that keeps no
    /// state between calls and forgets `with_feedback(false)` accepts a
    /// program with a call before its last batch, whose later batches then
    /// find fresh states, with no error.
    fn default() -> Self {
        Self {
            qubits: None,
            feedback: true,
            states: None,
            replayable: true,
            resets: true,
            probabilities: false,
            rng_ids: RngIds::last_executor(),
            run_seed: None,
            instruction_set: None,
        }
    }
}

impl Capabilities {
    /// These capabilities, with at most `qubits` qubits per shot. No limit
    /// is the default.
    #[must_use]
    pub fn with_qubits(mut self, qubits: u32) -> Self {
        self.qubits = Some(qubits);
        self
    }

    /// These capabilities, with or without feedback.
    #[must_use]
    pub fn with_feedback(mut self, feedback: bool) -> Self {
        self.feedback = feedback;
        self
    }

    /// These capabilities, with at most `states` states at once. No limit
    /// is the default.
    #[must_use]
    pub fn with_states(mut self, states: u64) -> Self {
        self.states = Some(states);
        self
    }

    /// These capabilities, with or without replayable shots.
    #[must_use]
    pub fn with_replayable(mut self, replayable: bool) -> Self {
        self.replayable = replayable;
        self
    }

    /// These capabilities, with or without resets.
    #[must_use]
    pub fn with_resets(mut self, resets: bool) -> Self {
        self.resets = resets;
        self
    }

    /// These capabilities, with or without each shot's log probability at
    /// close.
    #[must_use]
    pub fn with_probabilities(mut self, probabilities: bool) -> Self {
        self.probabilities = probabilities;
        self
    }

    /// These capabilities, for a component that draws with `id` upstream of the
    /// components they describe.
    ///
    /// # Errors
    ///
    /// [`BuildError::RngInUse`] when a component downstream already draws with
    /// `id`.
    pub fn drawing(mut self, id: ComponentId) -> Result<Self, BuildError> {
        self.rng_ids = self.rng_ids.with(id)?;
        Ok(self)
    }

    /// These capabilities, with the instruction set `instruction_set`.
    #[must_use]
    pub fn with_instruction_set(mut self, instruction_set: Option<Arc<InstructionSet>>) -> Self {
        self.instruction_set = instruction_set;
        self
    }

    /// Matches `sends`, the set an adapter sends, in the set of the
    /// components these capabilities describe, as
    /// [`InstructionSet::opcodes_in`] does: the one check of
    /// [`compose`](fn@crate::compose), whose map an adapter is built with.
    ///
    /// # Errors
    ///
    /// [`BuildError::NoInstructionSetDownstream`] when none is described
    /// downstream; [`BuildError::Instructions`] when `sends` does not
    /// [fit](InstructionSet::fits) the instruction set downstream.
    pub(crate) fn match_sends(&self, sends: &InstructionSet) -> Result<OpcodeMap, BuildError> {
        let downstream = self
            .instruction_set
            .as_deref()
            .ok_or(BuildError::NoInstructionSetDownstream)?;
        sends
            .opcodes_in(downstream)
            .map_err(BuildError::Instructions)
    }
}
