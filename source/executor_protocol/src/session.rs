// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Level L2, sessions: a batch run on each shot of a set of shots; the shot
//! strategies that open sessions ([`ShotStrategy`]); and the first of them,
//! [`PerShot`], which runs each shot on its own L3 executor.

use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::task::Waker;

use crate::lifecycle::{Lifecycle, SessionBound};
use crate::shot::ShotIndex;
use crate::{
    Batch, Capabilities, Close, Completion, Describe, Executor, Fold, Map, Message, Needs, Outcome,
    Rejected, ShotContext, ShotId, ShotSet, Status, Value,
};

/// Level L2: a batch of instructions of type `I`, run on each shot of a set
/// of shots of a session, each shot's state carrying over from the batches
/// before it.
///
/// This type is never constructed: it only names the message kind.
pub struct Submission<I> {
    instructions: PhantomData<fn() -> I>,
}

impl<I> Message for Submission<I> {
    type Ref<'a>
        = SubmissionRef<'a, I>
    where
        Self: 'a;
    type Event = ShotOutcome;
}

/// A submission, as the caller lends it.
///
/// The shots are a [`ShotSet`], so a list that may repeat a shot, or name
/// none, is not a submission's shots.
#[derive(Debug)]
pub struct SubmissionRef<'a, I> {
    /// The shots to run the batch on, each once, in this order.
    pub shots: &'a ShotSet,
    /// The batch, run on each shot of `shots`.
    pub batch: &'a [I],
}

// Not derived: a derive would ask `I: Clone`, and a reference is copied
// whatever it refers to.
impl<I> Clone for SubmissionRef<'_, I> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<I> Copy for SubmissionRef<'_, I> {}

/// An outcome of one shot: what level L2 reports, where level L3 reports an
/// [`Outcome`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ShotOutcome {
    /// The shot that produced the outcome.
    pub shot: ShotId,
    /// The batch's name for the outcome, as in an [`Outcome`].
    pub tag: u32,
    /// The outcome's value.
    pub value: Value,
}

/// How the shots of a session are run: a strategy that maps the shots J onto
/// executors and states, and opens level-L2 sessions with
/// `open(seed, shots, needs)`.
///
/// A caller that runs shots (a driver, the Python API, an adapter for
/// another stack) needs only a session to submit batches to; how the shots
/// share work is the strategy's choice. So each way of running shots is one
/// implementation, and a new one is added without changing its callers:
///
/// | Strategy                            | How the shots of J run                                       | Replayable              |
/// | ----------------------------------- | ------------------------------------------------------------ | ----------------------- |
/// | [`PerShot`]                         | each on its own L3 executor, made by a [`MakeShot`]          | yes                     |
/// | an ensemble terminal (MPS batch)    | together, from one state and one seed; Base programs only    | no                      |
/// | grouped (not built yet)             | on shared states, split where their values differ            | yes                     |
/// | sharded (not built yet)             | J split over several strategies, their replies merged        | as the inner strategies |
///
/// An adapter that wraps a strategy (a lowering, a noise source, exclusive
/// use of a device) is again a strategy, so a stack is built by wrapping.
///
/// Every strategy keeps the same contract: the needs are checked at `open`,
/// before anything is allocated; each shot's state carries over from one
/// submission to the next; and its session is closed once, with a report
/// that folds. Whether the shots are replayable (each shot's outcomes
/// independent of the other shots of J) is the strategy's to offer, through
/// [`Capabilities`]; a caller that needs it says so in its [`Needs`].
///
/// The shots J are a [`ShotSet`]: a set by type is checked once, where J is
/// chosen, not by every strategy. J is borrowed, as a submission's shots
/// are: the caller keeps it to name the submissions' shots.
pub trait ShotStrategy<I>: Describe {
    /// An open session: it runs submissions on the shots it was opened on,
    /// describes what it offers, and is closed once.
    type Session: Executor<Submission<I>> + Describe + Close;

    /// A session on `shots`, each shot reset, shot `j`'s components drawing
    /// from the seeds of [`ShotContext::new`]`(seed, j)`.
    ///
    /// # Errors
    ///
    /// [`Rejected::Unmet`] when `needs` cannot be honoured, naming the
    /// component that cannot; nothing is allocated then. Other rejections
    /// as the strategy says ([`Rejected::Unavailable`] when it cannot open a
    /// session now).
    fn open(&self, seed: u64, shots: &ShotSet, needs: &Needs) -> Result<Self::Session, Rejected>;
}

