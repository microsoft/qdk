// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The facility that makes an executor from its logic alone
//! ([`executor_fn`]): what a call brings to the logic ([`CallInput`]),
//! where the logic puts its outcomes ([`Outcomes`]), the executor made
//! ([`ExecutorFn`]), which keeps the protocol's rules, and how it holds
//! its shots' states ([`ShotStrategy`]).

mod one_per_shot;

use std::marker::PhantomData;
use std::mem;
use std::slice;

pub use one_per_shot::{OnePerShot, OnePerShotExecutor, OnePerShotPlacedExecutor};

#[cfg(doc)]
use crate::OwnThread;
use crate::facilities::executor_lifecycle::failing_on_panic;
use crate::facilities::teardown::{DroppedOneByOne, HeldPart, HeldStates};
use crate::{
    Capabilities, Close, Describe, Executor, ExecutorLifecycle, ExecutorReport, Fold,
    ImmediateReply, InstructionBatch, InstructionBatchRef, PlacedExecutor, Placement, Rejected,
    ShotConditions, ShotContext, ShotId, ShotOutcome, Value,
};

/// An executor made from its promise, what makes the state its calls run
/// on and the logic that runs them, by [`executor_fn`]: the implementer
/// writes its own logic, and the executor keeps the protocol's rules.
///
/// Without a shot strategy ([`ExecutorFn::shots`]) it holds one state, for
/// one shot:
///
/// - it describes its promise lowered to one shot
///   ([`Capabilities::max_shots`]), so that a job of more shots is refused
///   when it is given to a chain ([`Chain::run`](crate::Chain::run)); and
///   with [`log_probability`](Capabilities::log_probability) exactly when
///   it is given how to read a state's log probability
///   ([`ExecutorFn::log_probability`]), whatever the promise says;
/// - it makes the state at its first call, from the context of that call's
///   shot in the run whose seed it is given ([`ExecutorFn::seed`]), as
///   every shot form does, and keeps it for the shot's later calls until it
///   closes, which drops it;
/// - it refuses a call that names another shot
///   ([`Rejected::ShotNotHeld`]), so that the rule holds when it is called
///   directly too, not only in a run; and, promised no shot
///   ([`Capabilities::max_shots`] `Some(0)`), it refuses the first call's
///   first shot before it makes a state, as [`OnePerShot`] does;
/// - a panic inside `call` (the check's, the state closure's or the
///   logic's) reaches the caller and fails the executor, as a `Mutex` is
///   poisoned, since it may have left the state part-way through a batch:
///   every later call is refused (`the executor panicked during the
///   call`); a state made for that call is dropped as the panic leaves,
///   and a panic of its drop, which would abort the process there, is left
///   to the panic hook, which reports it;
/// - `drop` never panics: a state's drop that panics as the executor is
///   dropped, the implementer's code, is absorbed, after the panic hook
///   reported it (Rust API Guidelines, C-DTOR-FAIL); call `close` to see it. So
///   is the drop of what its closures capture, each closure in a catch of its
///   own; `close` raises only the state's. It drops the state before its
///   closures, as `close` frees it first, so that a state whose drop
///   reaches its device through what a closure holds (a weak handle to the
///   device's connection, say) still finds it there;
/// - it refuses every call once closed or once a call failed
///   ([`ExecutorLifecycle`]), and so does every wrapper put on it
///   ([`ExecutorFn::shots`], [`ExecutorFn::run_on`]), in any order: a
///   wrapper never forgets a refusal, since it continues what it wraps,
///   failures included, as iterator adapters and Tower's `Buffer` do;
/// - its report counts the calls it accepted ([`ExecutorReport::calls`]),
///   and gives the state's report, when it is given how to make one
///   ([`ExecutorFn::state_report`]), except a state its failed first call
///   made: a failed call reports no shot of it.
///
/// Where it runs is given by [`ExecutorFn::run_on`]; by default, on the
/// caller's thread, inside `call`.
///
/// # Type parameters
///
/// - `I`: the instructions it runs.
/// - `S`: the state a shot's calls run on.
/// - `M`: what makes the state ([`MakeState`]), given to [`executor_fn`].
/// - `P`: the logic ([`ExecutorLogic`]), given to [`executor_fn`].
/// - `C`: the check run on each call before the state is made
///   ([`ExecutorFn::check`]); by default [`NoCheck`], which accepts every
///   call.
/// - `L`: what reads a state's log probability
///   ([`ExecutorFn::log_probability`]); by default none is held, so the
///   executor offers no `log_probability` and reports no log probability.
/// - `R`: what makes a state's report ([`ExecutorFn::state_report`]); by
///   default one that makes `()`, so the report's
///   [`states`](ExecutorReport::states) is `()`.
pub struct ExecutorFn<I, S, M, P, C = NoCheck<I>, L = fn(&S) -> f64, R = fn(&S)> {
    promise: Capabilities,
    /// The run's seed, which each shot's state is made from; `None` until
    /// [`ExecutorFn::seed`] gives it.
    seed: Option<u64>,
    /// The state, once made, and the shot it was made for: one at most.
    /// Dropped with the executor in a catch, its panic absorbed, so that
    /// `drop` never panics, during an unwind too; `close` raises it.
    /// Declared before the closures, so that it is dropped first (the
    /// reason is at `HeldPart`;
    /// `dropped_it_frees_its_state_before_its_closures`, and for a shot's
    /// executor on its placement,
    /// `dropped_each_shot_frees_its_state_before_its_copies_of_the_closures`).
    held: HeldStates<(ShotId, S)>,
    make_state: HeldPart<M>,
    logic: HeldPart<P>,
    check: HeldPart<C>,
    log_probability: HeldPart<Option<L>>,
    /// Makes a state's report; without one, the report is `()`.
    state_report: HeldPart<R>,
    calls: u64,
    lifecycle: ExecutorLifecycle,
    instructions: PhantomData<fn(&[I])>,
}

