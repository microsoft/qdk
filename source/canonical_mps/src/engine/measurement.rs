// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A measurement's operations on one site: probabilities, conditional
//! probabilities and projections, at the centre, on host copies of the
//! sites they read ([`host_site`](super::host_site)).
//!
//! Between gates, a program measures qubits. A simulator reads the
//! probability of a value, projects the state on the value it drew, and
//! draws samples from conditional probabilities. With the centre on a
//! site, the rest of the chain drops out of the site's reads. So a read
//! moves the centre there ([`centre_moves`](super::centre_moves)). It then
//! reads the site alone, through [`SiteAlgebra::read`], its pending gate
//! applied. The rules, stated for callers on [`MpsEngine`]'s methods:
//!
//! - **The state is kept.** A read changes the state only by applying a
//!   truncation a gate deferred. It moves the centre, which changes the
//!   tensors, not the state. It applies the window that fusion left
//!   pending, which truncates as any other operation's application of it
//!   would ([`fusion`](super::fusion)). A read with conditions projects
//!   them on a fork, so the caller's state is not projected.
//! - **Each value from its own weight.** A value's weight at the centre is
//!   the squared norm of the centre's elements for that value. A
//!   probability is that weight over the sum of both values' weights, not
//!   one minus the other's. So, when the other value's weight is positive
//!   and finite, a value of weight exactly zero reads exactly 0, and the
//!   other exactly 1. A rare value whose complement rounds to one is not
//!   refused.
//! - **Projections.** A projection refuses a value of probability exactly
//!   zero, or NaN, before it projects. It divides the site by the norm of
//!   what it kept, computed scaled rather than as `√p`. It uploads the
//!   result as the site's new tensor. It leaves the retained weight as it
//!   is and drops no singular value of its own.
//! - **Conditional reads.** A conditional probability, and so a sample,
//!   refuses a probability of NaN as a projection does. So a state whose
//!   norm is zero or NaN is refused even when no condition is given.
//!
//! A read at the centre `C`, every other site an isometry `iso`. A
//! value's weight is the state, top, contracted with its conjugate,
//! bottom, the qubit's value `s` fixed on both sides:
//!
//! ```text
//!   [iso]--[iso]--[C]--[iso]       the state
//!     |      |     s     |
//!   [iso*]-[iso*]-[C*]-[iso*]      its conjugate
//! ```
//!
//! Each isometry contracts with its conjugate to the identity, from the
//! chain's ends inwards, so only `C`'s elements count:
//!
//! ```text
//!   weight(s) = sum over a, b of |C[a, s, b]|²
//!   P(s)      = weight(s) / (weight(0) + weight(1))
//! ```

use super::host_site::HostSite;
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
        Ok(value_probabilities(&centre))
    }

    /// P(Zero) and P(One) of `site` given `given` (checked). When `given`
    /// is not empty, each condition is projected in order, on a fork.
    /// Refuses a probability of NaN, which a norm of zero or NaN gives,
    /// with [`Refusal::ZeroProbabilityOutcome`]: with an empty `given`, no
    /// projection has refused it.
    pub(super) fn conditional_probabilities(
        &self,
        state: &CanonicalMpsState<A>,
        site: usize,
        given: &[(usize, bool)],
    ) -> Result<[f64; 2], CanonicalMpsError<A::Error>> {
        let probabilities = if given.is_empty() {
            self.probabilities(&mut state.chain.borrow_mut(), site)?
        } else {
            let mut fork = self.fork(state)?;
            let chain = fork.chain.get_mut();
            for &(at, one) in given {
                self.project_on(chain, at, one)?;
            }
            self.probabilities(chain, site)?
        };
        if probabilities.iter().any(|p| p.is_nan()) {
            return Err(Refusal::ZeroProbabilityOutcome.into());
        }
        Ok(probabilities)
    }

    /// Projects `site` (checked) on One when `one` is true, on Zero when it
    /// is false, and returns that value's probability. Refuses a value of
    /// probability zero or NaN with [`Refusal::ZeroProbabilityOutcome`].
    pub(super) fn project_on(
        &self,
        chain: &mut Chain<A::Tensor>,
        site: usize,
        one: bool,
    ) -> Result<f64, CanonicalMpsError<A::Error>> {
        let value = usize::from(one);
        self.centre_on(chain, site)?;
        let mut centre = self.host_site(chain, site)?;
        let probability = value_probabilities(&centre)[value];
        if probability == 0.0 || probability.is_nan() {
            return Err(Refusal::ZeroProbabilityOutcome.into());
        }
        centre.project(value);
        let projected = self
            .algebra
            .upload(&centre.elements)
            .map_err(CanonicalMpsError::Algebra)?;
        // The projected tensor holds the gate that was pending on the site.
        chain.sites[site].tensor = projected;
        chain.pending[site] = None;
        Ok(probability)
    }
}

/// P(Zero) and P(One) read from the centre site `centre`: each value's own
/// weight over the sum of both weights.
#[inline]
fn value_probabilities(centre: &HostSite<'_>) -> [f64; 2] {
    let [zero, one] = [centre.weight_of(0), centre.weight_of(1)];
    let total = zero + one;
    [zero / total, one / total]
}

#[cfg(test)]
mod tests;
