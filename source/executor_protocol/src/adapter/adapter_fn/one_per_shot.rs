// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! An [`adapter_fn`](fn@crate::adapter_fn) with a state per shot
//! ([`OnePerShotAdapter`]), and the executor it gives upstream of one
//! ([`OnePerShotAdapterExecutor`]): the logic serves each shot of a call on
//! its shot's state, and the sends of the call's shots are merged, rank by
//! rank, into as few calls below as their contents allow ([`Merged`]).

use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use std::{mem, slice};

use super::owning::{CallDown, KeptState, MakeAdapterState, ServeAdapterCalls, WayDown};
use super::{
    AdapterFn, AdapterFnReply, CallsBelow, NoStateReport, free_then_close_below, report_states,
};
use crate::absorb::{DroppedOneByOne, HeldStates, absorb_each};
use crate::adapter::below::Route;
use crate::conditions::ConditionsSoFar;
use crate::lifecycle::failing_on_panic;
use crate::shot::ShotIndex;
use crate::{
    Adapter, Capabilities, Close, ComponentId, Describe, Executor, ExecutorLifecycle, Fold,
    ImmediateReply, Instruction, InstructionBatch, InstructionBatchRef, InstructionSet,
    LifecycleBoundReply, NoCheck, Outcomes, ProgramInput, Rejected, Reply, ShotId, ShotOutcome,
    ShotSet,
};
#[cfg(doc)]
use crate::{Below, OnePerShot, ReplyStatus};

