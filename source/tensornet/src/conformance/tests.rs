// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use crate::reference::ReferenceError;

#[test]
fn the_reference_conforms_to_itself() {
    let failures = check(&ExactReference::new());
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn an_exact_capacity_never_truncates() {
    let bonds = |sites: usize| {
        exact_capacity(sites).map(|m| {
            (0..sites - 1)
                .filter_map(|cut| m.bond_dim(cut))
                .collect::<Vec<_>>()
        })
    };
    assert_eq!(bonds(2), Ok(vec![2]));
    assert_eq!(bonds(5), Ok(vec![2, 4, 4, 2]));
    assert_eq!(
        exact_capacity(1).map(|m| m.sites().to_vec()),
        Ok(vec![vec![2]])
    );
}

/// A defect the checks must catch.
#[derive(Clone, Copy, PartialEq)]
enum Defect {
    /// Applies a two-site matrix with its sites the other way round.
    SwapsSites,
    /// Ignores the condition of a conditional probability.
    IgnoresCondition,
    /// Samples the same values whatever the seed.
    IgnoresSeed,
}

struct Defective(Defect);

impl MpsEngine for Defective {
    type State = ReferenceState;
    type Error = ReferenceError;

    fn prepare(&self, capacity: &Mps) -> Result<ReferenceState, ReferenceError> {
        REFERENCE.prepare(capacity)
    }

    fn apply(
        &self,
        state: &mut ReferenceState,
        sites: &[usize],
        matrix: &[Complex64],
    ) -> Result<(), ReferenceError> {
        if self.0 == Defect::SwapsSites && sites.len() == 2 {
            REFERENCE.apply(state, &[sites[1], sites[0]], matrix)
        } else {
            REFERENCE.apply(state, sites, matrix)
        }
    }

    fn probability_of_one(
        &self,
        state: &ReferenceState,
        site: usize,
    ) -> Result<f64, ReferenceError> {
        REFERENCE.probability_of_one(state, site)
    }

    fn conditional_probability_of_one(
        &self,
        state: &ReferenceState,
        site: usize,
        given: &[(usize, bool)],
    ) -> Result<f64, ReferenceError> {
        if self.0 == Defect::IgnoresCondition {
            REFERENCE.probability_of_one(state, site)
        } else {
            REFERENCE.conditional_probability_of_one(state, site, given)
        }
    }

    fn project(
        &self,
        state: &mut ReferenceState,
        site: usize,
        one: bool,
        probability: f64,
    ) -> Result<(), ReferenceError> {
        REFERENCE.project(state, site, one, probability)
    }

    fn fork(&self, state: &ReferenceState) -> Result<ReferenceState, ReferenceError> {
        REFERENCE.fork(state)
    }

    fn sample(
        &self,
        state: &ReferenceState,
        sites: &[usize],
        shots: usize,
        seed: SampleSeed,
    ) -> Result<Vec<bool>, ReferenceError> {
        let seed = match self.0 {
            Defect::IgnoresSeed => SampleSeed::try_from(1).unwrap_or(seed),
            _ => seed,
        };
        REFERENCE.sample(state, sites, shots, seed)
    }

    fn expectation(
        &self,
        state: &ReferenceState,
        observable: &PauliSum,
    ) -> Result<Complex64, ReferenceError> {
        REFERENCE.expectation(state, observable)
    }

    fn shape(&self, state: &ReferenceState) -> Result<Mps, ReferenceError> {
        REFERENCE.shape(state)
    }
}

fn failed_cases(defect: Defect) -> Vec<&'static str> {
    check(&Defective(defect))
        .into_iter()
        .map(|f| f.case)
        .collect()
}

#[test]
fn the_checks_catch_a_defective_engine() {
    let swapped = failed_cases(Defect::SwapsSites);
    assert!(swapped.contains(&"layout"), "{swapped:?}");
    assert_eq!(
        failed_cases(Defect::IgnoresCondition),
        ["conditional", "refusals"]
    );
    assert_eq!(failed_cases(Defect::IgnoresSeed), ["sample"]);
}
