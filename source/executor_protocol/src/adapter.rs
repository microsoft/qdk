// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What an adapter is: a map from an executor to an executor, which may
//! hold state: given the executor downstream of it, it gives the executor
//! upstream of it. It describes what it gives ([`Adapter`]), so that
//! [`compose`](fn@crate::compose) checks it against what is downstream
//! before anything runs. A state and a logic make the smallest adapter
//! ([`adapter_fn`](fn@adapter_fn)), whose logic serves each call and
//! reaches below with a [`Below`]; one written by hand maps the whole
//! executor ([`Apply`]). An adapter is not an executor: composed with an
//! executor, it gives an executor.
//!
//! - [`adapter_fn`](mod@adapter_fn): a state and a logic as an adapter,
//!   with one state or a state per shot.
//! - [`below`]: what the logic of an `adapter_fn` reaches below with.
//! - [`boundary`]: where an adapter meets what is downstream of it, when it
//!   is built ([`AdapterBoundary`]).

pub(crate) mod adapter_fn;
pub(crate) mod below;
pub(crate) mod boundary;

use std::sync::Arc;

use crate::{
    AdapterBoundary, Close, ComponentId, Executor, Instruction, InstructionBatch, InstructionSet,
};
#[cfg(doc)]
use crate::{
    AdapterExecutor, AdapterSeeds, Below, BuildError, Capabilities, ShotContext, adapter_fn,
};

/// What an adapter is, apart from the executors it applies to: the
/// instructions it takes and sends, and what changes in the capabilities
/// of what is downstream of it.
///
/// Each method states one thing [`compose`](fn@crate::compose) checks or
/// derives, and each has a default for the adapter that does not change
/// it: an adapter that passes the calls it is called with down (a recorder,
/// a debugger) implements none of them.
//
// The notes name the three cases that reach this bound, since a diagnostic
// attribute cannot pick its note by the type on stable Rust:
// `compose(adapter, executor)` with an adapter that applies to one
// executor's type only (`impl Apply<Toy>`) reports the executor as not an
// `Adapter`, the rule for two adapters being the one left to try; adapters
// composed are passed where an `Adapter` is expected (planned,
// `docs/planned.md`); and an executor is put where an adapter is expected
// (after a part whose `>>` its crate did not implement, such as
// `driver() >> hand`, `hand` an executor of another crate).
#[diagnostic::on_unimplemented(
    note = "an adapter written by hand implements `Apply<E>` for every \
            `E: Executor<InstructionBatch<Sends>> + Close`, not for one executor's type \
            (see `executor_protocol::Apply`)",
    note = "adapters composed are not yet an `Adapter`: compose them with the executor first, \
            `(adapters >> executor)?`, and pass the executor made",
    note = "an executor goes last in a chain; one written by hand takes `>>` only where its crate \
            implements `Shr` for the part on its left: else write `compose(left, executor)`"
)]
pub trait Adapter {
    /// The instructions it is called with.
    type Accepts: Instruction;
    /// The instructions it sends downstream.
    type Sends: Instruction;

    /// Every instruction it sends, numbered as its calls number them;
    /// `None` when it sends the calls it is called with: those of the set
    /// it [`accepts`](Adapter::accepts), or of the downstream set when it
    /// states none. The set it sends, stated or accepted, is checked to
    /// [fit](InstructionSet::fits) the set downstream, and its calls are
    /// renumbered on their way down where the downstream set numbers them
    /// otherwise; calls of the downstream set need neither. An adapter that
    /// makes calls by opcode from instructions of another type states the
    /// set it sends, or it is refused ([`BuildError::UnstatedSends`]): the
    /// set it accepts does not number the calls it makes, so its opcodes
    /// could not be checked. One that decodes calls by opcode into typed
    /// instructions needs no set here: what it sends is typed.
    #[must_use]
    fn sends(&self) -> Option<Arc<InstructionSet>> {
        None
    }

    /// The instruction set it accepts. `None` when it accepts the downstream
    /// set as it is, unless it [`sends`](Adapter::sends) a set of its own: it
    /// then describes none, and a program that needs instructions is refused.
    /// An adapter that states this set and not what it sends sends what it
    /// accepts: its calls are checked against the set downstream, in any order,
    /// and renumbered into it; except one that makes calls by opcode from
    /// instructions of another type, which must state what it sends, and one
    /// that decodes calls by opcode into typed instructions, whose calls are
    /// checked only upstream of it, against this set. It is read before
    /// [`build`](Adapter::build), which cannot change it, so an adapter whose
    /// set depends on downstream's is given downstream's set when constructed.
    #[must_use]
    fn accepts(&self) -> Option<Arc<InstructionSet>> {
        None
    }

