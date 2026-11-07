// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::conformance::fused_gates;
use crate::tests::{c, cx, hadamard};
use crate::{MpsShape, Pauli, PauliSum, PauliTerm, Refusal, checks};
use num_complex::Complex64;

fn diagonal(a: f64, b: f64) -> [Complex64; 4] {
    [c(a, 0.0), c(0.0, 0.0), c(0.0, 0.0), c(b, 0.0)]
}

fn z_terms(sites: &[&[usize]]) -> PauliSum {
    let terms = sites
        .iter()
        .map(|sites| {
            PauliTerm::new(c(1.0, 0.0), sites.iter().map(|&s| (s, Pauli::Z)).collect())
                .expect("a term on distinct sites")
        })
        .collect();
    PauliSum::new(c(0.0, 0.0), terms).expect("finite coefficients")
}

#[test]
fn prepare_accepts_a_chain_of_qubits() {
    let capacity = MpsShape::qubit_chain(3, 2).expect("three sites form a chain");
    assert_eq!(checks::prepare(&capacity), Ok(()));
}

#[test]
fn prepare_refuses_a_site_that_is_not_two_level() {
    let capacity = MpsShape::new(vec![vec![2, 1], vec![1, 3, 1], vec![1, 2]])
        .expect("a qutrit between qubits is a chain");
    assert_eq!(
        checks::prepare(&capacity),
        Err(Refusal::NotTwoLevel { site: 1, levels: 3 })
    );
}

/// A site of one level is no qubit either: two levels exactly, not at most.
#[test]
fn prepare_refuses_a_site_of_one_level() {
    let capacity = MpsShape::new(vec![vec![2, 1], vec![1, 1]])
        .expect("a qubit beside a one-level site is a chain");
    assert_eq!(
        checks::prepare(&capacity),
        Err(Refusal::NotTwoLevel { site: 1, levels: 1 })
    );
}

/// A lone site's physical axis is its only one, the case a lone-qubit
/// program meets.
#[test]
fn prepare_refuses_a_lone_site_that_is_not_two_level() {
    let capacity = MpsShape::new(vec![vec![3]]).expect("a lone qutrit is a chain");
    assert_eq!(
        checks::prepare(&capacity),
        Err(Refusal::NotTwoLevel { site: 0, levels: 3 })
    );
}

#[test]
fn apply_accepts_a_unitary_on_distinct_sites_in_range() {
    assert_eq!(checks::apply(3, &[2, 0], &cx()), Ok(()));
}

/// The one-element identity is the right length and unitary on no site:
/// the empty list of sites is the only fault.
#[test]
fn apply_refuses_no_site() {
    assert_eq!(checks::apply(2, &[], &[c(1.0, 0.0)]), Err(Refusal::NoSites));
}

#[test]
fn apply_refuses_a_site_out_of_range() {
    assert_eq!(
        checks::apply(2, &[0, 2], &cx()),
        Err(Refusal::SiteOutOfRange { site: 2, sites: 2 })
    );
}

#[test]
fn apply_refuses_a_site_named_twice() {
    assert_eq!(
        checks::apply(3, &[1, 1], &cx()),
        Err(Refusal::RepeatedSite { site: 1 })
    );
}

#[test]
fn apply_refuses_a_matrix_of_the_wrong_length() {
    assert_eq!(
        checks::apply(2, &[0, 1], &hadamard()),
        Err(Refusal::MatrixLength {
            sites: 2,
            expected: 16,
            actual: 4
        })
    );
    assert_eq!(
        checks::apply(1, &[0], &cx()),
        Err(Refusal::MatrixLength {
            sites: 1,
            expected: 4,
            actual: 16
        })
    );
}

/// `4^32` does not fit a `usize`, so no matrix has that length.
#[test]
fn apply_refuses_any_matrix_on_more_sites_than_a_length_can_count() {
    let sites: Vec<usize> = (0..32).collect();
    assert_eq!(
        checks::apply(32, &sites, &[c(1.0, 0.0)]),
        Err(Refusal::MatrixLength {
            sites: 32,
            expected: usize::MAX,
            actual: 1
        })
    );
}

/// Read as rows of a 4×4 matrix, the 2×2 projector diag(1, 0) is the one
/// row [1, 0, 0, 0], whose deviation from unitary is 0: its length is the
/// fault, whatever its entries.
#[test]
fn apply_refuses_a_matrix_of_the_wrong_length_whatever_its_entries() {
    assert_eq!(
        checks::apply(2, &[0, 1], &diagonal(1.0, 0.0)),
        Err(Refusal::MatrixLength {
            sites: 2,
            expected: 16,
            actual: 4
        })
    );
}

#[test]
fn apply_refuses_a_projector_with_its_deviation() {
    assert_eq!(
        checks::apply(1, &[0], &diagonal(1.0, 0.0)),
        Err(Refusal::NotUnitary { deviation: 1.0 })
    );
}

/// The identity on two or three sites with its last diagonal entry zero:
/// every row but the last is a row of a unitary, so a check that reads only
/// the first rows, or one site's worth of them, takes it.
#[test]
fn apply_refuses_a_matrix_on_several_sites_deviating_in_its_last_row() {
    for sites in [&[0, 1][..], &[2, 0, 1]] {
        let dim = 1 << sites.len();
        let matrix: Vec<Complex64> = (0..dim * dim)
            .map(|at| {
                c(
                    if at % (dim + 1) == 0 && at + 1 < dim * dim {
                        1.0
                    } else {
                        0.0
                    },
                    0.0,
                )
            })
            .collect();
        assert_eq!(
            checks::apply(3, sites, &matrix),
            Err(Refusal::NotUnitary { deviation: 1.0 }),
            "on sites {sites:?}"
        );
    }
}

