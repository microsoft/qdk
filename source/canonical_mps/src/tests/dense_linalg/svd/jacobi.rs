// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The tests of the rotations (`svd::jacobi`): they keep the columns
//! orthogonal when a rotation leaves one small, and report a failure when
//! the last sweep allowed still rotates. Their threshold and the pairs
//! they skip are pinned through the SVD, by its degenerate pairs
//! (`svd::accuracy`).

use super::super::{adjoint, distance, identity, matrix, naive_product};
use crate::dense_linalg::{
    Matrix,
    svd::jacobi::{MAX_SWEEPS, one_sided},
};

/// The columns of `m` stored as `[re…, im…]`, as the rotations take
/// them.
fn split_columns(m: &Matrix) -> Vec<f64> {
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
    let rotations_only = |sweeps| one_sided(6, 4, w.clone(), sweeps);
    let needed = (1..=MAX_SWEEPS)
        .find(|&sweeps| rotations_only(sweeps).is_some())
        .expect("converges within the SVD's sweeps");
    assert!(needed > 1, "a single sweep converges");
    assert!(rotations_only(needed - 1).is_none());
}

/// Two columns parallel to within 1e-4 to 1e-14, and a third coupled
/// to their difference: the rotation of the first pair leaves one of
/// them of norm about the difference, so the update of its squared
/// norm from the angle is all cancellation, and the norm is
/// recomputed. The columns come out orthogonal to rounding; with the
/// updated norm kept, the next pair's test and angle read a wrong
/// norm, and some come out nearly parallel.
#[test]
fn keeps_the_columns_orthogonal_when_a_rotation_leaves_one_small() {
    let rows = 8;
    for seed in 1..=300 {
        for k in [4, 6, 7, 8, 10, 12, 14] {
            let [base, step, other] = [0, 1000, 2000].map(|offset| matrix(rows, 1, seed + offset));
            #[expect(clippy::cast_precision_loss, reason = "a small seed")]
            let difference = 10_f64.powi(-k) * (1.0 + seed as f64 / 300.0);
            let (base, step, other) = (base.elements(), step.elements(), other.elements());
            let columns = Matrix::from_fn(rows, 3, |i, j| match j {
                0 => base[i],
                1 => base[i] + step[i] * difference,
                _ => step[i] + other[i] * 1e-3,
            });
            let (vectors, _, _) = one_sided(rows, 3, split_columns(&columns), MAX_SWEEPS)
                .unwrap_or_else(|| panic!("seed {seed}, 1e-{k}: converges"));
            let vectors = Matrix::new(rows, 3, vectors);
            let gram = naive_product(adjoint(&vectors).view(), vectors.view());
            assert!(
                distance(&gram, &identity(3)) < 1e-14,
                "seed {seed}, 1e-{k}: {gram:?}"
            );
        }
    }
}
