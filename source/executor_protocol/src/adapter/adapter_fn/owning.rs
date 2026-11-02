// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The owning form of an [`adapter_fn`](fn@crate::adapter_fn)
//! ([`owning_adapter_fn`]): each state owns its way below
//! ([`BelowHandle`]) for its life, and the logic is lent none; and how the
//! executors of an `adapter_fn` serve both forms, lent and owning, with
//! one implementation of the rules below ([`MakeAdapterState`],
//! [`ServeAdapterCalls`]).

use std::marker::PhantomData;
use std::mem;

use super::one_per_shot::{Merged, ShotRoute};
use super::{AdapterFn, CallRoute, CallsBelow};
use crate::adapter::below::{BelowHandle, Route, StateId, StateIds, lend_while};
use crate::{
    AdapterError, AdapterLogic, Below, CallInput, Executor, InstructionBatch, MakeState, Outcomes,
    Rejected, Reply, ShotContext, ShotId, ShotOutcome, ShotSet,
};
#[cfg(doc)]
use crate::{OnePerShot, OnePerShotAdapter};

/// A state and a logic as an adapter whose states own their way below: the
/// state closure is given the context of the state's first shot and the
/// executor below ([`BelowHandle`]), to keep in the state it makes, as a
/// code's runtime is made over a backend it owns; the logic serves each
/// call on the state, which reaches below through what it owns.
///
/// It is an [`adapter_fn`](fn@crate::adapter_fn) in every other way: the
/// same options ([`AdapterFn::accepts`], [`AdapterFn::sends`],
/// [`AdapterFn::check`], [`AdapterFn::drawing`],
/// [`AdapterFn::state_report`]), a state per shot under
/// [`OnePerShot`] ([`AdapterFn::shots`]), and the same rules, those of
/// [`adapter_fn`](fn@crate::adapter_fn) and of [`OnePerShotAdapter`]: what
/// the state sends or runs through its [`BelowHandle`] while its logic
/// serves a call goes below as through a lent [`Below`], and a refusal or
/// a failure below has the effect it has through a lent one. At
/// close the states are dropped, with what they own, before the executor
/// below is closed.
///
/// A runtime whose results name no shot, as a code's runtime's, which come in
/// the order of its instructions, needs a state per shot
/// ([`.shots(OnePerShot)`](AdapterFn::shots)). With one state, it serves all
/// the call's shots at once, and its results cannot tell them apart: a run
/// below gives the readouts of every shot, by shot, each shot's in its reply's
/// order, where it expects one shot's. Glue that gives it the one readout per
/// tag it expects gives it the lowest-numbered shot's, and every shot is given
/// that shot's outcomes, with no error; glue that checks that the reads match
/// its readouts one to one fails the call at its first batch that reads.
///
/// The repetition code as such a runtime, with its glue, is worked in
/// `tests/static_isa/code_runtime.rs`, described in `docs/adapters.md`
/// ("A state that owns its way below").
///
/// The adapter's executor is `Send` when what it owns is, the executor
/// below, the states and the closures, as a `Mutex<T>` is `Send` when `T`
/// is: two owning adapters whose states share a plan by `Rc` stack and run,
/// as codes stacked do, and one over a `Send` executor, with `Send` states,
/// can move to another thread. The executor below, its replies and the
/// instructions sent must be `'static`, since the adapter's executor lends
/// each state its way to them, at each call, through a handle whose type
/// names none of them. Over an executor that holds a borrow, the adapter
/// composes only when the borrow is `'static`, and, composed by
/// [`compose`](fn@crate::compose) in a function generic over the
/// executor, only when its type and its reply are `'static`, where an
/// [`adapter_fn`](fn@crate::adapter_fn) composes. The compiler says so:
/// that the borrowed value "is borrowed for `'static`", or, in a function
/// generic over the borrow's lifetime, that the lifetime "must outlive
/// `'static`"; in a function generic over the executor's type, that the
/// type, and its reply, "must be valid for the static lifetime". A bare
/// `&mut E` is not an executor, so it composes under neither form. Nor does
/// `>>` take an executor of a generic type, under either form
/// ([`compose`](fn@crate::compose) says why): the compiler says that "the
/// trait bound `E: executor_protocol::Adapter` is not satisfied" (E0277).
///
/// # Type parameters
///
/// - `I`, `O`, `S`: as [`AdapterFn`]'s.
/// - `M`: the state closure, given the executor below ([`BelowHandle`]);
///   the adapter holds it in an [`Owning`], as its `M`.
/// - `L`: the logic ([`OwningAdapterLogic`]), given no [`Below`], since
///   the state reaches below; the adapter holds it in an [`Owning`], as
///   its `L`.
//
// Planned (blocks neither): a facility for a runtime's glue, which maps its
// results, which name no shot, to the tags it sent, refusing a readout
// missing or stray and a call on more than one shot: it would catch one
// state serving such a runtime when it runs.
pub fn owning_adapter_fn<I, O, S, M, L>(
    make_state: M,
    logic: L,
) -> AdapterFn<I, O, S, Owning<M>, Owning<L>>
where
    M: FnMut(ShotContext, BelowHandle<O>) -> Result<S, Rejected>,
    L: OwningAdapterLogic<I, S>,
{
    AdapterFn::new(Owning(make_state), Owning(logic))
}

