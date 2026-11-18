// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The tests of the checks every SVD answer passes (`svd::checks`): an
//! answer that is not finite, does not give back the matrix or has singular
//! vectors that are not orthonormal fails the SVD, at any scale, and one
//! within the bounds, rounding's, is taken. The answers are a real SVD's,
//! spoiled.

use super::super::{matrix, times_power_of_two};
use crate::dense_linalg::{
    Matrix, MatrixRef, Svd, SvdError, kernels::Shipped, svd::checks::checked,
};
use num_complex::Complex64;

/// The SVD of `a` with its first entry of one factor (U, the singular
/// values, or V†, by `factor`) made NaN, the others finite: in a
/// vector's entry, its real part, or its `imaginary` part.
fn spoiled(a: MatrixRef<'_>, factor: usize, imaginary: bool) -> Svd {
    let Svd {
        u,
        mut singular_values,
        v_adjoint,
    } = Svd::of(a).expect("SVD");
    let spoil = |m: Matrix, spoils: bool| {
        let mut elements = m.elements().to_vec();
        if spoils {
            elements[0] = if imaginary {
                Complex64::new(elements[0].re, f64::NAN)
            } else {
                Complex64::new(f64::NAN, elements[0].im)
            };
        }
        Matrix::new(m.rows(), m.cols(), elements)
    };
    if factor == 1 {
        singular_values[0] = f64::NAN;
    }
    Svd {
        u: spoil(u, factor == 0),
        singular_values,
        v_adjoint: spoil(v_adjoint, factor == 2),
    }
}

/// The SVD of `a` with its largest singular value raised by `scale`
/// times the bound of an SVD's reconstruction error, LAPACK's ratio of
/// 50 written out rather than read from the module, so that a change of
/// the bound shows: the factors stay finite and orthonormal, and the
/// product's error is that much.
fn off_by(a: MatrixRef<'_>, scale: f64) -> Svd {
    let mut svd = Svd::of(a).expect("SVD");
    let norm = a
        .elements()
        .iter()
        .map(Complex64::norm_sqr)
        .sum::<f64>()
        .sqrt();
    #[expect(clippy::cast_precision_loss, reason = "a matrix dimension")]
    let size = a.rows().max(a.cols()) as f64;
    svd.singular_values[0] += scale * 50.0 * size * f64::EPSILON * norm;
    svd
}

/// The SVD of `a` with U's columns (or, when `on_v_adjoint`, V†'s rows)
/// scaled by `s` and the singular values divided by `s`, so the product
/// is still `a`, up to rounding, but `‖U†U − I‖` (or `‖V†V − I‖`) is
/// `scale` times the bound of an SVD's orthonormality test, LAPACK's
/// ratio of 50 written out rather than read from the module.
fn not_orthonormal(a: MatrixRef<'_>, on_v_adjoint: bool, scale: f64) -> Svd {
    let Svd {
        u,
        singular_values,
        v_adjoint,
    } = Svd::of(a).expect("SVD");
    #[expect(clippy::cast_precision_loss, reason = "matrix dimensions")]
    let (size, k) = (a.rows().max(a.cols()) as f64, singular_values.len() as f64);
    // `s²·I − I` has the norm `(s² − 1)·√k`.
    let s = (1.0 + scale * 50.0 * size * f64::EPSILON / k.sqrt()).sqrt();
    let scaled = |m: Matrix, by: f64| {
        let elements = m.elements().iter().map(|x| x * by).collect();
        Matrix::new(m.rows(), m.cols(), elements)
    };
    let (on_u, on_v) = if on_v_adjoint { (1.0, s) } else { (s, 1.0) };
    Svd {
        u: scaled(u, on_u),
        singular_values: singular_values.iter().map(|x| x / s).collect(),
        v_adjoint: scaled(v_adjoint, on_v),
    }
}

