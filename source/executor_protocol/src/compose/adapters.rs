// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Adapters composed ([`ComposedAdapters`]), or none ([`NoAdapter`]), as a
//! list of adapters that [`compose`](fn@crate::compose) builds against an
//! executor, giving an executor whose calls pass through the adapters
//! ([`UpstreamOf`]).
//!
//! Adapters are built once, when they reach an executor. Building starts at
//! that executor and goes upstream: each adapter is built against what is
//! downstream of it ([`Adapter::build`]), and the set it sends is matched
//! once in the set downstream.
//! Calls of a type pass as they are; calls by opcode pass as they are in the
//! downstream numbering, which an adapter may adopt
//! ([`AdapterBoundary::adopt_downstream_numbering`]), and in another each batch
//! is renumbered on its way down ([`Instruction::renumbering`]). Calls by
//! opcode are checked at each boundary, so a chain where none is described is
//! refused ([`BuildError::NoInstructionSetDownstream`]), as is an adapter that
//! makes calls by opcode and states no set it sends
//! ([`BuildError::UnstatedSends`]).

use std::any::type_name;
use std::marker::PhantomData;
use std::sync::Arc;

use crate::adapter::adapter_fn::{MakeAdapterState, ServeAdapterCalls, close_below_once};
use crate::lifecycle::failing_on_panic;
use crate::{
    Adapter, AdapterBoundary, AdapterFn, AdapterFnExecutor, Apply, BuildError, BuildFault,
    BuildFaults, Capabilities, Close, Describe, Executor, ExecutorLifecycle, Instruction,
    InstructionBatch, InstructionBatchRef, InstructionSet, LifecycleBoundReply, OnePerShotAdapter,
    OnePerShotAdapterExecutor, ProgramInput, Rejected, Renumbering,
};

/// One adapter, or adapters composed: what composes upstream of an executor. It
/// is implemented by the crate, for every [`Adapter`], for
/// [`ComposedAdapters`], and for [`NoAdapter`].
//
// The notes are on this trait, not only on `Adapter`: a `>>` that cannot
// take its right side reports this bound, the executor taken as adapters,
// and shows none of `Adapter`'s notes; `compose` reports `Adapter`'s.
#[diagnostic::on_unimplemented(
    note = "`>>` takes an executor on its right only where it is implemented for that executor",
    note = "an adapter on the left returned as a bare `impl Adapter` does not compose over an executor: return the facility's type, its closures named by role (`MakeState`, `AdapterLogic`; see `executor_protocol::compose`)",
    note = "an adapter written by hand composes with an executor when it implements `Apply<E>` for every `E: Executor<InstructionBatch<Sends>> + Close`, not for one executor's type (see `executor_protocol::Apply`)"
)]
pub trait Adapters: sealed::AdapterList {
    /// The instructions the most upstream adapter is called with.
    type Accepts;
    /// The instructions the most downstream adapter sends.
    type Sends;
}

/// What the crate alone does with a list of adapters: build it, then put
/// it upstream of an executor, so that no adapter runs before it is built
/// with its faults checked.
mod sealed {
    use crate::{BuildFault, Capabilities};

    /// How a list of adapters, one or composed, is built against an
    /// executor.
    pub trait AdapterList: Sized {
        /// The adapters, built.
        type Built;

        /// These adapters, built against what offers `downstream`: each adapter
        /// against what is downstream of it, starting at the executor and
        /// going upstream.
        /// Gives what the most upstream adapter offers upstream; each fault
        /// goes to `faults`, the most upstream adapter's first.
        fn build_each(
            self,
            downstream: Capabilities,
            faults: &mut Vec<BuildFault>,
        ) -> (Self::Built, Capabilities);
    }

    /// A list of adapters, built, upstream of the executor `E`: the built
    /// counterpart of [`UpstreamOf`](super::UpstreamOf).
    pub trait BuiltUpstreamOf<E> {
        /// The instructions the most upstream adapter is called with.
        type Accepts;
        /// These adapters upstream of `E`: an executor.
        type Output: crate::Executor<crate::InstructionBatch<Self::Accepts>>
            + crate::Describe
            + crate::Close;

