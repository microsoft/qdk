// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The argument checks, each refusing its faults and accepting its edges:
//! an engine calls them before it touches its state, so every engine
//! refuses a bad call with the same refusal.

use crate::tests::{c, cx, hadamard};
use crate::{MpsShape, Pauli, PauliSum, PauliTerm, Refusal, checks};
use num_complex::Complex64;

fn diagonal(a: f64, b: f64) -> [Complex64; 4] {
    [c(a, 0.0), c(0.0, 0.0), c(0.0, 0.0), c(b, 0.0)]
}

/// A two-site unitary built as the product of many gates, as fusion builds
/// one: unitary to rounding only.
fn fused_gates(count: usize) -> Vec<Complex64> {
    let (h, t) = (
        hadamard(),
        [
            c(1.0, 0.0),
            c(0.0, 0.0),
            c(0.0, 0.0),
            Complex64::from_polar(1.0, 0.7),
        ],
    );
    let h_t: Vec<Complex64> = (0..16)
        .map(|at| h[(at / 8) * 2 + (at % 4) / 2] * t[((at / 4) % 2) * 2 + at % 2])
        .collect();
    let layers = [cx().to_vec(), h_t];
    let identity: Vec<Complex64> = (0..16)
        .map(|at| c(if at % 5 == 0 { 1.0 } else { 0.0 }, 0.0))
        .collect();
    (0..count).fold(identity, |fused, k| {
        let layer = &layers[k % layers.len()];
        (0..16)
            .map(|at| {
                (0..4)
                    .map(|j| layer[at / 4 * 4 + j] * fused[j * 4 + at % 4])
                    .sum()
            })
            .collect()
    })
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

/// A chain of qubits, which every qubit program prepares.
#[test]
fn prepare_accepts_a_chain_of_qubits() {
    let capacity = MpsShape::qubit_chain(3, 2).expect("three sites form a chain");
    assert_eq!(checks::prepare(&capacity), Ok(()));
}

/// A site that is not exactly two-level, refused before an engine lays out
/// any site: a qutrit between qubits; a one-level site, so two levels
/// exactly, not at most; and a lone qutrit, whose physical axis is its
/// only one, the case a lone-qubit program meets.
#[test]
fn prepare_refuses_a_site_that_is_not_two_level() {
    let cases = [
        (vec![vec![2, 1], vec![1, 3, 1], vec![1, 2]], 1, 3),
        (vec![vec![2, 1], vec![1, 1]], 1, 1),
        (vec![vec![3]], 0, 3),
    ];
    for (sites, site, levels) in cases {
        let capacity = MpsShape::new(sites).expect("each case is a chain");
        assert_eq!(
            checks::prepare(&capacity),
            Err(Refusal::NotTwoLevel { site, levels })
        );
    }
}

/// A CNOT on two of three sites, given in reverse order, as a simulator
/// passes a gate whose control is the later qubit.
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

/// A one-site matrix given for two sites, and a two-site one for one: an
/// engine reading `4^k` entries would read past the first or ignore most
/// of the second.
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

/// The identity on three distinct sites of three, unitary and of the right
/// length, as a method would pass a Toffoli it did not decompose: its
/// number of sites is the only fault.
#[test]
fn apply_refuses_an_operator_on_three_sites() {
    let identity: Vec<Complex64> = (0..64)
        .map(|at| c(if at % 9 == 0 { 1.0 } else { 0.0 }, 0.0))
        .collect();
    assert_eq!(
        checks::apply(3, &[2, 0, 1], &identity),
        Err(Refusal::TooManySites { sites: 3 })
    );
}

/// Calls with several faults, each refused for the one checked first: the
/// number of sites, then the sites, then the matrix's length, then its
/// unitarity. The rows: three sites, one out of range and one repeated,
/// with an empty matrix; 32 sites, for which `4^32` would not fit a
/// `usize`; no site with an empty matrix, whose length is not 1; a site
/// out of range with a one-site matrix on two sites; and the 2×2 projector
/// diag(1, 0) on two sites, which, read as rows of a 4×4 matrix, is the
/// one row [1, 0, 0, 0], whose deviation from unitary is 0.
#[test]
fn apply_refuses_the_first_fault_in_the_order_it_checks() {
    let thirty_two: Vec<usize> = (0..32).collect();
    let cases: [(usize, &[usize], Vec<Complex64>, Refusal); 5] = [
        (
            2,
            &[0, 5, 0],
            Vec::new(),
            Refusal::TooManySites { sites: 3 },
        ),
        (
            32,
            &thirty_two,
            vec![c(1.0, 0.0)],
            Refusal::TooManySites { sites: 32 },
        ),
        (2, &[], Vec::new(), Refusal::NoSites),
        (
            2,
            &[0, 2],
            hadamard().to_vec(),
            Refusal::SiteOutOfRange { site: 2, sites: 2 },
        ),
        (
            2,
            &[0, 1],
            diagonal(1.0, 0.0).to_vec(),
            Refusal::MatrixLength {
                sites: 2,
                expected: 16,
                actual: 4,
            },
        ),
    ];
    for (site_count, sites, matrix, refusal) in cases {
        assert_eq!(
            checks::apply(site_count, sites, &matrix),
            Err(refusal),
            "{sites:?}"
        );
    }
}

/// A matrix of the right length that is not unitary is refused with its
/// deviation, the sum of the magnitudes of the entries of `M·M† − I`. The
/// rows: a measurement's projector passed as a gate, which only its
/// deviation tells apart; the identity on two sites with its last diagonal
/// entry zero, every row but the last a row of a unitary, which a check
/// reading only the first rows, or one site's worth of them, would take;
/// and M = [[1, 2], [0, 0]], whose `M·M† − I` = [[4, 0], [0, −1]] sums to
/// 5, while `M†·M − I` = [[0, 2], [2, 3]] sums to 7.
#[test]
fn apply_refuses_a_non_unitary_matrix_with_the_deviation_of_m_times_its_adjoint() {
    let last_diagonal_zero: Vec<Complex64> = (0..16)
        .map(|at| c(if at % 5 == 0 && at < 15 { 1.0 } else { 0.0 }, 0.0))
        .collect();
    // (scene, site count, sites, matrix, deviation)
    let rows = [
        ("a projector", 1, vec![0], diagonal(1.0, 0.0).to_vec(), 1.0),
        (
            "the last row not of a unitary",
            3,
            vec![0, 1],
            last_diagonal_zero,
            1.0,
        ),
        // COMPATIBILITY NOTE: this behavior was present in Q#'s `Apply` in
        // the evaluator's backend, which measures M·M† − I, hence we
        // measure it too, so that both refuse the same matrices.
        (
            "M·M†, not M†·M",
            1,
            vec![0],
            vec![c(1.0, 0.0), c(2.0, 0.0), c(0.0, 0.0), c(0.0, 0.0)],
            5.0,
        ),
    ];
    for (scene, site_count, sites, matrix, deviation) in rows {
        assert_eq!(
            checks::apply(site_count, &sites, &matrix),
            Err(Refusal::NotUnitary { deviation }),
            "{scene}"
        );
    }
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

/// A NaN or an infinite entry makes the deviation NaN or infinite, and a
/// NaN compares false with every bound, so a check written as "refuse
/// above the tolerance" would let it through.
#[test]
fn apply_refuses_a_matrix_with_a_non_finite_entry() {
    assert!(matches!(
        checks::apply(1, &[0], &diagonal(f64::NAN, 1.0)),
        Err(Refusal::NotUnitary { deviation }) if deviation.is_nan()
    ));
    assert!(matches!(
        checks::apply(1, &[0], &diagonal(f64::INFINITY, 1.0)),
        Err(Refusal::NotUnitary { .. })
    ));
}

/// Gate fusion multiplies many gates into one matrix, whose rounding the
/// tolerance must absorb.
#[test]
fn apply_tolerates_the_rounding_of_fused_gates() {
    assert_eq!(checks::apply(2, &[0, 1], &fused_gates(1000)), Ok(()));
}

/// The answer is then 0 or 1, which a caller may ask for.
#[test]
fn conditional_probability_of_one_accepts_the_site_among_the_conditions() {
    assert_eq!(
        checks::conditional_probability_of_one(2, 1, &[(1, true)]),
        Ok(())
    );
}

/// A sample of no qubits, which a program measuring none asks for, is no
/// fault in itself.
#[test]
fn sample_accepts_no_sites() {
    assert_eq!(checks::sample(2, &[]), Ok(()));
}

/// An observable whose terms name sites in any order, and the identity
/// alone, which has no factor.
#[test]
fn expectation_accepts_factors_in_range_and_the_identity_alone() {
    assert_eq!(checks::expectation(3, &z_terms(&[&[2, 0], &[1]])), Ok(()));
    assert_eq!(checks::expectation(1, &z_terms(&[])), Ok(()));
}

/// Every method that names sites refuses one past the chain, wherever it
/// names it, and takes the chain's last site: a gate on a qubit the
/// program never allocated; a measurement's read or projection one past
/// the last qubit; a shot's earlier outcomes, given as conditions, naming
/// a qubit past the chain, or the qubit asked about past it; a sample of a
/// qubit past the chain; and an observable with a site out of range in a
/// later term, or in a later factor of a term, since every factor of every
/// term is checked, not only the first.
#[test]
fn every_method_takes_the_last_site_and_refuses_the_next() {
    let out_of_range =
        |site, sites| -> Result<(), Refusal> { Err(Refusal::SiteOutOfRange { site, sites }) };
    let rows = [
        (
            "apply",
            checks::apply(2, &[0, 2], &cx()),
            out_of_range(2, 2),
        ),
        (
            "probability_of_one, the last site",
            checks::probability_of_one(3, 2),
            Ok(()),
        ),
        (
            "probability_of_one, the next",
            checks::probability_of_one(3, 3),
            out_of_range(3, 3),
        ),
        (
            "conditional_probability_of_one, a condition",
            checks::conditional_probability_of_one(2, 0, &[(1, true), (2, false)]),
            out_of_range(2, 2),
        ),
        (
            "conditional_probability_of_one, the site asked about",
            checks::conditional_probability_of_one(2, 2, &[(0, true)]),
            out_of_range(2, 2),
        ),
        ("project, the last site", checks::project(2, 1), Ok(())),
        (
            "project, the next",
            checks::project(2, 2),
            out_of_range(2, 2),
        ),
        ("sample", checks::sample(2, &[1, 2]), out_of_range(2, 2)),
        (
            "expectation, a later term",
            checks::expectation(2, &z_terms(&[&[0], &[1], &[7]])),
            out_of_range(7, 2),
        ),
        (
            "expectation, a later factor",
            checks::expectation(2, &z_terms(&[&[0], &[1, 7]])),
            out_of_range(7, 2),
        ),
    ];
    for (call, checked, expected) in rows {
        assert_eq!(checked, expected, "{call}");
    }
}

/// Every method that takes a list of sites refuses one named twice: a CNOT
/// whose control is its target, which no two-site contraction can apply;
/// a shot's earlier outcomes naming one qubit twice, which may give it two
/// values; a sample naming one qubit twice, whose two columns could
/// disagree.
#[test]
fn every_method_taking_a_list_of_sites_refuses_one_named_twice() {
    let rows = [
        ("apply", checks::apply(3, &[1, 1], &cx()), 1),
        (
            "conditional_probability_of_one",
            checks::conditional_probability_of_one(3, 0, &[(1, true), (2, false), (1, true)]),
            1,
        ),
        ("sample", checks::sample(2, &[0, 1, 0]), 0),
    ];
    for (call, checked, site) in rows {
        assert_eq!(checked, Err(Refusal::RepeatedSite { site }), "{call}");
    }
}
