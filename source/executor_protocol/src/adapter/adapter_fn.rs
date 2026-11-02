// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A state and a logic as an adapter ([`adapter_fn`]), as
//! [`executor_fn`](fn@crate::executor_fn) makes an executor of them: the
//! logic serves each call, on the state, and reaches the executor below
//! through a [`Below`]. Composed with an executor, it gives an executor
//! ([`AdapterFnExecutor`]) whose replies are [`AdapterFnReply`]s. Under
//! [`OnePerShot`] it keeps a state per shot ([`one_per_shot`]). Its states
//! may own their way below instead ([`owning`]).

mod one_per_shot;
mod owning;

use std::marker::PhantomData;
use std::mem;
use std::sync::Arc;
use std::task::Waker;

pub use one_per_shot::{OnePerShotAdapter, OnePerShotAdapterExecutor};
pub(crate) use owning::{MakeAdapterState, ServeAdapterCalls};
pub use owning::{Owning, OwningAdapterLogic, owning_adapter_fn};

use super::below::Route;
use crate::absorb::{DroppedOneByOne, HeldPart, HeldStates, Panic, caught, raise};
use crate::conditions::{ConditionsSoFar, Marks, Raised};
use crate::lifecycle::failing_on_panic;
use crate::reply::CANCELLATION;
use crate::{
    Adapter, AdapterBoundary, AdapterError, Below, Capabilities, Close, ComponentId, Describe,
    Executor, ExecutorLifecycle, Fold, ImmediateReply, Instruction, InstructionBatch,
    InstructionBatchRef, InstructionSet, LifecycleBoundReply, MakeState, NoCheck, OnePerShot,
    Outcomes, ProgramInput, Rejected, Reply, ReplyStatus, ShotContext, ShotId, ShotOutcome,
    ShotSet, wait,
};
use owning::{CallDown, KeptState, WayDown};

/// A state and a logic as an adapter: made by [`adapter_fn`], or by
/// [`owning_adapter_fn`] when its states own their way below.
///
/// It accepts the instructions `I` and sends `O`. It offers upstream what
/// the executor below offers, `resumable_shots`, `replayable` and
/// `log_probability` included: every readout it passes up comes from below,
/// or from its logic. Should its states draw ([`AdapterFn::drawing`]), it
/// still passes `resumable_shots` up, but offers no `log_probability`,
/// nor, with one state, `replayable`. With one state, it passes
/// `replayable` and `resumable_shots` up, where it does, on the assumption
/// that its logic keeps the shots apart: the state carries nothing from one
/// shot to another, and the logic writes each shot's outcomes from that
/// shot's readouts alone ([`Below::run`]). Where it changes the
/// instructions, it states the sets ([`AdapterFn::accepts`],
/// [`AdapterFn::sends`]). Given [`OnePerShot`] ([`AdapterFn::shots`]), it
/// keeps a state per shot.
///
/// Known issue (blocks neither): with one state, the adapter cannot check
/// that assumption, so a logic that copies one shot's outcome into another
/// offers `replayable` it does not keep, with no error. Such a logic is
/// given `.shots(OnePerShot)`, where each shot's logic is given its own
/// state and its own readouts alone, until it can clear `replayable`
/// (planned, below).
///
/// Planned: options that build it against what is below, on the Promise
/// axis (the crate's `docs/glossary.md`, §A part's axes) and named alike
/// on every component (`.offers(..)`, `.needs(..)`), for a code that offers
/// a qubit per block of three, a decoder that needs resumable shots below,
/// or a one-state logic that copies one shot's outcome into another, which
/// clears `replayable`; passing up what is below stays the default. Until
/// then, its instruction sets and seeds apart, it offers what is below and
/// needs nothing of it.
///
/// # Type parameters
///
/// - `I`: the instructions it accepts, which the part above sends it.
/// - `O`: the instructions it sends below ([`Below`]).
/// - `S`: its state, on which the logic serves each call.
/// - `M`: what makes the state ([`MakeState`]), given to [`adapter_fn`];
///   for [`owning_adapter_fn`], its state closure, held in an [`Owning`].
/// - `L`: the logic ([`AdapterLogic`]), given to [`adapter_fn`]; for
///   [`owning_adapter_fn`], its logic, held in an [`Owning`].
/// - `C`: the check run on each call before the state is made
///   ([`AdapterFn::check`]); by default [`NoCheck`], which accepts every
///   call.
/// - `R`: what makes the state's report ([`AdapterFn::state_report`]); by
///   default [`NoStateReport`], which adds nothing to the report below.
pub struct AdapterFn<I, O, S, M, L, C = NoCheck<I>, R = NoStateReport> {
    // Each closure is held apart, so that the drop of what it captures is
    // absorbed in a catch of its own ([`HeldPart`]).
    make_state: HeldPart<M>,
    logic: HeldPart<L>,
    check: HeldPart<C>,
    /// Makes a state's report; without one, [`NoStateReport`].
    state_report: HeldPart<R>,
    accepts: Option<Arc<InstructionSet>>,
    sends: Option<Arc<InstructionSet>>,
    drawing: Option<ComponentId>,
    instructions: PhantomData<fn(&I, &S) -> O>,
}

