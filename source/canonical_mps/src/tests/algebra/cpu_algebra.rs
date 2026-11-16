// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The reference algebra against `SiteAlgebra`'s contract, since a
//! backend on another library can be compared with its answers and the
//! engine's tests run on it: a gate split gives back the gated pair,
//! truncated to its largest singular values and normalised, with an
//! isometry away from the centre, and reports a failed SVD; a move of the
//! centre is exact and leaves an isometry behind; a read borrows a site's
//! elements and a duplicate copies them, each the site's alone. The
//! product, QR and SVD it computes them with are `dense_linalg`'s, tested
//! there.
//!
//! The expected values come from the contract's formulas, computed here
//! with plain loops, and from matrices whose SVD is known by construction.
#![expect(
    clippy::many_single_char_names,
    reason = "the contract's extents l, m, r and sites a, b"
)]

use super::*;
use crate::CpuAlgebra;
use crate::algebra::{CentreSide, GateSplitCall, QrMoveCall, SiteAlgebra};
use dense_linalg::{Matrix, Qr, SvdError};
use std::borrow::Cow;

/// The singular values of [`known_split`]'s pair.
const KNOWN_VALUES: [f64; 4] = [0.8, 0.5, 0.3, 0.1];

/// A gate split, without truncation, of random sites and a random gate,
/// both sides, with outer extents equal and unequal (so that a row taken
/// for a column, or one extent for the other, shows): the product of the
/// new sites is the gated pair, divided by its norm.
#[test]
fn a_split_without_truncation_gives_back_the_gated_pair_normalised() {
    for (l, m, r) in [(2, 3, 2), (1, 3, 2), (2, 3, 1)] {
        let (a, b) = (random(2 * l, m, 1), random(m, 2 * r, 2));
        let gate: [Complex64; 16] = random(4, 4, 3).elements().try_into().expect("16 entries");
        let theta = gated_pair(&a, &b, l, r, &gate);
        let expected = scaled(&theta, 1.0 / frobenius(&theta));
        for centre in [CentreSide::Left, CentreSide::Right] {
            let (left, right, _, _) = split(&a, &b, l, r, gate, 0.0, usize::MAX, centre);
            let error = distance(&product(&left, &right), &expected);
            assert!(error < 1e-13, "{l}×{m}×{r}, {centre:?}: {error:e}");
        }
    }
}

/// A pair whose singular values are 0.8, 0.5, 0.3 and 0.1 by construction,
/// capped at two: the product of the new sites is the pair's SVD with its
/// two largest values, divided by their norm, on either side.
#[test]
fn a_split_keeps_the_largest_singular_values() {
    let (a, b, u, v_adjoint) = known_split();
    let norm = (KNOWN_VALUES[0].powi(2) + KNOWN_VALUES[1].powi(2)).sqrt();
    let kept = Matrix::from_fn(4, 4, |i, j| {
        if j < 2 {
            u.elements()[i + 4 * j] * KNOWN_VALUES[j] / norm
        } else {
            c(0.0, 0.0)
        }
    });
    let expected = product(&kept, &v_adjoint);
    for centre in [CentreSide::Left, CentreSide::Right] {
        let (left, right, bond, _) = split(&a, &b, 2, 2, identity_gate(), 0.0, 2, centre);
        assert_eq!(bond, 2, "{centre:?}");
        let error = distance(&product(&left, &right), &expected);
        assert!(error < 1e-13, "{centre:?}: {error:e}");
    }
}

/// [`known_split`]'s pair capped at two: the split keeps the share
/// (0.64 + 0.25) / 0.99 of the squared values.
#[test]
fn a_split_reports_the_share_of_the_squares_it_kept() {
    let (a, b, _, _) = known_split();
    let (_, _, _, fraction) = split(&a, &b, 2, 2, identity_gate(), 0.0, 2, CentreSide::Left);
    let total: f64 = KNOWN_VALUES.iter().map(|s| s * s).sum();
    let expected = (0.64 + 0.25) / total;
    assert!((fraction - expected).abs() < 1e-14, "{fraction}");
}