        /// These adapters upstream of `executor`, which they were built
        /// against.
        fn put_upstream_of(self, executor: E) -> Self::Output;
    }
}

use sealed::{AdapterList, BuiltUpstreamOf};

/// Adapters that compose upstream of the executor `E`, built against it:
/// what [`compose`](fn@crate::compose) does with an executor on its right. The
/// result is an executor: an adapter maps executors to executors. No adapter
/// ([`NoAdapter`]) leaves the executor as it is.
pub trait UpstreamOf<E>: Adapters {
    /// These adapters upstream of `E`: an executor.
    type Output: Executor<InstructionBatch<Self::Accepts>> + Describe + Close;

    /// These adapters upstream of `executor`, built against it starting at
    /// the executor and going upstream. `upstream` is given what the chain
    /// offers, and returns the faults of what composes upstream of it,
    /// reported first (the crate's interpreter reports none: a job's needs
    /// are checked when the job is given to the chain).
    ///
    /// # Errors
    ///
    /// [`BuildFaults`]: every fault, `upstream`'s first, then each
    /// adapter's that cannot be built against what is downstream of it,
    /// the most upstream adapter's first.
    fn upstream_of(
        self,
        executor: E,
        upstream: impl FnOnce(&Capabilities) -> Vec<BuildFault>,
    ) -> Result<Self::Output, BuildFaults>;
}

/// Adapters upstream of an executor, if neither they nor what composes
/// upstream of them (`upstream`) has a fault.
impl<X, E> UpstreamOf<E> for X
where
    X: Adapters,
    X::Built: BuiltUpstreamOf<E, Accepts = X::Accepts>,
    E: Describe,
{
    type Output = <X::Built as BuiltUpstreamOf<E>>::Output;

    fn upstream_of(
        self,
        executor: E,
        upstream: impl FnOnce(&Capabilities) -> Vec<BuildFault>,
    ) -> Result<Self::Output, BuildFaults> {
        let mut faults = Vec::new();
        let (built, offered) = self.build_each(executor.capabilities(), &mut faults);
        let mut every = upstream(&offered);
        every.append(&mut faults);
        if every.is_empty() {
            Ok(built.put_upstream_of(executor))
        } else {
            Err(BuildFaults(every))
        }
    }
}

impl<A: Adapter> Adapters for A {
    type Accepts = A::Accepts;
    type Sends = A::Sends;
}

/// One adapter, built against what is downstream of it: the set it sends
/// matched in downstream's, once, then its own [`Adapter::build`]. An
/// adapter that passes calls by opcode through as they are, stating no
/// set, is checked by what is upstream of it, against the set downstream
/// it offers; with none downstream, it is refused
/// ([`BuildError::NoInstructionSetDownstream`]). One that makes calls by
/// opcode from instructions of another type and states no set it sends is
/// refused ([`BuildError::UnstatedSends`]), whatever it accepts: nothing
/// upstream numbers its calls. One that decodes calls by opcode into typed
/// instructions and states only the set it accepts sends no set: its
/// downstream boundary is typed, so it needs none there, and it offers the
/// set it accepts upstream, where its calls are checked.
impl<A: Adapter> AdapterList for A {
    type Built = BuiltAdapter<A>;