/// The executor whose calls run `logic` on a state that `make_state`
/// makes, and which offers what `promise` states, changed as below
/// ([`ExecutorFn`]).
///
/// - `promise` is what the executor offers ([`Describe`]), changed in two
///   ways only: it offers [`log_probability`](Capabilities::log_probability)
///   exactly when [`ExecutorFn::log_probability`] gives it how to read a
///   state's, whatever `promise` says; and without a shot strategy
///   ([`ExecutorFn::shots`]), it lowers [`max_shots`](Capabilities::max_shots)
///   to at most one.
/// - `make_state` makes the state a shot's calls run on, the executor's
///   working state (a simulator's qubits), from the context of the shot it
///   is made for, lazily, at the first call that needs it
///   ([`MakeState`]). A state of an `executor_fn` always belongs to one
///   shot: the one-state form makes one, for a single shot, and
///   [`OnePerShot`] one per shot. Its `Err` refuses the call, which then
///   runs nothing and leaves no state made, and a later call tries again,
///   for its own shots (any shot, while the one-state form holds none):
///   for example [`Rejected::Unavailable`] when its device is busy.
/// - `logic` runs a call's instructions on the state ([`CallInput`]),
///   and pushes each readout to [`Outcomes`]. Its `Err` fails the call: the
///   outcomes pushed before it are handed over, then the reply ends with its
///   reason, and the executor refuses every later call, since the state is
///   then part-way through a batch.
///
/// The closures' argument types are inferred from these bounds, so neither
/// needs annotations; a plain function or a method is passed by name. The
/// state comes before the logic so that its type is known when the logic
/// is checked.
///
/// # Example
///
/// An executor of a device of one bit, whose logic flips and reads it, called
/// on shot 0 (copied from the crate's tests of `executor_fn`, with the items
/// they use):
///
/// ```
/// use executor_protocol::{
///     CallInput, Capabilities, Executor, InstructionBatchRef, Outcome,
///     Outcomes, Rejected, ShotContext, ShotOutcome, ShotSet, TypedInstruction,
///     Value, executor_fn, wait,
/// };
///
/// /// An instruction of the tests' one-bit device.
/// #[derive(Clone, Copy, Debug)]
/// enum Op {
///     /// Flips the bit.
///     Flip,
///     /// Reads the bit, under a tag.
///     Read(u32),
///     /// Where the device stops: the logic fails.
///     Stop,
/// }
///
/// impl TypedInstruction for Op {}
///
/// /// A shot's state: one bit, zero when made.
/// struct Bit {
///     one: bool,
/// }
///
/// impl Bit {
///     #[expect(
///         clippy::unnecessary_wraps,
///         reason = "the form of a state closure, passed by name"
///     )]
///     fn new(_: ShotContext) -> Result<Self, Rejected> {
///         Ok(Self { one: false })
///     }
/// }
///
/// /// The logic: flips and reads the bit; fails at `Stop`, after the
/// /// readouts before it.
/// fn run_bit(
///     bit: &mut Bit,
///     input: &CallInput<'_, Op>,
///     outcomes: &mut Outcomes<'_>,
/// ) -> Result<(), String> {
///     for op in input.instructions() {
///         match *op {
///             Op::Flip => bit.one = !bit.one,
///             Op::Read(tag) => {
///                 outcomes.push(Outcome::new(tag, bit.one.into()));
///             }
///             Op::Stop => return Err("the device stopped".to_owned()),
///         }
///     }
///     Ok(())
/// }
///
/// /// The promise of the tests' device: no limit stated.
/// fn promise() -> Capabilities {
///     Capabilities::default()
/// }
///
/// let mut executor = executor_fn(promise(), Bit::new, run_bit);
/// let reply = executor
///     .call(InstructionBatchRef {
///         shots: &ShotSet::one(0),
///         instructions: &[Op::Flip, Op::Read(0)],
///     })
///     .expect("the batch runs");
/// let mut outcomes = Vec::new();
/// wait(reply, &mut outcomes).expect("the logic does not fail");
/// assert_eq!(outcomes, [ShotOutcome::new(0, 0, Value::One)]);
/// ```
///
/// # Type parameters
///
/// - `I` … `P`: as [`ExecutorFn`]'s, its options at their defaults.
pub fn executor_fn<I, S, M, P>(
    promise: Capabilities,
    make_state: M,
    logic: P,
) -> ExecutorFn<I, S, M, P>
where
    M: MakeState<S>,
    P: ExecutorLogic<I, S>,
{
    ExecutorFn {
        promise,
        seed: None,
        held: HeldStates::default(),
        make_state: HeldPart::new(make_state),
        logic: HeldPart::new(logic),
        check: HeldPart::new(|_| Ok(())),
        log_probability: HeldPart::new(None),
        state_report: HeldPart::new(|_| ()),
        calls: 0,
        lifecycle: ExecutorLifecycle::default(),
        instructions: PhantomData,
    }
}

