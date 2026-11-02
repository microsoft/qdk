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
//! order of the components does): `driver >> (a >> b) >> executor` is
//! `driver >> a >> b >> executor`. Adapters grouped either way among
//! themselves report the same faults; a part composed with the executor
//! first is built on its own, and its faults are returned before what is
//! upstream of it is built or a driver's needs are checked.
//! Adapters are held until the chain reaches an executor; then each is
//! built once against what is downstream of it, from that executor
//! upstream ([`Adapter::build`]), and a driver's needs are checked against
//! what the chain offers, so that every fault is reported together, before
//! anything runs ([`BuildFaults`], each a [`BuildFault`] with its
//! [`BuildError`]).
//!
//! A chain is a type, fixed when it is compiled. Type-erased chains, chosen
//! at run time, are planned, with no milestone yet, for cases such as a
//! chain composed from Python at run time, a C API, or a transpiler chosen
//! by a string (`docs/planned.md`).
//!
//! - [`adapters`]: adapters composed, and applied to an executor once
//!   checked.
//! - [`run`]: the run a driver gives with an executor.
//! - [`chain`]: the chain an interpreter's driver gives with an executor,
//!   which runs the job given to it.
//!
//! What is composed has its own module: an adapter ([`Adapter`]), where it
//! meets what is downstream of it ([`AdapterBoundary`]), and a driver
//! ([`Driver`], [`InterpreterDriver`]).

pub(crate) mod adapters;
pub(crate) mod chain;
pub(crate) mod run;

use std::any::type_name;
use std::fmt;
use std::ops::Shr;
use std::sync::Arc;

use crate::compose::adapters::DownstreamOf;
use crate::instruction_set::write_mismatches;
use crate::{
    Adapter, AdapterExecutor, AdapterFn, AdapterFnExecutor, Adapters, Apply, Chain, Close,
    ComponentId, ComposedAdapters, Drive, Driver, Executor, ExecutorFn, Instruction,
    InstructionBatch, InstructionMismatch, InterpreterDriver, NoAdapter, OnePerShotAdapter,
    OnePerShotAdapterExecutor, OnePerShotExecutor, OnePerShotPlacedExecutor, PlacedExecutor, Run,
    UnmetNeed, UpstreamOf,
};
#[cfg(doc)]
use crate::{AdapterBoundary, Rejected};

/// Two entities put together into one, `Self` upstream of `Rhs`: the rules
/// of [`compose`](fn@crate::compose), one impl each.
///
/// `Rule` says whether the right side is an adapter or an executor
/// ([`WithExecutor`]), so that a rule whose right side is any adapter and
/// one whose right side is any executor do not overlap: a type of another
/// crate could be both. [`compose`](fn@crate::compose) infers it.
///
/// A pair that no rule composes fails to compile, which no test pins, with
/// a note that lists the rules, one that points a pair `>>` does not take
/// to [`compose`](fn@crate::compose), and one that points a job written as
/// a component to [`Chain::run`].
#[diagnostic::on_unimplemented(
    message = "`{Self}` does not compose upstream of `{Rhs}`",
    label = "no rule of `compose` puts `{Self}` upstream of this",
    note = "`compose` puts an adapter or a driver upstream of an adapter or an executor, where what the \
            upstream one sends is what the downstream one accepts (docs/composition.md, \"Composing a run\")",
    note = "where `>>` does not apply (an executor of another crate, or an adapter already composed \
            with its executor), write `compose(upstream, downstream)`",
    note = "if the right side is a program and its shots: they are a job, not a component; give them \
            to the chain an interpreter makes, `(interpreter() >> executor)?.run(program, shots)`"
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

