// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! An observable's expectation, `⟨ψ|O|ψ⟩ / ⟨ψ|ψ⟩`, its terms sharing their
//! contractions.
//!
//! A simulator estimates observables between gates; a chemistry reading
//! holds hundreds of Pauli strings on two to four sites each, so one
//! contraction per term would cost the reading many times its terms. With
//! the centre on a site, the sites left of it contract to the identity
//! from the left and those right of it from the right, so a term needs
//! only the sites between its own and the centre; and the expectation
//! divides by the squared norm at the centre, so a gate's drift from norm 1
//! does not reach it. The terms share the rest:
//!
//! - the environments of the plain chain (left of a term's first site,
//!   right of its last) are computed once, for all terms;
//! - the terms are walked as a trie of their factors sorted by site, so
//!   terms with the same first factors share their walk;
//! - a term's last factor is closed by a right environment with its Pauli,
//!   computed once per site and Pauli.
//!
//! The sites are read once each, on the host ([`HostSite`]), with their
//! pending gates applied; the contractions are products of `dense_linalg`
//! matrices.

use super::{CanonicalMpsEngine, CanonicalMpsError, chain::Chain, reads::HostSite};
use crate::algebra::SiteAlgebra;
use dense_linalg::{Matrix, MatrixRef};
use num_complex::Complex64;
use std::borrow::Cow;
use std::collections::BTreeMap;
use tensornet::{Pauli, PauliSum};

impl<A: SiteAlgebra> CanonicalMpsEngine<A> {
    /// `observable`'s expectation (checked) on the normalised state.
    pub(super) fn expectation_of(
        &self,
        chain: &mut Chain<A::Tensor>,
        observable: &PauliSum,
    ) -> Result<Complex64, CanonicalMpsError<A::Error>> {
        let centre = chain.centre;
        self.split_pair_toward(chain, centre)?;
        let mut value = observable.identity_coefficient();
        if observable.terms().is_empty() {
            return Ok(value);
        }
        let chain: &Chain<A::Tensor> = chain;
        let centre = chain.centre;
        let terms: Vec<(Complex64, Vec<(usize, Pauli)>)> = observable
            .terms()
            .iter()
            .map(|term| {
                let mut factors = term.factors().to_vec();
                factors.sort_by_key(|&(site, _)| site);
                (term.coefficient(), factors)
            })
            .collect();
        let sites = terms
            .iter()
            .flat_map(|(_, factors)| factors.iter().map(|&(site, _)| site));
        let low = sites.clone().fold(centre, usize::min);
        let high = sites.fold(centre, usize::max);
        let hosts = (low..=high)
            .map(|at| self.host_site(chain, at))
            .collect::<Result<Vec<_>, _>>()?;
        let walk = Walk::new(&hosts, low, centre, &terms);
        let norm = walk.site(centre).weight();
        let mut by_first: BTreeMap<usize, Vec<Term<'_>>> = BTreeMap::new();
        for (coefficient, factors) in &terms {
            by_first
                .entry(factors[0].0)
                .or_default()
                .push((*coefficient, factors));
        }
        for (first, group) in by_first {
            let start = walk.left_environment(first);
            value += walk.descend(start, first, &group) / norm;
        }
        Ok(value)
    }
}

/// An environment on a bond, `E[bra, ket]` column-major, or the identity
/// (`None`), which the canonical form gives left of the centre's left bond
/// and right of its right bond.
type Environment = Option<Vec<Complex64>>;

/// A term's coefficient and its factors, sorted by site.
type Term<'t> = (Complex64, &'t [(usize, Pauli)]);

/// The expectation's walk over the sites `low..=high`, with the plain
/// chain's environments and the closing environments computed once.
struct Walk<'s, 'h> {
    sites: &'s [HostSite<'h>],
    low: usize,
    /// `left[s − low]`: the environment on site `s`'s left bond, from the
    /// left.
    left: Vec<Environment>,
    /// The environment on site `s`'s left bond, from the right, with the
    /// Pauli `P` on `s`, for each last factor `(s, P)`, stored transposed
    /// (`R[ket, bra]`).
    closing: BTreeMap<(usize, u8), Vec<Complex64>>,
}

