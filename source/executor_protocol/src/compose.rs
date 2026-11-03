// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Composition: how a driver, adapters and an executor are put together into
//! a run, or into a chain that runs a job, checked wherever two of them meet.
//!
//! [`compose`](fn@crate::compose) takes two of them and gives one, by the
//! rules its documentation lists; `a >> b` is the same as `compose(a, b)` where
//! `>>` is implemented: for the crate's types with an adapter on the right, and
//! with an executor on the right where the executor's crate implements it.
//! A component made by a function composes when the function returns the
//! type the facility gives, its closures named by role, not a bare
//! `impl Adapter`: [`compose`](fn@crate::compose)'s documentation says
//! how, and why.
//!
//! Composition guarantees that entities that are compatible compose into a
//! valid one, whatever the order in which their instruction sets number
//! their instructions: where an adapter's calls meet the set downstream, they
//! are checked against it when the chain is built, and renumbered into it
//! on their way down where it numbers them otherwise. An adapter sends
//! the set it states, or else, when it sends instructions of the kind it
//! is called with, the set it accepts ([`Adapter::sends`]).
//! Entities that are not compatible are refused when composed.
//!
//! Composing runs nothing, and the grouping does not change what runs (the
//! order of the components does): `static_driver() >> (a >> b) >> executor` is
//! `static_driver() >> a >> b >> executor`, and
//! `(static_driver() >> a) >> executor` is
//! `static_driver() >> (a >> executor)?`, the adapter composed with the
//! executor first, its `Result` unwrapped. A driver with adapters below it is
//! still the chain's prefix ([`ChainPrefix`]), as the driver alone is; it is
//! not yet a [`Driver`] itself (planned, `docs/planned.md`). Adapters grouped
//! either way among themselves report the same faults; a part composed with the
//! executor first is built on its own, and its faults are returned before what
//! is upstream of it is built. Adapters are held until the chain reaches an
//! executor; then each is built once against what is downstream of it, from
//! that executor upstream ([`Adapter::build`]), so that every fault is reported
//! together, before anything runs ([`BuildFaults`], each a [`BuildFault`] with
//! its [`BuildError`]). A job's needs are checked against what the chain offers
//! when the job is given to it ([`Chain::run`]).
//!
//! A chain is a type, fixed when it is compiled. Type-erased chains, chosen
//! at run time, are planned, with no milestone yet, for cases such as a
//! chain composed from Python at run time, a C API, or a transpiler chosen
//! by a string (`docs/planned.md`).
//!
//! - [`adapters`]: adapters composed, and applied to an executor once
//!   checked.
//! - [`run`]: the run a chain gives for a job.
//! - [`chain`]: the chain a driver gives with an executor, which runs the
//!   job given to it.
//! - [`operator`]: the operator `>>`, `compose(a, b)` where it is
//!   implemented.
//! - [`build_faults`]: what building a chain reports.
//!
//! What is composed has its own module: an adapter ([`Adapter`]), where it
//! meets what is downstream of it ([`AdapterBoundary`]), and a driver
//! ([`Driver`], in the chain's prefix, [`ChainPrefix`]).

pub(crate) mod adapters;
pub(crate) mod build_faults;
pub(crate) mod chain;
pub(crate) mod operator;
pub(crate) mod run;

#[cfg(doc)]
use crate::{
    Adapter, AdapterBoundary, AdapterExecutor, AdapterFnExecutor, Apply, BuildError, BuildFault,
    Close, Driver, DriverExt, InstructionBatch, Rejected,
};
use crate::{Adapters, BuildFaults, Chain, ChainPrefix, ComposedAdapters, UpstreamOf};

