// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The tests of the thin LQ (`lq`): it gives back the matrix, with `L`
//! lower trapezoidal and orthonormal rows in `Q`, at any scale and for an
//! empty matrix of any size. On an entry that is not finite, it still
//! answers factors of the thin shapes rather than panic.

use crate::test_utils::{
    adjoint, c, distance, identity, matrix, naive_product, scenes_at_each_scale,
};
use crate::{Lq, Matrix};

/// Checks that `Lq::of(a)` gives back `a` to rounding of its largest entry,
/// with `L` lower trapezoidal and orthonormal rows in `Q`.
fn check_lq(a: &Matrix, scene: &str) {
    let (rows, cols) = (a.rows(), a.cols());
    let Lq { l, q } = Lq::of(a.view());
    let k = rows.min(cols);
    let shape = (l.rows(), l.cols(), q.rows(), q.cols());
    assert_eq!(shape, (rows, k, k, cols), "{scene}");
    let largest = a.elements().iter().map(|x| x.norm()).fold(0.0, f64::max);
    let error = distance(&naive_product(l.view(), q.view()), a);
    assert!(
        error <= 1e-13 * largest,
        "{scene}, {rows} × {cols}: {error:e} of {largest:e}"
    );
    let gram = naive_product(q.view(), adjoint(&q).view());
    assert!(
        distance(&gram, &identity(k)) < 1e-13,
        "{scene}, {rows} × {cols}: Q's rows"
    );
    for j in 0..k {
        for i in 0..j {
            assert_eq!(
                l.elements()[i + rows * j],
                c(0.0, 0.0),
                "{scene}, {rows} × {cols}: L[{i}, {j}]"
            );
        }
    }
}

/// A thin LQ gives back its matrix, with `L` lower trapezoidal and
/// orthonormal rows in `Q`, in each of the [`scenes_at_each_scale`] and in
/// its adjoint: at every scale of the `f64` range. The LQ of `a` is the QR
/// of `a†`, so a scene's adjoint (a row `2⁻⁵³³` times the others,
/// `[t + it, 1]`) meets that QR as the scene itself.
#[test]
fn a_thin_lq_gives_back_the_matrix_with_orthonormal_rows_at_any_scale() {
    for (scene, matrices) in &scenes_at_each_scale() {
        for a in matrices {
            check_lq(a, scene);
            check_lq(&adjoint(a), &format!("{scene}, adjointed"));
        }
    }
}

/// A matrix without elements has empty factors of the thin shapes, even
/// with a dimension of `2⁶³` (`2³¹` on 32 bits), which no matrix with
/// elements can have.
#[test]
fn decomposes_an_empty_matrix_of_any_size() {
    let huge = 1 << (usize::BITS - 1);
    for (rows, cols) in [(huge, 0), (0, huge), (3, 0), (0, 3)] {
        let Lq { l, q } = Lq::of(Matrix::new(rows, cols, Vec::new()).view());
        assert_eq!((l.rows(), l.cols(), q.rows(), q.cols()), (rows, 0, 0, cols));
    }
}

/// A matrix with an entry that is not finite has no LQ decomposition, and
/// its factors are unspecified; the LQ still answers, with the thin
/// shapes, rather than panic.
#[test]
fn an_entry_that_is_not_finite_gives_thin_factors_not_a_panic() {
    for x in [f64::NAN, f64::INFINITY] {
        let mut elements = matrix(3, 4, 1).into_elements();
        elements[5] = c(x, 0.0);
        let Lq { l, q } = Lq::of(Matrix::new(3, 4, elements).view());
        assert_eq!((l.rows(), l.cols(), q.rows(), q.cols()), (3, 3, 3, 4));
    }
}