    /// The id it draws random numbers with, shot `j`'s from
    /// [`ShotContext::rng_seed`] of the run's seed and `j`, the run's seed
    /// being the one the last executor below is given: an
    /// [`adapter_fn`](fn@adapter_fn) is given it, and an adapter written by
    /// hand is given its seeds, bound to this id, when built
    /// ([`AdapterBoundary::drawing_seeds`]), so that it states the id here
    /// only. `None` when it draws none. Refused with [`BuildError::RngInUse`]
    /// when a component downstream draws with it, and with
    /// [`BuildError::NoRunSeed`] when no run's seed reaches it: only an
    /// [`executor_fn`](fn@crate::executor_fn) given a seed passes one up today.
    /// So an adapter written by hand that draws is tested composed over such an
    /// executor, which is how it gets its seeds, as in the simulator guide
    /// (`tests/simulator.rs`). An adapter that draws offers its id with those
    /// below it ([`Capabilities::rng_ids`]) and no `log_probability`, whatever
    /// it offers ([`AdapterBoundary::offered_upstream`]): the log probability
    /// its downstream executor reports does not hold its draws. One id per
    /// adapter: one that draws several streams derives them from its one seed,
    /// as a JAX key is split: stream `k` of a shot from
    /// [`shot_seed`](crate::shot_seed)`(s, k)`, with `k` in the place of the
    /// shot and `s` the adapter's seed for the shot
    /// ([`AdapterSeeds::shot`](crate::AdapterSeeds::shot)). The streams of one
    /// shot then have distinct seeds, since `k ↦ shot_seed(s, k)` is
    /// one-to-one; across shots, and against other components' seeds, they are
    /// distinct only as 64-bit hashes, not by construction.
    #[must_use]
    fn draws(&self) -> Option<ComponentId> {
        None
    }

    /// This adapter, built against what is downstream of it, once, before
    /// anything runs: for what it needs of what is downstream (a code's number
    /// of blocks, [`AdapterBoundary::downstream`], or the seeds it draws
    /// from, [`AdapterBoundary::drawing_seeds`]), to send in the downstream
    /// numbering ([`AdapterBoundary::adopt_downstream_numbering`]), to change
    /// what it offers upstream ([`AdapterBoundary::offered_upstream`]), and to
    /// report its own faults ([`AdapterBoundary::fault`]).
    ///
    /// It is called even when the boundary has faults of the protocol's
    /// (what it sends does not fit), so that its own are reported with
    /// them. The default changes nothing: an adapter that passes the calls
    /// it is called with down offers what downstream offers.
    #[must_use]
    fn build(self, boundary: &mut AdapterBoundary<'_>) -> Self
    where
        Self: Sized,
    {
        let _ = boundary;
        self
    }
}

/// How an adapter written by hand maps `E`, the executor downstream of it,
/// to the executor upstream of it, as a Tower layer (`tower::Layer`) maps a
/// service. Composed with an executor, it is applied once, to the whole
/// executor (its calls renumbered on their way down where that executor
/// numbers them otherwise), and gives the executor `apply` gives, within an
/// [`AdapterExecutor`].
///
/// An adapter implements it for every
/// `E: Executor<InstructionBatch<Self::Sends>> + Close`, as
/// `impl<E: ..> Apply<E> for Counter`: `E` is a type of the crate's that
/// passes each call and close to the executor below, not that executor's
/// type, so an impl for one executor's type composes with none. What the
/// adapter needs of the executor below it reads when built
/// ([`Adapter::build`]): what it offers ([`AdapterBoundary::downstream`]),
/// and its seeds ([`AdapterBoundary::drawing_seeds`]).
///
/// The executor it gives runs or refuses each call, closes `E` when it is
/// closed, and reports what it holds, `E`'s report included. It need not
/// keep the rest of the [`Executor`]'s rules: the executor composed with it
/// refuses every call once closed or after a call failed, a call that
/// panicked included, and closes it once.
///
/// It holds one state for the whole executor. State per shot is the
/// adapter's own, or the Shots axis (the crate's `docs/glossary.md`, §A
/// part's axes):
/// [`adapter_fn(..).shots(OnePerShot)`](crate::AdapterFn::shots)
/// ([`OnePerShot`](crate::OnePerShot)). An adapter written by hand that
/// draws otherwise than from one stream per shot (one stream for the whole
/// executor, say) clears [`replayable`](crate::Capabilities::replayable)
/// itself ([`AdapterBoundary::offered_upstream`]), since a shot's draws then
/// depend on the shots it is called with; one that draws one stream per
/// shot, from the shot's seed ([`AdapterSeeds::shot`]), keeps the
/// `replayable` of the executor below.
pub trait Apply<E>: Adapter {
    /// The executor it gives.
    type Output: Executor<InstructionBatch<Self::Accepts>> + Close;

    /// The executor upstream of `downstream`. An adapter that draws draws
    /// for shot `j` from the [`seed`](crate::ShotContext::seed) of
    /// `seeds.shot(j)`, `seeds` being those it kept when built
    /// ([`AdapterBoundary::drawing_seeds`], [`AdapterSeeds::shot`]).
    fn apply(self, downstream: E) -> Self::Output;
}
