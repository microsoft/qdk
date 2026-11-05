// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Reads of the state: probabilities, conditional probabilities, projections
//! and expectations.
//!
//! Between gates, a program measures qubits and a simulator estimates
//! observables: it reads the probability of a value, projects the state on the
//! value it drew, and reads expectations, as a program with mid-circuit
//! measurements does on every shot. These reads serve [`MpsEngine`]'s
//! `probability_of_one`, `conditional_probability_of_one`, `project` and
//! `expectation`, and the engine's [`sampling`](super::sampling). With the
//! centre on a site, the rest of the chain drops out ([`chain`](crate::chain)),
//! so a read of one qubit reads its site alone. The reads guarantee:
//!
//! - **The state is kept.** A read does not change the state, except by
//!   applying a truncation a gate deferred: it moves the centre
//!   ([`canonical_form`](super::canonical_form)), which changes the tensors,
//!   not the state, and splits a block that fusion left unsplit, which
//!   truncates as any other operation's split would ([`split`](super::split)).
//!   A read with conditions projects them on a fork, a copy of the chain, so
//!   the caller's state is not projected.
//! - **Each value from its own weight.** A probability is the value's weight at
//!   the centre (the squared norm of the centre's entries for that value) over
//!   the sum of both values' weights, not one minus the other's: when a value's
//!   weight is exactly zero it reads exactly 0 and the other exactly 1, and a
//!   rare value whose complement rounds to one is not refused.
//! - **Projections.** A projection refuses a value of probability exactly zero,
//!   or NaN, before it projects. It divides the site by the norm of what it
//!   kept, computed scaled rather than as `√p`, so the state's norm is 1
//!   however few digits `p` holds. It leaves the retained weight as it is and
//!   drops no singular value of its own (no recompression).
//! - **Expectations.** An expectation contracts only the sites between the
//!   observable's sites and the centre, the rest of the chain contracting to
//!   the identity, and divides by the squared norm at the centre, so that a
//!   gate's drift from norm 1 does not reach it.
//!
//! [`Engine::probabilities`] reads a site;
//! [`Engine::conditional_probabilities`] projects the conditions first;
//! [`Engine::project_on`] projects; and [`Engine::expectation_of`] sums
//! [`Engine::unnormalized_expectation`] over the observable's terms.
//!
//! [`MpsEngine`]: tensornet::MpsEngine

use super::Engine;
use crate::{
    CanonicalMps, CpuMpsError,
    chain::{Chain, c},
    linear_algebra::{LinearAlgebra, Matrix, MatrixRef, adjoint},
};
use num_complex::Complex64;
use tensornet::{MpsEngine, PauliSum, Refusal};

impl<A: LinearAlgebra> Engine<A> {
    /// P(Zero) and P(One) of `site`, with the centre moved there: each
    /// value's weight over the sum of the two, so that when one value's
    /// weight is exactly zero, the other's probability is exactly one and a
    /// sample never draws the impossible value. Over the site's whole weight,
    /// summed in another order, it could read `1 − 2⁻⁵³`.
    pub(super) fn probabilities(
        &self,
        chain: &mut Chain,
        site: usize,
    ) -> Result<[f64; 2], CpuMpsError> {
        self.centre_on(chain, site)?;
        let centre = &chain.sites[site];
        let [zero, one] = [centre.weight_of(0), centre.weight_of(1)];
        let total = zero + one;
        Ok([zero / total, one / total])
    }

    /// P(Zero) and P(One) of `site` given `given`, whose sites are distinct
    /// and in range: on a fork when `given` is not empty, each condition
    /// projected in order, each value's probability read from its own
    /// weight, not as one minus the other's, so an impossible value reads
    /// its rounding, not a difference.
    pub(super) fn conditional_probabilities(
        &self,
        state: &CanonicalMps,
        site: usize,
        given: &[(usize, bool)],
    ) -> Result<[f64; 2], CpuMpsError> {
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

    /// Projects `site` (in range) on `one` and returns the value's
    /// probability `p`, read from its own weight at the centre, having
    /// divided the site by the norm of what the projection kept (computed
    /// scaled, not as `√p`, so that the state's norm is 1 however few
    /// digits `p` holds); refuses a value of probability exactly zero or
    /// NaN, before it projects.
    pub(super) fn project_on(
        &self,
        chain: &mut Chain,
        site: usize,
        one: bool,
    ) -> Result<f64, CpuMpsError> {
        let level = usize::from(one);
        let p = self.probabilities(chain, site)?[level];
        if p == 0.0 || p.is_nan() {
            return Err(Refusal::ZeroProbabilityOutcome.into());
        }
        chain.sites[site].project(level);
        Ok(p)
    }

    /// `⟨ψ|P|ψ⟩` of the unnormalized state for the Pauli string `factors`.
    pub(super) fn unnormalized_expectation(
        &self,
        chain: &Chain,
        factors: &[(usize, tensornet::Pauli)],
    ) -> Complex64 {
        let sites = factors.iter().map(|&(site, _)| site);
        let low = sites.clone().fold(chain.centre, usize::min);
        let high = sites.fold(chain.centre, usize::max);
        let first = &chain.sites[low];
        let mut environment =
            Matrix::from_fn(
                first.left,
                first.left,
                |i, j| {
                    if i == j { c(1.0) } else { c(0.0) }
                },
            );
        for at in low..=high {
            let site = &chain.sites[at];
            let operated;
            let ket = match factors.iter().find(|&&(s, _)| s == at) {
                Some(&(_, pauli)) => {
                    let mut copy = site.clone();
                    copy.apply(&pauli.matrix());
                    operated = copy;
                    &operated
                }
                None => site,
            };
            let x = self.algebra.multiply(environment.view(), ket.cols_right());
            let x = MatrixRef::new(2 * site.left, site.right, x.view().elements());
            environment = self.algebra.multiply(adjoint(site.rows_left()).view(), x);
        }
        let n = environment.rows();
        let elements = environment.view().elements();
        (0..n).map(|i| elements[i + n * i]).sum()
    }

    /// `observable`'s expectation (checked) on the normalized state.
    pub(super) fn expectation_of(
        &self,
        chain: &mut Chain,
        observable: &PauliSum,
    ) -> Result<Complex64, CpuMpsError> {
        let centre = chain.centre;
        self.split_pair_toward(chain, centre)?;
        let norm = chain.sites[chain.centre].weight();
        let mut value = observable.identity_coefficient();
        for term in observable.terms() {
            value +=
                term.coefficient() * self.unnormalized_expectation(chain, term.factors()) / norm;
        }
        Ok(value)
    }
}