/// An [`adapter_fn`](fn@crate::adapter_fn) under [`OnePerShot`]
/// ([`AdapterFn::shots`]): shot `j`'s state is made at the shot's first
/// call, from the context of `j` in the run, and kept for the shot's later
/// calls until the adapter closes, which drops them. It describes itself
/// as the adapter it was made from does.
///
/// A call runs the logic once per shot, in the batch's order, each on its
/// shot's state, with [`Outcomes`] that answer that shot alone and a
/// [`Below`] that keeps what the shot sends and runs what it runs on the
/// shot alone ([`Below::run`]). Once every shot's logic ran, the sends go
/// below, merged: a shot's `k`-th send is merged with the other shots'
/// `k`-th sends equal to it, into one call below over their shots, so
/// that an observer of a thousand shots makes the calls below that no
/// adapter would. Unequal sends go as calls of their own, each over
/// the shots that sent it: the `k`-th sends go before the `k + 1`-th, and
/// sends of one rank in the order of their first shots, so that each
/// shot's sends reach below in the order it made them. Below a replayable
/// executor, a shot gives the same outcomes alone or among others. Sends
/// of the call's own instructions, as an observer's, are merged with no
/// comparison; others are compared with the sends of their rank. With an
/// [`owning_adapter_fn`](fn@crate::owning_adapter_fn), each shot's state
/// owns its way below ([`OwnedBelow`](crate::OwnedBelow)), which keeps
/// and runs as that [`Below`] does; its sends are all compared, since it
/// does not know the call's instructions.
///
/// It keeps the rules of [`adapter_fn`](fn@crate::adapter_fn), applied to
/// the call's merged sends, and of a state per shot (the crate's
/// `docs/internals/adapters.md` lists the tests that pin them):
///
/// - a call is refused when closed ([`Rejected::Closed`]) and after a call
///   failed ([`Rejected::EarlierFailure`]), before anything runs; then
///   with the check's refusal ([`AdapterFn::check`]), once per call,
///   before any state is made and before any logic runs; then when the
///   state of a shot new to the adapter cannot be made, with the state
///   closure's refusal: the states of the shots new to the adapter are
///   made first, in the batch's order, before any logic runs;
/// - a [`Below::send`] returns `Ok` unless a run, or a send that went down
///   alone before it, ended the call: the send is kept, copied, to go below
///   merged once every shot's logic ran, or alone at the shot's run. The copies
///   are freed by the time the call ends, however it ends (a panic too; a
///   shot's sends that go alone, at its run), each instruction in a catch of
///   its own, the panic of its drop absorbed after the panic hook reported it,
///   as a placement absorbs its copy's: the call's reply is unharmed, and the
///   adapter goes on serving; should an instruction's copy panic, its panic
///   reaches the caller once the copies made before it are freed so;
/// - a [`Below::run`] goes below at once, on its shot alone, after the sends
///   the shot kept until then, each alone, so that the shot's calls below keep
///   its order; the shot's later sends rank anew, from its first, and are
///   merged with the other shots' sends of their rank. A kept send's panic
///   below comes as it goes below, at the shot's run, not at its send, so a
///   catch around the send alone does not catch it. Should a send alone panic
///   below, and the logic catch the panic, the sends the shot kept are kept no
///   more: those that went down went once, and the others are dropped unsent;
///   for a logic whose catch holds the calls from the send that panicked to the
///   run that sent it alone, the calls below are then those of a one-state
///   adapter, whose sends go down at once;
/// - the logic's `Err` on a shot fails the call and the adapter: the later
///   shots do not run, and nothing more goes down; the reply hands over
///   what the logic wrote and what the sends that went down alone, at a
///   run, read, then ends with the failure;
/// - a call whose first call below, a send or a run, is refused is refused with
///   the same [`Rejected`], and nothing else goes down; a refusal for an
///   earlier failure below ([`Rejected::EarlierFailure`]) fails the adapter
///   too; one refused after another went down, a run whose reply fails, is
///   cancelled or is not whole, and a send's reply that fails, fail the call
///   ([`ReplyStatus::Failed`]) and the adapter; a send's reply cancelled below
///   ends the call cancelled, unless it fails, and fails the adapter too. As
///   for [`adapter_fn`](fn@crate::adapter_fn), when a send's reply fails, or is
///   cancelled, depends on the executor below: below one that answers within
///   its call, the failure of a send, merged or alone, comes as it goes down,
///   and nothing more goes down; below one that answers later, the adapter sees
///   it by the next run, of any shot, or as the call ends, and what goes down
///   in between still does. An index the refusal names is that of the
///   instruction in the call below, a send or a run, unmapped for the caller of
///   a logic that sends other instructions than it is called with. A run, or a
///   send alone, that ends the call ends it at its shot: the later shots do not
///   run, and nothing more goes down. A merged send is refused once every
///   shot's logic ran: the states kept from earlier calls keep what the logic
///   changed in them ([`Below::send`]);
/// - the states made for a call that is refused, fails in it (the logic's
///   `Err`, a call below that did not go on, a send's reply that failed, a
///   panic) or is cancelled are dropped, whether their shot ran or not, as an
///   [`executor_fn`](fn@crate::executor_fn) under [`OnePerShot`] drops them;
///   those of a call whose send's reply fails only once the call returned are
///   kept, and reported, as the adapter learns of that failure only as the
///   call's reply is read ([`AdapterFn::state_report`]). The dropped states are
///   freed each in a catch of its own: should drops panic, the first one's
///   panic reaches the caller once all are freed, and the adapter fails, as an
///   executor's would; freed as another panic unwinds, they leave theirs to the
///   panic hook;
/// - should the check, `make_state` or the logic panic, the panic reaches the
///   caller, and the adapter fails (`the executor panicked during the
///   call`).
///
/// At close, each state is reported, if the adapter is given how
/// ([`AdapterFn::state_report`]), its report folded into the adapter's, and
/// dropped, each in a catch of its own; then the executor below is closed,
/// once, whatever failed: the report is its report, beside the adapter's if it
/// has one. The first panic, a report closure's or else a state drop's, reaches
/// the closer once the executor below is closed.
///
/// The adapter's executor, dropped rather than closed, never panics for the
/// adapter's parts: a state's drop that panics is absorbed, each state in a
/// catch of its own, after the panic hook reported it, during an unwind too;
/// call `close` to see the first one. So is the drop of what its closures
/// capture, each closure in a catch of its own, as with one state. The executor
/// below is dropped by its own rule.
///
/// # Type parameters
///
/// - `I` … `R`: as [`AdapterFn`]'s, from the adapter it was made from.
pub struct OnePerShotAdapter<I, O, S, M, L, C = NoCheck<I>, R = NoStateReport> {
    adapter: AdapterFn<I, O, S, M, L, C, R>,
}