impl<I, S, M, P, C, L, R> ExecutorFn<I, S, M, P, C, L, R> {
    /// This executor, refusing the calls `check` refuses, before their state is
    /// made and before the logic runs: what the logic cannot run whatever the
    /// state, such as a qubit beyond the device's, or a batch that repeats a
    /// readout's tag ([`check_unique_tags`](crate::check_unique_tags)). It
    /// replaces the check given before, if any, freeing it, and raising no
    /// panic of its drop, as `drop` does; without one, every call is accepted.
    /// Give it before the first call: a call made before it is not checked.
    #[must_use]
    pub fn check<V>(self, check: V) -> ExecutorFn<I, S, M, P, V, L, R>
    where
        V: FnMut(&CallInput<'_, I>) -> Result<(), Rejected>,
    {
        ExecutorFn {
            promise: self.promise,
            seed: self.seed,
            held: self.held,
            make_state: self.make_state,
            logic: self.logic,
            check: HeldPart::new(check),
            log_probability: self.log_probability,
            state_report: self.state_report,
            calls: self.calls,
            lifecycle: self.lifecycle,
            instructions: PhantomData,
        }
    }

    /// This executor, reading with `log_probability` the log probability of
    /// a state's shot, over every draw made for it: the only way to promise
    /// [`log_probability`](Capabilities::log_probability). It is read once per
    /// state reported ([`ExecutorFn::state_report`]), when the executor
    /// closes, into the report ([`ExecutorReport::log_p`]), and never on a
    /// state left out of it. It replaces the one given before, if any,
    /// freeing it as [`ExecutorFn::check`] does.
    #[must_use]
    pub fn log_probability<W>(self, log_probability: W) -> ExecutorFn<I, S, M, P, C, W, R>
    where
        W: FnMut(&S) -> f64,
    {
        ExecutorFn {
            promise: self.promise,
            seed: self.seed,
            held: self.held,
            make_state: self.make_state,
            logic: self.logic,
            check: self.check,
            log_probability: HeldPart::new(Some(log_probability)),
            state_report: self.state_report,
            calls: self.calls,
            lifecycle: self.lifecycle,
            instructions: PhantomData,
        }
    }

    /// This executor, making with `state_report` the report of each state it
    /// holds, when it closes, as the state's calls left it: what dies with the
    /// state, such as the largest bond dimension of an MPS, the weight it
    /// discarded, or a profiler's counts. Since the report sees each state as
    /// its calls left it, a state can track across its calls what it needs
    /// reported, such as a peak (the largest bond it reached, though a later
    /// call shrank it) or a count, and `state_report` reads it, by reference,
    /// once. Each state's report is folded once into the executor's
    /// ([`ExecutorReport::states`]), starting from the empty report, in an
    /// order not promised: by [`Fold`]'s rules, the total does not depend on
    /// it. It replaces the one given before, if any, freeing it as
    /// [`ExecutorFn::check`] does; without one, the report is `()`.
    ///
    /// Every state held at close is reported, as its calls left it, a failed
    /// one included, since the executor keeps it until it closes; but not a
    /// state made for a call that failed, or for a later one, whether its shot
    /// ran or not: a failed call reports no shot of it, so that every order of
    /// [`.shots`](ExecutorFn::shots) and [`.run_on`](ExecutorFn::run_on)
    /// reports the same states, as `futures`' `try_join_all` drops the others'
    /// results. Nor is a placed state whose placement stopped (its logic
    /// panicked, or a job was dropped unrun; [`PlacedExecutor`]). Neither
    /// `state_report` nor `log_probability` runs on a state left out, in any
    /// order. A shot that never ran holds no state, so it has no report, and a
    /// second `close` reports no state. Under
    /// `.run_on(placement).shots(OnePerShot)`, each shot's report is made on
    /// its placement once that placement started, by a copy of `state_report`
    /// (hence the bounds `Clone + Send` there, as for the logic); a state held
    /// from before it was placed, and not called since, is reported where it
    /// is, where its calls ran; should it panic there, `close` raises the panic
    /// again in its caller, as when not placed.
    ///
    /// # Example
    ///
    /// A simulator whose states count their flips, and report them under their
    /// shot (`Flips`, whose fold adds them shot by shot), runs shots 0 and 1,
    /// then shot 1 again; closing folds each state's report once. Excerpt of
    /// the crate's tests of a state per shot:
    ///
    /// ```ignore
    /// let mut executor = executor_fn(promise(), Flipped::new, count_flips)
    ///     .state_report(flips)
    ///     .shots(OnePerShot);
    /// run(&mut executor, &[0, 1], &[Op::Flip]);
    /// run(&mut executor, &[1], &[Op::Flip, Op::Flip]);
    /// assert_eq!(executor.close().states, Flips::of([(0, 1), (1, 3)]));
    /// ```
    #[must_use]
    pub fn state_report<K, Q>(self, state_report: K) -> ExecutorFn<I, S, M, P, C, L, K>
    where
        K: FnMut(&S) -> Q,
        Q: Fold,
    {
        ExecutorFn {
            promise: self.promise,
            seed: self.seed,
            held: self.held,
            make_state: self.make_state,
            logic: self.logic,
            check: self.check,
            log_probability: self.log_probability,
            state_report: HeldPart::new(state_report),
            calls: self.calls,
            lifecycle: self.lifecycle,
            instructions: PhantomData,
        }
    }

    /// This executor, whose states draw from the run's seed `seed`: shot `j`'s
    /// state is made from `ShotContext::new(seed, j)`, whose
    /// [`seed`](ShotContext::seed) depends only on `seed` and `j`, so that a
    /// shot draws the same values whatever the other shots of the run, and
    /// whatever the shot form: one state, [`OnePerShot`], placed either way. It
    /// replaces the seed given before, if any, for the states made after it: a
    /// state already made keeps the seed it was made from, so give the seed
    /// before the first call.
    ///
    /// Without it, no run's seed reaches the states: their context names
    /// the shot only, and drawing from it panics
    /// ([`ShotContext::run_seed`]), since a fixed seed made up for them
    /// would draw the same values in every run, and one drawn from the
    /// system could not be replayed; and an adapter that draws upstream of
    /// it is refused when built
    /// ([`BuildError::NoRunSeed`](crate::BuildError::NoRunSeed)). That
    /// serves logic that draws nothing, such as a device's or an existing
    /// tool's.
    ///
    /// # Example
    ///
    /// A simulator whose state is a value drawn from its shot's seed, given the
    /// run's seed, then a state per shot. Two lines of the crate's tests of
    /// `executor_fn`:
    ///
    /// ```ignore
    /// let drawing = || executor_fn(promise(), draw_value, read_drawn_bits).seed(SEED);
    /// run(&mut drawing().shots(OnePerShot), 4, &ops),
    /// ```
    #[must_use]
    pub fn seed(mut self, seed: u64) -> Self {
        self.seed = Some(seed);
        self
    }

    /// The executor that holds its shots' states as `strategy` says: a
    /// state per shot under [`OnePerShot`]. Given after the methods that
    /// shape the logic ([`ExecutorFn::seed`], [`ExecutorFn::check`],
    /// [`ExecutorFn::log_probability`], [`ExecutorFn::state_report`]),
    /// which the executor made keeps. It continues this executor: the state
    /// it holds, with the seed it was made from, its calls and its refusals,
    /// after close or a failure, since a wrapper never forgets a refusal.
    pub fn shots<H>(self, strategy: H) -> H::Executor
    where
        H: ShotStrategy<Self>,
    {
        strategy.hold_shots_of(self)
    }

    /// This executor, run where `placement` says ([`PlacedExecutor`]): on
    /// a thread of its own ([`OwnThread`]), or wherever a closure puts its
    /// work. Given after the methods that shape the logic, since it places
    /// the executor made so far whole. The order of `.run_on` and
    /// [`.shots`](ExecutorFn::shots) says where the shots run:
    /// `.shots(OnePerShot).run_on(OwnThread)` runs every shot on the
    /// one thread, and `.run_on(OwnThread).shots(OnePerShot)` each
    /// shot on a thread of its own ([`PlacedExecutor::shots`]). The order
    /// also says where a refusal arrives: under `.shots(..).run_on(..)`,
    /// the executor's refusals (its check's, such as a tag repeated within
    /// a batch, the one for want of room, its state closure's) arrive on
    /// its thread, as a failure that fails the executor
    /// ([`PlacedExecutor`]), while under `.run_on(..).shots(..)` they are
    /// refused at `call`. It
    /// continues this executor's refusals: placed after close or a failure,
    /// it refuses the next call inside `call`, since a wrapper never forgets
    /// a refusal.
    pub fn run_on<T: Placement>(self, placement: T) -> PlacedExecutor<Self, T> {
        let lifecycle = self.lifecycle.continued();
        PlacedExecutor::new(self, placement, lifecycle, Self::free_raising)
    }

    /// Frees it, raising the panic of its state's drop, which its `drop`
    /// absorbs (it still absorbs the drops of what its closures capture):
    /// for a placed executor that frees it on the caller's thread, as
    /// [`PlacedExecutor`] does in `close` once its placement stopped, and,
    /// placed per shot ([`OnePerShot`]), in `drop_and_wait` (a shot's
    /// placement stopped, or never started) and `drop_here_if_unstarted`
    /// (the states of a call refused, or failed, whose placements never
    /// started).
    fn free_raising(mut self) {
        drop(self.held.take_raising_first());
    }
}

/// How an executor holds its shots' states, given to
/// [`ExecutorFn::shots`]: a value, such as [`OnePerShot`], that
/// makes the executor `E` into one that holds them so.
///
/// Planned, none built: a state per distinct history of the values drawn
/// (a grouped simulator, for many shots of one program that rarely
/// diverge), and shots split over several executors (a run spread over the
/// cores, or over devices). A strategy from another crate can wrap `E`
/// whole; one that needs the executor's parts (the state closure, the logic)
/// waits for a way to take them, not built.
pub trait ShotStrategy<E> {
    /// The executor made.
    type Executor;