/// The SVD of `a` with U's first two columns (or, when `on_v_adjoint`,
/// V†'s first two rows) mixed by `M = I + δ·K`, with
/// `K = i·[[0, 1], [−1, 0]]` when `imaginary`, else `[[0, 1], [1, 0]]`,
/// so that `U†U − I` (or `V†V − I`), `M†M − I`, is `2δ·K` to first
/// order: off the diagonal, imaginary or real, of norm `scale` times the
/// bound of an SVD's orthonormality test, LAPACK's ratio of 50 written
/// out. The product moves by `δ·√(s₀² + s₁²)`, under half the
/// reconstruction's bound at `scale` 1.25.
fn skewed(a: MatrixRef<'_>, on_v_adjoint: bool, scale: f64, imaginary: bool) -> Svd {
    let Svd {
        u,
        singular_values,
        v_adjoint,
    } = Svd::of(a).expect("SVD");
    let (rows, cols, rank) = (a.rows(), a.cols(), singular_values.len());
    #[expect(clippy::cast_precision_loss, reason = "a matrix dimension")]
    let size = rows.max(cols) as f64;
    // `2δ·K` has the norm `2√2·δ`.
    let delta = scale * 50.0 * size * f64::EPSILON / (2.0 * 2_f64.sqrt());
    // U·M adds `to_first` times column 1 to column 0, and `to_second`
    // times column 0 to column 1; M·V† the conjugates, to the rows.
    let (to_first, to_second) = if imaginary {
        (Complex64::new(0.0, -delta), Complex64::new(0.0, delta))
    } else {
        (Complex64::new(delta, 0.0), Complex64::new(delta, 0.0))
    };
    let mut u = u.elements().to_vec();
    let mut v = v_adjoint.elements().to_vec();
    if on_v_adjoint {
        // M·V†: rows 0 and 1 of each column.
        for j in 0..cols {
            let (first, second) = (v[rank * j], v[1 + rank * j]);
            v[rank * j] = first + to_first.conj() * second;
            v[1 + rank * j] = second + to_second.conj() * first;
        }
    } else {
        // U·M: columns 0 and 1 of each row.
        for i in 0..rows {
            let (first, second) = (u[i], u[i + rows]);
            u[i] = first + to_first * second;
            u[i + rows] = second + to_second * first;
        }
    }
    Svd {
        u: Matrix::new(rows, rank, u),
        singular_values,
        v_adjoint: Matrix::new(rank, cols, v),
    }
}

/// A 5 × 3 matrix of norm about 1600, far from 1, so that a bound
/// that drops its scale by the norm shows.
fn large() -> Matrix {
    let a = matrix(5, 3, 7);
    let elements = a.elements().iter().map(|x| x * 1000.0).collect();
    Matrix::new(5, 3, elements)
}

/// An SVD with a NaN in one factor, in an entry's real or imaginary
/// part, as faer 0.24's own SVD returned on some finite matrices: the
/// SVD fails rather than answer it.
#[test]
fn refuses_an_svd_that_is_not_finite() {
    let a = matrix(5, 3, 7);
    for (factor, imaginary) in [(0, false), (0, true), (1, false), (2, false), (2, true)] {
        let answer = Some(spoiled(a.view(), factor, imaginary));
        let error = checked::<Shipped>(a.view(), answer).expect_err("not finite");
        assert_eq!(
            error,
            SvdError::FailedChecks { rows: 5, cols: 3 },
            "factor {factor}, imaginary {imaginary}"
        );
    }
}