impl<'s, 'h> Walk<'s, 'h> {
    fn new(
        sites: &'s [HostSite<'h>],
        low: usize,
        centre: usize,
        terms: &[(Complex64, Vec<(usize, Pauli)>)],
    ) -> Self {
        let high = low + sites.len() - 1;
        let mut walk = Self {
            sites,
            low,
            left: Vec::with_capacity(sites.len()),
            closing: BTreeMap::new(),
        };
        let last_first = terms
            .iter()
            .map(|(_, factors)| factors[0].0)
            .max()
            .expect("a term");
        for at in low..=high {
            // Left of the centre the sites are left-canonical: the
            // environment from the left stays the identity.
            let environment = if at <= centre || at > last_first {
                None
            } else {
                let previous = walk.left[at - 1 - low].as_deref();
                let site = walk.site(at - 1);
                Some(transfer_left(previous, site, site))
            };
            walk.left.push(environment);
        }
        // The plain environments from the right, on each site's right
        // bond, then the closing ones.
        let mut right: Vec<Environment> = vec![None; sites.len()];
        for at in (low..centre).rev() {
            let next = right[at + 1 - low].as_deref();
            right[at - low] = Some(transfer_right(next, walk.site(at + 1), walk.site(at + 1)));
        }
        for (_, factors) in terms {
            let &(at, pauli) = factors.last().expect("a factor");
            let key = (at, pauli_key(pauli));
            if walk.closing.contains_key(&key) {
                continue;
            }
            let site = walk.site(at);
            let ket = with_pauli(site, pauli);
            let closing = transfer_right(right[at - low].as_deref(), &ket, site);
            walk.closing.insert(key, closing);
        }
        walk
    }

    fn site(&self, at: usize) -> &'s HostSite<'h> {
        &self.sites[at - self.low]
    }

    fn left_environment(&self, at: usize) -> Environment {
        self.left[at - self.low].clone()
    }

    /// The sum of `coefficient · ⟨P…⟩` (unnormalised) over `items`, whose
    /// factors all lie at or right of `at`, `environment` on `at`'s left
    /// bond.
    fn descend(&self, environment: Environment, at: usize, items: &[Term<'_>]) -> Complex64 {
        let mut groups: BTreeMap<(usize, u8), (Pauli, Vec<Term<'_>>)> = BTreeMap::new();
        for &(coefficient, factors) in items {
            let (site, pauli) = factors[0];
            groups
                .entry((site, pauli_key(pauli)))
                .or_insert_with(|| (pauli, Vec::new()))
                .1
                .push((coefficient, factors));
        }
        let mut sum = Complex64::ZERO;
        let mut environment = environment;
        let mut position = at;
        for ((site, key), (pauli, group)) in groups {
            // Only a term's later factors skip sites, and their walk carries
            // an environment: a term's first factor is the walk's first site.
            while position < site {
                let plain = self.site(position);
                environment = Some(transfer_left(environment.as_deref(), plain, plain));
                position += 1;
            }
            let mut continuing = Vec::new();
            for (coefficient, factors) in group {
                if factors.len() == 1 {
                    let closing = &self.closing[&(site, key)];
                    sum += coefficient * close(environment.as_deref(), closing);
                } else {
                    continuing.push((coefficient, &factors[1..]));
                }
            }
            if !continuing.is_empty() {
                let plain = self.site(site);
                let ket = with_pauli(plain, pauli);
                let next = Some(transfer_left(environment.as_deref(), &ket, plain));
                sum += self.descend(next, site + 1, &continuing);
            }
        }
        sum
    }
}

/// A key that orders and groups the factors by Pauli.
fn pauli_key(pauli: Pauli) -> u8 {
    match pauli {
        Pauli::X => 0,
        Pauli::Y => 1,
        Pauli::Z => 2,
    }
}

/// A copy of `site` with `pauli` applied to its qubit.
fn with_pauli(site: &HostSite<'_>, pauli: Pauli) -> HostSite<'static> {
    let mut ket = HostSite {
        left: site.left,
        right: site.right,
        elements: Cow::Owned(site.elements.to_vec()),
    };
    ket.apply(&pauli.matrix());
    ket
}

/// `E'[b', b] = Σ conj(bra[a', s, b']) · E[a', a] · ket[a, s, b]`, on the
/// sites' right bond.
fn transfer_left(
    environment: Option<&[Complex64]>,
    ket: &HostSite<'_>,
    bra: &HostSite<'_>,
) -> Vec<Complex64> {
    let (l, r) = (bra.left, bra.right);
    let carried;
    let x: &[Complex64] = match environment {
        Some(environment) => {
            carried = Matrix::product(
                MatrixRef::new(l, l, environment),
                MatrixRef::new(l, 2 * r, &ket.elements),
            );
            carried.elements()
        }
        None => &ket.elements,
    };
    let bra_adjoint = MatrixRef::new(2 * l, r, &bra.elements).adjoint();
    Matrix::product(bra_adjoint.view(), MatrixRef::new(2 * l, r, x)).into_elements()
}

/// The transposed right environment `R'[a, a'] = Σ ket[a, s, b] ·
/// R[b, b'] · conj(bra[a', s, b'])`, on the sites' left bond.
fn transfer_right(
    environment: Option<&[Complex64]>,
    ket: &HostSite<'_>,
    bra: &HostSite<'_>,
) -> Vec<Complex64> {
    let (l, r) = (bra.left, bra.right);
    let carried;
    let y: &[Complex64] = match environment {
        Some(environment) => {
            carried = Matrix::product(
                MatrixRef::new(2 * l, r, &ket.elements),
                MatrixRef::new(r, r, environment),
            );
            carried.elements()
        }
        None => &ket.elements,
    };
    let bra_adjoint = MatrixRef::new(l, 2 * r, &bra.elements).adjoint();
    Matrix::product(MatrixRef::new(l, 2 * r, y), bra_adjoint.view()).into_elements()
}

/// `tr(E · R)` on one bond.
fn close(environment: Option<&[Complex64]>, closing: &[Complex64]) -> Complex64 {
    let n = closing.len().isqrt();
    match environment {
        Some(environment) => (0..n)
            .flat_map(|i| (0..n).map(move |j| (i, j)))
            .map(|(i, j)| environment[i + n * j] * closing[j + n * i])
            .sum(),
        None => (0..n).map(|i| closing[i + n * i]).sum(),
    }
}
