// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! An observable's expectation, `⟨ψ|O|ψ⟩ / ⟨ψ|ψ⟩`, all its terms in one
//! walk.
//!
//! An *observable* here is a sum of *Pauli strings* (terms). Each is a
//! coefficient times a product of Pauli operators (X, Y or Z), one per
//! site it names, its *factors*. Its *expectation* is the average a
//! measurement of it would give on the state. The crate README's Concepts
//! define the others (site, bond, centre, isometry, host). Two more are
//! this module's:
//!
//! - A *contraction* sums the products of tensors over the indices they
//!   share. Here, each is a product of matrices.
//! - An *environment* is one side of the chain, up to a bond, already
//!   contracted with its mirror image (the bra side, `⟨ψ|`). It is one
//!   small matrix of that bond's size, so what lies beyond the bond is
//!   never read again. It grows one site at a time
//!   ([`environment`]).
//!
//! A simulator estimates observables between gates. A chemistry reading
//! holds hundreds of Pauli strings that share their sites. Reading the
//! sites once per term would read them many times over. So the terms are
//! evaluated together.
//!
//! # One term
//!
//! With the centre on a site, the sites left of it are isometries, and
//! contract to the identity from the left. Likewise, those right of it
//! contract to the identity from the right. So a term needs only the sites
//! between its own and the centre. For the term Z₂X₅, with the centre on
//! site 3 (`A*` is a site's bra, its conjugate):
//!
//! ```text
//!  site   0     1     2     3     4     5        centre: 3
//!  ket   ─A₀────A₁────A₂────A₃────A₄────A₅─
//!         │     │     Z     │     │     X        the term Z₂X₅
//!  bra   ─A₀*───A₁*───A₂*───A₃*───A₄*───A₅*─
//!        └──────────┘└────────────────┘└────┘
//!          identity          E           R
//! ```
//!
//! - Sites 0 and 1 are not read: they contract to the identity.
//! - The left environment `E` starts as that identity, on site 2's left
//!   bond. It grows over site 2 with its Z, then over sites 3 and 4,
//!   *plain* (with no Pauli).
//! - The closing environment `R` is site 5 with its X. Right of site 5,
//!   the plain sites contract to the identity.
//! - The term's value is `tr(E·R)` on the bond between sites 4 and 5,
//!   divided by the squared norm at the centre, `‖A₃‖²`.
//!
//! The division by the squared norm at the centre makes the value that of
//! the normalised state. So a gate's drift from norm 1, within the
//! unitarity check's tolerance
//! ([`UNITARITY_TOLERANCE`](tensornet::checks::UNITARITY_TOLERANCE)),
//! divides out on the centre's site.
//! Elsewhere it shifts the value by at most that order.
//!
//! # All the terms in one walk
//!
//! [`expectation_of`](CanonicalMpsEngine::expectation_of) runs these
//! steps:
//!
//! 1. **Order** each term's factors by site ([`terms_in_site_order`]).
//!    Find the span, from the lowest site a term or the centre names to
//!    the highest ([`span_with_centre`]).
//! 2. **Read** the span's sites once each, on the host
//!    ([`read_sites`](CanonicalMpsEngine::read_sites)), with their pending
//!    gates applied, whatever the number of terms.
//! 3. **Prepare** what every term shares ([`Walk::new`]): the plain
//!    chain's environments from the left, up to each term's first site;
//!    for each term's last factor, the environment that closes it; and the
//!    squared norm at the centre.
//! 4. **Walk** the terms grouped by their first site
//!    ([`Walk::value_of_terms`]), then by their next factor. The terms
//!    that start with the same factors share one path, so the contractions
//!    they have in common are made once. For the terms Z₂X₅, Z₂Z₄ and X₃,
//!    with the centre on site 3, the paths are:
//!
//!    ```text
//!    first site 2:   Z₂ ─┬─ Z₄    Z₂Z₄ ends
//!                        └─ X₅    Z₂X₅ ends
//!    first site 3:   X₃           X₃ ends
//!    ```
//!
//!    The walk grows one environment over Z₂, then over site 3, closes
//!    Z₂Z₄, then grows it over site 4 and closes Z₂X₅. X₃ starts from the
//!    identity on site 3's left bond. The values are added in that order,
//!    each call of the walk into its own sum: Z₂Z₄ then Z₂X₅ into one,
//!    which goes into first site 2's sum; each first site's sum then goes
//!    into the total.
//!
//! # Further reading
//!
//! - U. Schollwöck, "The density-matrix renormalization group in the age
//!   of matrix product states", arXiv:1008.3477
//!   (<https://arxiv.org/abs/1008.3477>), 2011, published in Ann. Phys.
//!   326(1):96–192. §4.2,
//!   "Overlaps, expectation values and matrix elements", and §4.2.1,
//!   "Efficient evaluation of contractions", cover the environments and
//!   the order of the contractions.

mod environment;
mod term;

use super::{CanonicalMpsEngine, CanonicalMpsError, chain::Chain};
use crate::site_algebra::SiteAlgebra;
use environment::{ClosingEnvironment, HostSpan, LeftEnvironment};
use num_complex::Complex64;
use std::collections::BTreeMap;
use tensornet::PauliSum;
use term::{
    Factor, RemainingTerm, Term, last_starting_site, span_with_centre, terms_by_first_factor,
    terms_by_first_site, terms_in_site_order, terms_past_first_factor,
};