impl<I, O, S, M, L, C, R> OnePerShotAdapter<I, O, S, M, L, C, R> {
    /// `adapter`, with a state per shot.
    pub(super) fn new(adapter: AdapterFn<I, O, S, M, L, C, R>) -> Self {
        Self { adapter }
    }
}

impl<I: Instruction, O: Instruction, S, M, L, C, R> Adapter
    for OnePerShotAdapter<I, O, S, M, L, C, R>
{
    type Accepts = I;
    type Sends = O;

    fn sends(&self) -> Option<Arc<InstructionSet>> {
        Adapter::sends(&self.adapter)
    }

    fn accepts(&self) -> Option<Arc<InstructionSet>> {
        Adapter::accepts(&self.adapter)
    }

    fn draws(&self) -> Option<ComponentId> {
        self.adapter.draws()
    }
}

/// An [`adapter_fn`](fn@crate::adapter_fn) with a state per shot
/// ([`OnePerShotAdapter`]) upstream of the executor `D`: the executor `>>`
/// gives. It offers what the adapter offers upstream, and reports what `D`
/// reports, beside its states' report if it is given how to make one
/// ([`AdapterFn::state_report`]).
///
/// # Type parameters
///
/// - `I` … `R`: as [`AdapterFn`]'s.
/// - `D`: as [`AdapterFnExecutor`](crate::AdapterFnExecutor)'s: the
///   executor below, as the adapter's calls reach it.
//
// Known issue, open (blocks neither): its states' bookkeeping (made first,
// kept once the call succeeds, an index by shot) repeats that of
// `OnePerShotExecutor`; one facility for both would hold the rule once.
//
// Known issue, open (blocks neither this commit nor a later one): the
// executor below, `D`, is dropped bare, not in a catch, as at
// `AdapterExecutor` and `AdapterFnExecutor`. No executor's `drop` panics,
// by the crate's rule, which its own executors keep, so only an
// implementer's executor that breaks it makes this `drop` panic;
// absorbing it would hide that bug.
// The replies from below are each held in a catch all the same
// (`HeldPart`), because a call holds several: two of them panicking in one
// drop would abort the process, where one `D` cannot.
pub struct OnePerShotAdapterExecutor<I, O, S, M, L, C, R, D> {
    /// The states kept, each with its shot, in the order their shots first
    /// ran: shot `j`'s at `index.get(j)`. Declared first, so that they are
    /// dropped before the closures (the reason is at `HeldPart`), and before
    /// the executor below, as `close` frees them before closing below
    /// (`dropped_it_frees_its_states_before_its_closures_and_the_executor_below`).
    states: HeldStates<(ShotId, KeptState<S>)>,
    adapter: AdapterFn<I, O, S, M, L, C, R>,
    capabilities: Capabilities,
    way_down: WayDown<O, D>,
    /// Whether a close reached the executor below
    /// ([`close_below_once`](super::close_below_once)).
    below_closed: bool,
    index: ShotIndex,
    /// The sends of the call being served, kept across calls so that its
    /// buffers are reused.
    merged: Merged<O>,
    /// The conditions of each shot so far, which mark its outcomes.
    so_far: ConditionsSoFar,
    lifecycle: ExecutorLifecycle,
}

impl<I, O, S, M, L, C, R, D> OnePerShotAdapterExecutor<I, O, S, M, L, C, R, D> {
    /// `adapter`, built to offer `capabilities`, upstream of `below`.
    pub(crate) fn new(
        adapter: OnePerShotAdapter<I, O, S, M, L, C, R>,
        capabilities: Capabilities,
        below: D,
    ) -> Self {
        Self {
            states: HeldStates::default(),
            adapter: adapter.adapter,
            capabilities,
            way_down: WayDown::new(below),
            below_closed: false,
            index: ShotIndex::default(),
            merged: Merged::default(),
            so_far: ConditionsSoFar::default(),
            lifecycle: ExecutorLifecycle::default(),
        }
    }
}

impl<I, O, S, M, L, C, R, D> Executor<InstructionBatch<I>>
    for OnePerShotAdapterExecutor<I, O, S, M, L, C, R, D>