/// [`known_split`]'s pair at a cutoff of 0.1 and no cap: the squares sum to
/// 0.99, so the split may drop 0.099 of them; it drops the 0.01 of the
/// smallest value and keeps three, a share of 0.98 / 0.99.
#[test]
fn a_split_drops_the_tail_within_the_cutoff() {
    let (a, b, _, _) = known_split();
    let (gate, centre) = (identity_gate(), CentreSide::Left);
    let (_, _, bond, fraction) = split(&a, &b, 2, 2, gate, 0.1, usize::MAX, centre);
    assert_eq!(bond, 3);
    assert!((fraction - 0.98 / 0.99).abs() < 1e-14, "{fraction}");
}

/// Random pairs split with and without a cap, both sides: the site away
/// from the centre is an isometry.
#[test]
fn the_side_away_from_the_centre_is_an_isometry() {
    let (l, m, r) = (2, 3, 2);
    let (a, b) = (random(2 * l, m, 4), random(m, 2 * r, 5));
    let gate: [Complex64; 16] = random(4, 4, 6).elements().try_into().expect("16 entries");
    for max_extent in [2, usize::MAX] {
        let (left, _, _, _) = split(&a, &b, l, r, gate, 0.0, max_extent, CentreSide::Right);
        let error = distance(&product(&adjoint(&left), &left), &identity(left.cols()));
        assert!(error < 1e-13, "left, cap {max_extent}: {error:e}");
        let (_, right, _, _) = split(&a, &b, l, r, gate, 0.0, max_extent, CentreSide::Left);
        let error = distance(&product(&right, &adjoint(&right)), &identity(right.rows()));
        assert!(error < 1e-13, "right, cap {max_extent}: {error:e}");
    }
}

/// Pairs of every shape from one to two qubits a side, at caps of 1, 2, 3
/// and none and cutoffs from 0 to just below 1: the bond is at least 1,
/// and at most the cap and both sides of the gated pair.
#[test]
fn the_bond_of_a_split_lies_between_one_and_its_bounds() {
    for (l, m, r) in [(1, 2, 1), (1, 2, 4), (4, 2, 1), (2, 4, 2)] {
        let (a, b) = (random(2 * l, m, 7), random(m, 2 * r, 8));
        let gate: [Complex64; 16] = random(4, 4, 9).elements().try_into().expect("16 entries");
        for max_extent in [1, 2, 3, usize::MAX] {
            for cutoff in [0.0, 0.5, 1.0 - f64::EPSILON / 2.0] {
                let (_, _, bond, _) =
                    split(&a, &b, l, r, gate, cutoff, max_extent, CentreSide::Left);
                let largest = max_extent.min(2 * l).min(2 * r);
                assert!(
                    (1..=largest).contains(&bond),
                    "[{l}, {m}, {r}], cap {max_extent}, cutoff {cutoff}: {bond}"
                );
            }
        }
    }
}

/// Moves of the centre both ways, with and without a gate, across a bond
/// larger than the leaving site's side: the product of the new sites is
/// the gated pair.
#[test]
fn a_move_of_the_centre_gives_back_the_gated_pair() {
    for (centre, l, m, r) in [(CentreSide::Right, 1, 3, 2), (CentreSide::Left, 2, 3, 1)] {
        let (a, b) = (random(2 * l, m, 10), random(m, 2 * r, 11));
        for gate in [None, Some(random_one_site_gate(12))] {
            let (gated_left, gated_right) = match (centre, gate) {
                (CentreSide::Right, Some(g)) => (gated_site(&a, l, m, &g), b.clone()),
                (CentreSide::Left, Some(g)) => (a.clone(), gated_site(&b, m, r, &g)),
                (_, None) => (a.clone(), b.clone()),
            };
            let (left, right, _) = move_centre(&a, &b, l, r, gate, centre);
            let error = distance(&product(&left, &right), &product(&gated_left, &gated_right));
            assert!(
                error < 1e-13,
                "{centre:?}, gate {}: {error:e}",
                gate.is_some()
            );
        }
    }
}