/// M = [[1, 2], [0, 0]]: M·M† − I = [[4, 0], [0, −1]] sums to 5, while
/// M†·M − I = [[0, 2], [2, 3]] sums to 7. The sparse simulator computes
/// the first.
#[test]
fn apply_measures_the_deviation_of_m_times_its_adjoint() {
    let m = [c(1.0, 0.0), c(2.0, 0.0), c(0.0, 0.0), c(0.0, 0.0)];
    assert_eq!(
        checks::apply(1, &[0], &m),
        Err(Refusal::NotUnitary { deviation: 5.0 })
    );
}

/// diag(1 + δ, 1) deviates by 2δ + δ²: accepted at δ = 4e-10, refused at
/// δ = 1e-9, so the tolerance sits between.
#[test]
fn apply_takes_a_matrix_within_the_tolerance_as_unitary() {
    assert_eq!(checks::apply(1, &[0], &diagonal(1.0 + 4e-10, 1.0)), Ok(()));
    assert!(matches!(
        checks::apply(1, &[0], &diagonal(1.0 + 1e-9, 1.0)),
        Err(Refusal::NotUnitary { deviation }) if deviation > checks::UNITARITY_TOLERANCE
    ));
}

/// A NaN deviation compares false with every bound, so a check written as
/// "refuse above the tolerance" would let it through.
#[test]
fn apply_refuses_a_matrix_with_a_nan_entry() {
    assert!(matches!(
        checks::apply(1, &[0], &diagonal(f64::NAN, 1.0)),
        Err(Refusal::NotUnitary { deviation }) if deviation.is_nan()
    ));
}

#[test]
fn apply_refuses_a_matrix_with_an_infinite_entry() {
    assert!(matches!(
        checks::apply(1, &[0], &diagonal(f64::INFINITY, 1.0)),
        Err(Refusal::NotUnitary { .. })
    ));
}

/// Gate fusion multiplies many gates into one matrix, whose rounding the
/// tolerance must absorb.
#[test]
fn apply_accepts_a_product_of_a_thousand_gates() {
    assert_eq!(checks::apply(2, &[0, 1], &fused_gates(1000)), Ok(()));
}

#[test]
fn probability_of_one_accepts_the_last_site_and_refuses_the_next() {
    assert_eq!(checks::probability_of_one(3, 2), Ok(()));
    assert_eq!(
        checks::probability_of_one(3, 3),
        Err(Refusal::SiteOutOfRange { site: 3, sites: 3 })
    );
}

#[test]
fn a_condition_on_a_site_out_of_range_is_refused() {
    assert_eq!(
        checks::conditional_probability_of_one(2, 0, &[(1, true), (2, false)]),
        Err(Refusal::SiteOutOfRange { site: 2, sites: 2 })
    );
}

#[test]
fn a_condition_naming_a_site_twice_is_refused() {
    assert_eq!(
        checks::conditional_probability_of_one(3, 0, &[(1, true), (2, false), (1, true)]),
        Err(Refusal::RepeatedSite { site: 1 })
    );
}

#[test]
fn a_conditional_probability_of_a_site_out_of_range_is_refused() {
    assert_eq!(
        checks::conditional_probability_of_one(2, 2, &[(0, true)]),
        Err(Refusal::SiteOutOfRange { site: 2, sites: 2 })
    );
}

/// The answer is then 0 or 1, which a caller may ask for.
#[test]
fn the_site_may_be_among_the_condition() {
    assert_eq!(
        checks::conditional_probability_of_one(2, 1, &[(1, true)]),
        Ok(())
    );
}

#[test]
fn project_refuses_a_site_out_of_range() {
    assert_eq!(
        checks::project(2, 2),
        Err(Refusal::SiteOutOfRange { site: 2, sites: 2 })
    );
}

#[test]
fn project_accepts_a_site_in_range() {
    assert_eq!(checks::project(2, 1), Ok(()));
}

#[test]
fn sample_accepts_no_sites() {
    assert_eq!(checks::sample(2, &[]), Ok(()));
}

#[test]
fn sample_refuses_a_site_out_of_range() {
    assert_eq!(
        checks::sample(2, &[1, 2]),
        Err(Refusal::SiteOutOfRange { site: 2, sites: 2 })
    );
}

#[test]
fn sample_refuses_a_site_named_twice() {
    assert_eq!(
        checks::sample(2, &[0, 1, 0]),
        Err(Refusal::RepeatedSite { site: 0 })
    );
}

#[test]
fn expectation_accepts_factors_in_range_and_the_identity_alone() {
    assert_eq!(checks::expectation(3, &z_terms(&[&[2, 0], &[1]])), Ok(()));
    assert_eq!(checks::expectation(1, &z_terms(&[])), Ok(()));
}

/// Every term is checked, not only the first.
#[test]
fn expectation_refuses_a_factor_out_of_range_in_a_later_term() {
    assert_eq!(
        checks::expectation(2, &z_terms(&[&[0], &[1], &[7]])),
        Err(Refusal::SiteOutOfRange { site: 7, sites: 2 })
    );
}

/// Every factor of a term is checked, not only its first.
#[test]
fn expectation_refuses_a_later_factor_out_of_range() {
    assert_eq!(
        checks::expectation(2, &z_terms(&[&[0], &[1, 7]])),
        Err(Refusal::SiteOutOfRange { site: 7, sites: 2 })
    );
}