where
    O: Clone + PartialEq,
    M: MakeAdapterState<S, O>,
    L: ServeAdapterCalls<S, I, O, D>,
    C: FnMut(&ProgramInput<'_, I>) -> Result<(), Rejected>,
    D: Executor<InstructionBatch<O>>,
{
    type Reply = LifecycleBoundReply<AdapterFnReply<D::Reply>>;

    /// Serves the call with the logic, shot by shot, by the rules of
    /// [`OnePerShotAdapter`].
    ///
    /// # Errors
    ///
    /// [`Rejected::Closed`] once closed, then [`Rejected::EarlierFailure`]
    /// after a call failed; then the check's refusal
    /// ([`AdapterFn::check`]); then the refusal of the closure that makes a
    /// state; then the refusal of the first call below, a send or a run.
    ///
    /// # Panics
    ///
    /// When the check, a state closure or the logic panics, or the drop of
    /// a state made for a call that is refused or fails (the first such
    /// drop's panic, once every state made for it is dropped): the panic
    /// reaches the caller, and every later call is refused with
    /// [`Rejected::EarlierFailure`].
    fn call(&mut self, batch: InstructionBatchRef<'_, I>) -> Result<Self::Reply, Rejected> {
        failing_on_panic(self, |this| &this.lifecycle, |this| this.serve(batch))
    }
}

impl<I, O, S, M, L, C, R, D> OnePerShotAdapterExecutor<I, O, S, M, L, C, R, D>
where
    O: Clone + PartialEq,
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
        let shots = batch.shots.as_slice();
        // The states of the shots new to the adapter, made in this order;
        // they join the adapter's states only once the call succeeds
        // (`CallEnd::keeps_states`), so that a refusal, a failure, a
        // cancellation or a panic drops every state made for the call,
        // whether its shot ran or not, each in a catch of its own.
        let mut made = self.make_new_states(shots)?;
        let mut merged = ForTheCall(&mut self.merged);
        let mut new = 0;
        let mut written = ImmediateReply::default();
        let mut calls = CallsBelow::new();
        let mut returned = Ok(());
        for &j in shots {
            let kept = if let Some(at) = self.index.get(j) {
                &mut self.states[at].1
            } else {
                new += 1;
                &mut made[new - 1].1
            };
            let down = CallDown {
                way: &mut self.way_down,
                calls: &mut calls,
            };
            let mut outcomes = Outcomes::new(slice::from_ref(&j), &mut written);
            returned =
                (self.adapter.logic).serve_shot(kept, &input, j, down, &mut merged, &mut outcomes);
            calls.take_in_raised(slice::from_ref(&j), &outcomes);
            if returned.is_err() || calls.ended() {
                break;
            }
        }
        if returned.is_ok() {
            merged.send(&mut calls, self.way_down.below(), batch.shots);
        }
        drop(merged);
        let ended = calls.end_call(
            &mut self.lifecycle,
            &mut self.so_far,
            shots,
            written,
            returned,
        );
        if ended.keeps_states {
            for (j, _) in made.iter() {
                self.index.push(*j);
            }
            self.states.append(&mut made);
        }
        ended.reply
    }

    /// Makes the state of each shot of `shots` the adapter does not hold,
    /// in the order of `shots`, with its shot; it keeps none of them: the
    /// caller does, once the call succeeds.
    ///
    /// # Errors
    ///
    /// The state closure's refusal, after which the states made by this
    /// call are dropped, each in a catch of its own, and the first drop's
    /// panic, if any, is raised instead.
    fn make_new_states(
        &mut self,
        shots: &[ShotId],
    ) -> Result<DroppedOneByOne<(ShotId, KeptState<S>)>, Rejected> {
        let mut made = DroppedOneByOne::default();
        for &j in shots {
            if self.index.get(j).is_none() {
                let context = self.adapter.context(&self.capabilities, j);
                made.push((
                    j,
                    self.way_down.make(&mut *self.adapter.make_state, context)?,
                ));
            }
        }
        Ok(made)
    }
}

impl<I, O, S, M, L, C, R, D> Describe for OnePerShotAdapterExecutor<I, O, S, M, L, C, R, D> {
    fn capabilities(&self) -> Capabilities {
        self.capabilities.clone()
    }
}

