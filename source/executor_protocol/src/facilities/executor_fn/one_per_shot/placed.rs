// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`OnePerShot`] over a placed executor,
//! `.run_on(placement).shots(OnePerShot)`: a state per shot, each on
//! a placement of its own ([`OnePerShotPlacedExecutor`]).

use std::marker::PhantomData;

use super::{OnePerShot, check_room};
use crate::facilities::executor_fn::{
    CallInput, ExecutorFn, ExecutorLogic, MakeState, NoCheck, ShotStrategy, described,
};
use crate::facilities::executor_lifecycle::PartsFailure;
use crate::facilities::teardown::HeldPart;
use crate::{
    Capabilities, Close, Describe, Executor, ExecutorLifecycle, ExecutorPlacement, ExecutorReport,
    Fold, IncrementalReply, InstructionBatch, InstructionBatchRef, PlacedExecutor, Rejected,
    ShotContext, ShotId, ShotOutcome, ShotReplies, ShotStates,
};

impl<I, S, M, P, C, L, R, T> ShotStrategy<PlacedExecutor<ExecutorFn<I, S, M, P, C, L, R>, T>>
    for OnePerShot
where
    P: Clone,
    L: Clone,
    R: Clone,
    T: ExecutorPlacement + Clone,
{
    type Executor = OnePerShotPlacedExecutor<I, S, M, P, T, C, L, R>;

    /// Continues the executor it places, as [`OnePerShot`] does an
    /// executor not placed: its refusals (those of the executor placed, and
    /// those of the placed executor itself), its calls, and the state it
    /// holds, placed for its shot.
    ///
    /// # Panics
    ///
    /// When `executor` was called already: the state it holds is then on its
    /// placement's thread, and taking it back is not built. When it holds a
    /// state from before it was placed, and a `Clone` of the logic, of a report
    /// closure or of the placement panics as that state's shot's copies are
    /// made, the state and the placement are then freed as the panic unwinds,
    /// each drop's panic absorbed. A report closure's copy is made as in
    /// `call`.
    fn hold_shots_of(
        self,
        executor: PlacedExecutor<ExecutorFn<I, S, M, P, C, L, R>, T>,
    ) -> Self::Executor {
        let (executor, placement, lifecycle) = executor.into_unstarted();
        let placement = HeldPart::new(placement);
        let parts_failure = PartsFailure::default();
        let mut held = executor.held;
        // The shot's executor is made before the state leaves its guard
        // (`shot_executor`).
        let held = (!held.is_empty()).then(|| {
            let shot = shot_executor(
                &executor.promise,
                &*executor.logic,
                executor.log_probability.as_ref(),
                &*executor.state_report,
            );
            let (j, state) = held.pop().expect("a state is held");
            let placed = PlacedExecutor::part_of(
                shot((j, state)),
                T::clone(&placement),
                parts_failure.clone(),
                ExecutorFn::free_raising,
            );
            (j, placed)
        });
        OnePerShotPlacedExecutor {
            seed: executor.seed,
            promise: executor.promise,
            shots: ShotStates::continuing(lifecycle, executor.calls, held, parts_failure.clone()),
            make_state: executor.make_state,
            logic: executor.logic,
            check: executor.check,
            log_probability: executor.log_probability,
            state_report: executor.state_report,
            placement,
            parts_failure,
        }
    }
}

