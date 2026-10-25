// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Executors, what they offer, and how they are closed.

use crate::{Completion, Message, Rejected};

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
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
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
}

impl Default for Capabilities {
    /// An executor that states no limit: any number of qubits and states,
    /// feedback, and replayable shots.
    fn default() -> Self {
        Self {
            qubits: None,
            feedback: true,
            states: None,
            replayable: true,
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