// Known issue, open (blocks a later unit: `.log_probability` on adapters):
// in both impls below, the report below passes up with its log
// probabilities, which miss the adapter's draws when it draws
// (`AdapterFn::drawing`).
impl<I, O, S, M, L, C, D: Close> Close
    for OnePerShotAdapterExecutor<I, O, S, M, L, C, NoStateReport, D>
{
    type Report = D::Report;

    /// Refuses later calls, drops the states, each in a catch of its own,
    /// then closes the executor below, unless an earlier close did; gives
    /// its report, empty if so.
    ///
    /// # Panics
    ///
    /// When a state's drop panics: the first one's panic reaches the caller
    /// once every state is dropped and the executor below is closed, and when
    /// the close below panics too, its panic is left to the panic hook. When
    /// only the close below panics, its panic reaches the caller.
    fn close(&mut self) -> D::Report {
        self.lifecycle.close();
        self.index = ShotIndex::default();
        let states = self.states.take_raising_first();
        free_then_close_below(None, states, self.way_down.below(), &mut self.below_closed)
    }
}

impl<I, O, S, M, L, C, R, Q, D> Close for OnePerShotAdapterExecutor<I, O, S, M, L, C, R, D>
where
    R: FnMut(&S) -> Q,
    Q: Fold,
    D: Close,
{
    type Report = (Q, D::Report);

    /// Refuses later calls, reports each state ([`AdapterFn::state_report`])
    /// and drops it, then closes the executor below, unless an earlier close
    /// did; gives the states' reports, folded, beside its report, empty if
    /// so.
    ///
    /// # Panics
    ///
    /// When `state_report` (or `Q::default`), or a state's drop, panics: the
    /// first panic reaches the caller once every state is dropped, each in a
    /// catch of its own, and the executor below is closed, `state_report`'s
    /// first when it panics, which reports no later state. When the close below
    /// panics too, that first panic still reaches the caller, and the close
    /// below's is left to the panic hook. When only the close below panics, its
    /// panic reaches the caller.
    fn close(&mut self) -> (Q, D::Report) {
        self.lifecycle.close();
        self.index = ShotIndex::default();
        let states = self.states.take_raising_first();
        let (report, reported) = report_states(
            states.iter().map(|(_, kept)| &kept.state),
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

/// A logic's way below for one shot: its sends are kept, ranked in the
/// order the shot made them, to go below merged once every shot's logic
/// ran; a run goes below at once on the shot alone, after the shot's sends
/// kept so far, and its later sends rank anew.
pub(super) struct ShotRoute<'a, I, O, D, B> {
    pub(super) merged: &'a mut Merged<O>,
    pub(super) calls: &'a mut CallsBelow<B>,
    pub(super) below: &'a mut D,
    /// The call's instructions, where an observer's sends come from.
    pub(super) input: &'a [I],
    pub(super) shot: ShotId,
    /// The rank of the shot's next send: how many it keeps. Borrowed from
    /// whoever serves the shot, so that it holds for the shot's later
    /// calls below even when a call below panics and the logic catches it.
    pub(super) rank: &'a mut usize,
}

impl<I, O, D, B> Route<O> for ShotRoute<'_, I, O, D, B>
where
    O: Clone + PartialEq,
    D: Executor<InstructionBatch<O>, Reply = B>,
    B: Reply<Item = ShotOutcome>,
{
    fn send(&mut self, instructions: &[O]) -> Result<(), String> {
        self.calls.going_on()?;
        (self.merged).keep(self.input, *self.rank, self.shot, instructions);
        *self.rank += 1;
        Ok(())
    }

    fn run(&mut self, instructions: &[O]) -> Result<Vec<ShotOutcome>, String> {
        let alone = ShotSet::one(self.shot);
        // Taken first, so that the shot's later sends rank anew even when a
        // send alone panics and the logic catches it.
        let count = mem::take(self.rank);
        (self.merged).send_alone(count, &alone, self.calls, self.below);
        self.calls.run(self.below, &alone, instructions)
    }
}

/// The sends of a call's shots, kept until every shot's logic ran, merged:
/// by rank (a shot's first send, its second, …), each rank's sends grouped
/// by content, in the order of their first shots. A shot's sends kept
/// when it runs leave first, alone ([`Merged::send_alone`]).
pub struct Merged<O> {
    /// The groups of each rank, those of the first `used` ranks this
    /// call's; the others are empty, kept for their buffers.
    ranks: Vec<Vec<Group<O>>>,
    used: usize,
    /// Each send kept, by its rank, the index of its group in the rank,
    /// and its shot: in the batch's order within a group.
    members: Vec<(usize, usize, ShotId)>,
}

impl<O> Default for Merged<O> {
    fn default() -> Self {
        Self {
            ranks: Vec::new(),
            used: 0,
            members: Vec::new(),
        }
    }
}

/// Equal sends of one rank: what they send, copied, where the first was
/// if it was in the call's instructions, and how many shots sent it.
///
/// Wherever it is dropped (emptied after the call, dropped with the
/// executor, or as a panic unwinds), it absorbs the panics of its copies'
/// drops, the implementer's code, after the panic hook reported them, each
/// instruction in a catch of its own ([`absorb_each`]), as a placed
/// executor's copy of a batch does: so that the executor's `drop` never
/// panics (C-DTOR-FAIL), and two copies that panic, or one during an
/// unwind, do not abort the process.
struct Group<O> {
    instructions: Vec<O>,
    origin: Option<Region>,
    shots: usize,
}

impl<O> Group<O> {
    /// A group of `instructions`, copied one at a time into it, so that the
    /// copies made before one's `Clone` panics are dropped through it: the
    /// standard library does not promise that copying a slice into a `Vec`
    /// keeps them.
    fn copied(instructions: &[O], origin: Option<Region>) -> Self
    where
        O: Clone,
    {
        let mut group = Self {
            instructions: Vec::with_capacity(instructions.len()),
            origin,
            shots: 1,
        };
        for instruction in instructions {
            group.instructions.push(instruction.clone());
        }
        group
    }
}

impl<O> Drop for Group<O> {
    fn drop(&mut self) {
        absorb_each(mem::take(&mut self.instructions), drop);
    }
}

/// Where a send is in memory, by address and length, when it lies in the
/// call's instructions: that memory is lent to the call unchanged, so two
/// sends from one region of it are equal, as an observer's sends of the
/// call's own instructions are.
///
/// It assumes the memory lent is not changed during the call. An
/// instruction type with interior mutability (a `Cell`) that a logic
/// changes breaks it: a later send from the region joins the first, which
/// goes below as it was when kept, not as changed.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Region {
    address: usize,
    len: usize,
}

impl Region {
    /// Where `sent` is, if it lies in `input` and is not empty.
    fn of<I, O>(sent: &[O], input: &[I]) -> Option<Self> {
        let start = input.as_ptr().addr();
        let address = sent.as_ptr().addr();
        let size = size_of_val(sent);
        (size > 0 && start <= address && address + size <= start + size_of_val(input)).then_some(
            Self {
                address,
                len: sent.len(),
            },
        )
    }
}

impl<O> Merged<O> {
    /// Empties it for the next call, keeping its buffers.
    fn clear(&mut self) {
        for groups in &mut self.ranks[..self.used] {
            groups.clear();
        }
        self.used = 0;
        self.members.clear();
    }
}

/// The sends kept for one call, emptied as the call ends, however it ends:
/// sent below, or not (a refusal, a failed logic), or as a panic unwinds
/// (a logic's, or one below), so that none outlives its call.
struct ForTheCall<'a, O>(&'a mut Merged<O>);