/// Opens a session with the default needs, which every strategy accepts.
///
/// # Errors
///
/// As [`ShotStrategy::open`].
pub fn open<I, S: ShotStrategy<I>>(
    strategy: &S,
    seed: u64,
    shots: &ShotSet,
) -> Result<S::Session, Rejected> {
    strategy.open(seed, shots, &Needs::default())
}

/// Makes the L3 executor of one shot from the shot's context. Wrapped in
/// [`PerShot`], a maker runs each shot of a session on its own executor.
pub trait MakeShot<I>: Describe {
    /// The executor of one shot.
    type Shot: Executor<Batch<I>> + Close;

    /// A fresh shot whose components each draw from their own seed in `ctx`:
    /// the terminal from [`ShotContext::rng_seed`] of
    /// [`ComponentId::TERMINAL`](crate::ComponentId::TERMINAL).
    fn make(&self, ctx: &ShotContext) -> Self::Shot;

    /// `shot`'s log probability so far: the sum of ln p over every draw made
    /// for it, by every component in it. `None`, the default, where it is not
    /// known: absent, not 0. A maker whose capabilities offer
    /// `probabilities` overrides it.
    ///
    /// Read once per shot, as its session closes, since the consumers of a
    /// shot's probability weigh whole shots (importance sampling, a
    /// statistical test of a sampler, postselection rates), and draws that
    /// make no event (noise, a bare reset) count. An associated function, not
    /// a method, so that a session can keep it as a function pointer.
    #[must_use]
    fn log_p(shot: &Self::Shot) -> Option<f64> {
        let _ = shot;
        None
    }
}

/// The per-shot strategy: each shot of a session runs on its own L3
/// executor, made by the maker `F` from the shot's context, and nothing is
/// shared between shots.
///
/// ```text
/// PerShot(maker).open(seed, J) ──► PerShotSession ──► maker.make(ctx(seed, j)), one per j in J
/// ```
///
/// It is the simplest strategy, and the one for programs with feedback: a
/// shot's measurements steer its own executor only. It is a named type,
/// not a blanket implementation over every [`MakeShot`], because a blanket
/// implementation would make per-shot the only strategy a maker can have.
/// Rust would then refuse, as overlapping (E0119), a terminal that is both
/// a maker and its own ensemble strategy; an adapter that wraps both a
/// maker and a strategy; and a strategy shared through `Arc`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PerShot<F>(pub F);

/// What the maker offers, since every shot is one of its executors.
impl<F: Describe> Describe for PerShot<F> {
    fn capabilities(&self) -> Capabilities {
        self.0.capabilities()
    }
}

/// Opens a [`PerShotSession`] with one shot executor per shot of J,
/// made from shot `j`'s context.
///
/// Each shot draws only from its own seeds, so the session is replayable
/// when the maker's shots are. The needs are checked against the maker's
/// capabilities, and a rejection names the maker, the component that lacks
/// what is needed. The session offers `probabilities` when the maker does,
/// and then reports each shot's [`MakeShot::log_p`] at close.
impl<I, F: MakeShot<I>> ShotStrategy<I> for PerShot<F> {
    type Session = PerShotSession<F::Shot>;

    fn open(&self, seed: u64, shots: &ShotSet, needs: &Needs) -> Result<Self::Session, Rejected> {
        let PerShot(maker) = self;
        let capabilities = maker.capabilities();
        capabilities.admit(std::any::type_name::<F>(), shots, needs)?;
        Ok(PerShotSession {
            capabilities,
            ids: shots.to_vec(),
            shots: shots
                .iter()
                .map(|&j| maker.make(&ShotContext::new(seed, j)))
                .collect(),
            log_p: F::log_p,
            index: ShotIndex::new(shots),
            submissions: 0,
            lifecycle: Lifecycle::default(),
        })
    }
}

/// The session of the per-shot strategy: opened by [`PerShot`] over a
/// [`MakeShot`], it holds one executor of batches per shot, in which shot
/// `j`'s state lives. It is an unwrapper from level L2 to level L3 (one
/// submission into a batch per shot), the per-shot one: other strategies'
/// sessions unwrap submissions too, onto other states.
///
/// A submission calls the executor of each of its shots in the order of its
/// shots. A rejection by the first shot called rejects the submission, since
/// nothing ran; a rejection by a later shot fails the submission, and the
/// session, since the shots before it ran (see [`Lifecycle`]).
///
/// Each shot's executor, and the state it holds, stays reachable between
/// calls ([`PerShotSession::shot`]), so that a query of a shot's state
/// in the middle of a session can be added without changing the session.
pub struct PerShotSession<S> {
    capabilities: Capabilities,
    ids: Vec<ShotId>,
    shots: Vec<S>,
    log_p: fn(&S) -> Option<f64>,
    index: ShotIndex,
    submissions: u64,
    lifecycle: Lifecycle,
}