/// Two entities put together into one, `Self` upstream of `Rhs`: the rules
/// of [`compose`](fn@crate::compose), one impl each.
///
/// `Rule` says whether the right side is an adapter or an executor
/// ([`WithExecutor`]), so that a rule whose right side is any adapter and
/// one whose right side is any executor do not overlap: a type of another
/// crate could be both. [`compose`](fn@crate::compose) infers it.
///
/// A pair that no rule composes fails to compile, which no test pins.
/// Where the compiler reports this trait unmet, with the pair's types, it
/// shows four notes: one that lists the rules (a `Result` on the right
/// takes `?`), one that points a pair `>>` does not take to
/// [`compose`](fn@crate::compose), one that says a generic part does not
/// compose over an executor, and one that points a job written as a
/// component to [`Chain::run`]. That is where no rule has the left side's
/// type, or none the right side's for it: an executor or a chain on the
/// left, a generic part with no bound (`compose(a, toy)`, `a: A`), an
/// executor of another crate after a chain's prefix, where the executor's
/// crate implements no `>>` (`static_driver() >> hand`), an executor of another
/// crate that lacks [`Describe`](crate::Describe) or [`Close`], which the
/// rules require of an executor (in `compose`; after `>>`, see below), a
/// `Result` on the right of a chain's prefix or in `compose` (a `?`
/// forgotten: `static_driver() >> (a >> toy)`), or a program and its shots on
/// the right. An adapter given as a generic `A: Adapter`, or returned as a bare
/// `impl Adapter`, is reported otherwise, the executor after it as no
/// [`Adapter`] (in `compose`) or as not of the crate's `Adapters` (after `>>`),
/// with notes that name that case. Parts whose instruction types do not meet
/// (an adapter, or a chain's prefix with adapters below it, that sends `Op`,
/// upstream of a part that accepts `Gate`) show these notes in `compose`, and
/// after `>>` with a chain's prefix on the left and an adapter, or adapters
/// composed, on the right. After `>>`, with an executor on the right that has
/// `>>` (one of the crate's, or one whose crate implements it), they are
/// reported as no `>>` for the pair, with no note of the crate, since two `Shr`
/// impls match the executor's type; so is an executor whose crate implements
/// `>>` but not `Describe` or `Close`. With an adapter on the right of an
/// adapter, or of adapters composed, they are reported as a type mismatch on
/// what it accepts. Written as `compose(upstream, downstream)`, the pair shows
/// the rule. A chain's prefix with no adapter below it takes the instruction
/// type of the part it is composed with, an adapter or the executor: its
/// driver's own instruction type is checked when the job is given
/// ([`Chain::run`]), as a type mismatch with no note of the crate.
#[diagnostic::on_unimplemented(
    message = "`{Self}` does not compose upstream of `{Rhs}`",
    label = "no rule of `compose` puts `{Self}` upstream of this",
    note = "`compose` puts an adapter or a chain's prefix (`static_driver()`, `runtime.into_chain()`, \
            with adapters below it or not) upstream of an adapter, or of an executor \
            (`Executor<InstructionBatch<_>> + Describe + Close`), where what the upstream one \
            sends is what the downstream one accepts; a `Result` on the right is neither: apply `?` \
            to the step that made it (docs/composition.md, \"Composing a run\")",
    note = "where `>>` does not apply (an executor of another crate), write \
            `compose(upstream, downstream)`",
    note = "a generic part (`A`, or `A: Adapter`) does not compose over an executor: name the \
            facility's type, its closures named by role (see `executor_protocol::compose`)",
    note = "if the right side is a program and its shots: they are a job, not a component; give them \
            to the chain a driver heads, `(static_driver() >> executor)?.run(program, shots)`"
)]
pub trait Compose<Rhs, Rule> {
    /// What the two make.
    type Output;

    /// `self` upstream of `rhs`.
    fn compose(self, rhs: Rhs) -> Self::Output;
}

/// The rules of [`compose`](fn@crate::compose) whose right side is an adapter.
#[derive(Debug)]
pub enum WithAdapter {}

/// The rules of [`compose`](fn@crate::compose) whose right side is an executor.
#[derive(Debug)]
pub enum WithExecutor {}