/// What serves each call of an [`owning_adapter_fn`], of instructions of
/// type `I`, on its state, of type `S`: its `logic` closure, which is lent
/// no [`Below`], since its state reaches below through the [`BelowHandle`]
/// it owns. Its error is an [`AdapterError`](crate::AdapterError), as an
/// [`AdapterLogic`]'s is. Implemented for every closure of its signature,
/// so that a function returning the facility's type names the closure in
/// a word, as [`AdapterLogic`] is: `Owning<impl OwningAdapterLogic<I, S>>`
/// (see [`Owning`]).
pub trait OwningAdapterLogic<I, S>:
    FnMut(&mut S, &CallInput<'_, I>, &mut Outcomes<'_>) -> Result<(), AdapterError>
{
}

impl<I, S, F> OwningAdapterLogic<I, S> for F where
    F: FnMut(&mut S, &CallInput<'_, I>, &mut Outcomes<'_>) -> Result<(), AdapterError>
{
}

/// The state closure or the logic of an [`owning_adapter_fn`], `F`, as its
/// adapter holds them. A function that makes an owning adapter for `>>`
/// returns the type the facility gives, as for
/// [`adapter_fn`](fn@crate::adapter_fn) (see
/// [`compose`](fn@crate::compose)), each closure opaque inside an
/// `Owning`: the state closure by its signature, the logic by its role
/// ([`OwningAdapterLogic`]):
/// `-> OnePerShotAdapter<I, O, S,
/// Owning<impl FnMut(ShotContext, BelowHandle<O>) -> Result<S, Rejected>>,
/// Owning<impl OwningAdapterLogic<I, S>>>`.
pub struct Owning<F>(F);

/// A state, with its id: the id its way below is lent by, when the state
/// owns one.
pub struct KeptState<S> {
    pub(super) id: StateId,
    pub(super) state: S,
}

/// The executor below an `adapter_fn`'s executor, kept there between
/// calls, and the ids of the states that reach it, while it is lent to a
/// call, through what they own.
pub(super) struct WayDown<O, D> {
    below: Option<D>,
    ids: StateIds,
    instructions: PhantomData<fn(&[O])>,
}

/// Why the executor below is there: an owning call lends it, and gives it
/// back even when it panics.
const BELOW_KEPT: &str = "the executor below is kept between calls";

impl<O, D> WayDown<O, D> {
    /// The way down to `below`.
    pub(super) fn new(below: D) -> Self {
        Self {
            below: Some(below),
            ids: StateIds::new(),
            instructions: PhantomData,
        }
    }

    /// The executor below.
    pub(super) fn below(&mut self) -> &mut D {
        self.below.as_mut().expect(BELOW_KEPT)
    }

    /// A state made with `make`, from `context`, given its way below if it
    /// owns one, and kept with its id.
    ///
    /// # Errors
    ///
    /// The refusal of `make`.
    pub(super) fn make<S>(
        &mut self,
        make: &mut impl MakeAdapterState<S, O>,
        context: ShotContext,
    ) -> Result<KeptState<S>, Rejected> {
        let id = self.ids.next();
        let serving = self.ids.serving();
        let state = make.make(context, || BelowHandle::new(id, serving))?;
        Ok(KeptState { id, state })
    }
}