/// An [`executor_fn`](fn@crate::executor_fn) placed, then under
/// [`OnePerShot`] (`.run_on(placement).shots(OnePerShot)`): a state
/// per shot, each on a placement of its own, such as a thread per shot
/// under [`OwnThread`](crate::OwnThread), so that the shots of a batch run
/// in parallel.
///
/// It keeps the rules of [`OnePerShotExecutor`](super::OnePerShotExecutor),
/// the executor of the other order, but where the shots run:
///
/// - each shot's state is made on the caller's thread, at the shot's first
///   call, from the shot's context in the run ([`ExecutorFn::seed`]), then
///   moved to a copy of the placement, with a copy of the logic, of the
///   log probability's reader and of the state's report's maker (hence
///   their bounds `Clone + Send`): a logic that keeps state of its own
///   (a counter it captures) keeps one copy per shot, where the other order
///   keeps one for every shot;
/// - the check runs once per call, on the caller's thread, before any state
///   is made; a call beyond the promise's
///   [`max_shots`](Capabilities::max_shots) is refused
///   ([`Rejected::ShotNotHeld`]) before any state is made, and a state
///   that cannot be made refuses the call with the state closure's
///   refusal: nothing runs, and the states made for the call are dropped;
/// - each shot's placement holds its state until it closes or is dropped: under
///   [`OwnThread`](crate::OwnThread), one thread per shot held, as many as the
///   promise's [`max_shots`](Capabilities::max_shots) allows; dropped, each
///   shot cancels its calls not yet started, as a [`PlacedExecutor`] does, so a
///   call's reply ends
///   [`ReplyStatus::Cancelled`](crate::ReplyStatus::Cancelled) once a shot of
///   it was cancelled, unless a shot of it failed: a failure outranks a
///   cancellation, whatever the shots' order; the shots running finish, and
///   each state is then dropped on its placement once that placement started (a
///   state held from before it was placed, and not called since, is dropped
///   where it is, where its calls ran); a state's drop that panics is then
///   absorbed, as a [`PlacedExecutor`]'s drop absorbs it, so that `drop` never
///   panics (`close` raises it, as a refused call does for the states made for
///   it); so is the drop of what its closures and the placement capture, each
///   in a catch of its own. A shot's state is dropped, on its placement,
///   before the shot's copies of the logic, of the log probability's reader
///   and of the report's maker, so that a state whose drop reaches its
///   device through itself or through those copies (a weak handle to the
///   device's connection, which the logic's copy holds, say) still finds it
///   there.
///   The executor's own closures (the state closure, the check, and the
///   closures it copies for each shot) are dropped on the thread that drops
///   it, and `drop` does not wait for the shots, as no placed executor's
///   drop waits for its placement ([`ExecutorPlacement`]): they may be dropped
///   before a shot's state, so a state whose drop reaches its device
///   through them may not find it there; `close` waits, and frees every
///   state first. A state held from before it was placed, and not called
///   since, is dropped before the executor's closures;
/// - the shots of a batch run in parallel, so their outcomes interleave, each
///   shot's in order ([`ShotReplies`]); a logic that fails on a shot fails the
///   call, though the other shots of the batch may have run, and every later
///   call is refused, whatever shots it names, from the moment the failure
///   happens on the shot's placement, whether or not its reply was polled:
///   before the check, before any state is made and before any shot runs. The
///   shots' executors share their first failure for that, and each call reads
///   it once (one atomic load). A call accepted before the failure, and queued
///   on the failed shot behind the failing call, does not run there: its reply
///   fails with `an earlier call failed: …`, not named by the shot, and its
///   other shots, before or after the failed one in the batch, may run. A call
///   being started as the failure happens can miss it: it is refused still when
///   the failed shot comes first in its batch and the failure is recorded
///   before that shot is given the call; otherwise the call is accepted, and
///   its reply fails: with the failure named by its shot (`shot j: …`) when the
///   failed shot refused the call, after the shots before it in the batch may
///   have run, or as above when the failed shot had queued it behind the
///   failing call;
/// - a panic inside `call`, on the caller's thread (the check's, a state
///   closure's, a `Clone`'s of the logic, of a report closure or of the
///   placement as a shot's copies are made, or a shot's placement's as it
///   starts or is handed a job),
///   reaches the caller, who gets no reply, and fails the executor, as a
///   `Mutex` is poisoned, since the shots before it in the batch may have
///   run: every later call is refused (`the executor panicked during the
///   call`);
/// - its report counts every call it accepted, and gives each state's log
///   probability and report ([`ExecutorFn::state_report`]), made on its
///   placement at close once that placement started (where it is for a
///   state held from before it was placed and not called since); but not
///   those of a state made for a failed call,
///   or a later one, though its shot ran there before the failure, as the
///   other order's report, whose executor never reached it, does not; nor
///   those of a state whose placement stopped (its logic panicked, or its
///   placement dropped a job unrun), where the other order's report then
///   gives no state's. `close` waits for every shot's calls before it
///   closes one, so a failure it learns of then still decides, and it
///   closes only the shots it reports: the report closures never read a
///   state it leaves out, in either order. The other order's report
///   is its executor's, which leaves out the calls that never reached it
///   (queued behind a failure), so after a failure their counts may
///   differ; the states' reports do not, unless a placement stopped;
/// - a panic of a report closure (`log_probability`'s or `state_report`'s)
///   on a shot's placement at close is raised again in the caller of
///   `close`, its payload unchanged, as when not placed ([`PlacedExecutor`]):
///   work moved to another thread reports its panic to the caller that
///   waits for it, as `std::thread::scope` and rayon do.
///
/// # Type parameters
///
/// - `I`, `S`, `M`, `P`, `C`, `L`, `R`: as [`ExecutorFn`]'s, from the
///   executor it was made from.
/// - `T`: the placement ([`ExecutorPlacement`]) given to
///   [`ExecutorFn::run_on`]; each shot's state runs on a copy of it.
pub struct OnePerShotPlacedExecutor<I, S, M, P, T, C = NoCheck<I>, L = fn(&S) -> f64, R = fn(&S)> {
    /// The run's seed, if the executor was given one.
    seed: Option<u64>,
    promise: Capabilities,
    /// Declared before the closures, so that the shots' executors are
    /// dropped first (the reason is at `HeldPart`).
    /// The placement's place in the order is not promised: it is not a
    /// closure the states were made or run with.
    shots: ShotStates<PlacedShotExecutor<I, S, P, L, R, T>>,
    make_state: HeldPart<M>,
    logic: HeldPart<P>,
    check: HeldPart<C>,
    log_probability: HeldPart<Option<L>>,
    state_report: HeldPart<R>,
    placement: HeldPart<T>,
    /// The first failure of any shot's executor, which each records on its
    /// placement as it happens, and `shots` reads at each call.
    parts_failure: PartsFailure,
}