/// The adapter whose `logic` serves each call on one state, which
/// `make_state` makes at the first call, from that call's first shot, and
/// which the adapter keeps for every shot after ([`MakeState`]), and
/// reaches the executor below with a [`Below`].
/// Each [`Below::send`] is one call below, on the call's shots, whose reply
/// passes up as it comes; each [`Below::run`] is one too, whose readouts the
/// logic waits for and reads. What the logic writes to the [`Outcomes`]
/// answers every shot of the call, or the one shot it names. A call's reply
/// hands over what the logic wrote, then the replies of its sends, each as
/// it comes. So the logic writes no tag of a shot that one of its sends
/// reads: the reply would give that readout twice, where a shot reports
/// each tag once ([`ShotOutcome::tag`]). The adapter does not check it.
///
/// Each shot's conditions ([`ShotConditions`](crate::ShotConditions)) need
/// no code in the logic: the adapter keeps, for each shot, those raised in
/// its calls, by its logic ([`Outcomes::raise_conditions`]) and on the
/// outcomes its logic writes, its runs read and its sends read during the
/// call, while their reply is still pending too, and marks with them, as
/// each call ends, every outcome it gives a shot of the call, in that
/// call, failed or cancelled too, and in later calls on the shot. A
/// refused call raises none. The conditions a run read count even when it
/// then fails or is cancelled, but only on the shots it was given, as a
/// readout of another shot may belong to another call; those a send read
/// during the call count on whatever shot each names, as its reply passes
/// up as it comes, unchecked, and a readout of a shot the call was not
/// given, which only a faulty executor below gives, passes up with the
/// conditions it carries from below alone. The
/// adapter takes in a condition from below only from an outcome it reads
/// before its call ends: those a send's reply gives after its call
/// returned pass up on its outcomes, but the adapter never sees them. When
/// the executor below keeps them, as one that offers `resumable_shots`
/// must, the shot's outcomes that a later call sends for carry them again,
/// and so do those the logic writes in a later call that reads an outcome
/// of the shot from below before it ends, before or after the read, as a
/// decoder's run does, and in every call after it; those it writes in the
/// calls before lack them, a known issue (`docs/replies.md`, §Shot
/// conditions, where the tests are cited).
///
/// What the logic calls says what the adapter is: one that sends what it is
/// called with observes (a profiler); one that sends other instructions,
/// tags kept, rewrites (a transpiler, a code's lowering); one that runs
/// instructions and writes what it makes of their readouts decodes (a
/// code's lowering and decoder), or records them (a debugger).
///
/// The adapter keeps the rules of every executor of the crate, as
/// [`executor_fn`](fn@crate::executor_fn) does (the crate's
/// `docs/internals/adapters.md` lists the tests that pin them):
///
/// - a call is refused when closed ([`Rejected::Closed`]) and after a call
///   failed ([`Rejected::EarlierFailure`]), before anything runs; then
///   with the check's refusal ([`AdapterFn::check`]), before the state is
///   made and before the logic runs; then when `make_state` cannot make the
///   state, with its refusal, and the state is made again at the next
///   call;
/// - a call whose first call below, a send or a run, is refused is refused
///   with the same [`Rejected`], whatever the logic returns, and keeps no
///   state made for it: nothing went down. Nor does a call that fails in
///   it, as below, or panics: the adapter refuses every later call, and
///   reports no state made for it ([`AdapterFn::state_report`]). An index
///   the refusal names, such as [`Rejected::Instruction`]'s `at`, is that
///   of the instruction in the call below, not in the call: the caller of
///   a rewriting adapter gets it unmapped (planned: the logic maps it
///   through the refusal an [`AdapterError`] would carry). A refusal for
///   an earlier failure below ([`Rejected::EarlierFailure`]) fails the
///   adapter too. A state made
///   before the call keeps what the logic changed in it: the call is
///   refused though its logic ran, so a logic should change its state
///   only after `send(..)?` or `run(..)?` ([`Below::send`]);
/// - the logic's `Err`, a call below refused after an earlier one went
///   down, a run whose reply fails, is cancelled or is not whole
///   ([`Below::run`]), whatever the logic returns, and a send's reply that
///   fails, fail the call and the adapter: nothing more goes down in the
///   call, the call keeps no state made for it, and every later call is
///   refused with [`Rejected::EarlierFailure`]. The reply hands over what
///   the logic wrote and what its sends read, then ends with the failure:
///   that of the call below that ended the call, when one did by the time
///   the call returns, even if the logic returns an `Err` of its own;
/// - a send's reply cancelled below ends the call cancelled, unless the
///   call fails, and fails the adapter, as a failure does: the call keeps
///   no state made for it, and every later call is refused;
/// - when a send's reply fails, or is cancelled, depends on the executor below.
///   One that answers within its call, as an
///   [`executor_fn`](fn@crate::executor_fn) does unless placed, has ended the
///   send's reply when the send returns: a failure is the send's `Err`, and
///   nothing more goes down. One that answers later, as one placed on another
///   thread ([`ExecutorFn::run_on`](crate::ExecutorFn::run_on)), ends it when
///   its work ends. The adapter sees an end that came during the call by the
///   logic's next run, or as the logic returns: the sends the logic made in
///   between still go down. An end that comes only after the call returned
///   leaves a call that did not fail in it its state, as the adapter learns of
///   that end only as the call's reply is read ([`AdapterFn::state_report`]),
///   and fails the adapter as the call's reply gives it: the reply ends at
///   once, when that send's reply is polled, with the call's own failure if it
///   has one, the one later calls are refused with. Known issue: should that
///   reply be dropped before it ends, the adapter fails only when the executor
///   below refuses a later call for that failure;
/// - should the check, `make_state` or the logic panic, the panic reaches the
///   caller, and the adapter fails (`the executor panicked during the call`).
///   So does the panic of the drop of a state made for a call that is refused
///   or fails, freed in a catch of its own; freed as another panic unwinds, it
///   leaves its own to the panic hook.
///
/// At close, the state is reported, if the adapter is given how
/// ([`AdapterFn::state_report`]), and dropped, then the executor below is
/// closed: the report is its report, beside the adapter's if it has one. The
/// executor below is closed once, whatever failed, a report closure's or the
/// state's drop's panic included, which reaches the closer once it is closed: a
/// later close gives an empty report and closes nothing below.
///
/// The adapter's executor, dropped rather than closed, never panics for the
/// adapter's parts: the state's drop that panics is absorbed, in a catch of its
/// own, after the panic hook reported it, during an unwind too; call `close` to
/// see it. So is the drop of what its closures capture, each closure in a catch
/// of its own; `close` raises only the state's. The executor below is dropped
/// by its own rule.
///
/// Each reply a call below gives, a send's or a run's, is held in a catch of
/// its own from the moment it is given: its drop's panic is absorbed, whether
/// it is dropped pending, with the call's reply or as the logic's panic
/// unwinds, where two would abort the process, or once it ended, its call
/// having run.
///
/// The state closure is given the context of the call's first shot
/// (`ShotContext::shot`), as a one-state
/// [`executor_fn`](fn@crate::executor_fn)'s is: with no seed, unless the
/// adapter states the id its state draws with ([`AdapterFn::drawing`]),
/// and then it offers no `replayable`.
///
/// A state that keeps its way below for its whole life, such as a code's
/// runtime made over a backend it owns, is made by
/// [`owning_adapter_fn`] instead, by the same rules.
///
/// # Type parameters
///
/// - `I` … `L`: as [`AdapterFn`]'s, its options at their defaults.
pub fn adapter_fn<I, O, S, M, L>(make_state: M, logic: L) -> AdapterFn<I, O, S, M, L>
where
    M: MakeState<S>,
    L: AdapterLogic<I, O, S>,
{
    AdapterFn::new(make_state, logic)
}

impl<I, O, S, M, L> AdapterFn<I, O, S, M, L> {
    /// The adapter of `make_state` and `logic`, with no option given.
    fn new(make_state: M, logic: L) -> Self {
        Self {
            make_state: HeldPart::new(make_state),
            logic: HeldPart::new(logic),
            check: HeldPart::new(|_| Ok(())),
            state_report: HeldPart::new(NoStateReport),
            accepts: None,
            sends: None,
            drawing: None,
            instructions: PhantomData,
        }
    }
}