/// What a logic's calls below in one call go through: the adapter's way
/// down, and the call's calls below, which keep their rules.
pub struct CallDown<'a, O, D: Executor<InstructionBatch<O>>> {
    pub(super) way: &'a mut WayDown<O, D>,
    pub(super) calls: &'a mut CallsBelow<D::Reply>,
}

/// How an `adapter_fn`'s state closure makes a state: given the context of
/// its first shot, and, for an [`owning_adapter_fn`]'s, its way below.
pub trait MakeAdapterState<S, O> {
    /// The state of `context`'s shot; `below` gives its way below.
    ///
    /// # Errors
    ///
    /// The state closure's refusal.
    fn make(
        &mut self,
        context: ShotContext,
        below: impl FnOnce() -> BelowHandle<O>,
    ) -> Result<S, Rejected>;
}

impl<S, O, M: MakeState<S>> MakeAdapterState<S, O> for M {
    fn make(
        &mut self,
        context: ShotContext,
        _: impl FnOnce() -> BelowHandle<O>,
    ) -> Result<S, Rejected> {
        self(context)
    }
}

impl<S, O, M> MakeAdapterState<S, O> for Owning<M>
where
    M: FnMut(ShotContext, BelowHandle<O>) -> Result<S, Rejected>,
{
    fn make(
        &mut self,
        context: ShotContext,
        below: impl FnOnce() -> BelowHandle<O>,
    ) -> Result<S, Rejected> {
        (self.0)(context, below())
    }
}

