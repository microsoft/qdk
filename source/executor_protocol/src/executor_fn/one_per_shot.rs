// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The shot strategy of a state per shot ([`OnePerShot`]), and the
//! executor it makes of an [`executor_fn`](fn@crate::executor_fn)
//! ([`OnePerShotExecutor`]).

mod placed;

use std::marker::PhantomData;
use std::slice;

pub use placed::OnePerShotPlacedExecutor;

use super::{
    ExecutorFn, ExecutorLogic, MakeState, Outcomes, ProgramInput, ShotStrategy, check_room,
    described, fail, report_state, start_close,
};
use crate::absorb::{DroppedOneByOne, HeldPart, HeldStates};
use crate::lifecycle::failing_on_panic;
use crate::shot::ShotIndex;
use crate::{
    Capabilities, Close, Describe, Executor, ExecutorLifecycle, ExecutorReport, Fold,
    ImmediateReply, InstructionBatch, InstructionBatchRef, NoCheck, PlacedExecutor, Placement,
    Rejected, ShotContext, ShotId, ShotOutcome,
};

/// The shot strategy of a state per shot: shot `j`'s state, when this
/// strategy makes it, is made from the context of `j` in the run whose seed
/// the executor is given ([`ExecutorFn::seed`]), so that a state draws the
/// same values whatever the other shots of the run. It serves an executor
/// of many shots, such as a simulator of a whole run. A state the executor
/// made before [`ExecutorFn::shots`] is carried over with the context it
/// was made with.
///
/// Given to [`ExecutorFn::shots`], it makes a [`OnePerShotExecutor`].
///
/// # Example
///
/// An executor of a simulator with a bounded state per shot; its
/// implementer writes only its state, its logic and how to read its log
/// probability. The state draws from its shot's seed. Excerpt
/// of `bond_limited_executor`, in the guide test
/// `tests/per_shot_executor/bond_limited.rs`, which compiles it:
///
/// ```ignore
/// let promise = Capabilities::default()
///     .with_qubits(QUBITS)
///     .with_states(STATES);
/// executor_fn(
///     promise,
///     move |context: ShotContext| {
///         if busy.take_if(|&mut j| j == context.shot()).is_some() {
///             return Err(Rejected::Unavailable("the device is busy".into()));
///         }
///         Ok(BondLimitedState::new(context.seed()))
///     },
///     run_bond_limited,
/// )
/// .check(check)
/// .log_probability(|state| state.log_p)
/// ```
///
/// and a run of eight shots on it, from the per-shot executor guide
/// (`tests/per_shot_executor.rs`):
///
/// ```ignore
/// let shots = ShotSet::range(0..=7).expect("not empty");
/// let executor = bond_limited_executor(None).seed(3).shots(OnePerShot);
/// let (outcomes, _) = (Driver::new(TossAgainOnOne, shots) >> executor)
///     .expect("the executor has feedback, probabilities and 8 states")
///     .wait();
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OnePerShot;

impl<I, S, M, P, C, L, R> ShotStrategy<ExecutorFn<I, S, M, P, C, L, R>> for OnePerShot {
    type Executor = OnePerShotExecutor<I, S, M, P, C, L, R>;

    fn apply(self, executor: ExecutorFn<I, S, M, P, C, L, R>) -> Self::Executor {
        let mut index = ShotIndex::default();
        let states = executor.held;
        for (shot, _) in states.iter() {
            index.push(*shot);
        }
        OnePerShotExecutor {
            seed: executor.seed,
            promise: executor.promise,
            states,
            make_state: executor.make_state,
            logic: executor.logic,
            check: executor.check,
            log_probability: executor.log_probability,
            state_report: executor.state_report,
            index,
            calls: executor.calls,
            lifecycle: executor.lifecycle,
            instructions: PhantomData,
        }
    }
}

