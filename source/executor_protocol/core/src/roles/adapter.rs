// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The adapter's role, [`Adapter`]: a part between a driver and an
//! executor that receives and sends one instruction set type.

use crate::{
    Batch, BatchReadouts, Binding, Capabilities, ExecuteError, Executor, InstructionSet, NotReady,
    Seed, ShotSet,
};

/// Answers each `execute` it receives, in the instructions it accepts, by
/// calling the part below, in the instructions it sends, both of its set
/// type [`Set`](Self::Set), as many times as it needs, and maps the
/// readouts back. A part that sends another set type than it receives is
/// a [`TranslatingAdapter`](crate::TranslatingAdapter); every `Adapter` is
/// one.
///
/// Such as a code that turns each logical instruction into physical ones,
/// a noise model that adds instructions, or a debugger that splits a batch
/// to stop between its instructions. The protocol shapes the readouts of
/// each `execute` it receives and each it sends, and checks each batch of
/// both its sender marked [`checked`](Batch::checked), so it writes only
/// its own logic.
///
/// # Declarations
///
/// The joins ([`DriverJoins`](crate::DriverJoins) and
/// [`AdapterJoins`](crate::AdapterJoins)) read them before any run:
/// [`name`](Self::name), [`accepts`](Self::accepts),
/// [`sends`](Self::sends), [`provides`](Self::provides) and
/// [`requires`](Self::requires). The join to the part below hands back
/// what it made of them: [`bind`](Self::bind) gets the fit's binding.
/// A join only decides, from what the parts declare; what prepares a part
/// to run belongs to each run.
///
/// What it accepts, sends and provides must not change once it is joined:
/// the fits and `bind`'s binding are made once, from them, and each batch
/// sent to it is checked against what the join read, its count of
/// instructions always, its instructions when the batch is marked
/// checked. Each run's start reads what it provides again, to check what
/// arrives at it, and asks what it requires again, of what arrives then.
///
/// # Run
///
/// On every run, the runner gets it ready ([`ready`](Self::ready)) and
/// ends its shots ([`end_shots`](Self::end_shots)), and the part above
/// calls it ([`adapt`](Self::adapt)).
pub trait Adapter {
    /// The instruction set type it receives and sends: the parts above and
    /// below must send and accept the same, checked at compile time. What
    /// it accepts and what it sends may still differ in value.
    type Set: InstructionSet;

    /// As [`Executor::name`].
    fn name(&self) -> &str {
        std::any::type_name::<Self>()
    }

    /// The instructions it receives: what the part above sends must fit
    /// them.
    fn accepts(&self) -> &Self::Set;

    /// The instructions it sends: the part below must accept them.
    fn sends(&self) -> &Self::Set;

    /// Its fixed limits on what arrives from above, such as a job queue's
    /// most shots, or what its set's capabilities say it takes: what
    /// arrives must fit them, as [`Capabilities`] says.
    ///
    /// By default its set's
    /// [`unconstrained`](InstructionSet::unconstrained): it states nothing.
    /// Stating its real limits is the writer's job: an adapter that does
    /// not lets `on` accept a chain that sends it more than it can take,
    /// which then fails as it runs.
    fn provides(&self) -> &Capabilities<Self::Set> {
        Self::Set::unconstrained()
    }

    /// What it requires of the part below, given `above`, what arrives
    /// from the part above: such as a job splitter's runs of at most 100
    /// shots, whatever run arrives, or, in its set's capabilities, what an
    /// encoding needs below for what arrives.
    ///
    /// By default `above` itself: it passes what arrives below unchanged.
    /// Stating what it changes is the writer's job: an adapter that needs
    /// more below, and does not say so, lets `on` accept a chain whose
    /// executor cannot hold it, which then fails as it runs.
    ///
    /// It is deterministic, and monotone: given an `above` that requires
    /// no more than another, as [`Capabilities`] defines it, it requires
    /// no more than given the other. The protocol does not check either,
    /// and relies on both: `on` checks a chain with nothing arriving at
    /// its top, so a part short then is short on every run; and a joined
    /// part calls `requires` again at `ready`, for what its run's start
    /// checked (`docs/capabilities.md`, The rule).
    fn requires(&self, above: &Capabilities<Self::Set>) -> Capabilities<Self::Set> {
        above.clone()
    }