    fn build_each(
        self,
        downstream: Capabilities,
        faults: &mut Vec<BuildFault>,
    ) -> (BuiltAdapter<A>, Capabilities) {
        let mut fault = |error| {
            faults.push(BuildFault {
                component: type_name::<A>(),
                error,
            });
        };
        let sent = sent_set(&self);
        let mut offered = downstream.clone();
        let mut numbering = None;
        if let Some(sent) = &sent {
            match downstream.match_sends(sent) {
                Ok(map) => numbering = Some(map),
                Err(error) => fault(error),
            }
            offered.instruction_set = self.accepts();
        } else {
            if let Some(accepted) = self.accepts() {
                offered.instruction_set = Some(accepted);
            }
            if A::Sends::BY_OPCODE {
                if !A::Accepts::BY_OPCODE {
                    fault(BuildError::UnstatedSends);
                }
                if downstream.instruction_set.is_none() {
                    fault(BuildError::NoInstructionSetDownstream);
                }
            }
        }
        let draws = self.draws();
        if let Some(id) = draws {
            match offered.clone().drawing(id) {
                Ok(drawing) => offered = drawing,
                Err(error) => fault(error),
            }
            if downstream.run_seed.is_none() {
                fault(BuildError::NoRunSeed(id));
            }
            // Its draws are not in the log probability its downstream
            // executor reports.
            offered.log_probability = false;
        }
        let derived = offered.clone();
        let mut boundary = AdapterBoundary::new(&downstream, draws, numbering.as_ref(), offered);
        let adapter = self.build(&mut boundary);
        let (adopted, mut offered, own) = boundary.into_parts();
        keep_derived_fields(&mut offered, &derived);
        for error in own {
            fault(BuildError::Adapter(Arc::from(error)));
        }
        let renumbering = match numbering {
            Some(map) if !adopted => A::Sends::renumbering(map).map(Arc::new),
            _ => None,
        };
        let built = BuiltAdapter {
            adapter,
            capabilities: offered.clone(),
            renumbering,
        };
        (built, offered)
    }
}

/// Puts back in `offered`, what an adapter offers upstream once built, the
/// fields [`compose`](fn@crate::compose) derives, as `derived` (its offer
/// before it was built) holds them, whatever the adapter offers, even an
/// offer it assigns whole: the run's seed as it is; the instruction set,
/// as derived from the one the adapter states it accepts
/// ([`Adapter::accepts`]), downstream's when it passes its calls through
/// as they are; the ids drawn with at or below the adapter, its own
/// ([`Adapter::draws`]) and those below; and `log_probability`, which it may
/// turn off but not on, since its report passes up the log probability
/// from below, and `derived` holds none when it draws. The rest is the
/// adapter's to change.
fn keep_derived_fields(offered: &mut Capabilities, derived: &Capabilities) {
    offered.run_seed = derived.run_seed;
    offered.instruction_set.clone_from(&derived.instruction_set);
    offered.rng_ids.clone_from(&derived.rng_ids);
    offered.log_probability &= derived.log_probability;
}

/// The set `adapter` sends: the set it states, or else the set it accepts,
/// whose calls it sends as it is called with them ([`Adapter::sends`]).
/// `None` when it states neither: it accepts the set downstream as it is
/// ([`Adapter::accepts`]), and offers it upstream, where calls by opcode
/// are checked against it; typed calls need no set. Also `None`
/// when one of the two types it accepts and sends is by opcode and the
/// other typed, and it states no set it sends, since the set it accepts
/// does not number what it sends: one that makes calls by opcode is then
/// refused ([`BuildError::UnstatedSends`]); one that decodes calls into
/// typed instructions needs no set downstream.
fn sent_set<A: Adapter>(adapter: &A) -> Option<Arc<InstructionSet>> {
    let same_kind = A::Sends::BY_OPCODE == A::Accepts::BY_OPCODE;
    adapter
        .sends()
        .or_else(|| if same_kind { adapter.accepts() } else { None })
}

/// An adapter built against what is downstream of it
/// ([`Adapter::build`]): what it offers upstream, and how its calls reach
/// downstream.
pub struct BuiltAdapter<A> {
    adapter: A,
    capabilities: Capabilities,
    /// How the adapter's calls reach downstream, when not as they are.
    renumbering: Option<Arc<Renumbering>>,
}

/// One adapter written by hand upstream of an executor: the executor the
/// adapter gives, applied to the executor, whose calls are renumbered on
/// their way down where the executor numbers them otherwise.
impl<A, E> BuiltUpstreamOf<E> for BuiltAdapter<A>
where
    A: Adapter + Apply<DownstreamOf<A, E>>,
    E: Executor<InstructionBatch<A::Sends>> + Close,
{
    type Accepts = A::Accepts;
    type Output = AdapterExecutor<A, E>;

    fn put_upstream_of(self, executor: E) -> Self::Output {
        let below = A::Sends::downstream(executor, self.renumbering.as_ref());
        AdapterExecutor {
            executor: self.adapter.apply(AdapterDownstream(below)),
            capabilities: self.capabilities,
            executor_closed: false,
            lifecycle: ExecutorLifecycle::default(),
        }
    }
}

