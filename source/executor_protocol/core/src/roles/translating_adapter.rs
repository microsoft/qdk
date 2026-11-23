// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The general adapter's role, [`TranslatingAdapter`]: a part between a
//! driver and an executor that may send another instruction set type than
//! it receives. Every [`Adapter`] is one.

use super::Sealed;
use crate::composition::DeclaredParts;
use crate::part_id::ChainPosition;
use crate::{
    Adapter, Batch, BatchReadouts, Binding, Capabilities, ExecuteError, Executor, InstructionSet,
    NotReady, Seed, ShotSet,
};

/// A part between a driver and an executor whose instructions below may
/// be of another set type ([`Sends`](Self::Sends)) than those it receives
/// ([`Accepts`](Self::Accepts)), such as an encoding of logical gates into
/// physical ones, or any part that holds an adapter of either kind. The
/// joins and the runner take it; every [`Adapter`] is one, as is a
/// [`Lowered`](crate::Lowered) lowering, the per-instruction way to
/// write one.
///
/// Each method is as [`Adapter`]'s, or, for what it provides and
/// requires, and getting ready for what arrives, as
/// [`AdapterLimits`](crate::AdapterLimits)'s, but
/// [`requires`](Self::requires) has no default: what arrives is of another
/// set than what it requires.
pub trait TranslatingAdapter {
    /// The instruction set type it receives: the part above must send the
    /// same, checked at compile time.
    type Accepts: InstructionSet;

    /// The instruction set type it sends: the part below must accept the
    /// same, checked at compile time.
    type Sends: InstructionSet;

    /// As [`Adapter::name`].
    fn name(&self) -> &str {
        std::any::type_name::<Self>()
    }

    /// As [`Adapter::accepts`].
    fn accepts(&self) -> &Self::Accepts;

    /// As [`Adapter::sends`].
    fn sends(&self) -> &Self::Sends;

    /// As [`AdapterLimits::provides`](crate::AdapterLimits::provides).
    fn provides(&self) -> &Capabilities<Self::Accepts> {
        Self::Accepts::unconstrained()
    }

    /// As [`AdapterLimits::requires`](crate::AdapterLimits::requires), with
    /// no default.
    fn requires(&self, above: &Capabilities<Self::Accepts>) -> Capabilities<Self::Sends>;

    /// As [`Adapter::bind`].
    fn bind(&mut self, below: Binding<Self::Sends>);

    /// As [`AdapterLimits::ready_for`](crate::AdapterLimits::ready_for).
    fn ready(
        &mut self,
        above: &Capabilities<Self::Accepts>,
    ) -> impl Future<Output = Result<(), NotReady>> {
        let _ = above;
        async { Ok(()) }
    }

    /// As [`Adapter::adapt`].
    fn adapt(
        &mut self,
        batches: &[Batch<'_, Self::Accepts>],
        seed: Seed,
        below: &mut impl Executor<Accepts = Self::Sends>,
        readouts: &mut [BatchReadouts<Self::Accepts>],
    ) -> impl Future<Output = Result<(), ExecuteError>>;

    /// As [`Adapter::end_shots`].
    fn end_shots(&mut self, shots: &ShotSet) {
        let _ = shots;
    }

    // Protocol hooks: sealed and hidden, as `Driver`'s are.

    /// Adds this part to `parts`, given `above`, what arrives at it, and
    /// gives what it requires of the part below. Only the crate overrides
    /// it, to add each part it holds, top to bottom. The
    /// [`on`](crate::DriverJoins::on) that makes a runner calls it, and
    /// the runner on each run.
    #[doc(hidden)]
    fn declare<'a>(
        &'a self,
        above: &Capabilities<Self::Accepts>,
        parts: &mut DeclaredParts<'a>,
    ) -> Capabilities<Self::Sends> {
        parts.push(TranslatingAdapter::name(self));
        parts.receive(self.accepts(), self.provides(), above);
        let required = self.requires(above);
        parts.send(self.sends(), &required);
        required
    }

    /// Adds each part it holds to `parts`, top to bottom, by name alone,
    /// without asking what any requires: how a join counts the parts above
    /// the boundary it makes, before any run says what arrives. Only the
    /// crate overrides it, to add each part it holds.
    #[doc(hidden)]
    fn count_parts<'a>(&'a self, parts: &mut DeclaredParts<'a>) {
        parts.push(TranslatingAdapter::name(self));
    }

    /// [`ready`](Self::ready), for this part at `at` in its chain. Only
    /// the crate overrides it, to name a refusal of a part it holds by
    /// that part's position.
    #[doc(hidden)]
    fn ready_at(
        &mut self,
        above: &Capabilities<Self::Accepts>,
        at: ChainPosition<'_>,
    ) -> impl Future<Output = Result<(), NotReady>> {
        let _ = at;
        self.ready(above)
    }
}

// Every method forwards, but the protocol hooks: an `Adapter` cannot
// override them, so their defaults are its. What it provides and requires,
// and getting it ready for what arrives, are the adapter's own hooks: what
// a `Limited` adapter states, else nothing.
impl<A: Adapter> TranslatingAdapter for A {
    type Accepts = A::Set;
    type Sends = A::Set;