    /// Receives, once, the fit's binding: what the
    /// [`fits`](crate::BoundaryCompatible::fits) of the set it sends gave
    /// against the part below's, such as the receiver's encoding of each
    /// instruction it sends, which every instruction it sends must then
    /// carry. The join to the part below calls it once, after the fit and
    /// before any run; an adapter keeps the binding to encode each
    /// instruction it forwards or adds.
    fn bind(&mut self, below: Binding<Self::Set>);

    /// As [`Executor::ready`], for `above`, what arrives from the part
    /// above.
    fn ready(
        &mut self,
        above: &Capabilities<Self::Set>,
    ) -> impl Future<Output = Result<(), NotReady>> {
        let _ = above;
        async { Ok(()) }
    }

    /// Answers `(batches, seed)` by filling `readouts`, shaped by the
    /// protocol as [`Executor::execute`] says: `readouts[i]` holds those of
    /// `batches[i]`. It calls `below.execute`, the rest of the chain as one
    /// executor, any number of times, each with the shots it chooses,
    /// `seed` passed on unchanged, and readouts of its choice: `readouts`
    /// itself, when it sends below batches of the same shots and readout
    /// slots as those it got, or a [`Readouts`](crate::Readouts) it keeps
    /// to reuse, when it maps the readouts below to its own. Passed below,
    /// `readouts` are shaped again by the boundary below, so it writes into
    /// them only after `below.execute` answers. After a failure below, it
    /// may resend with the same readouts: the protocol shapes them again.
    /// As an executor does, it writes every value of `readouts` and hands
    /// each batch's up in that batch's shape, which the protocol does not
    /// check once it answers; a conformance kit, planned, will check both
    /// in the implementer's tests.
    ///
    /// Each batch it sends below holds only shots of the batches it got: a
    /// rule the adapter keeps, which the protocol does not check, and
    /// whose breach [`Driver::drive`](crate::Driver::drive) states.
    ///
    /// Each batch it sends below for a [`checked`](Batch::checked) batch is
    /// checked too, as
    /// [`checked_if(batch.is_checked())`](Batch::checked_if) marks it, so
    /// that the checks its sender chose reach every boundary below; it may
    /// mark checked a batch it sends for an unchecked one. It is a rule the
    /// adapter keeps, which the protocol does not check; a conformance kit,
    /// planned, will check it in the implementer's tests.
    ///
    /// An `adapt` does its work, calling `below` included, when its future
    /// is first polled, not when it is called, as Rust's futures do by
    /// convention (an `async fn adapt`'s does), and as
    /// [`Executor::execute`] asks of the `execute` the adapter answers for
    /// the chain it heads: a future dropped before its first poll has then
    /// sent nothing below. It is a rule for implementers, which the
    /// protocol does not check.
    fn adapt(
        &mut self,
        batches: &[Batch<'_, Self::Set>],
        seed: Seed,
        below: &mut impl Executor<Accepts = Self::Set>,
        readouts: &mut [BatchReadouts<Self::Set>],
    ) -> impl Future<Output = Result<(), ExecuteError>>;

    /// As [`Executor::end_shots`].
    fn end_shots(&mut self, shots: &ShotSet) {
        let _ = shots;
    }
}

// A lent adapter is the adapter itself: every method forwards, and the
// lint fails the build if a new one falls back to its default instead.
// Through the blanket `TranslatingAdapter` of every `Adapter`, a lent
// adapter is a translating one too.
#[deny(clippy::missing_trait_methods)]
impl<A: Adapter + ?Sized> Adapter for &mut A {
    type Set = A::Set;

    fn name(&self) -> &str {
        (**self).name()
    }

    fn accepts(&self) -> &A::Set {
        (**self).accepts()
    }

    fn sends(&self) -> &A::Set {
        (**self).sends()
    }

    fn provides(&self) -> &Capabilities<A::Set> {
        (**self).provides()
    }

    fn requires(&self, above: &Capabilities<A::Set>) -> Capabilities<A::Set> {
        (**self).requires(above)
    }

    fn bind(&mut self, below: Binding<A::Set>) {
        (**self).bind(below);
    }

    fn ready(
        &mut self,
        above: &Capabilities<A::Set>,
    ) -> impl Future<Output = Result<(), NotReady>> {
        (**self).ready(above)
    }

    fn adapt(
        &mut self,
        batches: &[Batch<'_, A::Set>],
        seed: Seed,
        below: &mut impl Executor<Accepts = A::Set>,
        readouts: &mut [BatchReadouts<A::Set>],
    ) -> impl Future<Output = Result<(), ExecuteError>> {
        (**self).adapt(batches, seed, below, readouts)
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        (**self).end_shots(shots);
    }
}