/// What serves each call of an [`adapter_fn`], of instructions of type `I`, on
/// its state, of type `S`, reaching the executor below with a [`Below`] that
/// takes instructions of type `O`: its `logic` closure, whose error is an
/// [`AdapterError`]. Implemented for every closure of its signature, so that a
/// function returning the facility's type names the closure in a word, as
/// [`MakeState`] is: `-> OnePerShotAdapter<I, O, S, impl MakeState<S>, impl
/// AdapterLogic<I, O, S>>`, which `>>` composes where a bare `impl Adapter`
/// does not (see [`compose`](fn@crate::compose)); `+ use<>` included, as for
/// [`MakeState`].
pub trait AdapterLogic<I, O, S>:
    FnMut(
    &mut S,
    &ProgramInput<'_, I>,
    &mut Below<'_, O>,
    &mut Outcomes<'_>,
) -> Result<(), AdapterError>
{
}

impl<I, O, S, F> AdapterLogic<I, O, S> for F where
    F: FnMut(
        &mut S,
        &ProgramInput<'_, I>,
        &mut Below<'_, O>,
        &mut Outcomes<'_>,
    ) -> Result<(), AdapterError>
{
}

impl<I, O, S, M, L, C, R> AdapterFn<I, O, S, M, L, C, R> {
    /// This adapter, accepting the calls of `set` ([`Adapter::accepts`]): it
    /// offers `set` upstream, where what is sent to it is checked against it:
    /// the set an adapter above sends, when the chain is built; the
    /// instructions a job's program needs, when its job is given to the chain
    /// ([`Chain::run`](crate::Chain::run)). The adapter does not check the
    /// calls it is given against it: to refuse here a call outside `set`, give
    /// a [`check`](AdapterFn::check) that does, such as one that runs
    /// [`InstructionSet::check_batch`] on a call's instructions.
    //
    // Planned, with no milestone yet (`docs/planned.md`): the
    // adapter checking the calls against `set` itself, so that a check
    // holds only its other rules.
    #[must_use]
    pub fn accepts(mut self, set: impl Into<Arc<InstructionSet>>) -> Self {
        self.accepts = Some(set.into());
        self
    }

    /// This adapter, refusing the calls `check` refuses, before their state is
    /// made and before the logic runs, so that nothing runs and nothing goes
    /// below: what the logic cannot serve whatever the state, such as an
    /// instruction a lowering has no rule for, or a qubit beyond those it
    /// offers. It replaces the check given before, if any, freeing it, and
    /// raising no panic of its drop, as `drop` does; without one, every call is
    /// accepted. The same option as
    /// [`ExecutorFn::check`](crate::ExecutorFn::check), with the same meaning.
    #[must_use]
    pub fn check<V>(self, check: V) -> AdapterFn<I, O, S, M, L, V, R>
    where
        V: FnMut(&ProgramInput<'_, I>) -> Result<(), Rejected>,
    {
        AdapterFn {
            make_state: self.make_state,
            logic: self.logic,
            check: HeldPart::new(check),
            state_report: self.state_report,
            accepts: self.accepts,
            sends: self.sends,
            drawing: self.drawing,
            instructions: PhantomData,
        }
    }

    /// This adapter, making with `state_report` the report of each state it
    /// holds, when it closes, as the state's calls left it: what a profiler
    /// counted, or what a probe saw pass. Each state's report is folded
    /// once into the adapter's, starting from the empty report, in an
    /// order not promised: by [`Fold`]'s rules, the total does not depend
    /// on it. The same option as
    /// [`ExecutorFn::state_report`](crate::ExecutorFn::state_report), but
    /// for which states are kept after a failure (below). It replaces the
    /// one given before, if any, freeing it as [`AdapterFn::check`] does.
    ///
    /// The adapter's report goes beside the one below it, as a pair,
    /// `(adapter's, below's)`, so that a chain's report holds a pair per
    /// adapter given a `state_report`, nested in the chain's order,
    /// whatever the grouping of `>>`. An adapter given none adds no pair:
    /// its report is the one below, unchanged ([`NoStateReport`]).
    ///
    /// Every state held at close is reported, a failed one included, but not a
    /// state made for a call that was refused or failed in it (the logic's
    /// `Err`, a call below that did not go on, a send's reply that failed or
    /// was cancelled before the call returned), nor one made for a call that
    /// panicked. Unlike an executor, which reports no state made for a call
    /// that failed or for a later one, the adapter learns that a send's reply
    /// failed after the call returned only as the reply is read, so it keeps,
    /// and reports, the states made for that call, and, with a state per shot,
    /// those made for a later call it admitted before the failure was recorded
    /// (as that reply was read). A shot that never ran has no state, so no
    /// report, and a second `close` reports none.
    ///
    /// # Example
    ///
    /// A profiler that counts its shots' instructions, a state per shot, over
    /// an executor; its counts are folded over the shots, beside the executor's
    /// report. Excerpt of the crate's tests of an adapter with a state per
    /// shot:
    ///
    /// ```ignore
    /// let counting = adapter_fn(no_counts, count_and_send)
    ///     .state_report(|tally: &Tally| tally.clone())
    ///     .shots(OnePerShot);
    /// ```
    #[must_use]
    pub fn state_report<K, Q>(self, state_report: K) -> AdapterFn<I, O, S, M, L, C, K>
    where
        K: FnMut(&S) -> Q,
        Q: Fold,
    {
        AdapterFn {
            make_state: self.make_state,
            logic: self.logic,
            check: self.check,
            state_report: HeldPart::new(state_report),
            accepts: self.accepts,
            sends: self.sends,
            drawing: self.drawing,
            instructions: PhantomData,
        }
    }

    /// This adapter, sending the calls of `set` ([`Adapter::sends`]): `set`
    /// is checked to fit the set below when the chain is built, and the
    /// calls sent are renumbered into it where it numbers them otherwise.
    #[must_use]
    pub fn sends(mut self, set: impl Into<Arc<InstructionSet>>) -> Self {
        self.sends = Some(set.into());
        self
    }

    /// This adapter, whose states draw random numbers with `id`
    /// ([`Adapter::draws`]). Under [`OnePerShot`] ([`AdapterFn::shots`]),
    /// shot `j`'s state is made from the context of `j` in the run, bound
    /// to `id`, so that its [`seed`](ShotContext::seed) is its own and is
    /// the same whatever the other shots of the run. With one state, the
    /// state is made from the context of the first shot of the call that
    /// makes it, so that a shot's draws depend on the shots it is called
    /// with: the adapter offers no `replayable` upstream. With one state,
    /// the logic runs once per call and each send goes below on every shot
    /// of the call, so the shots of a call share what it draws: a noise
    /// model that would flip one shot's bit flips it in every shot of the
    /// call. A noise model that draws per shot takes
    /// `.shots(`[`OnePerShot`]`)`.
    ///
    /// The run's seed is the one the last executor below is given
    /// ([`ExecutorFn::seed`](crate::ExecutorFn::seed)), through any
    /// adapters between: one seed replays the whole chain, each adapter
    /// that draws under an id of its own. The chain is refused when a
    /// component below draws with `id`, however far down
    /// ([`BuildError::RngInUse`]), or when no run's seed reaches the adapter
    /// ([`BuildError::NoRunSeed`]), and the adapter offers no
    /// `log_probability` upstream, since the log probability below does not
    /// hold its draws. Its run's report is still the one below, whose log
    /// probabilities miss its draws: they are not the shots'.
    ///
    /// [`BuildError::RngInUse`]: crate::BuildError::RngInUse
    /// [`BuildError::NoRunSeed`]: crate::BuildError::NoRunSeed
    //
    // Known issue, open (blocks a later unit: `.log_probability` on
    // adapters): the report below passes up as it is, its log
    // probabilities included, though the adapter offers no
    // `log_probability` (`a_drawing_adapters_run_reports_no_log_probability`,
    // ignored). Its type is any `Fold`, so the adapter cannot clear them;
    // its own report (`state_report`) goes beside it, and changes nothing
    // there.
    #[must_use]
    pub fn drawing(mut self, id: ComponentId) -> Self {
        self.drawing = Some(id);
        self
    }

    /// This adapter with a state per shot ([`OnePerShotAdapter`]): shot
    /// `j`'s state is made at the shot's first call, and the logic serves
    /// each shot of a call on its shot's state. A shot's `k`-th send goes
    /// below merged with the other shots' `k`-th sends equal to it, as one
    /// call over their shots; the sends a shot keeps before a run go alone,
    /// at that run, and its later sends rank anew, from its first.
    ///
    /// The merged sends go below rank by rank, and those of one rank in
    /// the order of their first shots, so whether a refusal below refuses
    /// the call or fails it depends on the order of the call's shots: when
    /// shot 5 sends what the executor below refuses and shot 2 something
    /// it accepts, a call of shots `[5, 2]` is refused, and the adapter
    /// lives on, while one of `[2, 5]` fails, with the adapter, since shot
    /// 2's send went down first.
    ///
    /// Its options ([`AdapterFn::accepts`], [`AdapterFn::sends`],
    /// [`AdapterFn::check`], [`AdapterFn::drawing`],
    /// [`AdapterFn::state_report`]) are given before, as on
    /// [`executor_fn`](fn@crate::executor_fn), in any order: each keeps the
    /// others given before it.
    #[must_use]
    pub fn shots(self, strategy: OnePerShot) -> OnePerShotAdapter<I, O, S, M, L, C, R> {
        let OnePerShot = strategy;
        OnePerShotAdapter::new(self)
    }
}

/// The report of an adapter given no [`AdapterFn::state_report`]: none, so
/// that the adapter adds nothing to the report below, which passes up
/// unchanged.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoStateReport;

impl<I: Instruction, O: Instruction, S, M, L, C, R> Adapter for AdapterFn<I, O, S, M, L, C, R> {
    type Accepts = I;
    type Sends = O;