/// `left` upstream of `right`: an adapter, or a driver, upstream of an adapter
/// or an executor. `left >> right` is the same where `>>` is implemented.
/// `Lhs` and `Rhs` are their types, and `Rule` the rule that composes them,
/// [`WithExecutor`] where `Rhs` is an executor, which the compiler infers.
///
/// | `compose`         | gives    | as                                       |
/// | ----------------- | -------- | ---------------------------------------- |
/// | adapter, executor | executor | the executor, calls through the adapters |
/// | adapter, adapter  | adapter  | [`ComposedAdapters`]                     |
/// | driver, adapter   | driver   | the driver with one more adapter         |
/// | driver, executor  | run      | [`Run`], which runs when waited for      |
/// |                   | chain    | [`Chain`], an interpreter's, runs a job  |
///
/// The grouping does not matter: a driver, adapters and an executor
/// composed in one order give the same run, or chain, however they are
/// grouped.
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
/// Where `right` is an executor, the driver's needs are checked against what
/// the chain offers, the number of its shots against the states the
/// executor holds at once ([`Capabilities::states`](crate::Capabilities)),
/// and each adapter against what is downstream of it, before anything
/// runs or any shot is made. An interpreter's driver has no program yet:
/// its adapters are checked here, and the job's needs and shots when the
/// job is given to the chain ([`Chain::run`]).
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
/// by hand over the executor `E`, so that `interpreter() >> counted()`
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
/// [`Adapter`] trait, so neither `>>` after a driver nor `compose` takes an
/// opaque adapter over an executor. An executor returned as a bare
/// `impl Executor<..> + Describe + Close` composes by `compose`, not by
/// `>>`, which takes an executor on its right only where it is implemented
/// for the executor's type.
///
/// # Errors
///
/// Where `right` is an executor, [`BuildFaults`]: every adapter that cannot
/// be built against what is downstream of it, and every need of a driver
/// that the chain does not meet ([`BuildError::Unmet`]), and why. The
/// executor, taken by value, is then dropped without being closed: an
/// executor whose `close` releases something (a device, a paid job) does so
/// only in its own `Drop`, if it has one. Known limit, open, which no test
/// pins: closing it, or giving it back with the faults, goes with the
/// planned cancellation.
pub fn compose<Lhs: Compose<Rhs, Rule>, Rhs, Rule>(left: Lhs, right: Rhs) -> Lhs::Output {
    left.compose(right)
}

/// Adapter, adapter: an adapter, `x` upstream of `y`. Checked when it reaches
/// an executor.
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
        self.upstream_of(executor, |_| Vec::new())
    }
}

/// Driver, adapter: the driver, with `y` downstream of its adapters.
impl<P: Drive, L: Adapters, Y: Adapters<Accepts = L::Sends>> Compose<Y, WithAdapter>
    for Driver<P, L>
{
    type Output = Driver<P, ComposedAdapters<L, Y>>;

    fn compose(self, y: Y) -> Self::Output {
        self.map_adapters(|adapters| ComposedAdapters::new(adapters, y))
    }
}

/// Driver, executor: the run of the driver on its adapters upstream of
/// `executor`, the driver's needs checked against what the chain offers,
/// with the adapters' faults.
impl<P, L, E> Compose<E, WithExecutor> for Driver<P, L>
where
    P: Drive,
    L: UpstreamOf<E, Accepts = P::Instruction>,
{
    type Output = Result<Run<P, L::Output>, BuildFaults>;

    fn compose(self, executor: E) -> Self::Output {
        let (program, shots, adapters) = self.into_parts();
        let needs = program.needs();
        let mut renumbering = None;
        let executor = adapters.upstream_of(executor, |offered| {
            let checked = offered.check_needs(
                type_name::<L::Output>(),
                Some(shots.as_slice().len()),
                &needs,
                P::Instruction::BY_OPCODE,
            );
            match checked {
                Ok(map) => {
                    renumbering = map.and_then(P::Instruction::renumbering).map(Arc::new);
                    Vec::new()
                }
                Err(unmet) => unmet
                    .into_iter()
                    .map(|unmet| BuildFault {
                        component: type_name::<P>(),
                        error: BuildError::Unmet(unmet),
                    })
                    .collect(),
            }
        })?;
        let executor = P::Instruction::downstream(executor, renumbering.as_ref());
        Ok(Run::new(program, shots, executor))
    }
}