impl<S> PerShotSession<S> {
    /// The executor of shot `shot`, with the state the submissions so far
    /// left; `None` when the session does not hold the shot, or was closed.
    #[must_use]
    pub fn shot(&self, shot: ShotId) -> Option<&S> {
        self.index.get(shot).and_then(|i| self.shots.get(i))
    }

    /// The executor of shot `shot`, mutably; `None` as for
    /// [`PerShotSession::shot`].
    #[must_use]
    pub fn shot_mut(&mut self, shot: ShotId) -> Option<&mut S> {
        self.index.get(shot).and_then(|i| self.shots.get_mut(i))
    }
}

impl<I, S: Executor<Batch<I>>> Executor<Submission<I>> for PerShotSession<S> {
    type Response = SessionBound<Gather<S::Response>>;

    /// Runs the batch on each shot of the submission, in its order.
    ///
    /// # Errors
    ///
    /// [`Rejected::Closed`] and [`Rejected::SessionFailed`] first
    /// ([`Lifecycle::admit`]); then [`Rejected::ShotNotInSession`] for the
    /// first shot the session does not hold; then the rejection of the first
    /// shot called. Nothing ran in each case.
    fn call(&mut self, submission: SubmissionRef<'_, I>) -> Result<Self::Response, Rejected> {
        self.lifecycle.admit()?;
        let at = submission
            .shots
            .iter()
            .map(|&j| self.index.get(j).ok_or(Rejected::ShotNotInSession(j)))
            .collect::<Result<Vec<_>, _>>()?;
        let mut parts = Vec::with_capacity(at.len());
        let mut failure = None;
        for (&j, i) in submission.shots.iter().zip(at) {
            let Some(shot) = self.shots.get_mut(i) else {
                // The index and the shots are made together from J, and
                // shrink together only at close, which `admit` checked.
                return Err(Rejected::ShotNotInSession(j));
            };
            match shot.call(submission.batch) {
                Ok(part) => parts.push((j, Some(part))),
                // Nothing started yet: the submission is refused as a whole.
                Err(rejected) if parts.is_empty() => return Err(rejected),
                // Other shots already run it: too late to refuse, so the
                // submission fails, and the rest of its shots do not run.
                Err(rejected) => {
                    let reason = format!("shot {j}: {rejected}");
                    self.lifecycle.fail(&reason);
                    failure = Some(reason);
                    break;
                }
            }
        }
        self.submissions += 1;
        // The shots' replies show a failure only when polled.
        Ok(self.lifecycle.bind(Gather::new(parts, failure)))
    }
}

impl<S> Describe for PerShotSession<S> {
    /// The maker's capabilities.
    fn capabilities(&self) -> Capabilities {
        self.capabilities.clone()
    }
}

impl<S: Close> Close for PerShotSession<S> {
    type Report = SessionReport<S::Report>;

    /// Closes each shot's executor and folds their reports; adds each shot's
    /// log probability, when the maker offers `probabilities` and knows the
    /// shot's. The shots' states are dropped.
    fn close(&mut self) -> Self::Report {
        self.lifecycle.close();
        let mut report: Self::Report = SessionReport {
            submissions: std::mem::take(&mut self.submissions),
            ..SessionReport::default()
        };
        for (&j, mut shot) in self.ids.iter().zip(self.shots.drain(..)) {
            if self.capabilities.probabilities
                && let Some(log_p) = (self.log_p)(&shot)
            {
                report.log_p.add(j, log_p);
            }
            report.shots.fold(shot.close());
        }
        self.ids.clear();
        report
    }
}

/// The replies of one batch on several shots, as one reply: each L3 outcome
/// becomes an L2 outcome of its shot. Each poll polls every part still
/// pending, with the caller's waker, so the first part with news wakes the
/// caller.
pub struct Gather<C> {
    parts: Vec<(ShotId, Option<C>)>,
    failure: Option<String>,
    buffer: Vec<Outcome>,
}

impl<C> Gather<C> {
    /// The replies `parts` of their shots; `failure` when the call could
    /// start only some of its shots, reported once every part has ended.
    #[must_use]
    pub fn new(parts: Vec<(ShotId, Option<C>)>, failure: Option<String>) -> Self {
        Self {
            parts,
            failure,
            buffer: Vec::new(),
        }
    }
}