    fn sends(&self) -> Option<Arc<InstructionSet>> {
        self.sends.clone()
    }

    fn accepts(&self) -> Option<Arc<InstructionSet>> {
        self.accepts.clone()
    }

    fn draws(&self) -> Option<ComponentId> {
        self.drawing
    }

    /// Offers no `replayable` when its state draws: its one state is made
    /// from the context of the call's first shot, so a shot's draws depend
    /// on the shots it is called with ([`AdapterFn::drawing`]).
    fn build(self, boundary: &mut AdapterBoundary<'_>) -> Self {
        if self.drawing.is_some() {
            boundary.offered_upstream().replayable = false;
        }
        self
    }
}

/// An [`adapter_fn`] upstream of the executor `D`: the executor `>>` gives.
/// It offers what the adapter offers upstream, and reports what `D`
/// reports, beside its state's report if it is given how to make one
/// ([`AdapterFn::state_report`]).
///
/// # Type parameters
///
/// - `I` … `R`: as [`AdapterFn`]'s.
/// - `D`: the executor below, as the adapter's calls reach it
///   ([`Instruction::Downstream`] of `O`): the executor the adapter is
///   composed over, its calls renumbered on their way down where it
///   numbers `O` otherwise.
//
// Known issue, open (blocks neither this commit nor a later one): the
// executor below, `D`, is dropped bare, not in a catch, as at
// `AdapterExecutor`. No executor's `drop` panics, by the crate's rule,
// which its own executors keep, so only an implementer's executor that
// breaks it makes this `drop` panic; absorbing it would hide that bug.
// The replies from below are each held in a catch all the same
// (`HeldPart`), because a call holds several: two of them panicking in one
// drop would abort the process, where one `D` cannot.
pub struct AdapterFnExecutor<I, O, S, M, L, C, R, D> {
    /// The state, once a call kept it: at most one. Declared first, so that
    /// it is dropped before the closures (the reason is at `HeldPart`), and
    /// before the executor below, as `close` frees it before closing below
    /// (`dropped_it_frees_its_state_before_its_closures_and_the_executor_below`).
    held: HeldStates<KeptState<S>>,
    adapter: AdapterFn<I, O, S, M, L, C, R>,
    capabilities: Capabilities,
    way_down: WayDown<O, D>,
    /// Whether a close reached the executor below ([`close_below_once`]).
    below_closed: bool,
    /// The conditions of each shot so far, which mark its outcomes.
    so_far: ConditionsSoFar,
    lifecycle: ExecutorLifecycle,
}

impl<I, O, S, M, L, C, R, D> AdapterFnExecutor<I, O, S, M, L, C, R, D> {
    /// `adapter`, built to offer `capabilities`, upstream of `below`.
    pub(crate) fn new(
        adapter: AdapterFn<I, O, S, M, L, C, R>,
        capabilities: Capabilities,
        below: D,
    ) -> Self {
        Self {
            held: HeldStates::default(),
            adapter,
            capabilities,
            way_down: WayDown::new(below),
            below_closed: false,
            so_far: ConditionsSoFar::default(),
            lifecycle: ExecutorLifecycle::default(),
        }
    }
}

impl<I, O, S, M, L, C, R, D> Executor<InstructionBatch<I>>
    for AdapterFnExecutor<I, O, S, M, L, C, R, D>
where
    M: MakeAdapterState<S, O>,
    L: ServeAdapterCalls<S, I, O, D>,
    C: FnMut(&ProgramInput<'_, I>) -> Result<(), Rejected>,
    D: Executor<InstructionBatch<O>>,
{
    type Reply = LifecycleBoundReply<AdapterFnReply<D::Reply>>;

    /// Serves the call with the logic, by the rules of [`adapter_fn`].
    ///
    /// # Errors
    ///
    /// [`Rejected::Closed`] once closed, then [`Rejected::EarlierFailure`]
    /// after a call failed; then the check's refusal
    /// ([`AdapterFn::check`]); then the refusal of the closure that makes
    /// the state; then the refusal of the first call below, a send or a
    /// run.
    ///
    /// # Panics
    ///
    /// When the check, the state closure or the logic panics, or the drop
    /// of the state made for a call that is refused or fails: the panic
    /// reaches the caller, and every later call is refused with
    /// [`Rejected::EarlierFailure`].
    fn call(&mut self, batch: InstructionBatchRef<'_, I>) -> Result<Self::Reply, Rejected> {
        failing_on_panic(self, |this| &this.lifecycle, |this| this.serve(batch))
    }
}

impl<I, O, S, M, L, C, R, D> AdapterFnExecutor<I, O, S, M, L, C, R, D>
where
    M: MakeAdapterState<S, O>,
    L: ServeAdapterCalls<S, I, O, D>,
    C: FnMut(&ProgramInput<'_, I>) -> Result<(), Rejected>,
    D: Executor<InstructionBatch<O>>,
{
    /// Serves `batch`: the steps of `call`, which runs them failing the
    /// adapter should they panic ([`failing_on_panic`]).
    fn serve(
        &mut self,
        batch: InstructionBatchRef<'_, I>,
    ) -> Result<LifecycleBoundReply<AdapterFnReply<D::Reply>>, Rejected> {
        self.lifecycle.admit()?;
        let input = ProgramInput {
            instructions: batch.instructions,
        };
        (self.adapter.check)(&input)?;
        // Kept only once its call succeeds (`CallEnd::keeps_states`): a call
        // refused, failed or cancelled keeps, and reports, no state made
        // for it, which is freed in a catch of its own.
        let mut made = DroppedOneByOne::default();
        let kept = if let Some(kept) = self.held.first_mut() {
            kept
        } else {
            // A shot set is never empty.
            let first = batch.shots.as_slice()[0];
            let context = (self.adapter).context(&self.capabilities, first);
            made.push(self.way_down.make(&mut *self.adapter.make_state, context)?);
            made.last_mut().expect("pushed above")
        };
        let mut written = ImmediateReply::default();
        let mut calls = CallsBelow::new();
        let returned = {
            let down = CallDown {
                way: &mut self.way_down,
                calls: &mut calls,
            };
            let mut outcomes = Outcomes::new(batch.shots.as_slice(), &mut written);
            let returned =
                (self.adapter.logic).serve_call(kept, &input, batch.shots, down, &mut outcomes);
            calls.take_in_raised(batch.shots.as_slice(), &outcomes);
            returned
        };
        let ended = calls.end_call(
            &mut self.lifecycle,
            &mut self.so_far,
            batch.shots.as_slice(),
            written,
            returned,
        );
        if ended.keeps_states {
            self.held.append(&mut made);
        }
        ended.reply
    }
}

impl<I, O, S, M, L, C, R> AdapterFn<I, O, S, M, L, C, R> {
    /// Shot `shot`'s context, as this adapter's states see it, built to
    /// offer `capabilities`: the run's seed below, bound to the id the
    /// adapter draws with, if it draws ([`ShotContext::of_adapter`]).
    fn context(&self, capabilities: &Capabilities, shot: ShotId) -> ShotContext {
        ShotContext::of_adapter(self.drawing, capabilities.run_seed, shot)
    }
}

impl<I, O, S, M, L, C, R, D> Describe for AdapterFnExecutor<I, O, S, M, L, C, R, D> {
    fn capabilities(&self) -> Capabilities {
        self.capabilities.clone()
    }
}

// Known issue, open (blocks a later unit: `.log_probability` on adapters):
// in both impls below, the report below passes up with its log
// probabilities, which miss the adapter's draws when it draws
// (`AdapterFn::drawing`).
impl<I, O, S, M, L, C, D: Close> Close for AdapterFnExecutor<I, O, S, M, L, C, NoStateReport, D> {
    type Report = D::Report;

    /// Refuses later calls, drops the state, then closes the executor
    /// below, unless an earlier close did; gives its report, empty if so.
    ///
    /// # Panics
    ///
    /// When the state's drop panics: the panic reaches the caller once the
    /// executor below is closed, and when the close below panics too, its panic
    /// is left to the panic hook. When only the close below panics, its panic
    /// reaches the caller.
    fn close(&mut self) -> D::Report {
        self.lifecycle.close();
        let states = self.held.take_raising_first();
        free_then_close_below(None, states, self.way_down.below(), &mut self.below_closed)
    }
}

impl<I, O, S, M, L, C, R, Q, D> Close for AdapterFnExecutor<I, O, S, M, L, C, R, D>
where
    R: FnMut(&S) -> Q,
    Q: Fold,
    D: Close,
{
    type Report = (Q, D::Report);

    /// Refuses later calls, reports the state, if one was made
    /// ([`AdapterFn::state_report`]), and drops it, then closes the
    /// executor below, unless an earlier close did; gives the state's
    /// report beside its report, empty if so.
    ///
    /// # Panics
    ///
    /// When `state_report` (or `Q::default`), or the state's drop, panics: the
    /// panic reaches the caller once the state is dropped and the executor
    /// below is closed, `state_report`'s when both panic. When the close below
    /// panics too, that first panic still reaches the caller, and the close
    /// below's is left to the panic hook. When only the close below panics, its
    /// panic reaches the caller.
    fn close(&mut self) -> (Q, D::Report) {
        self.lifecycle.close();
        let states = self.held.take_raising_first();
        let (report, reported) = report_states(
            states.iter().map(|kept| &kept.state),
            &mut *self.adapter.state_report,
        );
        let below = free_then_close_below(
            reported,
            states,
            self.way_down.below(),
            &mut self.below_closed,
        );
        (report.unwrap_or_default(), below)
    }
}

/// The reports of `states`, by `state_report`, folded into `Q::default()`,
/// in a catch, so that an adapter's close goes on to free its states and
/// close below whatever this code of the implementer's does: the report
/// (none when `Q::default` panicked), and the panic caught. The close
/// raises that panic ([`free_then_close_below`]), so it returns a report
/// only when none was caught, or while the thread unwinds another panic,
/// which goes on; there, a report `Q::default` failed to make is made
/// again, and a second panic aborts the process.
//
// Known issue, open (it needs two bugs at once; blocks neither this commit
// nor a later one): a `close` run while the thread unwinds another panic,
// whose `Q::default` panics, aborts the process, as said above: `close`
// makes the default again outside the catch (`report.unwrap_or_default()`).
// Returning `Option<Q>` up to `close` would not help, since `close` must
// return a `Q`.
pub(super) fn report_states<'a, S: 'a, Q: Fold>(
    states: impl IntoIterator<Item = &'a S>,
    state_report: &mut impl FnMut(&S) -> Q,
) -> (Option<Q>, Option<Panic>) {
    let mut report = None;
    let panic = caught(|| {
        let report = report.insert(Q::default());
        for state in states {
            report.fold(state_report(state));
        }
    });
    (report, panic)
}

/// An adapter's close, once its states are reported: frees `states`, each
/// in a catch of its own, then closes `below` ([`close_below_once`]) in a
/// catch too, then raises the first panic: `earlier` (a report closure's),
/// else the first state drop's, else the close below's; the others are
/// left to the panic hook, which reported them, as
/// [`end_each_raising_first`](crate::absorb::end_each_raising_first) does.
/// So the executor below is closed whatever the states' code did, and
/// after them, since a state may own its way below ([`owning_adapter_fn`]).
//
// Known issue, open (it needs two bugs at once; blocks neither this commit
// nor a later one), as at `report_states`: a close run while the thread
// unwinds another panic, whose close below panics and whose
// `D::Report::default` panics too, aborts the process, since the report
// the close below failed to give is made outside the catch
// (`report.unwrap_or_default()`), as the close must return a report.
pub(super) fn free_then_close_below<H, D: Close>(
    earlier: Option<Panic>,
    states: DroppedOneByOne<H>,
    below: &mut D,
    closed: &mut bool,
) -> D::Report {
    let dropped = states.free_keeping_first();
    let mut report = None;
    let closing = caught(|| report = Some(close_below_once(below, closed)));
    raise(earlier.or(dropped).or(closing));
    report.unwrap_or_default()
}

/// Closes `below`, the executor below an adapter (an `adapter_fn`'s, or
/// the executor an adapter written by hand gives), at the first close of
/// the adapter that reaches it, and gives its report; a later close gives
/// an empty report, as a second close below would, and closes nothing, so
/// that what is below is closed once, whatever failed: a placed executor
/// below is not handed a second close's job. `closed` records the close
/// before it runs, so a close below that panics is not run again.
pub(crate) fn close_below_once<D: Close>(below: &mut D, closed: &mut bool) -> D::Report {
    if mem::replace(closed, true) {
        D::Report::default()
    } else {
        below.close()
    }
}

/// A one-state logic's way below: each send or run is one call below, on
/// the call's shots, by the rules of the call's calls below.
struct CallRoute<'a, D, B> {
    calls: &'a mut CallsBelow<B>,
    below: &'a mut D,
    shots: &'a ShotSet,
}

impl<O, D, B> Route<O> for CallRoute<'_, D, B>
where
    D: Executor<InstructionBatch<O>, Reply = B>,
    B: Reply<Item = ShotOutcome>,
{
    fn send(&mut self, instructions: &[O]) -> Result<(), String> {
        self.calls.send(self.below, self.shots, instructions)
    }

    fn run(&mut self, instructions: &[O]) -> Result<Vec<ShotOutcome>, String> {
        self.calls.run(self.below, self.shots, instructions)
    }
}

/// The calls below of one call, sends and runs: the replies of the sends
/// still pending, in order, and what those polled read, whether a call went
/// down, and how the call's calls below ended, if one did not go on; and
/// the conditions raised in the call, by its logic, on what it wrote, and
/// below.
pub(super) struct CallsBelow<B> {
    /// Each reply held as it is given ([`HeldPart`]): dropped in a catch of
    /// its own, its panic absorbed, as its call ran.
    parts: Vec<HeldPart<B>>,
    /// What the sends' replies read when polled during the call.
    read: Vec<ShotOutcome>,
    /// The conditions raised in the call: by its logic, and on the
    /// outcomes it wrote and read below, from its runs and from its sends
    /// polled during the call.
    raised: Raised,
    /// Whether a send's reply ended cancelled.
    cancelled: bool,
    went_down: bool,
    end: Option<CallsBelowEnd>,
}

/// How a call's calls below ended: refused before anything went down, or
/// failed after.
enum CallsBelowEnd {
    /// The first call below was refused: so is the call.
    Refused(Rejected),
    /// A call below was refused after an earlier one went down, a run's
    /// reply failed, or a send's reply failed during the call: the call
    /// fails.
    Failed(String),
}

/// How a call ended ([`CallsBelow::end_call`]): its reply, or its refusal,
/// and whether it keeps the states made for it.
pub(super) struct CallEnd<B> {
    pub(super) reply: Result<LifecycleBoundReply<AdapterFnReply<B>>, Rejected>,
    /// Whether the call keeps the states made for it: only when it was
    /// neither refused, failed nor cancelled as it returned.
    pub(super) keeps_states: bool,
}

impl<B: Reply<Item = ShotOutcome>> CallsBelow<B> {
    /// No call below yet.
    pub(super) fn new() -> Self {
        Self {
            parts: Vec::new(),
            read: Vec::new(),
            raised: Raised::default(),
            cancelled: false,
            went_down: false,
            end: None,
        }
    }

    /// Ends a call whose logic returned `returned`, what it wrote being
    /// `written`, by the rules of [`adapter_fn`], the same for every shot
    /// form, once the sends' replies still pending are polled: refused with
    /// the first call below's refusal, failing the adapter if that was for
    /// an earlier failure below; else failed, failing the adapter, by a
    /// call below that did not go on, else by the logic's `Err`; else the
    /// reply of the logic's writes and its sends, which fails the adapter
    /// at once if a send's reply was cancelled. The call keeps the states
    /// made for it only when it ends none of these ways.
    ///
    /// Unless refused, it merges into `so_far`, the adapter's, the
    /// conditions raised in the call: those the logic raised, those carried
    /// by what it wrote and by what the sends read so far (a run's read
    /// was taken in as it returned). Then it marks with the conditions so
    /// far of the call's `shots` every outcome its reply gives: those
    /// written and read now, and those of its sends' replies still pending
    /// as they pass up. While no shot of the call has a condition, its
    /// passes over the outcomes only read them.
    pub(super) fn end_call(
        mut self,
        lifecycle: &mut ExecutorLifecycle,
        so_far: &mut ConditionsSoFar,
        shots: &[ShotId],
        mut written: ImmediateReply<ShotOutcome>,
        returned: Result<(), String>,
    ) -> CallEnd<B> {
        self.poll_sends();
        let failure = match self.end {
            Some(CallsBelowEnd::Refused(rejected)) => {
                return CallEnd {
                    reply: Err(lifecycle.pass_up_refusal(rejected)),
                    keeps_states: false,
                };
            }
            Some(CallsBelowEnd::Failed(reason)) => Some(reason),
            None => returned.err(),
        };
        // A send's reply cancelled fails the adapter, as the call's reply
        // would once polled to its end: here at once, though it is never
        // polled.
        let cancellation = self.cancelled.then_some(CANCELLATION);
        if let Some(reason) = failure.as_deref().or(cancellation) {
            lifecycle.fail(reason);
        }
        self.raised.raise_carried(written.items());
        self.raised.raise_carried(&self.read);
        self.raised.into_so_far(so_far);
        let mut marks = so_far.of_shots(shots);
        marks.mark(written.items_mut());
        marks.mark(&mut self.read);
        if self.parts.is_empty() {
            marks = Marks::default();
        }
        CallEnd {
            keeps_states: failure.is_none() && !self.cancelled,
            reply: Ok(lifecycle.bind(AdapterFnReply {
                written,
                read: self.read,
                parts: self.parts,
                marks,
                failure,
                cancelled: self.cancelled,
            })),
        }
    }

    /// Takes in the conditions a logic serving `shots` raised on them in
    /// the call ([`Outcomes::raise_conditions`]); those its outcomes carry
    /// are taken in as the call ends.
    pub(super) fn take_in_raised(&mut self, shots: &[ShotId], outcomes: &Outcomes<'_>) {
        let raised = outcomes.raised();
        if !raised.is_empty() {
            for &shot in shots {
                self.raised.raise(shot, raised);
            }
        }
    }

    /// Whether the call's calls below go on: why not, if one was refused,
    /// or a run's or a send's reply failed.
    pub(super) fn going_on(&self) -> Result<(), String> {
        match &self.end {
            Some(CallsBelowEnd::Refused(rejected)) => Err(rejected.to_string()),
            Some(CallsBelowEnd::Failed(reason)) => Err(reason.clone()),
            None => Ok(()),
        }
    }

    /// Whether the call's calls below ended ([`going_on`](Self::going_on)
    /// is `Err`).
    pub(super) fn ended(&self) -> bool {
        self.end.is_some()
    }

    /// Polls the replies of the sends still pending, with no waker: one
    /// that failed ends the call's calls below, so that nothing more goes
    /// down; one cancelled ends the call cancelled, as its reply's poll
    /// would.
    fn poll_sends(&mut self) {
        let (read, cancelled) = (&mut self.read, &mut self.cancelled);
        if let Err(reason) = poll_parts(&mut self.parts, read, Waker::noop(), cancelled) {
            self.send_failed(reason);
        }
    }

    /// Ends the call's calls below for `reason`, a send's reply's failure,
    /// unless they ended before, and drops the other sends' replies: the
    /// call's reply ends with it at once, as when it is seen there
    /// ([`AdapterFnReply`]).
    fn send_failed(&mut self, reason: String) {
        self.parts.clear();
        self.end.get_or_insert(CallsBelowEnd::Failed(reason));
    }

    /// Calls `below` with `instructions` on `shots`, unless the call's
    /// calls below ended; a refusal ends them: refused if nothing went
    /// down before, else failed.
    fn call<O, D>(
        &mut self,
        below: &mut D,
        shots: &ShotSet,
        instructions: &[O],
    ) -> Result<B, String>
    where
        D: Executor<InstructionBatch<O>, Reply = B>,
    {
        self.going_on()?;
        match below.call(InstructionBatchRef {
            shots,
            instructions,
        }) {
            Ok(reply) => {
                self.went_down = true;
                Ok(reply)
            }
            Err(rejected) => {
                let reason = rejected.to_string();
                self.end = Some(if self.went_down {
                    CallsBelowEnd::Failed(reason.clone())
                } else {
                    CallsBelowEnd::Refused(rejected)
                });
                Err(reason)
            }
        }
    }

    /// Sends `instructions` down to `below` on `shots` ([`Below::send`]):
    /// its reply passes up as part of the call's. The reply is polled at
    /// once, so that one that failed inside the call below ends the call's
    /// calls below here, and this send gives the failure.
    pub(super) fn send<O, D>(
        &mut self,
        below: &mut D,
        shots: &ShotSet,
        instructions: &[O],
    ) -> Result<(), String>
    where
        D: Executor<InstructionBatch<O>, Reply = B>,
    {
        // Known issue, open (blocks neither): a reply that fails after it
        // was polled here is seen only at the next poll of the pending
        // replies, before a later run or as the logic returns, so the sends
        // made in between go down. Polling every pending reply at each
        // send would cost a time quadratic in a call's sends. By the rule
        // at `adapter_fn`'s rules, one that fails, or is cancelled, after
        // the call returned is seen only once the call's reply is polled,
        // and the call keeps its states; should its reply be dropped
        // unpolled, the next call runs its logic and reaches below, which
        // refuses it for the earlier failure: the adapter fails one call
        // late (the known issue there).
        let mut part = HeldPart::new(self.call(below, shots, instructions)?);
        // Kept only while pending: a call below that answers in its call
        // costs the adapter no `parts` buffer.
        let (read, cancelled) = (&mut self.read, &mut self.cancelled);
        match poll_part(&mut part, read, Waker::noop(), cancelled) {
            Ok(true) => self.parts.push(part),
            Ok(false) => {}
            Err(reason) => self.send_failed(reason),
        }
        self.going_on()
    }

    /// Runs `instructions` below on `shots` and waits for what they read
    /// ([`Below::run`]), once the sends' replies still pending are polled,
    /// so that one that failed stops the run; a reply that fails, is
    /// cancelled or is not whole fails the call. It takes in the conditions
    /// of what the run read, whether it ends well or not: when not, only on
    /// the run's `shots`, as a readout of another shot may belong to
    /// another call. Those a send reads during the call are taken in on
    /// whatever shot each names ([`end_call`](Self::end_call)), as a
    /// send's reply passes up as it comes, unchecked.
    pub(super) fn run<O, D>(
        &mut self,
        below: &mut D,
        shots: &ShotSet,
        instructions: &[O],
    ) -> Result<Vec<ShotOutcome>, String>
    where
        D: Executor<InstructionBatch<O>, Reply = B>,
    {
        self.poll_sends();
        let reply = HeldPart::new(self.call(below, shots, instructions)?);
        let asked = shots.sorted();
        let mut read = Vec::new();
        match wait(reply, &mut read).and_then(|()| whole(&mut read, &asked)) {
            Ok(()) => {
                // Whole, so every readout is of one of `shots`: no filter.
                self.raised.raise_carried(&read);
                Ok(read)
            }
            Err(reason) => {
                self.raised.raise_carried_on(&read, &asked);
                self.end = Some(CallsBelowEnd::Failed(reason.clone()));
                Err(reason)
            }
        }
    }
}

/// Puts `read`, the readouts of a run on the shots `asked` (given in
/// increasing order), by shot, in ascending order, each shot's in the
/// order the reply gave them, so that a logic passing them up passes them
/// as they came: the order the shot read them only when the executor below
/// keeps it ([`Below::run`]).
///
/// # Errors
///
/// When it is not whole: it reads a tag of a shot twice, or a shot not in
/// `asked`.
fn whole(read: &mut [ShotOutcome], asked: &[ShotId]) -> Result<(), String> {
    // Stable: a shot's readouts keep their order. A reply often gives its
    // readouts by shot already, and always for one shot: no sort then.
    if !read.is_sorted_by_key(|outcome| outcome.shot) {
        read.sort_by_key(|outcome| outcome.shot);
    }
    let mut tags = Vec::new();
    for of_shot in read.chunk_by(|a, b| a.shot == b.shot) {
        if let Some(tag) = repeated_tag(of_shot, &mut tags) {
            let shot = of_shot[0].shot;
            return Err(format!(
                "the call below read tag {tag} of shot {shot} twice"
            ));
        }
    }
    // Both are by shot: one pass over each.
    let mut rest = asked.iter().peekable();
    for outcome in &*read {
        while rest.next_if(|&&j| j < outcome.shot).is_some() {}
        if rest.peek() != Some(&&outcome.shot) {
            let shot = outcome.shot;
            return Err(format!(
                "the call below read shot {shot}, which it was not given"
            ));
        }
    }
    Ok(())
}

/// A tag that `of_shot`, one shot's readouts, reads twice, if any. A few
/// readouts, up to 16, are compared pairwise, with no buffer; more are
/// sorted in `tags`, a buffer that the shots of a run share, since the
/// pairs grow as the square of the readouts.
//
// Known issue, for a later efficiency pass (blocks neither): no bench of
// the crate measures `whole`, so nothing here shows that 16 is the right
// bound; it was chosen by a timing made outside the crate.
fn repeated_tag(of_shot: &[ShotOutcome], tags: &mut Vec<u32>) -> Option<u32> {
    if of_shot.len() <= 16 {
        let mut rest = of_shot;
        while let [first, others @ ..] = rest {
            if others.iter().any(|other| other.tag == first.tag) {
                return Some(first.tag);
            }
            rest = others;
        }
        return None;
    }
    tags.clear();
    tags.extend(of_shot.iter().map(|outcome| outcome.tag));
    tags.sort_unstable();
    tags.windows(2)
        .find(|pair| pair[0] == pair[1])
        .map(|pair| pair[0])
}

/// The reply to a call of an [`adapter_fn`]: what its logic wrote, then the
/// events of each send's reply, polled in the order of the sends, each as they
/// come: first what they read when polled during the call ([`Below::send`]),
/// then the rest. It ends when every send's reply has ended, or, failed, at the
/// first that fails, with the call's own failure if it has one, else that
/// reply's. When every one has ended, it ends failed with the call's own
/// failure (the logic's, a call below refused after another went down, a run
/// whose reply failed, was cancelled or was not whole, or a send's reply that
/// failed during the call, which drops the others: it ends at the first poll)
/// if any, else cancelled if one was, else done.
///
/// # Type parameters
///
/// - `B`: the reply of each call below, the executor below's
///   ([`Executor::Reply`]).
pub struct AdapterFnReply<B> {
    written: ImmediateReply<ShotOutcome>,
    /// What the sends' replies read when polled during the call.
    read: Vec<ShotOutcome>,
    /// The replies of the sends still pending, in the order of the sends,
    /// each held as [`CallsBelow`] holds it.
    parts: Vec<HeldPart<B>>,
    /// The conditions of the call's shots as the call ended, which mark
    /// the outcomes `parts` give: none when none has any, or none is
    /// pending.
    marks: Marks,
    failure: Option<String>,
    /// Whether a send's reply ended cancelled.
    cancelled: bool,
}

impl<B: Reply<Item = ShotOutcome>> Reply for AdapterFnReply<B> {
    type Item = ShotOutcome;

    fn poll_events(&mut self, out: &mut Vec<ShotOutcome>, waker: &Waker) -> ReplyStatus {
        // Never failed: the call's failure is this reply's own.
        self.written.poll_events(out, waker);
        out.append(&mut self.read);
        let from = out.len();
        let polled = poll_parts(&mut self.parts, out, waker, &mut self.cancelled);
        self.marks.mark(&mut out[from..]);
        if let Err(reason) = polled {
            // The call's own failure is the one its adapter kept.
            return ReplyStatus::Failed(self.failure.take().unwrap_or(reason));
        }
        if !self.parts.is_empty() {
            ReplyStatus::Pending
        } else if let Some(reason) = self.failure.take() {
            ReplyStatus::Failed(reason)
        } else if self.cancelled {
            ReplyStatus::Cancelled
        } else {
            ReplyStatus::Done
        }
    }
}

/// Polls `parts`, the replies of a call's sends still pending, in order,
/// into `out` ([`poll_part`]), and drops each that ended; at the first that
/// fails, gives its reason, leaving the others unpolled. Each is dropped in
/// a catch of its own ([`HeldPart`]).
//
// It drops the others there too, so that they are freed at the failure,
// not with the reply. No test pins when: only a double that counts its
// replies' drops would see it, as the reply ends there and is not polled
// again. During the call, `send_failed` drops them as well, and that drop
// is pinned
// (`src/tests/adapter/adapter_fn/one_per_shot.rs`:
// `a_send_whose_reply_fails_ends_the_call_though_another_is_pending`).
//
// Known issue, open (wording only; blocks neither this commit nor a later
// one): over a placed executor, a later send's or run's reply may fail
// first, for the earlier send's failure, with the reason `an earlier call
// failed: …`; the call fails with that reason, and the adapter's next call
// is refused with it prefixed again (`an earlier call failed: an earlier
// call failed: …`). Which reason the call ends with then varies with
// timing: the earlier send's own, or the later reply's. Keeping the first
// failure's own reason would need the earlier sends' replies polled first,
// the design question held with the order of a call's readouts.
fn poll_parts<B: Reply<Item = ShotOutcome>>(
    parts: &mut Vec<HeldPart<B>>,
    out: &mut Vec<ShotOutcome>,
    waker: &Waker,
    cancelled: &mut bool,
) -> Result<(), String> {
    let mut i = 0;
    while i < parts.len() {
        match poll_part(&mut parts[i], out, waker, cancelled) {
            Ok(true) => i += 1,
            Ok(false) => {
                parts.remove(i);
            }
            Err(reason) => {
                parts.clear();
                return Err(reason);
            }
        }
    }
    Ok(())
}

/// Polls `part`, a send's reply, into `out`: whether it is still pending,
/// setting `cancelled` when it ended cancelled; or, failed, why.
fn poll_part<B: Reply<Item = ShotOutcome>>(
    part: &mut B,
    out: &mut Vec<ShotOutcome>,
    waker: &Waker,
    cancelled: &mut bool,
) -> Result<bool, String> {
    match part.poll_events(out, waker) {
        ReplyStatus::Pending => Ok(true),
        ReplyStatus::Done => Ok(false),
        ReplyStatus::Cancelled => {
            *cancelled = true;
            Ok(false)
        }
        ReplyStatus::Failed(reason) => Err(reason),
    }
}
