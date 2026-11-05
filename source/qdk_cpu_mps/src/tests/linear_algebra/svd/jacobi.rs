// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The tests of the rotations (`linear_algebra::cpu::svd::jacobi`): they
//! end once a sweep finds the columns orthogonal to `√n · ε`, rotate
//! every pair coupled above it, keep the columns orthogonal when a
//! rotation leaves one small, and report a failure when the last sweep
//! allowed still rotates.

use super::super::{adjoint, distance, identity, matrix, naive_product};
use crate::linear_algebra::{
    Matrix,
    cpu::svd::jacobi::{MAX_SWEEPS, one_sided},
};
use num_complex::Complex64;

/// The columns of `m` stored as `[re…, im…]`, as the rotations take
/// them.
fn split_columns(m: &Matrix) -> Vec<f64> {
    let rows = m.rows();
    let elements = m.view().elements();
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
/// cap one sweep off shifts it too. The count is pinned by
/// `ends_once_a_sweep_finds_the_columns_orthogonal_to_rounding` and
/// `rotates_a_pair_coupled_above_sqrt_n_epsilon`.
#[test]
fn fails_when_the_last_sweep_allowed_still_rotates() {
    let w = split_columns(&matrix(6, 4, 3));
    let rotations_only = |sweeps| one_sided(6, 4, w.clone(), sweeps);
    let needed = (1..=MAX_SWEEPS)
        .find(|&sweeps| rotations_only(sweeps).is_some())
        .expect("converges within the engine's sweeps");
    assert!(needed > 1, "a single sweep converges");
    assert!(rotations_only(needed - 1).is_none());
}

/// Two columns of 1,000 entries, as `W` has when a split of rank 2
/// has 1,000 columns: one rotation makes them orthogonal up to the
/// rounding of their 1,000-term inner product, and the next sweep,
/// which finds them within `√n · ε` (LAPACK `zgesvj`'s test, `n` the
/// columns' length), ends the iteration. Allowed only the first
/// sweep, the iteration fails; a test at `ε` alone rotates the
/// rounding again on some pairs, and takes a third sweep.
#[test]
fn ends_once_a_sweep_finds_the_columns_orthogonal_to_rounding() {
    for seed in 1..=6 {
        let w = split_columns(&matrix(1_000, 2, seed));
        assert!(
            one_sided(1_000, 2, w.clone(), 1).is_none(),
            "seed {seed}: the first sweep rotates"
        );
        assert!(
            one_sided(1_000, 2, w, 2).is_some(),
            "seed {seed}: the second sweep finds nothing to rotate"
        );
    }
}

/// Two columns of 1,000 entries, as `W` has when a split of rank 2
/// has 1,000 columns, orthonormal but for a coupling of `300 · ε`:
/// above `√n · ε` (about `31.6 · ε`) and below `n · ε`, so a looser
/// test would leave the pair, and `U` would lose orthonormality by
/// that much. The first sweep rotates the pair, and the second ends
/// the iteration.
#[test]
fn rotates_a_pair_coupled_above_sqrt_n_epsilon() {
    let rows = 1_000;
    for seed in 1..=6 {
        let columns = matrix(rows, 2, seed);
        let (p, q) = columns.view().elements().split_at(rows);
        let norm = |x: &[Complex64]| x.iter().map(Complex64::norm_sqr).sum::<f64>().sqrt();
        let p: Vec<Complex64> = p.iter().map(|x| x / norm(p)).collect();
        let mut q = q.to_vec();
        // Gram–Schmidt twice, so that `q` is orthogonal to `p` to
        // rounding.
        for _ in 0..2 {
            let overlap: Complex64 = p.iter().zip(&q).map(|(a, b)| a.conj() * b).sum();
            for (b, a) in q.iter_mut().zip(&p) {
                *b -= overlap * a;
            }
        }
        let q_norm = norm(&q);
        for (b, a) in q.iter_mut().zip(&p) {
            *b = *b / q_norm + 300.0 * f64::EPSILON * a;
        }
        let w = split_columns(&Matrix::new(rows, 2, [p, q].concat()));
        assert!(
            one_sided(rows, 2, w.clone(), 1).is_none(),
            "seed {seed}: the first sweep rotates"
        );
        assert!(
            one_sided(rows, 2, w, 2).is_some(),
            "seed {seed}: the second sweep finds nothing to rotate"
        );
    }
}

/// Two columns of 4 entries, `e₁` and `(2ε, 1, 0, 0)`: both norms are
/// exactly 1 (`4ε²` vanishes beside 1) and the coupling exactly `2ε`,
/// which is `√n · ε` at `n` = 4, so the pair lies within the test's
/// bound, on its edge. The first sweep rotates nothing and ends the
/// iteration.
#[test]
fn a_pair_coupled_exactly_at_the_threshold_is_not_rotated() {
    let zero = Complex64::new(0.0, 0.0);
    let one = Complex64::new(1.0, 0.0);
    let coupling = Complex64::new(2.0 * f64::EPSILON, 0.0);
    let columns = Matrix::new(4, 2, vec![one, zero, zero, zero, coupling, one, zero, zero]);
    assert!(
        one_sided(4, 2, split_columns(&columns), 1).is_some(),
        "the first sweep finds nothing to rotate"
    );
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
            let (base, step, other) = (base.view(), step.view(), other.view());
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