/// Interpreter's driver, adapter: the driver, with `y` downstream of its
/// adapters.
impl<T, L: Adapters, Y: Adapters<Accepts = L::Sends>> Compose<Y, WithAdapter>
    for InterpreterDriver<T, L>
{
    type Output = InterpreterDriver<T, ComposedAdapters<L, Y>>;

    fn compose(self, y: Y) -> Self::Output {
        self.map_adapters(|adapters| ComposedAdapters::new(adapters, y))
    }
}

/// Interpreter's driver, executor: the chain of the interpreter on its
/// adapters upstream of `executor`, with the adapters' faults. It has no
/// program yet: the job's needs are checked against what the chain
/// offers when the job is given to it ([`Chain::run`]).
impl<T, L: UpstreamOf<E>, E> Compose<E, WithExecutor> for InterpreterDriver<T, L> {
    type Output = Result<Chain<T, L::Accepts, L::Output>, BuildFaults>;

    fn compose(self, executor: E) -> Self::Output {
        let (interpreter, adapters) = self.into_parts();
        let mut offered = None;
        let executor = adapters.upstream_of(executor, |capabilities| {
            offered = Some(capabilities.clone());
            Vec::new()
        })?;
        let offered = offered.expect("adapters give what the chain offers to what is upstream");
        Ok(Chain::new(interpreter, executor, offered))
    }
}

/// `>>` for the crate's types: `a >> b` is `compose(a, b)`, where `b` is
/// an adapter. A chain that starts at a driver is written with `>>` all
/// along, a component made by a function included, when the function
/// returns the type the facility gives ([`compose`]'s documentation).
///
/// Not for any executor: `>>` takes no rule to infer, and an impl for any
/// adapter and one for any executor would overlap. An executor is composed with
/// `compose`, or its crate implements `>>` on [`Driver`],
/// [`InterpreterDriver`] and [`ComposedAdapters`] for it, as this crate does
/// for the executors its facility makes and for an adapter, written by hand
/// or not, composed with an executor; likewise an adapter of another
/// crate, which this crate cannot give `>>`. Planned: a macro that writes
/// these impls (`docs/planned.md`).
macro_rules! compose_by_shr {
    ($($generics:ident),* ; $left:ty) => {
        impl<$($generics,)* Rhs> Shr<Rhs> for $left
        where
            Rhs: Adapters,
            Self: Compose<Rhs, WithAdapter>,
        {
            type Output = <Self as Compose<Rhs, WithAdapter>>::Output;

            fn shr(self, rhs: Rhs) -> Self::Output {
                self.compose(rhs)
            }
        }
    };
}

compose_by_shr!(P, L; Driver<P, L>);
compose_by_shr!(T, L; InterpreterDriver<T, L>);
compose_by_shr!(X, Y; ComposedAdapters<X, Y>);
compose_by_shr!(I; NoAdapter<I>);
compose_by_shr!(I, O, S, M, L, C, R; AdapterFn<I, O, S, M, L, C, R>);
compose_by_shr!(I, O, S, M, L, C, R; OnePerShotAdapter<I, O, S, M, L, C, R>);

