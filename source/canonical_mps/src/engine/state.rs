// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`CanonicalMpsState`]: a state of the
//! [`CanonicalMpsEngine`](super::CanonicalMpsEngine), which holds its chain
//! of site tensors.

use super::chain::Chain;
use super::statistics::CanonicalMpsStatistics;
use crate::site_algebra::SiteAlgebra;
use std::cell::RefCell;

/// A state of a [`CanonicalMpsEngine`](super::CanonicalMpsEngine): a chain
/// of site tensors in canonical form, wherever the algebra `A` keeps them,
/// with the bond each cut may grow to.
///
/// Every method that takes the state by shared reference may change its
/// chain. The reads move the centre, which changes the tensors but not the
/// state they describe. The reads and
/// [`MpsEngine::fork`](tensornet::MpsEngine::fork) apply the window that
/// fusion left pending, with the truncation its gates deferred. So the
/// chain sits in a [`RefCell`]. The state is `Send` when its algebra's
/// tensors are.
pub struct CanonicalMpsState<A: SiteAlgebra> {
    pub(super) chain: RefCell<Chain<A::Tensor>>,
}

impl<A: SiteAlgebra> CanonicalMpsState<A> {
    /// What the splits and moves of this state's history cost so far. A
    /// state made by [`MpsEngine::fork`](tensornet::MpsEngine::fork)
    /// starts with its parent's history.
    #[must_use]
    pub fn statistics(&self) -> CanonicalMpsStatistics {
        self.chain.borrow().statistics
    }
}

/// The chain's length, centre, retained weight and statistics. Not its
/// tensors: the algebra may keep them where the host cannot print them.
impl<A: SiteAlgebra> std::fmt::Debug for CanonicalMpsState<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_struct("CanonicalMpsState");
        if let Ok(chain) = self.chain.try_borrow() {
            debug
                .field("sites", &chain.sites.len())
                .field("centre", &chain.centre)
                .field("retained_weight", &chain.retained_weight)
                .field("statistics", &chain.statistics);
        }
        debug.finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests;
