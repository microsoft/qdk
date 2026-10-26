// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use crate::reference::ReferenceError;
use std::cell::Cell;

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
    /// Reports more weight than the state has.
    OverstatesWeight,
    /// Reports the weight right until its seventh read, the case's last,
    /// then more: only the check that the weight never increases sees it.
    RegainsWeight,
}

struct Defective {
    defect: Defect,
    weight_reads: Cell<u32>,
}

impl Defective {
    fn new(defect: Defect) -> Self {
        Self {
            defect,
            weight_reads: Cell::new(0),
        }
    }
}

impl MpsEngine for Defective {
    type State = ReferenceState;
    type Error = ReferenceError;

    fn prepare(&self, capacity: &MpsShape) -> Result<ReferenceState, ReferenceError> {
        REFERENCE.prepare(capacity)
    }

    fn apply(
        &self,
        state: &mut ReferenceState,
        sites: &[usize],
        matrix: &[Complex64],
    ) -> Result<(), ReferenceError> {
        if self.defect == Defect::SwapsSites && sites.len() == 2 {
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
        if self.defect == Defect::IgnoresCondition {
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
        let seed = match self.defect {
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

    fn shape(&self, state: &ReferenceState) -> Result<MpsShape, ReferenceError> {
        REFERENCE.shape(state)
    }

    fn retained_weight(&self, state: &ReferenceState) -> Result<f64, ReferenceError> {
        let reads = self.weight_reads.get() + 1;
        self.weight_reads.set(reads);
        let overstated = match self.defect {
            Defect::OverstatesWeight => true,
            Defect::RegainsWeight => reads >= 7,
            _ => false,
        };
        let weight = REFERENCE.retained_weight(state)?;
        Ok(if overstated { weight * 1.5 } else { weight })
    }
}

fn failed_cases(defect: Defect) -> Vec<&'static str> {
    check(&Defective::new(defect))
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
    assert_eq!(failed_cases(Defect::OverstatesWeight), ["retained weight"]);
    let regained = check(&Defective::new(Defect::RegainsWeight));
    assert!(
        matches!(regained.as_slice(), [failure] if failure.case == "retained weight"
            && failure.message.contains("rose")),
        "{regained:?}"
    );
}