impl<O> Deref for ForTheCall<'_, O> {
    type Target = Merged<O>;

    fn deref(&self) -> &Merged<O> {
        self.0
    }
}

impl<O> DerefMut for ForTheCall<'_, O> {
    fn deref_mut(&mut self) -> &mut Merged<O> {
        self.0
    }
}

impl<O> Drop for ForTheCall<'_, O> {
    fn drop(&mut self) {
        self.0.clear();
    }
}

impl<O: Clone + PartialEq> Merged<O> {
    /// Keeps `instructions`, the `rank`-th send of shot `shot` in a call of
    /// `input`: with the group of its rank it equals, else as a group of
    /// its own, copied. A send from the region of `input` a group's first
    /// send came from, as an observer's sends of the call's instructions,
    /// joins that group with no comparison; any other is compared to the
    /// rank's groups.
    fn keep<I>(&mut self, input: &[I], rank: usize, shot: ShotId, instructions: &[O]) {
        if rank == self.used {
            if self.ranks.len() == rank {
                self.ranks.push(Vec::new());
            }
            self.used += 1;
        }
        let groups = &mut self.ranks[rank];
        let origin = Region::of(instructions, input);
        let found = (origin.is_some())
            .then(|| groups.iter().position(|group| group.origin == origin))
            .flatten()
            .or_else(|| {
                (groups.iter()).position(|group| group.instructions.as_slice() == instructions)
            });
        let at = if let Some(at) = found {
            groups[at].shots += 1;
            at
        } else {
            groups.push(Group::copied(instructions, origin));
            groups.len() - 1
        };
        self.members.push((rank, at, shot));
    }