    fn name(&self) -> &str {
        Adapter::name(self)
    }

    fn accepts(&self) -> &A::Set {
        Adapter::accepts(self)
    }

    fn sends(&self) -> &A::Set {
        Adapter::sends(self)
    }

    fn provides(&self) -> &Capabilities<A::Set> {
        Adapter::provision(self, Sealed(()))
    }

    fn requires(&self, above: &Capabilities<A::Set>) -> Capabilities<A::Set> {
        Adapter::requirement(self, above, Sealed(()))
    }

    fn bind(&mut self, below: Binding<A::Set>) {
        Adapter::bind(self, below);
    }

    fn ready(
        &mut self,
        above: &Capabilities<A::Set>,
    ) -> impl Future<Output = Result<(), NotReady>> {
        Adapter::ready_at(self, above, ChainPosition::UNKNOWN)
    }

    fn adapt(
        &mut self,
        batches: &[Batch<'_, A::Set>],
        seed: Seed,
        below: &mut impl Executor<Accepts = A::Set>,
        readouts: &mut [BatchReadouts<A::Set>],
    ) -> impl Future<Output = Result<(), ExecuteError>> {
        Adapter::adapt(self, batches, seed, below, readouts)
    }

    fn end_shots(&mut self, shots: &ShotSet) {
        Adapter::end_shots(self, shots);
    }
}

/// A lent translating adapter of the crate's, `&mut $ty` for the generics
/// `[...]`, as the adapter itself: every method forwards, and the lint
/// fails the build if a new one falls back to its default instead.
///
/// One impl per type, not one for every `&mut A`: that one would overlap
/// the blanket impl above, since another crate may implement [`Adapter`]
/// for its own `&mut` type. A lent [`Adapter`] is one through `Adapter`'s
/// own `&mut A` impl.
macro_rules! lent_translating_adapter {
    ([$($generics:tt)*] $ty:ty) => {
        #[deny(clippy::missing_trait_methods)]
        impl<$($generics)*> $crate::TranslatingAdapter for &mut $ty {
            type Accepts = <$ty as $crate::TranslatingAdapter>::Accepts;
            type Sends = <$ty as $crate::TranslatingAdapter>::Sends;

            fn name(&self) -> &str {
                <$ty as $crate::TranslatingAdapter>::name(&**self)
            }

            fn accepts(&self) -> &Self::Accepts {
                <$ty as $crate::TranslatingAdapter>::accepts(&**self)
            }

            fn sends(&self) -> &Self::Sends {
                <$ty as $crate::TranslatingAdapter>::sends(&**self)
            }

            fn provides(&self) -> &$crate::Capabilities<Self::Accepts> {
                <$ty as $crate::TranslatingAdapter>::provides(&**self)
            }

            fn requires(
                &self,
                above: &$crate::Capabilities<Self::Accepts>,
            ) -> $crate::Capabilities<Self::Sends> {
                <$ty as $crate::TranslatingAdapter>::requires(&**self, above)
            }

            fn bind(&mut self, below: $crate::Binding<Self::Sends>) {
                <$ty as $crate::TranslatingAdapter>::bind(&mut **self, below);
            }

            fn ready(
                &mut self,
                above: &$crate::Capabilities<Self::Accepts>,
            ) -> impl Future<Output = Result<(), $crate::NotReady>> {
                <$ty as $crate::TranslatingAdapter>::ready(&mut **self, above)
            }

            fn adapt(
                &mut self,
                batches: &[$crate::Batch<'_, Self::Accepts>],
                seed: $crate::Seed,
                below: &mut impl $crate::Executor<Accepts = Self::Sends>,
                readouts: &mut [$crate::BatchReadouts<Self::Accepts>],
            ) -> impl Future<Output = Result<(), $crate::ExecuteError>> {
                <$ty as $crate::TranslatingAdapter>::adapt(&mut **self, batches, seed, below, readouts)
            }

            fn end_shots(&mut self, shots: &$crate::ShotSet) {
                <$ty as $crate::TranslatingAdapter>::end_shots(&mut **self, shots);
            }

            fn declare<'a>(
                &'a self,
                above: &$crate::Capabilities<Self::Accepts>,
                parts: &mut $crate::composition::DeclaredParts<'a>,
            ) -> $crate::Capabilities<Self::Sends> {
                <$ty as $crate::TranslatingAdapter>::declare(&**self, above, parts)
            }

            fn count_parts<'a>(&'a self, parts: &mut $crate::composition::DeclaredParts<'a>) {
                <$ty as $crate::TranslatingAdapter>::count_parts(&**self, parts);
            }

            fn ready_at(
                &mut self,
                above: &$crate::Capabilities<Self::Accepts>,
                at: $crate::part_id::ChainPosition<'_>,
            ) -> impl Future<Output = Result<(), $crate::NotReady>> {
                <$ty as $crate::TranslatingAdapter>::ready_at(&mut **self, above, at)
            }
        }
    };
}

pub(crate) use lent_translating_adapter;