/// How an `adapter_fn`'s logic serves a call, or a shot of one, on a
/// state: lent a [`Below`], or reaching below through what its state owns
/// ([`BelowHandle`]); either way its calls below go through the same
/// routes, which keep the rules of [`CallsBelow`].
pub trait ServeAdapterCalls<S, I, O, D: Executor<InstructionBatch<O>>> {
    /// Serves a call of `input` on `shots` with one state, `kept`, each
    /// call below one call on `shots`, writing to `outcomes`.
    ///
    /// # Errors
    ///
    /// The logic's `Err`.
    fn serve_call(
        &mut self,
        kept: &mut KeptState<S>,
        input: &CallInput<'_, I>,
        shots: &ShotSet,
        down: CallDown<'_, O, D>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), String>;

    /// Serves `shot` of a call of `input` on its state, `kept`: its sends
    /// are kept in `merged`, and its runs go below on the shot alone
    /// ([`ShotRoute`]).
    ///
    /// # Errors
    ///
    /// The logic's `Err`.
    fn serve_shot(
        &mut self,
        kept: &mut KeptState<S>,
        input: &CallInput<'_, I>,
        shot: ShotId,
        down: CallDown<'_, O, D>,
        merged: &mut Merged<O>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), String>
    where
        O: Clone + PartialEq;
}

impl<S, I, O, D, L> ServeAdapterCalls<S, I, O, D> for L
where
    L: AdapterLogic<I, O, S>,
    D: Executor<InstructionBatch<O>>,
{
    fn serve_call(
        &mut self,
        kept: &mut KeptState<S>,
        input: &CallInput<'_, I>,
        shots: &ShotSet,
        down: CallDown<'_, O, D>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), String> {
        let mut route = CallRoute {
            calls: down.calls,
            below: down.way.below(),
            shots,
        };
        self(
            &mut kept.state,
            input,
            &mut Below::new(&mut route),
            outcomes,
        )
        .map_err(String::from)
    }

    fn serve_shot(
        &mut self,
        kept: &mut KeptState<S>,
        input: &CallInput<'_, I>,
        shot: ShotId,
        down: CallDown<'_, O, D>,
        merged: &mut Merged<O>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), String>
    where
        O: Clone + PartialEq,
    {
        let mut rank = 0;
        let mut route = ShotRoute {
            merged,
            calls: down.calls,
            below: down.way.below(),
            input: input.instructions,
            shot,
            rank: &mut rank,
        };
        self(
            &mut kept.state,
            input,
            &mut Below::new(&mut route),
            outcomes,
        )
        .map_err(String::from)
    }
}

impl<S, I, O, D, L> ServeAdapterCalls<S, I, O, D> for Owning<L>
where
    L: OwningAdapterLogic<I, S>,
    O: 'static,
    D: Executor<InstructionBatch<O>> + 'static,
    D::Reply: 'static,
{
    /// Lends the executor below and the call's calls below to the state's
    /// way below for the length of the logic ([`OwnedCallRoute`]).
    fn serve_call(
        &mut self,
        kept: &mut KeptState<S>,
        input: &CallInput<'_, I>,
        shots: &ShotSet,
        down: CallDown<'_, O, D>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), String> {
        let CallDown { way, calls } = down;
        let WayDown { below, ids, .. } = way;
        let Self(logic) = self;
        let KeptState { id, state } = kept;
        lend_while(
            ids.serving(),
            *id,
            &mut (below, calls),
            |(below, calls)| OwnedCallRoute {
                below: below.take().expect(BELOW_KEPT),
                calls: mem::replace(*calls, CallsBelow::new()),
                shots: shots.clone(),
            },
            || logic(state, input, outcomes).map_err(String::from),
            |(below, calls), route| {
                **below = Some(route.below);
                **calls = route.calls;
            },
        )
    }

    /// Lends the executor below, the call's calls below and its kept sends
    /// to the state's way below for the length of the shot's logic
    /// ([`OwnedShotRoute`]).
    fn serve_shot(
        &mut self,
        kept: &mut KeptState<S>,
        input: &CallInput<'_, I>,
        shot: ShotId,
        down: CallDown<'_, O, D>,
        merged: &mut Merged<O>,
        outcomes: &mut Outcomes<'_>,
    ) -> Result<(), String>
    where
        O: Clone + PartialEq,
    {
        let CallDown { way, calls } = down;
        let WayDown { below, ids, .. } = way;
        let Self(logic) = self;
        let KeptState { id, state } = kept;
        lend_while(
            ids.serving(),
            *id,
            &mut (below, calls, merged),
            |(below, calls, merged)| OwnedShotRoute {
                below: below.take().expect(BELOW_KEPT),
                calls: mem::replace(*calls, CallsBelow::new()),
                merged: mem::take(*merged),
                shot,
                rank: 0,
            },
            || logic(state, input, outcomes).map_err(String::from),
            |(below, calls, merged), route| {
                **below = Some(route.below);
                **calls = route.calls;
                **merged = route.merged;
            },
        )
    }
}

/// A one-state logic's way below while its state owns it: a [`CallRoute`]
/// over what the call lends it.
struct OwnedCallRoute<D, B> {
    below: D,
    calls: CallsBelow<B>,
    /// The call's shots, copied: a call only lends them.
    shots: ShotSet,
}

impl<O, D, B> Route<O> for OwnedCallRoute<D, B>
where
    D: Executor<InstructionBatch<O>, Reply = B>,
    B: Reply<Item = ShotOutcome>,
{
    fn send(&mut self, instructions: &[O]) -> Result<(), String> {
        self.calls.send(&mut self.below, &self.shots, instructions)
    }

    fn run(&mut self, instructions: &[O]) -> Result<Vec<ShotOutcome>, String> {
        self.calls.run(&mut self.below, &self.shots, instructions)
    }
}

/// A shot's logic's way below while its state owns it: a [`ShotRoute`]
/// over what the shot lends it. Its sends are not in the call's
/// instructions as far as it knows, so each is compared with the sends of
/// its rank to be merged.
struct OwnedShotRoute<O, D, B> {
    below: D,
    calls: CallsBelow<B>,
    merged: Merged<O>,
    shot: ShotId,
    rank: usize,
}

impl<O, D, B> OwnedShotRoute<O, D, B> {
    /// `call` on the route this one lends to, which borrows its rank.
    fn through<T>(&mut self, call: impl FnOnce(&mut ShotRoute<'_, O, O, D, B>) -> T) -> T {
        call(&mut ShotRoute {
            merged: &mut self.merged,
            calls: &mut self.calls,
            below: &mut self.below,
            input: &[],
            shot: self.shot,
            rank: &mut self.rank,
        })
    }
}

impl<O, D, B> Route<O> for OwnedShotRoute<O, D, B>
where
    O: Clone + PartialEq,
    D: Executor<InstructionBatch<O>, Reply = B>,
    B: Reply<Item = ShotOutcome>,
{
    fn send(&mut self, instructions: &[O]) -> Result<(), String> {
        self.through(|route| route.send(instructions))
    }

    fn run(&mut self, instructions: &[O]) -> Result<Vec<ShotOutcome>, String> {
        self.through(|route| route.run(instructions))
    }
}