impl<C: Completion<Item = Outcome>> Completion for Gather<C> {
    type Item = ShotOutcome;

    fn poll_events(&mut self, out: &mut Vec<ShotOutcome>, waker: &Waker) -> Status {
        let mut pending = false;
        for (shot, slot) in &mut self.parts {
            let Some(part) = slot else { continue };
            let status = part.poll_events(&mut self.buffer, waker);
            out.extend(self.buffer.drain(..).map(|outcome| ShotOutcome {
                shot: *shot,
                tag: outcome.tag,
                value: outcome.value,
            }));
            match status {
                Status::Pending => pending = true,
                Status::Done => *slot = None,
                Status::Failed(reason) => {
                    *slot = None;
                    self.failure.get_or_insert(reason);
                }
            }
        }
        if pending {
            return Status::Pending;
        }
        match self.failure.take() {
            Some(reason) => Status::Failed(reason),
            None => Status::Done,
        }
    }
}

/// The wrapper from level L3 to level L2: puts each batch in a submission
/// on shot 1 of a session, so that an L3 driver can use an L2 session. The
/// session must hold shot 1 (opened on [`ShotSet::one`]`(1)`); a call to one
/// that does not is refused with [`Rejected::ShotNotInSession`].
///
/// Over a per-shot session it gives the outcomes of the shot's executor
/// alone: this wrapper over a [`PerShotSession`] is the identity.
pub struct SubmissionWrapper<S> {
    inner: S,
    shot_1: ShotSet,
}

impl<S> SubmissionWrapper<S> {
    /// Wraps a session opened on shot 1.
    pub fn new(session_on_shot_1: S) -> Self {
        Self {
            inner: session_on_shot_1,
            shot_1: ShotSet::one(1),
        }
    }

    /// The session.
    #[must_use]
    pub fn inner(&self) -> &S {
        &self.inner
    }

    /// The session, unwrapped.
    #[must_use]
    pub fn into_inner(self) -> S {
        self.inner
    }
}

impl<I, S: Executor<Submission<I>>> Executor<Batch<I>> for SubmissionWrapper<S> {
    type Response = Map<S::Response, Outcome>;

    /// Runs `batch` on shot 1 of the session.
    ///
    /// # Errors
    ///
    /// The session's rejection of the submission.
    fn call(&mut self, batch: &[I]) -> Result<Self::Response, Rejected> {
        let inner = self.inner.call(SubmissionRef {
            shots: &self.shot_1,
            batch,
        })?;
        Ok(Map::new(inner, |event| Outcome {
            tag: event.tag,
            value: event.value,
        }))
    }
}

impl<S: Describe> Describe for SubmissionWrapper<S> {
    fn capabilities(&self) -> Capabilities {
        self.inner.capabilities()
    }
}

impl<S: Close> Close for SubmissionWrapper<S> {
    type Report = S::Report;

    fn close(&mut self) -> S::Report {
        self.inner.close()
    }
}

/// What a session ran, returned when it closes.
///
/// Reports fold. When sessions that split J run the same submissions, their
/// folded `log_p`, and shot reports that count per shot, are those of one
/// session over all of J; `submissions` counts each session's calls, so it
/// adds up across the sessions.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SessionReport<R> {
    /// The submissions it ran (accepted; refused ones are not counted).
    pub submissions: u64,
    /// Per shot, the log probability of every draw made for it, where it is
    /// known ([`MakeShot::log_p`]).
    pub log_p: LogP,
    /// The reports of the shots' executors, folded.
    pub shots: R,
}

impl<R: Fold> Fold for SessionReport<R> {
    fn fold(&mut self, other: Self) {
        self.submissions += other.submissions;
        self.log_p.fold(other.log_p);
        self.shots.fold(other.shots);
    }
}

/// Per shot, a sum of ln p. Each component that draws adds its own as the
/// session closes, so folding two is adding shot by shot.
///
/// Absent, not 0, for a shot where no component knows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LogP(BTreeMap<ShotId, f64>);

impl LogP {
    /// Adds `ln_p` to shot `shot`'s sum.
    pub fn add(&mut self, shot: ShotId, ln_p: f64) {
        *self.0.entry(shot).or_insert(0.0) += ln_p;
    }

    /// Shot `shot`'s sum, if any component reported one.
    #[must_use]
    pub fn get(&self, shot: ShotId) -> Option<f64> {
        self.0.get(&shot).copied()
    }

    /// Whether no component reported any.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Fold for LogP {
    fn fold(&mut self, other: Self) {
        for (shot, ln_p) in other.0 {
            self.add(shot, ln_p);
        }
    }
}