/// An SVD whose factors are finite but whose product is not the
/// matrix, as faer 0.24's divide and conquer returned without reporting
/// it: the SVD fails, its error saying that the factors failed the
/// checks. An error just under the bound, rounding's of a large matrix,
/// is taken. The same at 2^-700 (entries near 1e-208) and 2^700 (near
/// 5e213), where `‖A‖²` underflows to 0 or overflows, of a complex
/// matrix, of its real part, and of its real part times `i`: the check
/// is relative to the largest entry, whichever part holds it.
#[test]
fn refuses_factors_that_do_not_give_back_the_matrix() {
    type Part = fn(Complex64) -> Complex64;
    let parts: [(&str, Part); 3] = [
        ("complex", |x| x),
        ("real", |x| Complex64::new(x.re, 0.0)),
        ("imaginary", |x| Complex64::new(0.0, x.re)),
    ];
    let complex = large();
    for (kind, part) in parts {
        let elements = complex.elements().iter().copied().map(part);
        let a = Matrix::new(5, 3, elements.collect());
        for exponent in [0, -700, 700] {
            let scaled = |svd: Svd| Svd {
                singular_values: (svd.singular_values.iter())
                    .map(|s| s * 2_f64.powi(exponent))
                    .collect(),
                ..svd
            };
            let a_scaled = times_power_of_two(&a, exponent);
            let under = scaled(off_by(a.view(), 0.9));
            let under_values = under.singular_values.clone();
            let over = Some(scaled(off_by(a.view(), 1.1)));
            let taken = checked::<Shipped>(a_scaled.view(), Some(under))
                .unwrap_or_else(|_| panic!("{kind}, 2^{exponent}: an error under the bound"));
            assert_eq!(taken.singular_values, under_values, "{kind}, 2^{exponent}");
            let error = checked::<Shipped>(a_scaled.view(), over)
                .expect_err(&format!("{kind}, 2^{exponent}: an error over the bound"));
            assert_eq!(
                error,
                SvdError::FailedChecks { rows: 5, cols: 3 },
                "{kind}, 2^{exponent}"
            );
        }
    }
}

/// An SVD whose factors give back the matrix but whose U or V† is not
/// orthonormal, which a caller keeping them as a basis relies on, by a
/// deviation of `U†U − I` (or `V†V − I`) that is real on the diagonal, or
/// imaginary or real off it: the SVD fails rather than take it. A
/// deviation just under the bound, rounding's, is taken.
#[test]
fn refuses_singular_vectors_that_are_not_orthonormal() {
    type Deviation = fn(MatrixRef<'_>, bool, f64) -> Svd;
    let a = large();
    let deviations: [(&str, Deviation); 3] = [
        ("real, on the diagonal", not_orthonormal),
        ("imaginary, off the diagonal", |a, on_v_adjoint, scale| {
            skewed(a, on_v_adjoint, scale, true)
        }),
        ("real, off the diagonal", |a, on_v_adjoint, scale| {
            skewed(a, on_v_adjoint, scale, false)
        }),
    ];
    for (kind, deviated) in deviations {
        for on_v_adjoint in [false, true] {
            let under = Some(deviated(a.view(), on_v_adjoint, 0.8));
            let over = Some(deviated(a.view(), on_v_adjoint, 1.25));
            let taken = checked::<Shipped>(a.view(), under)
                .unwrap_or_else(|_| panic!("{kind}: under the bound, V† {on_v_adjoint}"));
            assert_eq!(
                taken.singular_values,
                deviated(a.view(), on_v_adjoint, 0.8).singular_values
            );
            let error = checked::<Shipped>(a.view(), over)
                .expect_err(&format!("{kind}: over the bound, V† {on_v_adjoint}"));
            assert_eq!(
                error,
                SvdError::FailedChecks { rows: 5, cols: 3 },
                "{kind}: V† {on_v_adjoint}"
            );
        }
    }
}

/// An SVD that reports a failure, as the rotations do when they still
/// turn after their last sweep: the SVD fails, its error saying that it
/// did not converge.
#[test]
fn fails_when_the_svd_reports_a_failure() {
    let a = matrix(5, 3, 7);
    let error = checked::<Shipped>(a.view(), None).expect_err("a failure");
    assert_eq!(error, SvdError::DidNotConverge { rows: 5, cols: 3 });
}
