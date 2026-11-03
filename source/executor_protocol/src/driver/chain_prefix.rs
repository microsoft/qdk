// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A chain's prefix ([`ChainPrefix`]): a driver, with the adapters composed
//! below it so far, which composes into a [`Chain`]; and how a driver
//! becomes one ([`DriverExt::into_chain`]).

#[cfg(doc)]
use crate::{BuiltinDriver, Chain, driver};
use crate::{Driver, Instruction, NoAdapter};

/// A chain's prefix: a driver, with the adapters composed downstream of it
/// so far (none at first), as a chain is built left to right. Composed with an adapter, it gives a
/// `ChainPrefix` with that adapter below the others; composed with an
/// executor, it gives a [`Chain`], which runs the job given to it. The
/// grouping does not matter: `(driver() >> a) >> executor` and
/// `driver() >> (a >> executor)?` build chains with the same run.
///
/// It is a newtype over the driver, because Rust's orphan rule lets a
/// crate implement `>>` on its own types only, not on any type that
/// implements [`Driver`]; [`into_chain`](DriverExt::into_chain) wraps a
/// driver in it. A prefix is not itself a [`Driver`], even with no
/// adapter below its driver; that it is one, so that a driver with
/// adapters below it can be used wherever a driver is expected, is
/// planned (`docs/planned.md`).
///
/// Another crate cannot name a prefix's type, whose adapters start from a
/// type private to this crate (planned: exporting it, `docs/planned.md`):
/// a runtime's crate has a function return its driver, and the caller
/// writes `runtime.into_chain() >> …`.
///
/// # Type parameters
///
/// - `T`: the driver ([`Driver`]), given to
///   [`into_chain`](DriverExt::into_chain); the crate's ([`BuiltinDriver`])
///   from [`driver()`](fn@driver).
/// - `L`: the adapters composed downstream of it by `>>`
///   ([`ComposedAdapters`](crate::ComposedAdapters)). At first no adapter
///   (`NoAdapter<I>`), whose `I`, the instructions the driver sends, is
///   fixed when the chain is composed, before any program is given: by the
///   first adapter downstream of it, or by the executor.
pub struct ChainPrefix<T, L> {
    driver: T,
    adapters: L,
}

impl<T, I: Instruction> ChainPrefix<T, NoAdapter<I>> {
    /// `driver` as a chain's prefix, sending `I`s, with no adapter yet.
    #[must_use]
    pub(crate) fn new(driver: T) -> Self {
        Self {
            driver,
            adapters: NoAdapter::default(),
        }
    }
}

impl<T, L> ChainPrefix<T, L> {
    /// This prefix, with its adapters replaced by `f` of them.
    pub(crate) fn map_adapters<M>(self, f: impl FnOnce(L) -> M) -> ChainPrefix<T, M> {
        ChainPrefix {
            driver: self.driver,
            adapters: f(self.adapters),
        }
    }

    /// The driver and the adapters.
    pub(crate) fn into_parts(self) -> (T, L) {
        (self.driver, self.adapters)
    }
}

/// How a driver joins a chain: `runtime.into_chain() >> adapters >>
/// executor` builds a [`Chain`] of a driver from any crate, as
/// [`driver()`](fn@driver) does of the crate's own. Implemented for every
/// [`Driver`].
///
/// A driver of several program types names one of them,
/// `DriverExt::<P>::into_chain(runtime)`, only so that the call resolves:
/// the call alone does not say which impl it means. The chain runs any
/// program the driver drives.
///
/// A type that drives no program has no `into_chain`:
///
/// ```compile_fail,E0277
/// use executor_protocol::{DriverExt, InstructionCall};
///
/// struct Interpreter; // implements no `Driver`
///
/// let prefix = DriverExt::<()>::into_chain::<InstructionCall>(Interpreter);
/// ```
pub trait DriverExt<P>: Driver<P> + Sized {
    /// This driver as a chain's prefix, sending `I`s, with no adapter
    /// yet. `I` is fixed by what it is composed with: the first adapter
    /// downstream of it, or the executor.
    #[must_use]
    fn into_chain<I: Instruction>(self) -> ChainPrefix<Self, NoAdapter<I>> {
        ChainPrefix::new(self)
    }
}

impl<T: Driver<P>, P> DriverExt<P> for T {}