/// An [`executor_fn`](fn@crate::executor_fn) under [`OnePerShot`]: a state
/// per shot, each made at its shot's first call, from the shot's context
/// in the run ([`ExecutorFn::seed`]), and kept for the shot's later calls
/// until the executor closes, which drops them. A state made before
/// [`ExecutorFn::shots`] is carried over and keeps the context it was made
/// with.
///
/// - It describes its promise as given, since a state per shot lowers
///   nothing, but with [`probabilities`](Capabilities::probabilities)
///   exactly when it is given how to read them
///   ([`ExecutorFn::log_probability`]), whatever the promise says.
/// - A call runs the logic on each of its shots in the batch's order,
///   inside `call` (the reply is [`ImmediateReply`]): the check first,
///   once; then the state of each shot new to the executor is made; then
///   the logic runs, shot by shot.
/// - A call that would hold more states than the promise's
///   [`states`](Capabilities::states) is refused
///   ([`Rejected::ShotNotHeld`], for the first shot it has no room for)
///   before any state is made, so that the promise holds when it is called
///   directly too, not only in a run.
/// - A state that cannot be made refuses the whole call with the state
///   closure's refusal: no shot runs, and the states made for the call are
///   dropped.
/// - A logic that fails on a shot fails the call: the outcomes pushed
///   before it are handed over, then the reply ends with its reason; the
///   later shots of the batch do not run. Every state made for the call is
///   dropped, whether its shot ran it or not, each in a catch of its own:
///   should drops panic, the implementer's code, the first one's panic
///   reaches the caller once all are dropped, as one state's would, and
///   the others are left to the panic hook, so that two such drops do not
///   abort the process. The executor then refuses every later call.
/// - A panic inside `call` (the check's, a state closure's, or the
///   logic's on a shot) reaches the caller, who gets none of the call's
///   outcomes, and fails the executor, as a `Mutex` is poisoned, since the
///   shots before it ran the batch and the one that panicked may be
///   part-way through it: every later call is refused (`the executor
///   panicked during the call`). Every state made for the call is dropped,
///   whether its shot ran it or not, as after a failure, but as the panic
///   leaves: a panic of a state's drop is left to the panic hook, so that
///   the call's panic is the one that reaches the caller.
/// - `drop` never panics: a state's drop that panics as the executor is dropped
///   is absorbed, each state in a catch of its own, after the panic hook
///   reported it (Rust API Guidelines, C-DTOR-FAIL); call `close` to see the
///   first one. So is the drop of what its closures capture, each closure in a
///   catch of its own; `close` raises only the states'.
/// - Its report counts the calls it accepted, and gives each state's log
///   probability and report, when it is given how to make them
///   ([`ExecutorFn::state_report`]), as their calls left them, the failed
///   call included; but not those of the states made for the failed call,
///   whether their shots ran or not: a failed call reports no shot of it,
///   so that the report is the same placed or not, per shot or whole.
///
/// # Type parameters
///
/// - `I` … `R`: as [`ExecutorFn`]'s, from the executor it was made from.
pub struct OnePerShotExecutor<I, S, M, P, C = NoCheck<I>, L = fn(&S) -> f64, R = fn(&S)> {
    /// The run's seed, if the executor was given one.
    seed: Option<u64>,
    promise: Capabilities,
    /// The states kept, each with its shot, in the order their shots first
    /// ran: shot `j`'s at `index.get(j)`. Declared before the closures, so
    /// that they are dropped first (the reason, and that no test pins the
    /// order, are at `HeldPart`).
    states: HeldStates<(ShotId, S)>,
    make_state: HeldPart<M>,
    logic: HeldPart<P>,
    check: HeldPart<C>,
    log_probability: HeldPart<Option<L>>,
    state_report: HeldPart<R>,
    index: ShotIndex,
    calls: u64,
    lifecycle: ExecutorLifecycle,
    instructions: PhantomData<fn(&[I])>,
}

