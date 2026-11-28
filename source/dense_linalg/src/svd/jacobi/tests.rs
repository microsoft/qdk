// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The tests of the rotations (`svd::jacobi`): they report a failure when
//! the last sweep allowed still rotates, and, at each pair, settle an
//! orthogonal pair, rotate a coupled one and unsettle the pairs of its
//! columns, and skip a settled one (`Orthogonalization`, on hand-made
//! columns). Their parts, each called directly on hand-made columns, have
//! their tests beside them: the threshold below which a pair needs no
//! rotation and the norms a rotation keeps (`NormedColumns`), and the
//! pairs a sweep skips (`SettledPairs`).

use super::{MAX_SWEEPS, Orthogonalization, SplitComplexColumns, svd_by_rotations};
use crate::Matrix;
use crate::test_utils::{c, matrix};

/// The columns of `m` stored as `[re…, im…]`, as the rotations take
/// them. Here, in the rotations' module, because the tests of
/// `NormedColumns` use it too.
pub(super) fn split_columns(m: &Matrix) -> Vec<f64> {
    let rows = m.rows();
    let elements = m.elements();
    (0..m.cols())
        .flat_map(|j| {
            let column = &elements[rows * j..rows * (j + 1)];
            column
                .iter()
                .map(|x| x.re)
                .chain(column.iter().map(|x| x.im))
        })
        .collect()
}

/// The rotations of the columns of a 6 × 4 matrix of full rank converge
/// after a few sweeps; allowed one sweep fewer than they take, they
/// report a failure rather than return columns that are not yet
/// orthogonal.
/// This pins that the last sweep allowed reports a failure, not how many
/// sweeps the cap allows: `needed` is found by the same rotations, so a
/// cap one sweep off shifts it too.
#[test]
fn fails_when_the_last_sweep_allowed_still_rotates() {
    let w = split_columns(&matrix(6, 4, 3));
    let rotations_only = |sweeps| svd_by_rotations(6, 4, w.clone(), sweeps);
    let needed = (1..=MAX_SWEEPS)
        .find(|&sweeps| rotations_only(sweeps).is_some())
        .expect("converges within the SVD's sweeps");
    assert!(needed > 1, "a single sweep converges");
    assert!(rotations_only(needed - 1).is_none());
}

/// Three columns of length 3: `w₀ = e₀`, orthogonal to the others, and
/// `w₁ = e₁ + e₂`, `w₂ = e₁`, coupled. Rotating `(w₁, w₂)` keeps both in
/// the plane of `e₁` and `e₂`, still orthogonal to `w₀`.
fn one_coupled_pair() -> Orthogonalization {
    let columns = Matrix::from_fn(3, 3, |i, j| match (i, j) {
        (0, 0) | (1, 1 | 2) | (2, 1) => c(1.0, 0.0),
        _ => c(0.0, 0.0),
    });
    Orthogonalization::of(SplitComplexColumns::new(3, 3, split_columns(&columns)))
}

/// `(w₀, w₁)` is orthogonal: the step leaves it as it is, and settles it,
/// so that the next sweeps skip it.
#[test]
fn settles_an_orthogonal_pair_without_rotating_it() {
    let mut orthogonalization = one_coupled_pair();
    assert!(!orthogonalization.make_pair_orthogonal(0, 1));
    assert!(orthogonalization.settled.contains(0, 1));
}

/// `(w₀, w₁)` and `(w₀, w₂)` settled, the step rotates the coupled pair
/// `(w₁, w₂)`, which makes it orthogonal, and unsettles the pairs of `w₁`
/// and `w₂`: their products changed, so the next sweep tests them again.
#[test]
fn rotates_a_coupled_pair_and_unsettles_the_pairs_of_its_columns() {
    let mut orthogonalization = one_coupled_pair();
    for q in [1, 2] {
        orthogonalization.make_pair_orthogonal(0, q);
        assert!(orthogonalization.settled.contains(0, q), "precondition");
    }
    assert!(orthogonalization.make_pair_orthogonal(1, 2));
    let columns = &orthogonalization.columns;
    assert!(columns.rotation_making_orthogonal(1, 2).is_none());
    for q in [1, 2] {
        assert!(!orthogonalization.settled.contains(0, q), "(0, {q})");
    }
}

/// A settled pair is skipped, even when its columns are coupled: the step
/// neither tests nor rotates it. (A pair is settled only when found
/// orthogonal; settling the coupled pair by hand shows that it is not
/// tested again.)
#[test]
fn skips_a_settled_pair() {
    let mut orthogonalization = one_coupled_pair();
    orthogonalization.settled.settle(1, 2);
    assert!(!orthogonalization.make_pair_orthogonal(1, 2));
    let columns = &orthogonalization.columns;
    assert!(columns.rotation_making_orthogonal(1, 2).is_some());
}
