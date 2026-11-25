// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`Term`]: one Pauli string of the observable, its factors in site order,
//! the order the walk visits them in. Its parts are its [`Factor`]s, and
//! the [`RemainingTerm`] the walk carries: the factors it has not reached
//! yet. Then what the walk reads from the terms: their span and last
//! starting site, its groupings (by first site, by first factor), and its
//! step past a factor.

use num_complex::Complex64;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use tensornet::{Pauli, PauliSum, PauliTerm};

/// A term of the observable: its coefficient, and its factors sorted by
/// site.
pub(super) struct Term {
    coefficient: Complex64,
    factors: Vec<Factor>,
}

/// One factor of a term: a Pauli on a site.
///
/// Factors are ordered by site, then by Pauli (X, Y, Z): the order the
/// walk visits them in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Factor {
    pub(super) site: usize,
    pub(super) pauli: Pauli,
}

/// What is left of a term once the walk has gone past some of its factors:
/// its coefficient, and the factors not reached yet, in site order. Never
/// empty: the walk drops a term once its last factor closes it.
#[derive(Clone, Copy)]
pub(super) struct RemainingTerm<'t> {
    pub(super) coefficient: Complex64,
    factors: &'t [Factor],
}

/// Each term of `observable`, its factors sorted by site.
pub(super) fn terms_in_site_order(observable: &PauliSum) -> Vec<Term> {
    observable.terms().iter().map(Term::in_site_order).collect()
}

/// The lowest and the highest site among the `terms`' factors and the
/// `centre`. The expectation reads the sites between them.
pub(super) fn span_with_centre(terms: &[Term], centre: usize) -> (usize, usize) {
    let sites = terms
        .iter()
        .flat_map(|term| term.factors.iter().map(|factor| factor.site));
    let lowest_site = sites.clone().fold(centre, usize::min);
    let highest_site = sites.fold(centre, usize::max);
    (lowest_site, highest_site)
}

/// The last site a term starts on: past it, no term's walk begins.
pub(super) fn last_starting_site(terms: &[Term]) -> usize {
    terms
        .iter()
        .map(|term| term.first_factor().site)
        .max()
        .expect("the observable has a term")
}

/// The `terms` grouped by the site of their first factor, in site order.
/// Each group keeps the terms' order.
pub(super) fn terms_by_first_site(terms: &[Term]) -> BTreeMap<usize, Vec<RemainingTerm<'_>>> {
    let mut groups: BTreeMap<usize, Vec<RemainingTerm<'_>>> = BTreeMap::new();
    for term in terms {
        groups
            .entry(term.first_factor().site)
            .or_default()
            .push(term.not_started());
    }
    groups
}

/// The `terms` grouped by their first factor, in factor order. Each group
/// keeps the terms' order.
pub(super) fn terms_by_first_factor<'t>(
    terms: &[RemainingTerm<'t>],
) -> BTreeMap<Factor, Vec<RemainingTerm<'t>>> {
    let mut groups: BTreeMap<Factor, Vec<RemainingTerm<'t>>> = BTreeMap::new();
    for &term in terms {
        groups.entry(term.first_factor()).or_default().push(term);
    }
    groups
}

/// The terms of `group` that do not end at their first factor, each past
/// it, in their order.
pub(super) fn terms_past_first_factor<'t>(group: &[RemainingTerm<'t>]) -> Vec<RemainingTerm<'t>> {
    group
        .iter()
        .filter(|term| !term.ends_at_first_factor())
        .map(RemainingTerm::past_first_factor)
        .collect()
}

impl Term {
    /// The observable's `term`, its factors sorted by site.
    fn in_site_order(term: &PauliTerm) -> Self {
        let mut factors: Vec<Factor> = term
            .factors()
            .iter()
            .map(|&(site, pauli)| Factor { site, pauli })
            .collect();
        factors.sort_by_key(|factor| factor.site);
        Self {
            coefficient: term.coefficient(),
            factors,
        }
    }

    /// The factor on the term's lowest site, where its walk starts.
    pub(super) fn first_factor(&self) -> Factor {
        self.factors[0]
    }

    /// The factor on the term's highest site, which closes it.
    pub(super) fn last_factor(&self) -> Factor {
        *self.factors.last().expect("a term has a factor")
    }

    /// The term before the walk reaches it: every factor remains.
    pub(super) fn not_started(&self) -> RemainingTerm<'_> {
        RemainingTerm {
            coefficient: self.coefficient,
            factors: &self.factors,
        }
    }
}

impl<'t> RemainingTerm<'t> {
    /// The factor on the lowest site the walk has not reached.
    pub(super) fn first_factor(&self) -> Factor {
        self.factors[0]
    }

    /// Whether the first factor is the last: the term ends there.
    pub(super) fn ends_at_first_factor(&self) -> bool {
        self.factors.len() == 1
    }

    /// The term once the walk has gone past its first factor.
    fn past_first_factor(&self) -> RemainingTerm<'t> {
        RemainingTerm {
            coefficient: self.coefficient,
            factors: &self.factors[1..],
        }
    }
}

impl Ord for Factor {
    fn cmp(&self, other: &Self) -> Ordering {
        self.order_key().cmp(&other.order_key())
    }
}

impl PartialOrd for Factor {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Factor {
    /// The site, then the Pauli as X, Y, Z.
    fn order_key(self) -> (usize, u8) {
        let pauli = match self.pauli {
            Pauli::X => 0,
            Pauli::Y => 1,
            Pauli::Z => 2,
        };
        (self.site, pauli)
    }
}