impl<I, S, M, P, C, L, R> OnePerShotExecutor<I, S, M, P, C, L, R> {
    /// This executor, run where `placement` says ([`PlacedExecutor`]):
    /// every shot's state on the one placement, such as a thread of its
    /// own ([`OwnThread`](crate::OwnThread)).
    pub fn run_on<T: Placement>(self, placement: T) -> PlacedExecutor<Self, T> {
        let lifecycle = self.lifecycle.continued();
        PlacedExecutor::new(self, placement, lifecycle, Self::free_raising)
    }

    /// Frees it, raising the first panic of its states' drops, which its
    /// `drop` absorbs (it still absorbs the drops of what its closures
    /// capture): for a placed executor that frees it on the caller's
    /// thread ([`PlacedExecutor`]'s `close` once its placement stopped).
    fn free_raising(mut self) {
        drop(self.states.take_raising_first());
    }
}

impl<I, S, M, P, C, L, R> OnePerShotExecutor<I, S, M, P, C, L, R>
where
    M: MakeState<S>,
{
    /// Makes the state of each shot of `shots` the executor does not hold,
    /// in the order of `shots`, each with its shot; it keeps none of them:
    /// the caller does, as each shot runs.
    ///
    /// # Errors
    ///
    /// [`Rejected::ShotNotHeld`] for the first shot beyond the promise's
    /// [`states`](Capabilities::states), checked before any state is made;
    /// then the state closure's refusal, after which the states made by
    /// this call are dropped.
    ///
    /// # Panics
    ///
    /// When a state closure panics; or, after its refusal, when the drop of
    /// a state made by this call panics: the first such panic, once every
    /// one is dropped.
    fn make_new_states(
        &mut self,
        shots: &[ShotId],
    ) -> Result<DroppedOneByOne<(ShotId, S)>, Rejected> {
        let index = &self.index;
        let new = shots.iter().copied().filter(|&j| index.get(j).is_none());
        check_room(&self.promise, self.states.len(), new)?;
        // Pushed one by one, so that every state made so far is in it when
        // a state closure refuses or panics.
        let mut made = DroppedOneByOne::default();
        for &j in shots {
            if self.index.get(j).is_none() {
                made.push((j, (self.make_state)(ShotContext::in_run(self.seed, j))?));
            }
        }
        Ok(made)
    }
}

