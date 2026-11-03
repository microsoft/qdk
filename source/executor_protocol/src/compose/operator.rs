// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The operator `>>`: `a >> b` is `compose(a, b)` where it is implemented,
//! for the crate's types with an adapter on the right, and with one of the
//! crate's executors on the right.

use std::ops::Shr;

#[cfg(test)]
use crate::ComposedJob;
use crate::compose::adapters::DownstreamOf;
#[cfg(doc)]
use crate::compose::compose;
use crate::compose::{Compose, WithAdapter, WithExecutor};
use crate::{
    Adapter, AdapterExecutor, AdapterFn, AdapterFnExecutor, Adapters, Apply, ChainPrefix, Close,
    ComposedAdapters, Executor, ExecutorFn, InstructionBatch, NoAdapter, OnePerShotAdapter,
    OnePerShotAdapterExecutor, OnePerShotExecutor, OnePerShotPlacedExecutor, PlacedExecutor,
};

/// `>>` for the crate's types: `a >> b` is `compose(a, b)`, where `b` is
/// an adapter. A chain that starts at a driver is written with `>>` all
/// along, a component made by a function included, when the function
/// returns the type the facility gives ([`compose`]'s documentation).
///
/// Not for any executor: `>>` takes no rule to infer, and an impl for any
/// adapter and one for any executor would overlap. An executor is composed with
/// `compose`, or its crate implements `>>` for it on each of the four
/// public types that may be on its left, [`ChainPrefix`],
/// [`ComposedAdapters`], [`AdapterFn`] and [`OnePerShotAdapter`], so that
/// a chain groups either way, as this crate does
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

#[cfg(test)]
pub(crate) use compose_by_shr;

compose_by_shr!(T, L; ChainPrefix<T, L>);
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
        #[cfg(test)]
        compose_executor_by_shr!(@left [$($right_generics),*] $right [$($bounds)*]; P0, L0; ComposedJob<P0, L0>);
        compose_executor_by_shr!(@left [$($right_generics),*] $right [$($bounds)*]; T0, L0; ChainPrefix<T0, L0>);
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