/// The executor of one shot's state, on the shot's placement.
type PlacedShotExecutor<I, S, P, L, R, T> = PlacedExecutor<ShotExecutor<I, S, P, L, R>, T>;

/// The executor of one shot's state: it holds the state from the start,
/// and leaves the check to the executor of every shot.
type ShotExecutor<I, S, P, L, R> =
    ExecutorFn<I, S, fn(ShotContext) -> Result<S, Rejected>, P, NoCheck<I>, L, R>;

/// The maker of shot `held.0`'s executor, holding its state `held.1`: the
/// logic, the log probability's reader and the report's maker are copied
/// first, so that no `Clone` of theirs (the implementer's code) runs while
/// a state is held outside a guard, where, should it panic, the state's
/// drop during the unwind could panic too and abort the process. Each copy
/// is held ([`HeldPart`]) as soon as it is made, so that, should a later
/// copy panic, or the state closure, which `call` runs while the maker
/// holds the copies, panic or refuse, the copies made are dropped each in
/// a catch of its own.
fn shot_executor<I, S, P: Clone, L: Clone, R: Clone>(
    promise: &Capabilities,
    logic: &P,
    log_probability: Option<&L>,
    state_report: &R,
) -> impl FnOnce((ShotId, S)) -> ShotExecutor<I, S, P, L, R> {
    let promise = promise.clone();
    let logic = HeldPart::new(logic.clone());
    let log_probability = HeldPart::new(log_probability.cloned());
    let state_report = HeldPart::new(state_report.clone());
    move |held| ExecutorFn {
        promise,
        seed: None,
        held: vec![held].into(),
        make_state: HeldPart::new(made_before),
        logic,
        check: HeldPart::new(|_| Ok(())),
        log_probability,
        state_report,
        calls: 0,
        lifecycle: ExecutorLifecycle::default(),
        instructions: PhantomData,
    }
}

