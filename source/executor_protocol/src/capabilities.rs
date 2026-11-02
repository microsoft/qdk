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

/// What an executor offers, so that a chain can check that a caller's needs
/// fit it before anything runs, instead of failing, or not, during the run.
/// An adapter's executor, once composed, describes itself too; the adapter
/// maps what the executor downstream of it offers when it is built
/// ([`AdapterBoundary`]), not through this trait.
pub trait Describe {
    /// Its capabilities. An adapter's executor gives those its adapter
    /// mapped when built; one that only passes the work on gives those below
    /// unchanged.
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
    /// `changes_no_shot_a_call_does_not_name`). A shot's conditions are part
    /// of that state: an executor that offers it keeps each shot's conditions
    /// from one call to the next (`docs/replies.md`, §Shot conditions), which
    /// is checked only at run time, by the kit's
    /// `keeps_each_shots_conditions_from_one_call_to_the_next`, for the
    /// conditions an earlier outcome of the shot carried.
    pub resumable_shots: bool,
    /// The most shots it holds at once, however it holds them (a state
    /// each, or shared); `None` when it knows no limit. A limit it always
    /// has (its memory holds that many); a shortage now (another run holds
    /// the memory) is [`Rejected::Unavailable`] instead.
    pub max_shots: Option<u64>,
    /// Whether each shot replays: run again on the run's seed, alone or
    /// among other shots, in one call or split over several, it gives what it
    /// gave (the conformance kit's
    /// `gives_each_shot_alone_what_it_gives_among_others`), since it draws
    /// only from its own random numbers, so its outcomes do not depend on
    /// which shots share its executor.
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
    pub log_probability: bool,
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
    /// its calls through as they are offers downstream's, and one that states
    /// none and sends a set of its own offers none.
    /// [`resets`](Capabilities::resets) is not derived from it: a component
    /// need not describe a set.
    pub instruction_set: Option<Arc<InstructionSet>>,
}

impl PartialEq for Capabilities {
    fn eq(&self, other: &Self) -> bool {
        // Destructured, so that a new field is not left out unseen.
        let Self {
            qubits,
            resumable_shots,
            max_shots,
            replayable,
            resets,
            log_probability,
            rng_ids,
            run_seed: _,
            instruction_set,
        } = self;
        *qubits == other.qubits
            && *resumable_shots == other.resumable_shots
            && *max_shots == other.max_shots
            && *replayable == other.replayable
            && *resets == other.resets
            && *log_probability == other.log_probability
            && *rng_ids == other.rng_ids
            && *instruction_set == other.instruction_set
    }
}

impl fmt::Debug for Capabilities {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            qubits,
            resumable_shots,
            max_shots,
            replayable,
            resets,
            log_probability,
            rng_ids,
            run_seed: _,
            instruction_set,
        } = self;
        f.debug_struct("Capabilities")
            .field("qubits", qubits)
            .field("resumable_shots", resumable_shots)
            .field("max_shots", max_shots)
            .field("replayable", replayable)
            .field("resets", resets)
            .field("log_probability", log_probability)
            .field("rng_ids", rng_ids)
            .field("instruction_set", instruction_set)
            .finish()
    }
}

impl Default for Capabilities {
    /// An executor that states no limit, any number of qubits and shots at
    /// once, and offers nothing it does not state: no resumable shots,
    /// replayable shots, resets or log probability, and no instruction set,
    /// drawing with the last executor's id only. So an executor states what
    /// it offers, and one that forgets to state a capability is refused a
    /// job that needs it, naming the need ([`Need`](crate::Need)), rather
    /// than running it wrong.
    fn default() -> Self {
        Self {
            qubits: None,
            resumable_shots: false,
            max_shots: None,
            replayable: false,
            resets: false,
            log_probability: false,
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

    /// These capabilities, with or without resumable shots.
    #[must_use]
    pub fn with_resumable_shots(mut self, resumable_shots: bool) -> Self {
        self.resumable_shots = resumable_shots;
        self
    }

    /// These capabilities, with at most `max_shots` shots at once. No limit
    /// is the default.
    #[must_use]
    pub fn with_max_shots(mut self, max_shots: u64) -> Self {
        self.max_shots = Some(max_shots);
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
    pub fn with_log_probability(mut self, log_probability: bool) -> Self {
        self.log_probability = log_probability;
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