/// Moves of the centre both ways, with and without a gate: the site the
/// centre leaves is an isometry.
#[test]
fn a_move_of_the_centre_leaves_an_isometry_behind() {
    let (l, m, r) = (2, 3, 2);
    let (a, b) = (random(2 * l, m, 13), random(m, 2 * r, 14));
    for gate in [None, Some(random_one_site_gate(15))] {
        let (left, _, _) = move_centre(&a, &b, l, r, gate, CentreSide::Right);
        let error = distance(&product(&adjoint(&left), &left), &identity(left.cols()));
        assert!(error < 1e-13, "left, gate {}: {error:e}", gate.is_some());
        let (_, right, _) = move_centre(&a, &b, l, r, gate, CentreSide::Left);
        let error = distance(&product(&right, &adjoint(&right)), &identity(right.rows()));
        assert!(error < 1e-13, "right, gate {}: {error:e}", gate.is_some());
    }
}

/// Moves of the centre across bonds smaller than, equal to and larger than
/// the leaving site's side, a pair `[1, 2, m]`, `[m, 2, 2]` with `m` from 1
/// to 5: the bond becomes the smaller of the bond and that side (2 to the
/// right, 4 to the left), exactly.
#[test]
fn the_bond_after_a_move_is_the_smaller_of_the_bond_and_the_leaving_sites_side() {
    for m in 1..=5 {
        let (a, b) = (random(2, m, 16), random(m, 4, 17));
        let (_, _, bond) = move_centre(&a, &b, 1, 2, None, CentreSide::Right);
        assert_eq!(bond, m.min(2), "right, bond {m}");
        let (_, _, bond) = move_centre(&a, &b, 1, 2, None, CentreSide::Left);
        assert_eq!(bond, m.min(4), "left, bond {m}");
    }
}

/// Random pairs of rank 3 and of rank 1, each `θ` 4 × 4 through its bond,
/// split at a cutoff of zero and no cap: the split drops only zeros, so the
/// share it reports is exactly 1.
#[test]
#[expect(clippy::float_cmp, reason = "the share is exactly 1")]
fn a_split_at_a_cutoff_of_zero_reports_a_share_of_exactly_one() {
    for (l, m, r) in [(2, 3, 2), (2, 1, 2)] {
        let (a, b) = (random(2 * l, m, 18), random(m, 2 * r, 19));
        for centre in [CentreSide::Left, CentreSide::Right] {
            let (_, _, bond, fraction) =
                split(&a, &b, l, r, identity_gate(), 0.0, usize::MAX, centre);
            assert!(
                bond < 2 * l.min(r),
                "precondition: the split drops a zero ({l}×{m}×{r}, {centre:?}, \
                 bond {bond})"
            );
            assert_eq!(fraction, 1.0, "{l}×{m}×{r}, {centre:?}, bond {bond}");
        }
    }
}

/// A rare branch, the pair `√(1 − a²)|00⟩ + a|11⟩` with `a = 1e-8`, whose
/// singular value `a` is far above the SVD's rounding though its square is
/// 1e-16, split at a cutoff of zero, both sides: the split keeps both
/// values, and the product of the new sites still holds `a`.
#[test]
fn a_split_at_a_cutoff_of_zero_keeps_a_small_singular_value() {
    let a: f64 = 1e-8;
    let large = (1.0 - a * a).sqrt();
    let pair = Matrix::from_fn(2, 2, |i, j| match (i, j) {
        (0, 0) => c(large, 0.0),
        (1, 1) => c(a, 0.0),
        _ => c(0.0, 0.0),
    });
    for centre in [CentreSide::Left, CentreSide::Right] {
        let (left, right, bond, _) = split(
            &pair,
            &identity(2),
            1,
            1,
            identity_gate(),
            0.0,
            usize::MAX,
            centre,
        );
        assert_eq!(bond, 2, "{centre:?}");
        let error = distance(&product(&left, &right), &pair);
        assert!(error < 1e-6 * a, "{centre:?}: {error:e}");
    }
}

/// A pair with a value that is not finite, as a fault upstream could leave:
/// the split reports the SVD's failure, with the gated pair's shape, rather
/// than spreading the value through the state.
#[test]
fn a_split_whose_svd_fails_reports_the_failure() {
    let (l, m, r) = (2, 3, 1);
    let mut a = random(2 * l, m, 22).elements().to_vec();
    a[1] = c(f64::NAN, 0.0);
    let b = random(m, 2 * r, 23).elements().to_vec();
    let call = GateSplitCall {
        left: &a,
        right: &b,
        left_extent: l,
        bond: m,
        right_extent: r,
        gate: identity_gate(),
        max_extent: usize::MAX,
        cutoff: 0.0,
        centre: CentreSide::Left,
    };
    let error = CpuAlgebra
        .gate_split(&call)
        .expect_err("a value that is not finite");
    assert_eq!(error, SvdError::FailedChecks { rows: 4, cols: 2 });
}

