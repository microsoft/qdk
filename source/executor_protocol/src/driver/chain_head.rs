// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The head of a chain being built ([`ChainHead`]): a driver, with the
//! adapters composed below it so far, which composes into a [`Chain`].

#[cfg(doc)]
use crate::{BuiltinDriver, Chain, Driver, driver};
use crate::{Instruction, NoAdapter};

/// The head of a chain being built: a driver, with the adapters composed
/// downstream of it so far (none at first). Composed with an adapter, it
/// gives a `ChainHead` with that adapter below the others; composed with an
/// executor, it gives a [`Chain`], which runs the job given to it. The
/// grouping does not matter: `(driver() >> a) >> executor` and
/// `driver() >> (a >> executor)?` build chains with the same run.
///
/// It holds the driver so that the driver composes with `>>`: Rust lets
/// this crate implement `>>` on its own types only, not on any type that
/// implements [`Driver`]. A chain's head is not itself a [`Driver`], even
/// with no adapter below its driver; that it is one, so that a driver with
/// adapters below it can be used wherever a driver is expected, is planned
/// (`docs/planned.md`).
///
/// Lifting a driver into a head is planned to become obsolete
/// (`docs/planned.md`): a driver will compose directly, by
/// `compose(driver, …)` if it drives one program type, and by
/// `driver >> …` if it is a crate's own, through the planned macro that
/// writes `>>` impls. Until then, `ChainHead::new(driver)` lifts it.
///
/// Another crate cannot name a head's type, whose adapters start from a
/// type private to this crate (planned: exporting it, `docs/planned.md`):
/// a runtime's crate has a function return its driver, and the caller
/// writes `ChainHead::new(runtime) >> …`.
///
/// # Type parameters
///
/// - `T`: the driver ([`Driver`]), given to [`ChainHead::new`]; the crate's
///   ([`BuiltinDriver`]) from [`driver()`](fn@driver).
/// - `L`: the adapters composed downstream of it by `>>`
///   ([`ComposedAdapters`](crate::ComposedAdapters)). At first no adapter
///   (`NoAdapter<I>`), whose `I`, the instructions the driver sends, is
///   fixed when the chain is composed, before any program is given: by the
///   first adapter downstream of it, or by the executor.
pub struct ChainHead<T, L> {
    driver: T,
    adapters: L,
}

impl<T, I: Instruction> ChainHead<T, NoAdapter<I>> {
    /// `driver` at the head of a chain, sending `I`s, with no adapter yet.
    #[must_use]
    pub fn new(driver: T) -> Self {
        Self {
            driver,
            adapters: NoAdapter::default(),
        }
    }
}

impl<T, L> ChainHead<T, L> {
    /// This head, with its adapters replaced by `f` of them.
    pub(crate) fn map_adapters<M>(self, f: impl FnOnce(L) -> M) -> ChainHead<T, M> {
        ChainHead {
            driver: self.driver,
            adapters: f(self.adapters),
        }
    }

    /// The driver and the adapters.
    pub(crate) fn into_parts(self) -> (T, L) {
        (self.driver, self.adapters)
    }
}