    /// `executor`, made to hold its shots' states as this strategy says.
    fn hold_shots_of(self, executor: E) -> Self::Executor;
}

impl<I, S, M, P, C, L, R> Executor<InstructionBatch<I>> for ExecutorFn<I, S, M, P, C, L, R>
where
    M: MakeState<S>,
    P: ExecutorLogic<I, S>,
    C: FnMut(&CallInput<'_, I>) -> Result<(), Rejected>,
{
    /// The logic runs inside `call`.
    type Reply = ImmediateReply<ShotOutcome>;

    /// # Errors
    ///
    /// [`Rejected::Closed`] once closed, then
    /// [`Rejected::EarlierFailure`] after a call failed; then the check's
    /// refusal ([`ExecutorFn::check`]); then [`Rejected::ShotNotHeld`] for
    /// the first call's first shot when the promise holds no shot
    /// ([`Capabilities::max_shots`] `Some(0)`), or for a shot other than the
    /// one it holds (the first call's first), and the refusal of the
    /// closure that makes the state: the order of [`OnePerShotExecutor`]'s
    /// refusals. Nothing ran in each case.
    ///
    /// # Panics
    ///
    /// When the check, the state closure or the logic panics, or the drop
    /// of a state made for a call whose logic failed: the panic reaches
    /// the caller, and every later call is refused with
    /// [`Rejected::EarlierFailure`]: with the logic's reason when it
    /// failed (the first failure is kept), else `the executor panicked
    /// during the call`.
    fn call(&mut self, batch: InstructionBatchRef<'_, I>) -> Result<Self::Reply, Rejected> {
        failing_on_panic(self, |this| &this.lifecycle, |this| this.serve(batch))
    }
}

impl<I, S, M, P, C, L, R> ExecutorFn<I, S, M, P, C, L, R>
where
    M: MakeState<S>,
    P: ExecutorLogic<I, S>,
    C: FnMut(&CallInput<'_, I>) -> Result<(), Rejected>,
{
    /// Serves `batch`: the steps of `call`, which runs them failing the
    /// executor should they panic ([`failing_on_panic`]).
    fn serve(
        &mut self,
        batch: InstructionBatchRef<'_, I>,
    ) -> Result<ImmediateReply<ShotOutcome>, Rejected> {
        self.lifecycle.admit()?;
        let input = CallInput {
            instructions: batch.instructions,
        };
        (self.check)(&input)?;
        let shot = if let Some(&(held, _)) = self.held.first() {
            held
        } else {
            let first = batch.shots.as_slice()[0];
            // Before the check that the batch names one shot, so that a
            // promise of no shot refuses the first shot, as
            // `OnePerShotExecutor` does.
            check_room(&self.promise, 0, std::iter::once(first))?;
            first
        };
        batch.shots.check_only(shot)?;
        // Kept only once its call succeeds: a failed call reports no state
        // made for it. Should the logic panic, its panic goes on, rather
        // than meet that of the state's drop (`DroppedOneByOne`).
        let mut made = None;
        let state = if let Some((_, state)) = self.held.first_mut() {
            state
        } else {
            let state = (self.make_state)(ShotContext::in_run(self.seed, shot))?;
            let made = made.insert(DroppedOneByOne::from(vec![(shot, state)]));
            &mut made[0].1
        };
        self.calls += 1;
        let mut reply = ImmediateReply::default();
        let mut outcomes = Outcomes::new(slice::from_ref(&shot), &mut reply);
        if let Err(reason) = (self.logic)(state, &input, &mut outcomes) {
            fail(&mut self.lifecycle, &mut reply, reason);
            return Ok(reply);
        }
        if let Some(mut made) = made {
            self.held.append(&mut made);
        }
        Ok(reply)
    }
}

impl<I, S, M, P, C, L, R> Describe for ExecutorFn<I, S, M, P, C, L, R> {
    /// The promise, lowered to one shot, with `log_probability` exactly when
    /// it is given how to read a state's log probability
    /// ([`ExecutorFn::log_probability`]), whatever the promise says.
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            max_shots: Some(self.promise.max_shots.map_or(1, |max| max.min(1))),
            ..described(&self.promise, self.log_probability.as_ref(), self.seed)
        }
    }
}

impl<I, S, M, P, C, L, R, Q> Close for ExecutorFn<I, S, M, P, C, L, R>
where
    L: FnMut(&S) -> f64,
    R: FnMut(&S) -> Q,
    Q: Fold,
{
    type Report = ExecutorReport<Q>;

    /// Refuses later calls, drops the state, and reports the calls accepted
    /// since the last report and, when a state was made, its report
    /// ([`ExecutorFn::state_report`]) and its log probability, when it was
    /// given how to read it.
    ///
    /// # Panics
    ///
    /// When `log_probability`, `state_report` or the report's `Q::default`
    /// panics, or the drop of the state it reports, outside an unwind: the
    /// panic reaches the caller, placed or not ([`PlacedExecutor`]), whereas
    /// `drop` absorbs it. The state is taken first, so it is freed before the
    /// panic leaves, and a later `close` reports no state (it calls
    /// `Q::default` again, which may panic again). A panic of that drop while
    /// the panic of a report closure or of `Q::default` unwinds is left to the
    /// panic hook.
    fn close(&mut self) -> ExecutorReport<Q> {
        // Known issue, open (it needs two bugs at once; blocks neither this
        // commit nor a later one): run from a `drop` while another panic
        // unwinds, a panic of a report closure or of `Q::default` here
        // aborts the process, placed or not (as the panic leaves that
        // `drop`); the `# Panics` promise holds outside an unwind only.
        let calls = start_close(&mut self.lifecycle, &mut self.calls);
        // Taken before the report is made, whose `Q::default` is the
        // implementer's code, so that the state is freed whatever panics;
        // should a report closure panic, its panic goes on, rather than meet
        // that of the state's drop (`DroppedOneByOne`).
        let held = self.held.take_raising_first();
        let mut report = ExecutorReport::of_calls(calls);
        if let Some(held) = held.first() {
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

/// `promise` as every form of an [`executor_fn`] describes it: with
/// [`log_probability`](Capabilities::log_probability) exactly when it is given
/// how to read a state's log probability (`log_probability`), and the run's
/// `seed` it is given, which adapters upstream that draw derive their seeds
/// from.
fn described<L>(
    promise: &Capabilities,
    log_probability: Option<&L>,
    seed: Option<u64>,
) -> Capabilities {
    Capabilities {
        log_probability: log_probability.is_some(),
        run_seed: seed,
        ..promise.clone()
    }
}

/// Checks that an executor of `promise` holding `held` states has room for
/// the states of `new`, the shots new to it: the rule of every form of an
/// [`executor_fn`] (one state; [`OnePerShot`], placed or not), checked
/// before any state is made, so that a call refused for room runs no state
/// closure. `new` is not read when the promise does not bound the states.
///
/// # Errors
///
/// [`Rejected::ShotNotHeld`] with the first shot of `new` past the
/// promise's [`max_shots`](Capabilities::max_shots).
fn check_room(
    promise: &Capabilities,
    held: usize,
    mut new: impl Iterator<Item = ShotId>,
) -> Result<(), Rejected> {
    let Some(max_shots) = promise.max_shots else {
        return Ok(());
    };
    let room = u64::try_from(held).map_or(0, |held| max_shots.saturating_sub(held));
    // Past `usize::MAX` new shots cannot come: a batch is a slice.
    let room = usize::try_from(room).unwrap_or(usize::MAX);
    match new.nth(room) {
        Some(shot) => Err(Rejected::ShotNotHeld(shot)),
        None => Ok(()),
    }
}

/// Fails `reply` with `reason`, the logic's, after the outcomes it
/// holds, and `lifecycle` with it, so that every later call is refused:
/// every form's failed call (placed per shot, in each shot's executor).
fn fail(
    lifecycle: &mut ExecutorLifecycle,
    reply: &mut ImmediateReply<ShotOutcome>,
    reason: String,
) {
    lifecycle.fail(&reason);
    reply.fail(reason);
}

/// Refuses later calls, and takes the calls accepted since the last
/// report, which it returns. Called by every form's close (placed per
/// shot, each shot's executor's) before that close takes its states and
/// makes its report (`ExecutorReport::of_calls`), whose `Q::default` may
/// panic.
fn start_close(lifecycle: &mut ExecutorLifecycle, calls: &mut u64) -> u64 {
    lifecycle.close();
    mem::take(calls)
}

/// Adds to `report` what a state held at close gives, `held` being the
/// state and its shot: the state's log probability, when it is given how
/// to read it, and the state's report, folded. Every form's close calls it
/// once per state it holds (placed per shot, each shot's executor's).
fn report_state<S, L, R, Q>(
    report: &mut ExecutorReport<Q>,
    (shot, state): &(ShotId, S),
    log_probability: Option<&mut L>,
    state_report: &mut R,
) where
    L: FnMut(&S) -> f64,
    R: FnMut(&S) -> Q,
    Q: Fold,
{
    if let Some(log_probability) = log_probability {
        report.log_p.add(*shot, log_probability(state));
    }
    report.states.fold(state_report(state));
}

/// The check of an [`executor_fn`], or of an
/// [`adapter_fn`](fn@crate::adapter_fn), given none ([`ExecutorFn::check`]): it
/// accepts every call of instructions of type `I`. A function pointer, so that
/// a function returning the facility's type names it, where a later option's
/// closure follows it: `-> OnePerShotExecutor<I, S, impl MakeState<S>, impl
/// ExecutorLogic<I, S>, NoCheck<I>, impl FnMut(&S) -> f64>` for one given
/// `.log_probability(..)`.
pub type NoCheck<I> = fn(&CallInput<'_, I>) -> Result<(), Rejected>;

/// What makes the state of an [`executor_fn`], or of an
/// [`adapter_fn`](fn@crate::adapter_fn): their `make_state` closure. It makes
/// the component's working state, of type `S` (a simulator's qubits, a
/// profiler's counts, a decoder's history) from the [`ShotContext`] of the shot
/// it starts with, lazily, at the first call that needs it; its refusal refuses
/// that call and leaves nothing made. An `executor_fn`'s state always belongs
/// to one shot (one state, for a single shot; under [`OnePerShot`], one per
/// shot), and so does an `adapter_fn`'s under [`OnePerShot`]; a one-state
/// `adapter_fn` makes one state, from its first call's first shot, and keeps it
/// for every shot, as a profiler's counts sum all its shots. Hence not
/// `make_shot_state`: that last state is not a shot's, and the name would
/// suggest it is made anew for each shot.
///
/// Implemented for every closure of its signature, so that
/// a function returning the facility's type names the closure in a word,
/// as `TryFuture` names a `Future` of a `Result`: `-> OnePerShotExecutor<I,
/// S, impl MakeState<S>, impl ExecutorLogic<I, S>>`, which `>>` composes
/// where a bare `impl Executor` does not (see [`compose`](fn@crate::compose)).
/// A function that takes a reference adds `+ use<>` to each `impl`, so that
/// the closures borrow none of it and the chain may move to another thread.
pub trait MakeState<S>: FnMut(ShotContext) -> Result<S, Rejected> {}

impl<S, F: FnMut(ShotContext) -> Result<S, Rejected>> MakeState<S> for F {}

/// What runs a call's instructions, of type `I`, on the state of an
/// [`executor_fn`], of type `S`, and pushes its readouts: its logic.
/// Implemented for every closure of its signature, to name it in a function's
/// return type, as [`MakeState`] is, `+ use<>` included. An executor given
/// `.run_on(..)` then `.shots(OnePerShot)`, a placement per shot, copies its
/// logic to each shot's placement, so its function names it `impl
/// ExecutorLogic<I, S> + Clone`: `-> OnePerShotPlacedExecutor<I, S, impl
/// MakeState<S>, impl ExecutorLogic<I, S> + Clone, OwnThread>`.
pub trait ExecutorLogic<I, S>:
    FnMut(&mut S, &CallInput<'_, I>, &mut Outcomes<'_>) -> Result<(), String>
{
}

impl<I, S, F> ExecutorLogic<I, S> for F where
    F: FnMut(&mut S, &CallInput<'_, I>, &mut Outcomes<'_>) -> Result<(), String>
{
}

/// What a call brings to the logic of an [`executor_fn`], or to the logic of an
/// [`adapter_fn`](fn@crate::adapter_fn): its instructions, of type `I`,
/// borrowed for `'a`. More may come by methods (the shots of a call that runs
/// several on one state), so that no logic breaks.
#[derive(Debug)]
pub struct CallInput<'a, I> {
    pub(crate) instructions: &'a [I],
}

impl<'a, I> CallInput<'a, I> {
    /// The call's instructions, in order.
    #[must_use]
    pub fn instructions(&self) -> &'a [I] {
        self.instructions
    }
}

/// One readout without its shot: its tag and its value. The logic of an
/// [`executor_fn`](fn@crate::executor_fn) pushes it
/// ([`Outcomes`](crate::Outcomes)), which names it by the shot the logic
/// runs on, so that the logic cannot name the wrong one. Like a
/// [`ShotOutcome`], it carries no probability.
///
/// It is non-exhaustive, as [`ShotOutcome`] is, so that a field may be
/// added without breaking the crates that use it. Another crate makes one
/// with [`Outcome::new`], with no condition, sets its conditions with
/// [`with_conditions`](Self::with_conditions), and matches it with `..`:
///
/// ```
/// use executor_protocol::{Outcome, ShotConditions, Value};
///
/// let outcome = Outcome::new(0, Value::One)
///     .with_conditions(ShotConditions::UNRESOLVED);
/// let Outcome { tag, conditions, .. } = outcome;
/// assert_eq!((tag, conditions), (0, ShotConditions::UNRESOLVED));
/// ```
///
/// A literal from another crate does not compile (`E0639`):
///
/// ```compile_fail,E0639
/// use executor_protocol::{Outcome, ShotConditions, Value};
///
/// let outcome = Outcome {
///     tag: 0,
///     value: Value::One,
///     conditions: ShotConditions::NONE,
/// };
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Outcome {
    /// The batch's name for the readout ([`ShotOutcome::tag`]).
    pub tag: u32,
    /// What was read.
    pub value: Value,
    /// The conditions the logic raises with it on the shots it answers: the
    /// [`ShotOutcome::conditions`] of each outcome it makes, with those the
    /// logic raised in the call ([`Outcomes::raise_conditions`]).
    pub conditions: ShotConditions,
}

impl Outcome {
    /// The readout `tag`, which read `value`, with no condition.
    #[must_use]
    pub const fn new(tag: u32, value: Value) -> Self {
        Self {
            tag,
            value,
            conditions: ShotConditions::NONE,
        }
    }

    /// The same readout, with `conditions` in place of its own.
    #[must_use]
    pub const fn with_conditions(self, conditions: ShotConditions) -> Self {
        Self { conditions, ..self }
    }
}

/// Where the logic of an [`executor_fn`], or of an
/// [`adapter_fn`](fn@crate::adapter_fn), puts its readouts: straight into
/// the call's reply, so that nothing is copied after. An [`Outcome`] is
/// named by every shot its state serves in the call: an `executor_fn`'s
/// one shot; a one-state `adapter_fn`'s every shot of the call, since its
/// one state serves them all; with a state per shot
/// ([`OnePerShot`]), its own shot. A [`ShotOutcome`]
/// answers the one shot it names, among
/// those, as a decoder writes what it made of each shot's readouts.
///
/// Each outcome carries the conditions it is written with, and those the
/// logic raises on its shots in the call
/// ([`raise_conditions`](Self::raise_conditions)).
#[derive(Debug)]
pub struct Outcomes<'a> {
    shots: &'a [ShotId],
    reply: &'a mut ImmediateReply<ShotOutcome>,
    /// Where the logic's outcomes start in `reply`.
    from: usize,
    /// What the logic raised on its shots in the call.
    raised: ShotConditions,
}

impl<'a> Outcomes<'a> {
    /// Where a logic serving `shots` writes, after what `reply` holds.
    #[inline]
    pub(crate) fn new(shots: &'a [ShotId], reply: &'a mut ImmediateReply<ShotOutcome>) -> Self {
        let from = reply.items().len();
        Self {
            shots,
            reply,
            from,
            raised: ShotConditions::NONE,
        }
    }

    /// What the logic raised on its shots in the call
    /// ([`raise_conditions`](Self::raise_conditions)).
    pub(crate) fn raised(&self) -> ShotConditions {
        self.raised
    }

    /// Adds `outcome` after those pushed before it, once for each shot it
    /// answers, named by that shot, with its conditions and those the logic
    /// raised in the call.
    #[inline]
    pub fn push(&mut self, outcome: Outcome) {
        let conditions = outcome.conditions | self.raised;
        for &shot in self.shots {
            self.reply.push(ShotOutcome {
                shot,
                tag: outcome.tag,
                value: outcome.value,
                conditions,
            });
        }
    }

    /// Raises `conditions` on every shot it answers: every outcome the
    /// logic writes in the call carries them, those written before as well
    /// as after. An [`adapter_fn`](fn@crate::adapter_fn) also marks with
    /// them the outcomes it gives those shots in later calls on them (a
    /// refused call raises none), so that its logic may raise them in a
    /// call that writes none; a condition raised in a call that gives the
    /// shot no outcome reaches no caller in that call. An [`executor_fn`]
    /// keeps nothing across calls: its logic keeps its shot's conditions in
    /// its state, and raises them again in each call (`docs/replies.md`,
    /// §Shot conditions).
    pub fn raise_conditions(&mut self, conditions: ShotConditions) {
        if conditions.is_empty() {
            return;
        }
        self.raised |= conditions;
        for outcome in &mut self.reply.items_mut()[self.from..] {
            outcome.conditions |= conditions;
        }
    }
}

/// Adds `outcomes` in order, after those pushed before them, each as
/// [`Outcomes::push`] adds it: once for each shot it answers.
impl Extend<Outcome> for Outcomes<'_> {
    fn extend<It: IntoIterator<Item = Outcome>>(&mut self, outcomes: It) {
        for outcome in outcomes {
            self.push(outcome);
        }
    }
}

/// Adds `outcomes` in order, after those pushed before them, each for its
/// own shot alone: a logic's readouts of a shot it serves, such as those
/// it decodes from a run below ([`Below::run`](crate::Below::run)).
///
/// # Panics
///
/// When an outcome is of a shot the state does not serve in the call: a
/// logic writes the outcomes of its own shots only.
impl Extend<ShotOutcome> for Outcomes<'_> {
    fn extend<It: IntoIterator<Item = ShotOutcome>>(&mut self, outcomes: It) {
        // A call's shots are increasing as a rule: a search for each.
        //
        // Known issue, open (blocks neither): shots not increasing, as a
        // driver given `[6, 2, 9]` sends them, cost a pass over the call's
        // shots per outcome, quadratic in its shots for a decoder writing
        // each shot's. Not measured.
        let increasing = self.shots.is_sorted();
        for outcome in outcomes {
            let shot = outcome.shot;
            let served = if increasing {
                self.shots.binary_search(&shot).is_ok()
            } else {
                self.shots.contains(&shot)
            };
            assert!(
                served,
                "a logic writes outcomes of its own shots only, not of shot {shot}"
            );
            let conditions = outcome.conditions | self.raised;
            self.reply.push(outcome.with_conditions(conditions));
        }
    }
}