/// A tensor longer than the site it holds, as a backend that reuses
/// buffers may keep: a read returns the site's elements alone, borrowed
/// from the vector rather than copied.
#[test]
fn a_read_borrows_the_first_len_elements() {
    let tensor: Vec<Complex64> = (0..6).map(|k| c(f64::from(k), -1.0)).collect();
    let read = CpuAlgebra.read(&tensor, 4).expect("reads never fail");
    assert_eq!(*read, tensor[..4]);
    assert!(
        matches!(read, Cow::Borrowed(_)),
        "a host read copies nothing"
    );
}

/// A fork of a state whose tensor is longer than its site: the copy holds
/// the site's elements alone.
#[test]
fn a_duplicate_holds_the_first_len_elements() {
    let tensor: Vec<Complex64> = (0..6).map(|k| c(f64::from(k), 1.0)).collect();
    let copy = CpuAlgebra
        .duplicate(&tensor, 4)
        .expect("duplicates never fail");
    assert_eq!(copy, tensor[..4]);
}

/// A pair `[2, 2, 4]`, `[4, 2, 2]` whose product, under the identity gate,
/// is `u·diag(KNOWN_VALUES)·v†` with `u` and `v` random unitaries: the
/// sites, `u`, and `v†`.
fn known_split() -> (Matrix, Matrix, Matrix, Matrix) {
    let u = orthonormal(4, 20);
    let v_adjoint = adjoint(&orthonormal(4, 21));
    let a = Matrix::from_fn(4, 4, |i, j| u.elements()[i + 4 * j] * KNOWN_VALUES[j]);
    (a, v_adjoint.clone(), u, v_adjoint)
}

/// The reference algebra's gate split of the sites `a` (read as
/// `(2·l) × m`) and `b` (`m × (2·r)`): the new sites as matrices, the bond
/// and the retained fraction.
#[expect(clippy::too_many_arguments, reason = "the fields of the call")]
fn split(
    a: &Matrix,
    b: &Matrix,
    l: usize,
    r: usize,
    gate: [Complex64; 16],
    cutoff: f64,
    max_extent: usize,
    centre: CentreSide,
) -> (Matrix, Matrix, usize, f64) {
    let (left, right) = (a.elements().to_vec(), b.elements().to_vec());
    let call = GateSplitCall {
        left: &left,
        right: &right,
        left_extent: l,
        bond: a.cols(),
        right_extent: r,
        gate,
        max_extent,
        cutoff,
        centre,
    };
    let outcome = CpuAlgebra.gate_split(&call).expect("the split");
    let bond = outcome.bond;
    (
        Matrix::new(2 * l, bond, outcome.left),
        Matrix::new(bond, 2 * r, outcome.right),
        bond,
        outcome.retained_fraction,
    )
}

/// The reference algebra's move of the centre across the bond of `a` and
/// `b`: the new sites as matrices, and the bond.
fn move_centre(
    a: &Matrix,
    b: &Matrix,
    l: usize,
    r: usize,
    gate: Option<[Complex64; 4]>,
    centre: CentreSide,
) -> (Matrix, Matrix, usize) {
    let (left, right) = (a.elements().to_vec(), b.elements().to_vec());
    let call = QrMoveCall {
        left: &left,
        right: &right,
        left_extent: l,
        bond: a.cols(),
        right_extent: r,
        gate,
        centre,
    };
    let outcome = CpuAlgebra.qr_move(&call).expect("the move");
    let bond = outcome.bond;
    (
        Matrix::new(2 * l, bond, outcome.left),
        Matrix::new(bond, 2 * r, outcome.right),
        bond,
    )
}