/// `>>` with one of the crate's executors on the right, as an executor's
/// crate implements it: `a >> executor` is `compose(a, executor)` for each
/// executor the facility makes (`executor_fn`, and an adapter over an
/// executor, `adapter_fn` or written by hand), so that a chain ending at
/// one is written with `>>` all along, grouped either way.
macro_rules! compose_executor_by_shr {
    ([$($right_generics:ident),*] $right:ty) => {
        compose_executor_by_shr!([$($right_generics),*] $right where);
    };
    // The bounds of the executor's type, which every impl that names it
    // states.
    ([$($right_generics:ident),*] $right:ty where $($bounds:tt)*) => {
        compose_executor_by_shr!(@left [$($right_generics),*] $right [$($bounds)*]; P0, L0; Driver<P0, L0>);
        compose_executor_by_shr!(@left [$($right_generics),*] $right [$($bounds)*]; T0, L0; InterpreterDriver<T0, L0>);
        compose_executor_by_shr!(@left [$($right_generics),*] $right [$($bounds)*]; X0, Y0; ComposedAdapters<X0, Y0>);
        compose_executor_by_shr!(@left [$($right_generics),*] $right [$($bounds)*]; I0; NoAdapter<I0>);
        compose_executor_by_shr!(@left [$($right_generics),*] $right [$($bounds)*]; I0, O0, S0, M0, L0, C0, R0; AdapterFn<I0, O0, S0, M0, L0, C0, R0>);
        compose_executor_by_shr!(@left [$($right_generics),*] $right [$($bounds)*]; I0, O0, S0, M0, L0, C0, R0; OnePerShotAdapter<I0, O0, S0, M0, L0, C0, R0>);
    };
    (@left [$($right_generics:ident),*] $right:ty [$($bounds:tt)*]; $($left_generics:ident),*; $left:ty) => {
        impl<$($left_generics,)* $($right_generics),*> Shr<$right> for $left
        where
            Self: Compose<$right, WithExecutor>,
            $($bounds)*
        {
            type Output = <Self as Compose<$right, WithExecutor>>::Output;

            fn shr(self, executor: $right) -> Self::Output {
                self.compose(executor)
            }
        }
    };
}

compose_executor_by_shr!([I, S, M, P, C, L, R] ExecutorFn<I, S, M, P, C, L, R>);
compose_executor_by_shr!([I, S, M, P, C, L, R] OnePerShotExecutor<I, S, M, P, C, L, R>);
compose_executor_by_shr!(
    [I, S, M, P, T, C, L, R] OnePerShotPlacedExecutor<I, S, M, P, T, C, L, R>
);
compose_executor_by_shr!([E, T] PlacedExecutor<E, T>);
compose_executor_by_shr!([I, O, S, M, L, C, R, D] AdapterFnExecutor<I, O, S, M, L, C, R, D>);
compose_executor_by_shr!(
    [I, O, S, M, L, C, R, D] OnePerShotAdapterExecutor<I, O, S, M, L, C, R, D>
);
compose_executor_by_shr!(
    [A, E] AdapterExecutor<A, E>
    where
        A: Adapter + Apply<DownstreamOf<A, E>>,
        E: Executor<InstructionBatch<A::Sends>> + Close,
);

/// A component of a chain that cannot be built against what is downstream
/// of it, and why: an adapter, or a driver whose needs the chain does not
/// meet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildFault {
    /// The component's type.
    pub component: &'static str,
    /// Why it cannot be built.
    pub error: BuildError,
}

/// Every fault of a chain built against an executor, upstream first (the
/// driver's, then each adapter's), so that whoever builds the chain can fix
/// them all at once. It reads as one message: each fault as its component
/// and its error (`component: error`), in their order, joined by `"; "`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildFaults(pub Vec<BuildFault>);

impl fmt::Display for BuildFaults {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_faults(f, &self.0, |f, BuildFault { component, error }| {
            write!(f, "{component}: {error}")
        })
    }
}

