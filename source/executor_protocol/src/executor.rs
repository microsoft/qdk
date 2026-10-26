// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Executors, what they offer, and how they are closed.

use crate::{
    BuildError, Completion, ComponentId, ComponentRngs, Message, Need, Rejected, ShotSet, Unmet,
};

/// An executor of messages of kind `M`: a terminal that runs them, or an
/// adapter that keeps their meaning and passes the work on to the executor it
/// wraps.
///
/// Its reply carries `M`'s events, so an executor cannot reply with another
/// level's events. The reply's type is the executor's own, not a boxed one,
/// so a chain of executors costs no allocation or dynamic dispatch per call.
pub trait Executor<M: Message> {
    /// The reply to one call, which delivers `M`'s events.
    type Response: Completion<Item = M::Event>;

    /// Starts the work of `message` and returns its reply.
    ///
    /// # Errors
    ///
    /// [`Rejected`] when the message is refused. A rejection means that no
    /// work was started, so the caller may correct the message and send it
    /// again without undoing anything; a run that starts and cannot finish
    /// is reported through the reply instead.
    fn call(&mut self, message: M::Ref<'_>) -> Result<Self::Response, Rejected>;
}

/// What an executor offers, so that a stack can check that a caller's needs
/// fit every component before anything runs, instead of failing, or not,
/// during the run.
///
/// The instruction set an executor accepts is not described here: it is the
/// instruction type of its message kind (see [`Batch`](crate::Batch)), so it
/// is checked when a stack is compiled.
pub trait Describe {
    /// Its capabilities. An adapter maps those of what it wraps; a component
    /// that only passes the work on reports them unchanged.
    fn capabilities(&self) -> Capabilities;
}

/// What an executor offers.
///
/// New capabilities are added as fields whose value in
/// [`Capabilities::default`] keeps existing executors' meaning, hence
/// `#[non_exhaustive]`: build one from the default and its `with_` methods.
///
/// There is no instruction-set field: the instructions an executor accepts
/// are its message's instruction type ([`Describe`]), checked by the
/// compiler. An instruction set chosen at run time (one that crosses a
/// link) would be added as a field `isa: Option<Arc<Isa>>`, `None` for the
/// executors of today. For the same reason
/// [`resets`](Capabilities::resets) is a field here, where the protocol's
/// prototype derives it from the instruction set. The crate's README has
/// the details.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each is an independent capability, matched one to one by a need"
)]
pub struct Capabilities {
    /// The most qubits a shot may use; `None` when it does not say.
    pub qubits: Option<u32>,
    /// Whether a later call may continue a shot after its outcomes were read:
    /// the executor keeps a state per shot between calls.
    pub feedback: bool,
    /// The most states it holds at once; `None` when it knows no limit.
    pub states: Option<u64>,
    /// Whether each shot draws only from its own random numbers, so that a
    /// shot's outcomes do not depend on which shots share its session.
    pub replayable: bool,
    /// Whether it can prepare a qubit again during a shot (a reset).
    pub resets: bool,
    /// Whether the report at close gives each shot's log probability, over
    /// every draw made for it. Only a simulator knows it; not hardware, and
    /// not a sampler that returns values only.
    pub probabilities: bool,
    /// The ids of the components that draw random numbers at or below it,
    /// each with its own seed per shot: a component that draws is refused
    /// when built over one that already draws with its id
    /// ([`Capabilities::drawing`]).
    pub rngs: ComponentRngs,
}

impl Default for Capabilities {
    /// An executor that states no limit: any number of qubits and states,
    /// feedback, replayable shots and resets, drawing with the terminal's id
    /// only. No probabilities, which an executor states only when it
    /// computes them.
    fn default() -> Self {
        Self {
            qubits: None,
            feedback: true,
            states: None,
            replayable: true,
            resets: true,
            probabilities: false,
            rngs: ComponentRngs::terminal(),
        }
    }
}

impl Capabilities {
    /// These capabilities, with at most `qubits` qubits per shot.
    #[must_use]
    pub fn with_qubits(mut self, qubits: Option<u32>) -> Self {
        self.qubits = qubits;
        self
    }

    /// These capabilities, with or without feedback.
    #[must_use]
    pub fn with_feedback(mut self, feedback: bool) -> Self {
        self.feedback = feedback;
        self
    }

