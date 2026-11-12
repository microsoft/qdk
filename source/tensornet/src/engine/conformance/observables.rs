// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The observables the cases read: every Pauli string on a few sites,
//! and one sum of them all.

use super::gates::c;
use super::twin::describe;
use crate::{Pauli, PauliError, PauliSum, PauliTerm};
use num_complex::Complex64;
use std::fmt::Write;

/// The factors of every Pauli string on `sites` sites but the identity:
/// 4ⁿ − 1 strings.
fn pauli_factors(sites: usize) -> impl Iterator<Item = Vec<(usize, Pauli)>> {
    let paulis = [None, Some(Pauli::X), Some(Pauli::Y), Some(Pauli::Z)];
    (1..4usize.pow(u32::try_from(sites).unwrap_or(0))).map(move |mut index| {
        let mut factors = Vec::new();
        for site in 0..sites {
            if let Some(Some(pauli)) = paulis.get(index % 4) {
                factors.push((site, *pauli));
            }
            index /= 4;
        }
        factors
    })
}

/// Every Pauli string on `sites` sites but the identity, each alone with
/// coefficient one: 4ⁿ − 1 observables.
pub(super) fn pauli_strings(sites: usize) -> Vec<PauliSum> {
    pauli_factors(sites)
        .filter_map(|factors| {
            let term = PauliTerm::new(c(1.0, 0.0), factors).ok()?;
            PauliSum::new(c(0.0, 0.0), vec![term]).ok()
        })
        .collect()
}

/// One observable of many terms: every Pauli string on `sites` sites, the
/// k-th with the complex coefficient e^{0.9ik}/k and its factors in
/// decreasing site order for even k, or rotated by one for odd k (sites
/// `[0, 1, 2]` given as `[1, 2, 0]`, a mixed order), then the last string,
/// Z on every site, again, plus `0.25 − 0.5i` times the identity. Each
/// coefficient differs from the others in modulus and phase, so an engine
/// that conjugates the coefficients, keeps only their real parts, or reads
/// only some of the terms is caught; and one that pairs a term's sites with
/// its Paulis right only in a monotone order, or merges equal strings
/// wrongly.
pub(super) fn weighted_sum(sites: usize) -> Result<PauliSum, PauliError> {
    let mut terms: Vec<PauliTerm> = (1u32..)
        .zip(pauli_factors(sites))
        .map(|(k, mut factors)| {
            if k % 2 == 1 {
                factors.rotate_left(1);
            } else {
                factors.reverse();
            }
            let k = f64::from(k);
            PauliTerm::new(Complex64::from_polar(1.0 / k, 0.9 * k), factors)
        })
        .collect::<Result<_, _>>()?;
    if let Some(last) = terms.last() {
        terms.push(last.clone());
    }
    PauliSum::new(c(0.25, -0.5), terms)
}

pub(super) fn name(observable: &PauliSum) -> String {
    observable
        .terms()
        .iter()
        .map(|term| {
            term.factors()
                .iter()
                .fold(String::new(), |mut text, (site, pauli)| {
                    let _ = write!(text, "{pauli:?}{site}");
                    text
                })
        })
        .collect::<Vec<_>>()
        .join(" + ")
}

pub(super) fn z_on(site: usize) -> Result<PauliSum, String> {
    let term = describe("term", PauliTerm::new(c(1.0, 0.0), vec![(site, Pauli::Z)]))?;
    describe("observable", PauliSum::new(c(0.0, 0.0), vec![term]))
}