    /// Sends the `count` sends kept last, those of the shot of `alone`, down
    /// to `below` on that shot alone, in the order the shot made them,
    /// through `calls`, which keeps the rules of a call's calls below;
    /// stops once they end (a refusal, a failed reply), or as one panics
    /// below. However it stopped, the shot's sends are then no longer kept
    /// ([`ShotSendsLeaving`]): those not sent are dropped unsent, as after
    /// a refusal, and none goes below again. A one-state adapter's calls
    /// below are the same for a logic whose catch holds the calls from the
    /// send that panicked to the run: its sends go below at once, and the
    /// panic leaves the catch before its logic makes the later ones. Keeps
    /// the others' as they were.
    fn send_alone<D>(
        &mut self,
        count: usize,
        alone: &ShotSet,
        calls: &mut CallsBelow<D::Reply>,
        below: &mut D,
    ) where
        D: Executor<InstructionBatch<O>>,
    {
        let from = self.members.len() - count;
        let leaving = ShotSendsLeaving { merged: self, from };
        for &(rank, at, _) in &leaving.merged.members[from..] {
            let instructions = &leaving.merged.ranks[rank][at].instructions;
            if (calls.send(below, alone, instructions)).is_err() {
                break;
            }
        }
    }

    /// Sends each group down to `below` as one call over its shots, rank by
    /// rank, through `calls`, which keeps the rules of a call's calls below; a
    /// group of every shot of the call, `all`, is sent on `all`, which
    /// allocates nothing. Stops once the calls below end (a refusal, a
    /// failed reply).
    //
    // Known issue, open (blocks neither): the shots of a group of some of
    // the call's shots are found by a pass over every send kept, so a call
    // whose shots all send different batches costs a time quadratic in its
    // shots, as finding the groups does by comparison.
    fn send<D>(&self, calls: &mut CallsBelow<D::Reply>, below: &mut D, all: &ShotSet)
    where
        D: Executor<InstructionBatch<O>>,
    {
        for (rank, groups) in self.ranks[..self.used].iter().enumerate() {
            for (at, group) in groups.iter().enumerate() {
                let part;
                let shots = if group.shots == all.len() {
                    all
                } else {
                    part = ShotSet::part(
                        (self.members.iter())
                            .filter(|&&(r, a, _)| (r, a) == (rank, at))
                            .map(|&(.., j)| j)
                            .collect(),
                    );
                    &part
                };
                if calls.send(below, shots, &group.instructions).is_err() {
                    return;
                }
            }
        }
    }
}

/// A shot's kept sends as they go below alone, from `from` on in the
/// members: no longer kept once they went, however their sending ended,
/// a panic below too.
struct ShotSendsLeaving<'a, O> {
    merged: &'a mut Merged<O>,
    from: usize,
}

impl<O> Drop for ShotSendsLeaving<'_, O> {
    fn drop(&mut self) {
        let merged = &mut *self.merged;
        for (rank, at, _) in merged.members.drain(self.from..) {
            let groups = &mut merged.ranks[rank];
            groups[at].shots -= 1;
            if groups[at].shots == 0 {
                // The shot made the group: it is its rank's last, since the
                // shot runs after the others and keeps one send per rank.
                debug_assert_eq!(at + 1, groups.len(), "the shot's group is its rank's last");
                groups.pop();
            }
        }
    }
}