/// `θ[a, s, t, b] = Σ G[(s, t), (p, q)] · A[a, p, j] · B[j, q, b]`, as
/// `GateSplitCall` defines it, by the formula's own loops.
fn gated_pair(a: &Matrix, b: &Matrix, l: usize, r: usize, gate: &[Complex64; 16]) -> Matrix {
    let m = a.cols();
    let (a, b) = (a.elements(), b.elements());
    let site_a = |i: usize, p: usize, j: usize| a[i + l * (p + 2 * j)];
    let site_b = |j: usize, q: usize, k: usize| b[j + m * (q + 2 * k)];
    Matrix::from_fn(2 * l, 2 * r, |row, col| {
        let (i, s, t, k) = (row % l, row / l, col % 2, col / 2);
        let mut sum = c(0.0, 0.0);
        for (p, q, j) in
            (0..2).flat_map(|p| (0..2).flat_map(move |q| (0..m).map(move |j| (p, q, j))))
        {
            sum += gate[(2 * s + t) * 4 + 2 * p + q] * site_a(i, p, j) * site_b(j, q, k);
        }
        sum
    })
}

/// The site of extents `[left, 2, right]`, held as the matrix `m`, with the
/// one-site `gate` (`G[s, p]` at `2s + p`) applied, by its own loops.
fn gated_site(m: &Matrix, left: usize, right: usize, gate: &[Complex64; 4]) -> Matrix {
    let x = m.elements();
    let elements = (0..right)
        .flat_map(|b| (0..2).flat_map(move |s| (0..left).map(move |a| (a, s, b))))
        .map(|(a, s, b)| {
            (0..2)
                .map(|p| gate[2 * s + p] * x[a + left * (p + 2 * b)])
                .sum()
        })
        .collect();
    Matrix::new(m.rows(), m.cols(), elements)
}

fn identity_gate() -> [Complex64; 16] {
    std::array::from_fn(|i| c(if i % 5 == 0 { 1.0 } else { 0.0 }, 0.0))
}

fn random_one_site_gate(seed: u64) -> [Complex64; 4] {
    random(2, 2, seed).elements().try_into().expect("4 entries")
}

/// A `rows × cols` matrix of entries in `[-0.5, 0.5)`, real and imaginary
/// parts alike, from a linear congruential generator seeded by `seed`.
fn random(rows: usize, cols: usize, seed: u64) -> Matrix {
    let mut state = seed;
    let mut next = move || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        // The top 53 bits, exactly representable.
        #[expect(clippy::cast_precision_loss, reason = "53 bits fit a double")]
        let unit = (state >> 11) as f64 / (1u64 << 53) as f64;
        unit - 0.5
    };
    Matrix::from_fn(rows, cols, |_, _| {
        let re = next();
        c(re, next())
    })
}

/// A random `n × n` unitary: the Q of the QR of [`random`]'s matrix.
fn orthonormal(n: usize, seed: u64) -> Matrix {
    Qr::of(random(n, n, seed).view()).q
}

fn product(a: &Matrix, b: &Matrix) -> Matrix {
    assert_eq!(a.cols(), b.rows(), "the inner extents");
    let (x, y) = (a.elements(), b.elements());
    Matrix::from_fn(a.rows(), b.cols(), |i, j| {
        (0..a.cols())
            .map(|k| x[i + a.rows() * k] * y[k + b.rows() * j])
            .sum()
    })
}

fn adjoint(m: &Matrix) -> Matrix {
    let x = m.elements();
    Matrix::from_fn(m.cols(), m.rows(), |i, j| x[j + m.rows() * i].conj())
}

fn identity(n: usize) -> Matrix {
    Matrix::from_fn(n, n, |i, j| c(if i == j { 1.0 } else { 0.0 }, 0.0))
}

fn scaled(m: &Matrix, factor: f64) -> Matrix {
    Matrix::new(
        m.rows(),
        m.cols(),
        m.elements().iter().map(|x| x * factor).collect(),
    )
}

fn frobenius(m: &Matrix) -> f64 {
    m.elements()
        .iter()
        .map(Complex64::norm_sqr)
        .sum::<f64>()
        .sqrt()
}

/// The largest difference between two matrices of one shape, element by
/// element; infinite when any difference is not finite (a NaN, which
/// `f64::max` would skip), so that no tolerance accepts it.
fn distance(a: &Matrix, b: &Matrix) -> f64 {
    assert_eq!((a.rows(), a.cols()), (b.rows(), b.cols()), "one shape");
    let pairs = a.elements().iter().zip(b.elements());
    pairs
        .map(|(x, y)| (x - y).norm())
        .map(|d| if d.is_finite() { d } else { f64::INFINITY })
        .fold(0.0, f64::max)
}