impl<A: SiteAlgebra> CanonicalMpsEngine<A> {
    /// `observable`'s expectation (checked) on the normalised state, by
    /// the steps of the module doc.
    pub(super) fn expectation_of(
        &self,
        chain: &mut Chain<A::Tensor>,
        observable: &PauliSum,
    ) -> Result<Complex64, CanonicalMpsError<A::Error>> {
        self.flush_window_toward_centre(chain)?;
        let mut value = observable.identity_coefficient();
        if observable.terms().is_empty() {
            return Ok(value);
        }
        let chain: &Chain<A::Tensor> = chain;
        let terms = terms_in_site_order(observable);
        let (lowest_site, highest_site) = span_with_centre(&terms, chain.centre);
        let span = self.read_sites(chain, lowest_site, highest_site)?;
        let walk = Walk::new(span, chain.centre, &terms);
        for (first_site, group) in terms_by_first_site(&terms) {
            let start = walk.plain_environment_before(first_site);
            value += walk.value_of_terms(start, &group);
        }
        Ok(value)
    }

    /// The sites `lowest_site..=highest_site` on the host, read once each,
    /// their pending gates applied.
    fn read_sites<'c>(
        &self,
        chain: &'c Chain<A::Tensor>,
        lowest_site: usize,
        highest_site: usize,
    ) -> Result<HostSpan<'c>, CanonicalMpsError<A::Error>> {
        let sites = (lowest_site..=highest_site)
            .map(|at| self.host_site(chain, at))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(HostSpan { lowest_site, sites })
    }
}

/// The expectation's walk over the span's sites, with what every term
/// shares.
struct Walk<'h> {
    /// The sites from the lowest to the highest that the terms or the
    /// centre touch, those between included, read on the host.
    span: HostSpan<'h>,
    /// The plain chain's environment from the left on each site's left
    /// bond, from the span's first site.
    plain_from_the_left: Vec<LeftEnvironment>,
    /// For each term's last factor, the environment that closes the terms
    /// ending with it.
    closing: BTreeMap<Factor, ClosingEnvironment>,
    /// The state's squared norm: the centre site's weight.
    squared_norm: f64,
}

impl<'h> Walk<'h> {
    /// The walk over the `span`'s sites, with what the `terms` share: the
    /// plain environments from the left, the closing environments, and the
    /// squared norm at the `centre`.
    fn new(span: HostSpan<'h>, centre: usize, terms: &[Term]) -> Self {
        Self {
            plain_from_the_left: span
                .plain_environments_from_the_left(centre, last_starting_site(terms)),
            closing: span.closing_environments(centre, terms),
            squared_norm: span.site(centre).weight(),
            span,
        }
    }

    /// The plain chain's environment from the left on `site`'s left bond,
    /// where the terms that start on `site` start their walk.
    fn plain_environment_before(&self, site: usize) -> LeftEnvironment {
        self.plain_from_the_left[site - self.span.lowest_site].clone()
    }

    /// The sum of `coefficient · (value / squared_norm)` over the `terms`.
    /// The factors the walk has gone past are in `environment`; the
    /// remaining ones lie at or right of its next site.
    ///
    /// For each group of the terms that share their first factor, in
    /// factor order, it:
    ///
    /// 1. grows the environment over the plain sites before the factor's
    ///    site; the next group goes on from there;
    /// 2. adds the values of the terms that end at the factor;
    /// 3. grows a new environment from it over the factor (it is kept for
    ///    the next group), and adds the values of the terms that go on
    ///    past it, by the same steps.
    ///
    /// So the terms that start alike share every contraction up to where
    /// they differ. The values are added in that order, into this call's
    /// sum.
    fn value_of_terms(
        &self,
        mut environment: LeftEnvironment,
        terms: &[RemainingTerm<'_>],
    ) -> Complex64 {
        let mut sum = Complex64::ZERO;
        for (factor, group) in terms_by_first_factor(terms) {
            environment = self.grow_over_plain_sites_before(factor.site, environment);
            for value in self.values_of_terms_ending_at(factor, &group, &environment) {
                sum += value;
            }
            let going_on = terms_past_first_factor(&group);
            if !going_on.is_empty() {
                let past_factor = self.grow_over_factor(factor, &environment);
                sum += self.value_of_terms(past_factor, &going_on);
            }
        }
        sum
    }

    /// `environment` grown, plain, up to `site`'s left bond.
    fn grow_over_plain_sites_before(
        &self,
        site: usize,
        mut environment: LeftEnvironment,
    ) -> LeftEnvironment {
        // Only a term's later factors skip sites, and their walk carries
        // an environment. A term's first factor is the walk's first site.
        while environment.next_site() < site {
            environment = environment.grown_over(&self.span);
        }
        environment
    }

    /// `coefficient · (value / squared_norm)` for each of the `group`'s
    /// terms that ends at `factor`, in their order. Its value is
    /// `environment`, on the factor site's left bond, closed by the
    /// factor's closing environment.
    ///
    /// The caller adds each value to its sum as it comes, not a subtotal
    /// of them: a subtotal would change the order of additions, so the
    /// rounding.
    ///
    /// Each value is divided by the squared norm before its coefficient
    /// multiplies it. So a large coefficient overflows only when its
    /// normalised term does.
    fn values_of_terms_ending_at(
        &self,
        factor: Factor,
        group: &[RemainingTerm<'_>],
        environment: &LeftEnvironment,
    ) -> impl Iterator<Item = Complex64> {
        group
            .iter()
            .filter(|term| term.ends_at_first_factor())
            .map(move |term| {
                let closing = &self.closing[&factor];
                term.coefficient * (environment.closed_by(closing) / self.squared_norm)
            })
    }

    /// `environment`, on `factor`'s site's left bond, grown over that site
    /// with the factor's Pauli.
    fn grow_over_factor(&self, factor: Factor, environment: &LeftEnvironment) -> LeftEnvironment {
        debug_assert_eq!(factor.site, environment.next_site());
        environment.grown_over_with_pauli(&self.span, factor.pauli)
    }
}

#[cfg(test)]
mod tests;