/// Writes `faults` as one message, in their order, each by `write`,
/// separated by `"; "`: the one spelling of a list of faults, for
/// [`BuildFaults`] and [`JobFaults`](crate::JobFaults).
pub(crate) fn write_faults<F>(
    f: &mut fmt::Formatter<'_>,
    faults: &[F],
    write: impl Fn(&mut fmt::Formatter<'_>, &F) -> fmt::Result,
) -> fmt::Result {
    for (i, fault) in faults.iter().enumerate() {
        if i > 0 {
            f.write_str("; ")?;
        }
        write(f, fault)?;
    }
    Ok(())
}

impl std::error::Error for BuildFaults {}

/// Why a component of a chain cannot be built against the components
/// downstream of it.
///
/// Two errors are equal when they state the same fault; an adapter's own
/// fault is equal only to itself and its clones, as its type is the
/// adapter's.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum BuildError {
    /// A component downstream already draws with this id, so the two would draw
    /// the same numbers in every shot: one of them must be given another id
    /// ([`AdapterFn::drawing`](crate::AdapterFn::drawing)).
    RngInUse(ComponentId),
    /// An adapter draws with this id ([`Adapter::draws`]), and no run's seed
    /// reaches it: the last executor below is given none
    /// ([`ExecutorFn::seed`](crate::ExecutorFn::seed)). Refused when built
    /// rather than at its first draw, since a seed made up for it would draw
    /// the same values in every run, and one drawn from the system could not be
    /// replayed; an [`adapter_fn`](fn@crate::adapter_fn) and an adapter written
    /// by hand alike.
    ///
    /// Only an [`executor_fn`](fn@crate::executor_fn) given a seed passes
    /// one up today. An executor written by hand cannot state one yet
    /// (planned), so an adapter that draws is refused over it: for example
    /// a noise model over the user's own simulator.
    NoRunSeed(ComponentId),
    /// An adapter sends instructions that the downstream executor does not
    /// accept: each of them, in the order the adapter declares them.
    Instructions(Vec<InstructionMismatch>),
    /// An adapter is built against an executor that describes no
    /// instruction set, so what it sends cannot be checked: an adapter that
    /// states the set it sends, or only the set it accepts and sends
    /// instructions of the kind it is called with (by opcode or typed), or
    /// one that sends calls by opcode and states no set: one that passes
    /// them through as they are, or one that makes them from instructions
    /// of another type, then also refused as [`Self::UnstatedSends`].
    NoInstructionSetDownstream,
    /// An adapter makes calls by opcode from instructions of another type,
    /// and states no set they are numbered by
    /// ([`Adapter::sends`]); the set it accepts, if
    /// any, describes the instructions it is called with, not its calls. So
    /// its opcodes cannot be checked downstream.
    UnstatedSends,
    /// A fault of the adapter's own, of a type it defines
    /// ([`AdapterBoundary::fault`](crate::AdapterBoundary::fault)): for example
    /// a code whose block needs more qubits than the downstream executor holds.
    /// Equal only to itself: two faults compare equal when they share one
    /// error.
    Adapter(Arc<dyn std::error::Error + Send + Sync>),
    /// A need of the driver that the chain downstream of it does not meet
    /// ([`UnmetNeed`], as [`Rejected::Unmet`] states it), reported when the
    /// driver is composed with an executor.
    Unmet(UnmetNeed),
}

impl PartialEq for BuildError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (BuildError::RngInUse(a), BuildError::RngInUse(b))
            | (BuildError::NoRunSeed(a), BuildError::NoRunSeed(b)) => a == b,
            (BuildError::Instructions(a), BuildError::Instructions(b)) => a == b,
            (BuildError::NoInstructionSetDownstream, BuildError::NoInstructionSetDownstream)
            | (BuildError::UnstatedSends, BuildError::UnstatedSends) => true,
            (BuildError::Adapter(a), BuildError::Adapter(b)) => Arc::ptr_eq(a, b),
            (BuildError::Unmet(a), BuildError::Unmet(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for BuildError {}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BuildError::RngInUse(id) => write!(
                f,
                "{id} are already drawn from downstream: two components would share them; \
                 give one of them another id, for example with \
                 adapter_fn(..).drawing(ComponentId::named(name))"
            ),
            BuildError::NoRunSeed(id) => write!(
                f,
                "{id} have no run's seed to be drawn from: give the last executor one, \
                 for example with executor_fn(..).seed(seed); for an executor written by \
                 hand, see BuildError::NoRunSeed"
            ),
            BuildError::Instructions(mismatches) => {
                f.write_str("the instructions sent do not fit downstream: ")?;
                write_mismatches(f, mismatches)
            }
            BuildError::NoInstructionSetDownstream => {
                f.write_str("the downstream executor describes no instruction set")
            }
            BuildError::UnstatedSends => f.write_str(
                "the adapter makes calls by opcode and states no set they are numbered by",
            ),
            BuildError::Adapter(fault) => fault.fmt(f),
            BuildError::Unmet(unmet) => unmet.fmt(f),
        }
    }
}

impl std::error::Error for BuildError {
    /// An adapter's own fault's source, as its text is the fault's.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            BuildError::Adapter(fault) => fault.source(),
            _ => None,
        }
    }
}