    /// These capabilities, with at most `states` states at once.
    #[must_use]
    pub fn with_states(mut self, states: Option<u64>) -> Self {
        self.states = states;
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

    /// These capabilities, for a component that draws with `id` above the
    /// components they describe.
    ///
    /// # Errors
    ///
    /// [`BuildError::RngInUse`] when a component below already draws with
    /// `id`.
    pub fn drawing(mut self, id: ComponentId) -> Result<Self, BuildError> {
        self.rngs = self.rngs.with(id)?;
        Ok(self)
    }

    /// Checks `needs`, for a session of `shots`, against these capabilities.
    /// For the component that sets the limits; a rejection names
    /// `component`, so the caller knows which one to change.
    ///
    /// # Errors
    ///
    /// [`Rejected::Unmet`] for the first need these capabilities do not
    /// meet, in the order qubits, feedback, resets, replayable,
    /// probabilities, states (one per shot of `shots`).
    pub fn admit(&self, component: &str, shots: &ShotSet, needs: &Needs) -> Result<(), Rejected> {
        if let (Some(needed), Some(offered)) = (needs.qubits, self.qubits) {
            Unmet::check(component, Need::Qubits, needed.into(), offered.into())?;
        }
        Unmet::check(
            component,
            Need::Feedback,
            needs.feedback.into(),
            self.feedback.into(),
        )?;
        Unmet::check(
            component,
            Need::Resets,
            needs.resets.into(),
            self.resets.into(),
        )?;
        Unmet::check(
            component,
            Need::Replayable,
            needs.replayable.into(),
            self.replayable.into(),
        )?;
        Unmet::check(
            component,
            Need::Probabilities,
            needs.probabilities.into(),
            self.probabilities.into(),
        )?;
        if let Some(offered) = self.states {
            let needed = u64::try_from(shots.len()).unwrap_or(u64::MAX);
            Unmet::check(component, Need::States, needed, offered)?;
        }
        Ok(())
    }
}

/// What a caller states when it opens a session, for the whole session, so
/// that the stack can refuse it before anything runs rather than fail during
/// the run.
///
/// The default states nothing but the one state per shot that every session
/// implies, so every shot strategy accepts it that can hold the session's
/// shots: a caller states a need only when it wants it checked. New needs
/// are added as fields whose default states nothing, hence `#[non_exhaustive]`: build one from the
/// default and its `with_` methods.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each is an independent need, checked one to one against a capability"
)]
pub struct Needs {
    /// The qubits each shot uses; `None`: not stated, not checked.
    pub qubits: Option<u32>,
    /// An outcome is read before the end and decides what runs next, so the
    /// strategy must keep a state per shot between calls.
    pub feedback: bool,
    /// The program prepares qubits again during the run.
    pub resets: bool,
    /// Each shot draws only from its own random numbers, so its outcomes do
    /// not depend on the other shots of its session.
    pub replayable: bool,
    /// The report at close gives each shot's log probability.
    pub probabilities: bool,
}

impl Needs {
    /// These needs, with `qubits` qubits per shot.
    #[must_use]
    pub fn with_qubits(mut self, qubits: Option<u32>) -> Self {
        self.qubits = qubits;
        self
    }

    /// These needs, with or without feedback.
    #[must_use]
    pub fn with_feedback(mut self, feedback: bool) -> Self {
        self.feedback = feedback;
        self
    }

    /// These needs, with or without resets.
    #[must_use]
    pub fn with_resets(mut self, resets: bool) -> Self {
        self.resets = resets;
        self
    }

    /// These needs, with or without replayable shots.
    #[must_use]
    pub fn with_replayable(mut self, replayable: bool) -> Self {
        self.replayable = replayable;
        self
    }

    /// These needs, with or without each shot's log probability at close.
    #[must_use]
    pub fn with_probabilities(mut self, probabilities: bool) -> Self {
        self.probabilities = probabilities;
        self
    }
}

/// A report that composes: an empty report, [`Default`], and a combination,
/// [`Fold::fold`].
///
/// Needed because each component of a stack keeps totals of its own (the
/// operations a simulator applied, the programs an adapter ran), and the
/// report of a stack must hold them all: an adapter's report holds its own
/// and the one of the executor it wraps, and the reports of parts of one run
/// (shots run separately) fold into the report of the whole.
pub trait Fold: Default {
    /// Adds `other` into this report.
    fn fold(&mut self, other: Self);
}

impl Fold for () {
    fn fold(&mut self, (): Self) {}
}

/// Ends an executor's work and returns its report. Adapters close the
/// executor they wrap and include its report in theirs.
pub trait Close {
    /// The executor's totals.
    type Report: Fold;

    /// Ends the executor's work: later calls are rejected with
    /// [`Rejected::Closed`]. A second `close` returns an empty report.
    fn close(&mut self) -> Self::Report;
}
