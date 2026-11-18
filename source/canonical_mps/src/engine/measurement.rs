// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A measurement's operations on one site: probabilities, conditional
//! probabilities and projections, at the centre, on host copies of the
//! sites they read ([`host_site`](super::host_site)).
//!
//! Between gates, a program measures qubits: a simulator reads the
//! probability of a value, projects the state on the value it drew, and
//! draws samples from conditional probabilities. With the centre on a
//! site, the rest of the chain drops out of the site's reads, so a read
//! moves the centre there ([`centre_moves`](super::centre_moves)) and
//! reads the site alone, through [`SiteAlgebra::read`], its pending gate
//! applied. The rules, stated for callers on [`MpsEngine`]'s methods:
//!
//! - **The state is kept.** A read does not change the state, except by
//!   applying a truncation a gate deferred: it moves the centre, which
//!   changes the tensors, not the state, and applies the window that
//!   fusion left pending, which truncates as any other operation's
//!   application of it would ([`fusion`](super::fusion)). A read with
//!   conditions projects them on a fork, so the caller's state is not
//!   projected.
//! - **Each value from its own weight.** A probability is the value's
//!   weight at the centre (the squared norm of the centre's elements for
//!   that value) over the sum of both values' weights, not one minus the
//!   other's: a value of weight exactly zero reads exactly 0, and the other
//!   exactly 1, and a rare value whose complement rounds to one is not
//!   refused.
//! - **Projections.** A projection refuses a value of probability exactly
//!   zero, or NaN, before it projects. It divides the site by the norm of
//!   what it kept, computed scaled rather than as `√p`, and uploads the
//!   result as the site's new tensor. It leaves the retained weight as it
//!   is and drops no singular value of its own.

use super::{CanonicalMpsEngine, CanonicalMpsError, CanonicalMpsState, chain::Chain};
use crate::site_algebra::SiteAlgebra;
use tensornet::{MpsEngine, Refusal};

impl<A: SiteAlgebra> CanonicalMpsEngine<A> {
    /// P(Zero) and P(One) of `site`, with the centre moved there.
    pub(super) fn probabilities(
        &self,
        chain: &mut Chain<A::Tensor>,
        site: usize,
    ) -> Result<[f64; 2], CanonicalMpsError<A::Error>> {
        self.centre_on(chain, site)?;
        let centre = self.host_site(chain, site)?;
        let [zero, one] = [centre.weight_of(0), centre.weight_of(1)];
        let total = zero + one;
        Ok([zero / total, one / total])
    }

    /// P(Zero) and P(One) of `site` given `given` (checked): on a fork when
    /// `given` is not empty, each condition projected in order.
    pub(super) fn conditional_probabilities(
        &self,
        state: &CanonicalMpsState<A>,
        site: usize,
        given: &[(usize, bool)],
    ) -> Result<[f64; 2], CanonicalMpsError<A::Error>> {
        let mut fork;
        let mut borrowed;
        let chain = if given.is_empty() {
            borrowed = state.chain.borrow_mut();
            &mut *borrowed
        } else {
            fork = self.fork(state)?;
            fork.chain.get_mut()
        };
        for &(at, one) in given {
            self.project_on(chain, at, one)?;
        }
        self.probabilities(chain, site)
    }

    /// Projects `site` (checked) on `one`, and returns the value's
    /// probability, or refuses one of probability zero or NaN with
    /// [`Refusal::ZeroProbabilityOutcome`].
    pub(super) fn project_on(
        &self,
        chain: &mut Chain<A::Tensor>,
        site: usize,
        one: bool,
    ) -> Result<f64, CanonicalMpsError<A::Error>> {
        let level = usize::from(one);
        self.centre_on(chain, site)?;
        let mut centre = self.host_site(chain, site)?;
        let weights = [centre.weight_of(0), centre.weight_of(1)];
        let p = weights[level] / (weights[0] + weights[1]);
        if p == 0.0 || p.is_nan() {
            return Err(Refusal::ZeroProbabilityOutcome.into());
        }
        centre.project(level);
        let tensor = self
            .algebra
            .upload(&centre.elements)
            .map_err(CanonicalMpsError::Algebra)?;
        chain.sites[site].tensor = tensor;
        chain.pending[site] = None;
        Ok(p)
    }
}

#[cfg(test)]
impl<A: SiteAlgebra> CanonicalMpsEngine<A> {
    /// P(One) of `site` over the site's whole weight, summed in another
    /// order than the two values' weights: the read the probabilities
    /// avoid, which a sampling test uses to check that the site it chose
    /// still rounds below 1 over that weight.
    pub(crate) fn probability_of_one_over_the_whole_weight(
        &self,
        state: &CanonicalMpsState<A>,
        site: usize,
    ) -> Result<f64, CanonicalMpsError<A::Error>> {
        let mut chain = state.chain.borrow_mut();
        self.centre_on(&mut chain, site)?;
        let centre = self.host_site(&chain, site)?;
        Ok(centre.weight_of(1) / centre.weight())
    }
}