/// The state closure of a shot's executor, which never runs: the state is
/// made before the executor.
fn made_before<S>(context: ShotContext) -> Result<S, Rejected> {
    unreachable!(
        "shot {}'s executor holds its state from the start",
        context.shot()
    )
}

impl<I, S, M, P, T, C, L, R> Executor<InstructionBatch<I>>
    for OnePerShotPlacedExecutor<I, S, M, P, T, C, L, R>
where
    I: Clone + Send + 'static,
    S: Send + 'static,
    M: MakeState<S>,
    P: ExecutorLogic<I, S> + Clone + Send + 'static,
    C: FnMut(&CallInput<'_, I>) -> Result<(), Rejected>,
    L: Clone + Send + 'static,
    R: Clone + Send + 'static,
    T: ExecutorPlacement + Clone,
{
    /// The replies of the shots' placements, merged.
    type Reply = ShotReplies<IncrementalReply<ShotOutcome>>;

    /// # Errors
    ///
    /// [`Rejected::Closed`] once closed, then
    /// [`Rejected::EarlierFailure`] after a call failed, from the moment
    /// its failure happened on a shot's placement, whatever shots this call
    /// names, with the earliest failed call's reason, whatever order the
    /// calls failed in; then the check's refusal ([`ExecutorFn::check`]); then
    /// [`Rejected::ShotNotHeld`] beyond the promise's
    /// [`max_shots`](Capabilities::max_shots), checked before any state is
    /// made, and the refusal of the closure that makes a state. Nothing ran in
    /// each case. A failure that happens while this call starts, after its
    /// failure check, refuses it with [`Rejected::EarlierFailure`] only when
    /// the failed shot comes first in the batch and the failure is recorded
    /// before that shot is given the call; otherwise the reply fails: with the
    /// failure named by the failed shot, when that shot refused the call, or
    /// with `an earlier call failed: …`, when that shot had queued the call
    /// behind the failing one; and the batch's other shots may have run.
    ///
    /// # Panics
    ///
    /// When the check or a state closure panics, or a `Clone` of the logic, of
    /// a report closure or of the placement, as a new shot's copies are made:
    /// the shot's state, when made, is then freed as the panic unwinds, its
    /// drop's panic absorbed; or a shot's placement as it starts, at the shot's
    /// first call ([`OwnThread`](crate::OwnThread) when the system cannot make
    /// a thread), or as it is handed a job ([`PlacedExecutor`]'s `call`); or,
    /// when the call is refused (a state closure's refusal) or fails as it is
    /// handed to a shot, the drop of a state made for the call whose placement
    /// never started (the first such drop's panic, once every such state is
    /// dropped, on the caller's thread). A shot fails as it is handed the call
    /// when its placement runs the job there and then (a placement that runs
    /// each job as it is handed); on [`OwnThread`](crate::OwnThread), only when
    /// the shot's thread fails before `call` reads its reply, a race. The panic
    /// reaches the caller, though the shots queued before it in the batch may
    /// run, and every later call is refused with [`Rejected::EarlierFailure`]:
    /// with the failed logic's reason after a shot failed, otherwise with
    /// `the executor panicked during the call`.
    fn call(&mut self, batch: InstructionBatchRef<'_, I>) -> Result<Self::Reply, Rejected> {
        let Self {
            seed,
            promise,
            make_state,
            logic,
            check,
            log_probability,
            state_report,
            placement,
            shots,
            parts_failure,
        } = self;
        // Checked before any state is made, but refused only where the
        // states are made: after the refusals of the lifecycle and check.
        let new = (batch.shots.as_slice().iter().copied()).filter(|&j| !shots.holds(j));
        let mut room = Some(check_room(promise, shots.len(), new));
        shots.call(
            batch,
            |instructions| check(&CallInput { instructions }),
            |j| {
                room.take().unwrap_or(Ok(()))?;
                let shot =
                    shot_executor(promise, &**logic, log_probability.as_ref(), &**state_report);
                let made = make_state(ShotContext::in_run(*seed, j))?;
                Ok(PlacedExecutor::part_of(
                    shot((j, made)),
                    T::clone(placement),
                    parts_failure.clone(),
                    ExecutorFn::free_raising,
                ))
            },
            PlacedExecutor::drop_here_if_unstarted,
        )
    }
}

impl<I, S, M, P, T, C, L, R> Describe for OnePerShotPlacedExecutor<I, S, M, P, T, C, L, R> {
    /// The promise as given, but with `log_probability` exactly when it is
    /// given how to read a state's log probability
    /// ([`ExecutorFn::log_probability`]), whatever the promise says, and
    /// the run's seed it is given ([`ExecutorFn::seed`]), none without it.
    fn capabilities(&self) -> Capabilities {
        described(&self.promise, self.log_probability.as_ref(), self.seed)
    }
}

impl<I, S, M, P, T, C, L, R, Q> Close for OnePerShotPlacedExecutor<I, S, M, P, T, C, L, R>
where
    I: Send + 'static,
    S: Send + 'static,
    P: Send + 'static,
    L: FnMut(&S) -> f64 + Send + 'static,
    R: FnMut(&S) -> Q + Send + 'static,
    Q: Fold + Send,
    T: ExecutorPlacement,
{
    type Report = ExecutorReport<Q>;

    /// Refuses later calls, waits for the calls queued on each shot's
    /// placement, then closes there the executor of each shot it reports,
    /// in the order of the shots' first calls, and drops the others unread,
    /// on their placements, waiting until they are dropped, so that what
    /// every state holds (a device, GPU memory) is free when it returns, as
    /// in the other order; it reports the calls accepted since the last
    /// report, and each reported state's report
    /// ([`ExecutorFn::state_report`]) and log probability, when it was
    /// given how to read it, once; a state whose placement stopped gives
    /// neither.
    ///
    /// # Panics
    ///
    /// When a shot's placement panics as it is handed a job at close
    /// ([`PlacedExecutor`]'s `close`): the shots not closed yet are then
    /// dropped before the panic leaves `close`, not as it unwinds, so that a
    /// placement that panics too as their drops hand it a job (against
    /// [`ExecutorPlacement`]'s rule) does not abort the process, and the first
    /// panic is the one raised. When a report closure (`log_probability`'s or
    /// `state_report`'s), or the drop of a state it reports, panics on a shot's
    /// placement: that panic is raised again here, its payload unchanged, as
    /// when not placed (the type's doc), once the shots not closed yet were
    /// dropped on their placements, unread, so when several shots' closures or
    /// drops would panic, the first shot's panic is raised. So too when the
    /// report's `Q::default` panics, here or on a shot's placement, outside an
    /// unwind: every call accepted has ended first, none cancelled, and every
    /// shot not closed yet is dropped on its placement, and waited for, before
    /// the panic leaves `close`. A later `close` reports no shot's state (it
    /// calls `Q::default` again, which may panic again). A state whose drop
    /// panics as `close` frees it, left out of the report, panics on its
    /// placement, not here: the panic hook reports it, and `close` goes on;
    /// unless that shot's placement stopped (a call panicked on it): the state
    /// is then dropped here, on the closer's thread, and the first such panic
    /// leaves `close` once the other shots are freed. Not placed, such a state
    /// is dropped in the call that failed, whose caller the first such drop's
    /// panic reaches ([`OnePerShotExecutor`](crate::OnePerShotExecutor)). Never
    /// for the drop of what a shot's copies of the closures, or its placement,
    /// capture: each is absorbed, as `drop` does, both where `close` frees it
    /// (a shot's placement, and the copies too of a shot whose placement
    /// stopped or whose state it leaves out) and where the shot's placement
    /// frees it (the copies of a shot it reports, which `close` does not wait
    /// for; the placement of such a shot `close` frees).
    fn close(&mut self) -> ExecutorReport<Q> {
        let report = self.shots.close(
            PlacedExecutor::wait_for_calls,
            |_| None,
            PlacedExecutor::drop_and_wait,
        );
        ExecutorReport {
            calls: report.calls,
            log_p: report.states.log_p,
            states: report.states.states,
        }
    }
}