impl<I, S, M, P, C, L, R> Executor<InstructionBatch<I>> for OnePerShotExecutor<I, S, M, P, C, L, R>
where
    M: MakeState<S>,
    P: ExecutorLogic<I, S>,
    C: FnMut(&ProgramInput<'_, I>) -> Result<(), Rejected>,
{
    /// The logic runs inside `call`.
    type Reply = ImmediateReply<ShotOutcome>;

    /// # Errors
    ///
    /// [`Rejected::Closed`] once closed, then
    /// [`Rejected::EarlierFailure`] after a call failed; then the check's
    /// refusal ([`ExecutorFn::check`]); then [`Rejected::ShotNotHeld`]
    /// beyond the promise's [`states`](Capabilities::states), and the
    /// refusal of the closure that makes a state. Nothing ran in each case.
    ///
    /// # Panics
    ///
    /// When the check, a state closure or the logic panics, or the drop
    /// of a state made for a call that is refused or fails (the first such
    /// drop's panic, once every state made for it is dropped): the panic
    /// reaches the caller, and every later call is refused with
    /// [`Rejected::EarlierFailure`]: with the logic's reason when it
    /// failed (the first failure is kept), else `the executor panicked
    /// during the call`.
    fn call(&mut self, batch: InstructionBatchRef<'_, I>) -> Result<Self::Reply, Rejected> {
        failing_on_panic(self, |this| &this.lifecycle, |this| this.serve(batch))
    }
}

impl<I, S, M, P, C, L, R> OnePerShotExecutor<I, S, M, P, C, L, R>
where
    M: MakeState<S>,
    P: ExecutorLogic<I, S>,
    C: FnMut(&ProgramInput<'_, I>) -> Result<(), Rejected>,
{
    /// Serves `batch`: the steps of `call`, which runs them failing the
    /// executor should they panic ([`failing_on_panic`]).
    fn serve(
        &mut self,
        batch: InstructionBatchRef<'_, I>,
    ) -> Result<ImmediateReply<ShotOutcome>, Rejected> {
        self.lifecycle.admit()?;
        let input = ProgramInput {
            instructions: batch.instructions,
        };
        (self.check)(&input)?;
        let shots = batch.shots.as_slice();
        let mut made = self.make_new_states(shots)?;
        self.calls += 1;
        let mut reply = ImmediateReply::default();
        // The states of the shots new to the executor were made above, in
        // this order; they join the executor's states only once the call
        // succeeds, so that a failure or a panic drops every state made for
        // the call, whether its shot ran or not, each in a catch of its own
        // (`DroppedOneByOne`).
        // Known issue (blocks neither): a second index lookup per shot, after
        // the one in `make_new_states`, where one would do; a third when the
        // promise bounds the states (the room check, before any state is
        // made), and a reply not presized to the shots; not measured yet.
        let mut next_made = 0;
        for &j in shots {
            let state = if let Some(at) = self.index.get(j) {
                &mut self.states[at].1
            } else {
                next_made += 1;
                &mut made[next_made - 1].1
            };
            let mut outcomes = Outcomes {
                shots: slice::from_ref(&j),
                reply: &mut reply,
            };
            if let Err(reason) = (self.logic)(state, &input, &mut outcomes) {
                fail(&mut self.lifecycle, &mut reply, reason);
                return Ok(reply);
            }
        }
        for &(j, _) in made.iter() {
            self.index.push(j);
        }
        self.states.append(&mut made);
        Ok(reply)
    }
}

impl<I, S, M, P, C, L, R> Describe for OnePerShotExecutor<I, S, M, P, C, L, R> {
    /// The promise as given, but with probabilities exactly when it is
    /// given how to read them ([`ExecutorFn::log_probability`]), whatever
    /// the promise says.
    fn capabilities(&self) -> Capabilities {
        described(&self.promise, self.log_probability.as_ref(), self.seed)
    }
}

impl<I, S, M, P, C, L, R, Q> Close for OnePerShotExecutor<I, S, M, P, C, L, R>
where
    L: FnMut(&S) -> f64,
    R: FnMut(&S) -> Q,
    Q: Fold,
{
    type Report = ExecutorReport<Q>;

    /// Refuses later calls, drops the states, and reports the calls
    /// accepted since the last report, and each state's report
    /// ([`ExecutorFn::state_report`]) and log probability, when it was
    /// given how to read it, once.
    ///
    /// # Panics
    ///
    /// When `log_probability`, `state_report` or the report's `Q::default`
    /// panics, or the drop of a state it reports (the first such drop's panic,
    /// once every state is dropped), outside an unwind: the panic reaches the
    /// caller, placed or not ([`PlacedExecutor`]), whereas `drop` absorbs it.
    /// The states are taken first, so they are freed before the panic leaves,
    /// and a later `close` reports none of them (it calls `Q::default` again,
    /// which may panic again). A panic of a state's drop as the panic of a
    /// report closure or of `Q::default` leaves is left to the panic hook.
    fn close(&mut self) -> ExecutorReport<Q> {
        // Known issue, open (it needs two bugs at once; blocks neither this
        // commit nor a later one): run from a `drop` while another panic
        // unwinds, a panic of a report closure or of `Q::default` here
        // aborts the process, placed or not (as the panic leaves that
        // `drop`); the `# Panics` promise holds outside an unwind only.
        let calls = start_close(&mut self.lifecycle, &mut self.calls);
        self.index = ShotIndex::default();
        // Taken before the report is made, as one state's close does.
        let states = self.states.take_raising_first();
        let mut report = ExecutorReport::of_calls(calls);
        for held in states.iter() {
            report_state(
                &mut report,
                held,
                self.log_probability.as_mut(),
                &mut *self.state_report,
            );
        }
        report
    }
}