/// An [`adapter_fn`](fn@crate::adapter_fn) upstream of an executor: the
/// adapter's logic serves each call, on the whole executor, its sends
/// renumbered on their way down where the executor numbers them otherwise.
//
// Design question, held for the user (blocks neither): this impl and the
// next are on the concrete types, so an adapter returned as a bare
// `impl Adapter` does not compose over an executor, even by `compose`; a
// function returns the type spelled, its closures opaque, as `>>` needs
// anyway (the documentation of the function `compose`). The map from an
// executor to an executor is not in `Adapter` but in `Apply` and in these
// impls, so a bare `impl Adapter` names none.
impl<I, O, S, M, L, C, R, E> BuiltUpstreamOf<E> for BuiltAdapter<AdapterFn<I, O, S, M, L, C, R>>
where
    I: Instruction,
    O: Instruction,
    M: MakeAdapterState<S, O>,
    L: ServeAdapterCalls<S, I, O, O::Downstream<E>>,
    C: FnMut(&ProgramInput<'_, I>) -> Result<(), Rejected>,
    E: Executor<InstructionBatch<O>> + Close,
    AdapterFnExecutor<I, O, S, M, L, C, R, O::Downstream<E>>: Close,
{
    type Accepts = I;
    type Output = AdapterFnExecutor<I, O, S, M, L, C, R, O::Downstream<E>>;

    fn put_upstream_of(self, executor: E) -> Self::Output {
        let below = O::downstream(executor, self.renumbering.as_ref());
        AdapterFnExecutor::new(self.adapter, self.capabilities, below)
    }
}

/// An [`adapter_fn`](fn@crate::adapter_fn) with a state per shot upstream
/// of an executor: the adapter's logic serves each shot of a call on its
/// shot's state, the shots' sends merged on their way down, and renumbered
/// where the executor numbers them otherwise.
impl<I, O, S, M, L, C, R, E> BuiltUpstreamOf<E>
    for BuiltAdapter<OnePerShotAdapter<I, O, S, M, L, C, R>>
where
    I: Instruction,
    O: Instruction + Clone + PartialEq,
    M: MakeAdapterState<S, O>,
    L: ServeAdapterCalls<S, I, O, O::Downstream<E>>,
    C: FnMut(&ProgramInput<'_, I>) -> Result<(), Rejected>,
    E: Executor<InstructionBatch<O>> + Close,
    OnePerShotAdapterExecutor<I, O, S, M, L, C, R, O::Downstream<E>>: Close,
{
    type Accepts = I;
    type Output = OnePerShotAdapterExecutor<I, O, S, M, L, C, R, O::Downstream<E>>;

    fn put_upstream_of(self, executor: E) -> Self::Output {
        let below = O::downstream(executor, self.renumbering.as_ref());
        OnePerShotAdapterExecutor::new(self.adapter, self.capabilities, below)
    }
}

/// Two adapters, `X` upstream of `Y`: an adapter, made by `x >> y` or
/// [`compose`](fn@crate::compose).
#[derive(Clone, Copy, Debug)]
pub struct ComposedAdapters<X, Y> {
    upstream: X,
    downstream: Y,
}

impl<X, Y> ComposedAdapters<X, Y> {
    /// `upstream`, upstream of `downstream`.
    pub(crate) fn new(upstream: X, downstream: Y) -> Self {
        Self {
            upstream,
            downstream,
        }
    }
}

impl<X: Adapters, Y: Adapters<Accepts = X::Sends>> Adapters for ComposedAdapters<X, Y> {
    type Accepts = X::Accepts;
    type Sends = Y::Sends;
}

/// Two adapters, built downstream first, so that the upstream one is
/// built against what the downstream one offers; the upstream one's faults
/// are reported first.
impl<X: Adapters, Y: Adapters<Accepts = X::Sends>> AdapterList for ComposedAdapters<X, Y> {
    type Built = ComposedAdapters<X::Built, Y::Built>;

    fn build_each(
        self,
        downstream: Capabilities,
        faults: &mut Vec<BuildFault>,
    ) -> (Self::Built, Capabilities) {
        let mut downstream_faults = Vec::new();
        let (built_downstream, between) = self
            .downstream
            .build_each(downstream, &mut downstream_faults);
        let (built_upstream, offered) = self.upstream.build_each(between, faults);
        faults.append(&mut downstream_faults);
        (
            ComposedAdapters::new(built_upstream, built_downstream),
            offered,
        )
    }
}

/// Two adapters upstream of an executor: the downstream one upstream of the
/// executor, then the upstream one upstream of that.
impl<X, Y, E> BuiltUpstreamOf<E> for ComposedAdapters<X, Y>
where
    Y: BuiltUpstreamOf<E>,
    X: BuiltUpstreamOf<Y::Output>,
{
    type Accepts = X::Accepts;
    type Output = X::Output;

    fn put_upstream_of(self, executor: E) -> X::Output {
        let between = self.downstream.put_upstream_of(executor);
        self.upstream.put_upstream_of(between)
    }
}

/// No adapter: the calls of type `I` reach the executor as they are. A
/// driver starts with it.
#[derive(Debug)]
pub struct NoAdapter<I>(PhantomData<fn() -> I>);

impl<I> Default for NoAdapter<I> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<I> Adapters for NoAdapter<I> {
    type Accepts = I;
    type Sends = I;
}

impl<I> AdapterList for NoAdapter<I> {
    type Built = Self;

    fn build_each(self, downstream: Capabilities, _: &mut Vec<BuildFault>) -> (Self, Capabilities) {
        (self, downstream)
    }
}

/// No adapter upstream of an executor: the executor as it is.
impl<I, E> BuiltUpstreamOf<E> for NoAdapter<I>
where
    E: Executor<InstructionBatch<I>> + Describe + Close,
{
    type Accepts = I;
    type Output = E;

    fn put_upstream_of(self, executor: E) -> E {
        executor
    }
}

/// An adapter written by hand `A` upstream of the executor `E`: the executor
/// `compose` and `>>` give, as [`AdapterFnExecutor`] is an
/// [`adapter_fn`](fn@crate::adapter_fn)'s. It runs on the executor `A` gives
/// ([`Apply::apply`]), applied to `E` (whose calls are renumbered on their way
/// down where `E` numbers them otherwise), and keeps it to the protocol's rules
/// of an executor: it offers what the adapter offers upstream
/// ([`AdapterBoundary::offered_upstream`]), refuses every call once closed or
/// after a call failed, a call that panicked included, and closes the executor
/// `A` gave once; it reports what that executor reports. So an adapter written
/// by hand states only its own logic: the executor it gives runs or refuses
/// each call, and closes the executor downstream of it. Dropped, it drops that
/// executor, and so `E` where that executor holds it: over the crate's
/// executors, which absorb every panic of their drops (their states' and what
/// their closures capture), it raises none.
///
/// A function that makes the chain's executor returns it by this name:
/// `-> AdapterExecutor<Counter, Toy>`.
//
// Known issues, open.
// - Blocks a later unit (`.log_probability` on adapters): the report of
//   the executor `A` gives passes up as it is, so under an adapter that
//   draws, which offers no log probability, it still gives the log
//   probabilities of the executor below, which miss the adapter's draws
//   (`a_run_through_an_adapter_that_draws_reports_no_log_probability`,
//   ignored); an `adapter_fn` that draws has the same issue.
// - Blocks neither this commit nor a later one: the drop of the executor
//   `A` gave, and so of `E`, which it holds, is not caught. No executor's
//   `drop` panics, by the crate's rule, which its own executors keep, so
//   only an implementer's executor that breaks it makes this `drop`
//   panic, as a `Vec`'s does when an element's drop panics. Absorbing it
//   would hide that implementer's bug, which the rule already forbids.
pub struct AdapterExecutor<A, E>
where
    A: Adapter + Apply<DownstreamOf<A, E>>,
    E: Executor<InstructionBatch<A::Sends>> + Close,
{
    executor: A::Output,
    capabilities: Capabilities,
    /// Whether a close reached the executor `A` gave
    /// ([`close_below_once`]).
    executor_closed: bool,
    lifecycle: ExecutorLifecycle,
}

/// The executor an adapter written by hand `A` is applied to, over the
/// executor `E`: `E` as `A`'s calls reach it, passed through
/// [`AdapterDownstream`].
pub(crate) type DownstreamOf<A, E> =
    AdapterDownstream<<<A as Adapter>::Sends as Instruction>::Downstream<E>>;

impl<A, E> Describe for AdapterExecutor<A, E>
where
    A: Adapter + Apply<DownstreamOf<A, E>>,
    E: Executor<InstructionBatch<A::Sends>> + Close,
{
    fn capabilities(&self) -> Capabilities {
        self.capabilities.clone()
    }
}

impl<A, E> Executor<InstructionBatch<A::Accepts>> for AdapterExecutor<A, E>
where
    A: Adapter + Apply<DownstreamOf<A, E>>,
    E: Executor<InstructionBatch<A::Sends>> + Close,
{
    type Reply = LifecycleBoundReply<<A::Output as Executor<InstructionBatch<A::Accepts>>>::Reply>;

    /// Runs the batch on the executor `A` gave, its reply bound to this
    /// executor's lifecycle, so that a reply that fails refuses every later
    /// call.
    ///
    /// # Errors
    ///
    /// [`Rejected::Closed`] once closed, then [`Rejected::EarlierFailure`]
    /// after a call failed; then the refusal of the executor `A` gave. A
    /// refusal of that executor for an earlier failure
    /// ([`Rejected::EarlierFailure`]) fails this executor too, so it
    /// refuses every later call, even one that executor would answer
    /// itself.
    ///
    /// # Panics
    ///
    /// When the executor `A` gave panics: the panic reaches the caller,
    /// and every later call is refused with [`Rejected::EarlierFailure`].
    fn call(
        &mut self,
        batch: InstructionBatchRef<'_, A::Accepts>,
    ) -> Result<Self::Reply, Rejected> {
        failing_on_panic(
            self,
            |this| &this.lifecycle,
            |this| {
                this.lifecycle.admit()?;
                let called = this.executor.call(batch);
                let reply = called.map_err(|rejected| this.lifecycle.pass_up_refusal(rejected))?;
                Ok(this.lifecycle.bind(reply))
            },
        )
    }
}

impl<A, E> Close for AdapterExecutor<A, E>
where
    A: Adapter + Apply<DownstreamOf<A, E>>,
    E: Executor<InstructionBatch<A::Sends>> + Close,
{
    type Report = <A::Output as Close>::Report;

    /// Refuses later calls, then closes the executor `A` gave, unless an
    /// earlier close did; gives its report, empty if so.
    ///
    /// # Panics
    ///
    /// When the close of the executor `A` gave panics, as with the first panic
    /// of the drops of the states that the crate's executors below it free as
    /// they close, the panic reaches the caller once every one is freed, and a
    /// later close does not close that executor again. `E` is closed only by
    /// that executor's close, where it reaches `E`: one that panics before then
    /// leaves `E` to its drop.
    fn close(&mut self) -> Self::Report {
        self.lifecycle.close();
        close_below_once(&mut self.executor, &mut self.executor_closed)
    }
}

/// The executor `D` downstream of an adapter written by hand, as its
/// [`Apply::apply`] is given it: `D` as it is, each call and close passed
/// on.
///
/// Unexported. A type of the crate's own, so that the rule that puts an
/// adapter written by hand over an executor and the rules that put the
/// facility's adapters over one do not overlap: another crate could
/// implement `Apply` of its own executor for an `adapter_fn`, but not of
/// this one.
pub struct AdapterDownstream<D>(D);

impl<I, D: Executor<InstructionBatch<I>>> Executor<InstructionBatch<I>> for AdapterDownstream<D> {
    type Reply = D::Reply;

    #[inline]
    fn call(&mut self, batch: InstructionBatchRef<'_, I>) -> Result<D::Reply, Rejected> {
        self.0.call(batch)
    }
}

impl<D: Close> Close for AdapterDownstream<D> {
    type Report = D::Report;

    #[inline]
    fn close(&mut self) -> D::Report {
        self.0.close()
    }
}