/// `left` upstream of `right`: an adapter, or a chain's prefix (a driver, with
/// the adapters composed below it so far), upstream of an adapter or an
/// executor. `left >> right` is the same where `>>` is implemented.
/// `Lhs` and `Rhs` are their types, and `Rule` the rule that composes them,
/// [`WithExecutor`] where `Rhs` is an executor, which the compiler infers.
///
/// | `compose`              | gives        | as                               |
/// | ---------------------- | ------------ | -------------------------------- |
/// | adapter, executor      | executor     | the executor, through adapters   |
/// | adapter, adapter       | adapters     | [`ComposedAdapters`], as one     |
/// | chain prefix, adapter  | chain prefix | the prefix with one more adapter |
/// | chain prefix, executor | chain        | [`Chain`], which runs a job      |
///
/// A driver alone is a chain's prefix with no adapter ([`ChainPrefix`],
/// [`static_driver()`](fn@crate::static_driver), [`DriverExt::into_chain`]); a
/// prefix is not itself a [`Driver`](crate::Driver) (planned,
/// `docs/planned.md`). The grouping does not matter: a driver, adapters and an
/// executor composed in one order give a chain with the same run, however they
/// are grouped: the prefix with adapters below it composed with the executor,
/// or the prefix composed with the suffix, the adapters composed with the
/// executor first.
///
/// The right side is an executor of batches ([`InstructionBatch`]) that
/// describes what it offers ([`Describe`](crate::Describe)) and is closed
/// with a report ([`Close`]): the last executor, built with its seed, or
/// adapters composed with it, which together are an executor. Adapters
/// compose with any executor: an [`adapter_fn`](fn@crate::adapter_fn)
/// serves each call with its logic, on its one state, or on each shot's
/// with [`.shots(OnePerShot)`](crate::AdapterFn::shots), sending down what
/// the logic sends; an adapter written by hand maps the whole executor
/// once, and the result runs on the executor it gives ([`Apply`]).
///
/// Where `right` is an executor, each adapter is checked against what is
/// downstream of it, before anything runs or any shot is made. The driver
/// has no program yet: the job's needs are checked against
/// what the chain offers, and the number of its shots against the shots
/// the executor holds at once
/// ([`Capabilities::max_shots`](crate::Capabilities)), when the job is
/// given to the chain ([`Chain::run`]).
///
/// # Components made by functions
///
/// A function that makes a component for `>>` returns the type the
/// facility gives, its closures opaque, as iterator code returns
/// `Map<I, impl FnMut(..)>`, each named by its role: `-> OnePerShotAdapter<I,
/// O, S, impl MakeState<S>, impl AdapterLogic<I, O, S>>` for an
/// [`adapter_fn`](fn@crate::adapter_fn) given `.shots(OnePerShot)`
/// ([`MakeState`](crate::MakeState), [`AdapterLogic`](crate::AdapterLogic),
/// [`ExecutorLogic`](crate::ExecutorLogic)), and `Owning<impl ..>` for
/// each closure of an [`owning_adapter_fn`](fn@crate::owning_adapter_fn)
/// ([`Owning`](crate::Owning)). An executor made of an adapter
/// over an executor is named likewise: [`AdapterFnExecutor`] for an
/// `adapter_fn`, and [`AdapterExecutor`]`<A, E>` for an adapter `A` written
/// by hand over the executor `E`, so that `static_driver() >> counted()`
/// takes `fn counted() -> AdapterExecutor<Counter, Toy>`. A function that
/// keeps a shared handle takes it by value, as an `Arc` is passed, so that
/// its closures borrow nothing and the chain may move to another thread.
/// One that takes a reference adds `+ use<>` to each `impl`: its closures
/// keep no borrow of the argument (they hold what they cloned of it), so
/// the chain may move to another thread; without it, an `impl` in a return
/// type captures the lifetime of every argument (edition 2024).
///
/// A bare `-> impl Adapter` does not compose, for two reasons. Rust lets
/// this crate implement `>>` only on its own types, so not on an opaque
/// one on the left: there the compiler gives a bare E0369, `no
/// implementation for impl Adapter<..> >> ..`, with no note of this
/// crate's. And the rule that puts an adapter over an executor is
/// implemented for each of the facility's adapters, not carried by the
/// [`Adapter`] trait, so neither `>>` after a chain's prefix nor `compose`
/// takes an opaque adapter over an executor. An executor returned as a bare
/// `impl Executor<..> + Describe + Close` composes by `compose`, not by
/// `>>`, which takes an executor on its right only where it is implemented
/// for the executor's type.
///
/// # Errors
///
/// Where `right` is an executor, [`BuildFaults`]: every adapter that cannot
/// be built against what is downstream of it, and why. The
/// executor, taken by value, is then dropped without being closed: an
/// executor whose `close` releases something (a device, a paid job) does so
/// only in its own `Drop`, if it has one. Known limit, open, which no test
/// pins: closing it, or giving it back with the faults, goes with the
/// planned cancellation.
pub fn compose<Lhs: Compose<Rhs, Rule>, Rhs, Rule>(left: Lhs, right: Rhs) -> Lhs::Output {
    left.compose(right)
}

/// Adapter, adapter: `x` upstream of `y`, which composes as one adapter
/// wherever `>>` or [`compose`](fn@crate::compose) takes one, but does not
/// implement [`Adapter`](crate::Adapter) (planned, `docs/planned.md`).
/// Checked when it reaches an executor.
impl<X: Adapters, Y: Adapters<Accepts = X::Sends>> Compose<Y, WithAdapter> for X {
    type Output = ComposedAdapters<X, Y>;

    fn compose(self, y: Y) -> ComposedAdapters<X, Y> {
        ComposedAdapters::new(self, y)
    }
}

/// Adapter, executor: an executor whose calls pass through the adapters, once
/// they are checked.
impl<X: UpstreamOf<E>, E> Compose<E, WithExecutor> for X {
    type Output = Result<X::Output, BuildFaults>;

    fn compose(self, executor: E) -> Self::Output {
        self.upstream_of(executor, |_| ())
    }
}

/// Chain prefix, adapter: the prefix, with `y` downstream of its adapters.
impl<T, L: Adapters, Y: Adapters<Accepts = L::Sends>> Compose<Y, WithAdapter>
    for ChainPrefix<T, L>
{
    type Output = ChainPrefix<T, ComposedAdapters<L, Y>>;

    fn compose(self, y: Y) -> Self::Output {
        self.map_adapters(|adapters| ComposedAdapters::new(adapters, y))
    }
}

/// Chain prefix, executor: the chain of the prefix's driver on its adapters
/// upstream of `executor`, with the adapters' faults. It has no
/// program yet: the job's needs are checked against what the chain
/// offers when the job is given to it ([`Chain::run`]).
impl<T, L: UpstreamOf<E>, E> Compose<E, WithExecutor> for ChainPrefix<T, L> {
    type Output = Result<Chain<T, L::Accepts, L::Output>, BuildFaults>;

    fn compose(self, executor: E) -> Self::Output {
        let (driver, adapters) = self.into_parts();
        let mut offered = None;
        let executor = adapters.upstream_of(executor, |capabilities| {
            offered = Some(capabilities.clone());
        })?;
        let offered = offered.expect("adapters give what the chain offers to what is upstream");
        Ok(Chain::new(driver, executor, offered))
    }
}
