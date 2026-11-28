// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The tests of `svd`. The SVD's guarantees, through
//! [`Svd::of`](crate::Svd::of): accuracy on the matrices an MPS simulation
//! meets, leaving out only rounding, every scale, the errors, and the
//! truncation. The steps it is built of, each called directly: the trim's
//! choice of rows of `R`, and the factoring of the rows kept. The checks
//! and the rotations have their tests beside them.

/// The SVD gives back the matrix with orthonormal factors and
/// nonincreasing values, on random matrices of every shape and on the
/// rank-deficient ones an MPS simulation meets all the time, and leaves
/// out only the values of rounding.
mod decomposition {
    use crate::test_utils::{SHAPES, check_svd, matrix, naive_product};
    use crate::{Matrix, Qr, Svd};
    use num_complex::Complex64;

    /// Random matrices, tall, wide, square, a single row, and up to
    /// 128 × 128, complex and one real; those of 48 columns or rows and
    /// more are full rank and far from orthogonal, as the splits of deep
    /// circuits are. The SVD gives back each, with orthonormal U and V and
    /// nonincreasing values, one per column of the smaller side.
    #[test]
    fn gives_back_every_shape_with_orthonormal_factors() {
        let larger = [(128, 128), (64, 128), (96, 48), (48, 96), (2, 2)];
        let mut cases: Vec<Matrix> = (1..)
            .zip(SHAPES.iter().chain(&larger))
            .map(|(seed, &(rows, cols))| matrix(rows, cols, seed))
            .collect();
        let a = matrix(64, 64, 2);
        cases.push(Matrix::from_fn(64, 64, |i, j| {
            Complex64::new(a.elements()[i + 64 * j].re, 0.0)
        }));
        for a in cases {
            let (rows, cols) = (a.rows(), a.cols());
            let values = check_svd(&a);
            assert_eq!(values.len(), rows.min(cols), "{rows} × {cols}");
        }
    }

    /// Rank-deficient matrices, with repeated and zero singular values,
    /// as an MPS simulation meets them all the time. A wrong singular value
    /// on one of them, as a general SVD has returned, misleads a caller
    /// truncating by the values. The SVD gives back each,
    /// with exactly `rank` values: on `U·diag(√½, …, √½, 0, …)·V`, the
    /// value √½ `rank` times, and on a product of random `rows × rank` and
    /// `rank × cols` factors, `rank` values; the trim leaves out the rows of
    /// `R` that the pivoting leaves at rounding's size. Among the products,
    /// a 128 × 128 one of rank 64, the size of a split of an MPS at bond
    /// 64. The zero matrix gives one value, exactly 0: the trim would keep
    /// none of its rows.
    #[test]
    fn gives_back_a_rank_deficient_matrix_with_exactly_its_rank_values() {
        for (rows, cols) in [(4, 3), (3, 4)] {
            let zero = Matrix::from_fn(rows, cols, |_, _| Complex64::new(0.0, 0.0));
            assert_eq!(check_svd(&zero), [0.0], "{rows} × {cols}");
        }
        let half = 0.5_f64.sqrt();
        let mut products = Vec::new();
        for (seed, case) in (0_u64..60).zip(0_usize..) {
            let n = 2 + case % 15;
            let rank = 1 + (case / 15) % n;
            let values = check_svd(&with_repeated_value(n, rank, half, seed));
            assert_eq!(values.len(), rank, "n {n}, rank {rank}: {values:?}");
            assert!(
                values.iter().all(|s| (s - half).abs() < 1e-13),
                "n {n}, rank {rank}: {values:?}"
            );
            let (rows, cols) = (n + case % 3, n + (case / 3) % 3);
            products.push((rows, cols, rank, 3 * seed + 1, 3 * seed + 2));
        }
        // faer 0.24.4's default SVD diagonalizes by divide and conquer from
        // size 128. On this product it returned a spurious singular value
        // of 2.4e-4 where the matrix has 0, and an entry error of 4e-5,
        // without reporting a failure.
        products.push((128, 128, 64, 1, 101));
        for (rows, cols, rank, left_seed, right_seed) in products {
            let product = naive_product(
                matrix(rows, rank, left_seed).view(),
                matrix(rank, cols, right_seed).view(),
            );
            let values = check_svd(&product);
            assert_eq!(
                values.len(),
                rank,
                "{rows} × {cols}, rank {rank}: {values:?}"
            );
        }
    }

    /// The SVD leaves out only the values of rounding: those within
    /// `max(m, n) · ε` times the largest, at any scale. On a diagonal
    /// 6 × 5 matrix of values 1 and one small value, the tolerance is `6ε`:
    /// a small value of 0.9 times it is left out, and one of 1.1 times is
    /// kept. The same at a scale of 1e-100 or 1e100, which the SVD scales
    /// near 1 before its trim. At scale 1, the factors are checked too.
    /// How the trim counts several small values against the tolerance is
    /// its own rule (`trim`).
    #[test]
    fn leaves_out_small_values_only_within_the_tolerance() {
        let tolerance = 6.0 * f64::EPSILON;
        for exponent in [0, -100, 100] {
            let scale = 10f64.powi(exponent);
            for (small, kept) in [(0.9, 4), (1.1, 5)] {
                let a = Matrix::from_fn(6, 5, |i, j| {
                    let value = if i != j {
                        0.0
                    } else if i == 4 {
                        small * tolerance
                    } else {
                        1.0
                    };
                    Complex64::new(value * scale, 0.0)
                });
                let values = if exponent == 0 {
                    check_svd(&a)
                } else {
                    Svd::of(a.view()).expect("SVD").singular_values
                };
                assert_eq!(
                    values.len(),
                    kept,
                    "small value {small} × 6ε, scale {scale}: {values:?}"
                );
            }
        }
    }

    /// `U·diag(value, …, value, 0, …)·V`, `n × n`, with `rank` copies of
    /// `value` and random unitary `U` and `V` drawn from `seed`.
    fn with_repeated_value(n: usize, rank: usize, value: f64, seed: u64) -> Matrix {
        let u = Qr::of(matrix(n, n, 2 * seed + 1).view()).q;
        let v = Qr::of(matrix(n, n, 2 * seed + 2).view()).q;
        let diagonal = Matrix::from_fn(n, n, |i, j| {
            Complex64::new(if i == j && i < rank { value } else { 0.0 }, 0.0)
        });
        naive_product(naive_product(u.view(), diagonal.view()).view(), v.view())
    }
}

/// What [`Svd::of`](crate::Svd::of) refuses, and how its errors
/// read.
mod errors {
    use crate::test_utils::{matrix, panic_message};
    use crate::{Matrix, Svd, SvdError};
    use num_complex::Complex64;

    /// The errors name the matrix's shape and what went wrong, for a
    /// caller that reports them: each names the shape, and each its own
    /// failure, the rotations' convergence or the factors' checks.
    #[test]
    fn errors_name_the_shape_and_the_failure() {
        let did_not_converge = SvdError::DidNotConverge { rows: 6, cols: 4 }.to_string();
        let failed_checks = SvdError::FailedChecks { rows: 6, cols: 4 }.to_string();
        for (message, failure) in [(&did_not_converge, "converge"), (&failed_checks, "check")] {
            assert!(message.contains("6 × 4"), "the shape is named: {message}");
            assert!(
                message.contains(failure),
                "the failure is named ({failure}): {message}"
            );
        }
    }

    /// A matrix with no rows or no columns has no singular value: the
    /// SVD panics, naming the shape, rather than answer an empty one.
    #[test]
    fn panics_on_a_matrix_without_elements() {
        for (rows, cols) in [(0, 3), (3, 0)] {
            let message = panic_message(|| {
                let _ = Svd::of(Matrix::new(rows, cols, Vec::new()).view());
            });
            assert!(
                message.contains(&format!("{rows} × {cols}")),
                "the shape is named: {message}"
            );
            assert!(message.contains("empty"), "the fault is named: {message}");
        }
    }

    /// A matrix with a NaN or an infinite entry, in its real or imaginary
    /// part, has no SVD to rounding: the SVD fails, its error saying that
    /// the factors failed the checks, rather than answer factors that are
    /// not finite. Tall, wide, square up to 64 × 64, and 1 × 1, and zero
    /// but for the entry, whose largest finite entry is 0.
    #[test]
    fn fails_on_a_matrix_with_an_entry_that_is_not_finite() {
        let not_finite = [
            Complex64::new(f64::NAN, 0.0),
            Complex64::new(0.0, f64::NAN),
            Complex64::new(f64::INFINITY, 0.0),
            Complex64::new(0.0, f64::NEG_INFINITY),
        ];
        let zero = Matrix::from_fn(6, 4, |_, _| Complex64::new(0.0, 0.0));
        let cases = [(6, 4), (4, 6), (64, 64), (1, 1)]
            .map(|(rows, cols)| matrix(rows, cols, 1))
            .into_iter()
            .chain([zero]);
        for a in cases {
            let (rows, cols) = (a.rows(), a.cols());
            for entry in not_finite {
                for at in [0, rows * cols - 1] {
                    let mut elements = a.elements().to_vec();
                    elements[at] = entry;
                    let a = Matrix::new(rows, cols, elements);
                    let error = Svd::of(a.view())
                        .expect_err(&format!("{entry} at {at} of {rows} × {cols}"));
                    assert_eq!(
                        error,
                        SvdError::FailedChecks { rows, cols },
                        "{entry} at {at} of {rows} × {cols}"
                    );
                }
            }
        }
    }
}

/// The SVD gives the values, to rounding, of the matrices on which
/// general SVDs have failed: degenerate, graded.
mod accuracy {
    use crate::test_utils::{adjoint, check_svd, matrix, naive_product};
    use crate::{Matrix, Qr};
    use num_complex::Complex64;

    /// A degenerate pair, as an MPS simulation meets them (its singular
    /// values come in degenerate pairs): the upper bidiagonal
    /// `[[1, 0, 0], [0, d, ε], [0, 0, d + ulp]]`, with `d` = 0.928…, a pair
    /// one ulp apart, coupled by `ε`. faer 0.24.4's QR algorithm reported
    /// that it does not converge on it, and its divide and conquer took the
    /// same QR algorithm at this size. The SVD gives the pair to rounding,
    /// and so with a phase on each row, which leaves the singular values as
    /// they are and makes the products of the pair's columns complex.
    #[test]
    fn decomposes_degenerate_pairs_to_rounding() {
        let (d, d_next) = (9.282_350_173_388_935e-1, 9.282_350_173_388_936e-1);
        let coupling = f64::EPSILON;
        let entries = [(0, 0, 1.0), (1, 1, d), (1, 2, coupling), (2, 2, d_next)];
        let [larger, smaller] = upper_triangular_values(d, coupling, d_next);
        let expected = [1.0, larger, smaller];
        for phase in [0.0, 0.7] {
            let values = check_svd(&with_row_phases(3, &entries, phase));
            assert_eq!(values.len(), 3, "phase {phase}");
            for (value, expected) in values.iter().zip(expected) {
                assert!(
                    (value - expected).abs() <= 2.0 * f64::EPSILON,
                    "phase {phase}: {values:?} vs {expected}"
                );
            }
        }
    }

    /// The `n × n` matrix of these `(row, column, value)` entries, row `r`
    /// times the phase `e^{i·phase·r}`, which leaves its singular values as
    /// they are.
    fn with_row_phases(n: usize, entries: &[(usize, usize, f64)], phase: f64) -> Matrix {
        let mut elements = vec![Complex64::new(0.0, 0.0); n * n];
        for &(row, col, value) in entries {
            #[expect(clippy::cast_precision_loss, reason = "a row index")]
            let row_phase = Complex64::from_polar(1.0, phase * row as f64);
            elements[row + n * col] = row_phase * value;
        }
        Matrix::new(n, n, elements)
    }

    /// The singular values of `[[a, b], [0, e]]`, larger first.
    fn upper_triangular_values(a: f64, b: f64, e: f64) -> [f64; 2] {
        let sum = (a + e).hypot(b);
        let difference = (a - e).hypot(b);
        [f64::midpoint(sum, difference), (sum - difference) / 2.0]
    }

    /// `U·diag(s)·V†` of size 128 with `s` from 1 down to 1e-14 geometrically:
    /// a one-sided Jacobi without the pivoted-QR trim does not converge in
    /// 30 sweeps on it. The SVD gives every value to rounding of the
    /// largest; those it leaves out are rounding's.
    #[test]
    fn decomposes_a_graded_spectrum_down_to_1e_14() {
        let n = 128;
        #[expect(clippy::cast_precision_loss, reason = "an index")]
        let spectrum: Vec<f64> = (0..n)
            .map(|i| 10_f64.powf(-14.0 * i as f64 / (n - 1) as f64))
            .collect();
        let u = Qr::of(matrix(n, n, 3).view()).q;
        let v = Qr::of(matrix(n, n, 4).view()).q;
        let diagonal = Matrix::from_fn(n, n, |i, j| {
            Complex64::new(if i == j { spectrum[i] } else { 0.0 }, 0.0)
        });
        let a = naive_product(
            naive_product(u.view(), diagonal.view()).view(),
            adjoint(&v).view(),
        );
        let values = check_svd(&a);
        for (i, &expected) in spectrum.iter().enumerate() {
            let value = values.get(i).copied().unwrap_or(0.0);
            assert!(
                (value - expected).abs() < 1e-13,
                "value {i}: {value} vs {expected}"
            );
        }
    }
}

/// The pivoted-QR trim's steps, called on hand-made `R` matrices and row
/// norms: it measures each row of `R` by its norm, measures its tolerance
/// against the largest row, leaves out the shortest rows while they fit
/// jointly within it, and keeps the others wherever the pivoting put them.
mod trim {
    use crate::Matrix;
    use crate::svd::{count_within_tolerance, row_norms, rows_kept_by_trim, trim_tolerance};
    use crate::test_utils::c;

    /// A 3 × 4 `R`, as the pivoted QR of a 3 × 4 matrix gives, whose row
    /// 2 has a smaller diagonal entry than row 1 but a larger norm:
    ///
    /// ```text
    ///   row 0   3    4i    0     0      norm √(9 + 16)       = 5
    ///   row 1   0    0.75  0     0      norm                 = 0.75
    ///   row 2   0    0     0.6   0.8    norm √(0.36 + 0.64)  = 1
    /// ```
    ///
    /// Each row is measured by its norm from the diagonal on, the squared
    /// moduli of its complex entries summed, not by its diagonal entry. So,
    /// of rows 1 and 2, the trim would leave out row 1 first, though row
    /// 2's pivot is the smaller.
    #[test]
    fn measures_a_row_by_its_norm_not_its_pivot() {
        let r = Matrix::from_fn(3, 4, |i, j| match (i, j) {
            (0, 0) => c(3.0, 0.0),
            (0, 1) => c(0.0, 4.0),
            (1, 1) => c(0.75, 0.0),
            (2, 2) => c(0.6, 0.0),
            (2, 3) => c(0.8, 0.0),
            _ => c(0.0, 0.0),
        });
        let norms = row_norms(&r);
        for (row, (norm, expected)) in norms.iter().zip([5.0, 0.75, 1.0]).enumerate() {
            assert!(
                (norm - expected).abs() <= f64::EPSILON,
                "row {row}: {norms:?}"
            );
        }
    }

    /// Rows of norms 1, 4 and 2, from a matrix whose larger side is 6:
    /// the tolerance is `6 · ε` times the largest norm, 4, so `24ε`
    /// (exact, a power of two times 6), wherever the largest row sits.
    #[test]
    fn measures_the_trims_tolerance_against_the_largest_row() {
        assert_eq!(trim_tolerance(&[1.0, 4.0, 2.0], 6), 24.0 * f64::EPSILON);
    }

    /// The shortest rows are left out while their joint norm, the
    /// Frobenius norm of the rows left out so far, stays within the
    /// tolerance, here 1 (0 for the zero rows). Each case: the row norms,
    /// shortest first, and how many the trim leaves out:
    ///
    /// - 0.9 is within 1, and 1.1 is not;
    /// - two rows of 0.8, each within 1, have a joint norm of 1.13: one is
    ///   left out;
    /// - four rows of 0.5 have a joint norm of exactly 1 (their squares
    ///   are exact): within, so all four are left out;
    /// - rows that are exactly zero are left out even at a tolerance of 0.
    #[test]
    fn leaves_out_the_shortest_rows_while_jointly_within_the_tolerance() {
        let cases: [(&[f64], f64, usize); 5] = [
            (&[0.9, 2.0], 1.0, 1),
            (&[1.1, 2.0], 1.0, 0),
            (&[0.8, 0.8, 2.0], 1.0, 1),
            (&[0.5, 0.5, 0.5, 0.5, 2.0], 1.0, 4),
            (&[0.0, 0.0, 2.0], 0.0, 2),
        ];
        for (norms, tolerance, left_out) in cases {
            let shortest_first: Vec<usize> = (0..norms.len()).collect();
            assert_eq!(
                count_within_tolerance(&shortest_first, norms, tolerance),
                left_out,
                "norms {norms:?}, tolerance {tolerance}"
            );
        }
    }

    /// A diagonal 4 × 4 `R` with its pivots in the order a blocked pivoting
    /// gave on a matrix from an MPS simulation of H₂: a pivot of rounding's
    /// size, 1.9e-16, before one of 1.3e-9, then 0. The tolerance is `4ε`
    /// (8.9e-16), so the trim leaves out rows 1 and 3, of joint norm
    /// 1.9e-16, and keeps row 2, behind the row left out. It answers the
    /// rows kept in increasing order, as the columns of `Q` they select
    /// are read: `[0, 2]`.
    #[test]
    fn leaves_out_the_shortest_rows_wherever_the_pivoting_put_them() {
        let diagonal = [1.0, 1.9e-16, 1.3e-9, 0.0];
        let r = Matrix::from_fn(4, 4, |i, j| c(if i == j { diagonal[i] } else { 0.0 }, 0.0));
        assert_eq!(rows_kept_by_trim(&r, 4), [0, 2]);
    }
}

/// The factoring of the rows the trim keeps: the matrix the rotations
/// take, and the factors carried back to the matrix decomposed.
mod factor {
    use crate::kernels::{Kernels, PivotedQr};
    use crate::svd::{adjoint_of_kept_rows, factor};
    use crate::test_utils::{adjoint, c, distance, naive_product};
    use crate::{Matrix, MatrixRef, Qr};
    use num_complex::Complex64;

    /// A 3 × 3 `R` whose column `j` is column `forward[j]` of the matrix
    /// (`forward = [2, 0, 1]`), the trim keeping rows 0 and 2:
    ///
    /// ```text
    ///   R = [ 1+i  2   3i  ]     R·Pᵀ, row 0 = [ 2  3i   1+i ]
    ///       [ 0    4   5   ]     R·Pᵀ, row 2 = [ 0  6−i  0   ]
    ///       [ 0    0   6−i ]
    /// ```
    ///
    /// `W` is the adjoint of those two rows of `R·Pᵀ`, in the matrix's
    /// column order: the columns `[2, −3i, 1−i]` and `[0, 6+i, 0]`, each
    /// stored as `[re…, im…]`, as the rotations read them.
    #[test]
    fn puts_the_kept_rows_in_the_matrixs_column_order_as_adjoint_columns() {
        let r = Matrix::new(
            3,
            3,
            vec![
                c(1.0, 1.0),
                c(0.0, 0.0),
                c(0.0, 0.0),
                c(2.0, 0.0),
                c(4.0, 0.0),
                c(0.0, 0.0),
                c(0.0, 3.0),
                c(5.0, 0.0),
                c(6.0, -1.0),
            ],
        );
        let w = adjoint_of_kept_rows(&r, &[2, 0, 1], &[0, 2]);
        let expected = [
            [2.0, 0.0, 1.0, 0.0, -3.0, -1.0],
            [0.0, 6.0, 0.0, 0.0, 1.0, 0.0],
        ];
        assert_eq!(w, expected.concat());
    }

    /// A fake of the [`Kernels`] whose column-pivoted QR answers its upper
    /// triangular input as its own `R`, with `Q` and `P` the identity. The
    /// matrix [`factor`] decomposes is then `R` itself, so the trim's choice
    /// of rows shows in the factors directly. `factor` takes no other
    /// kernel.
    struct FakeKernels;

    impl Kernels for FakeKernels {
        type Q = ();

        fn product(_: MatrixRef<'_>, _: MatrixRef<'_>) -> Matrix {
            unreachable!("`factor` takes no product")
        }

        fn qr(_: MatrixRef<'_>) -> Qr {
            unreachable!("`factor` takes no thin QR")
        }

        fn pivoted_qr(a: MatrixRef<'_>) -> PivotedQr<()> {
            PivotedQr {
                r: Matrix::new(a.rows(), a.cols(), a.elements().to_vec()),
                forward: (0..a.cols()).collect(),
                q: (),
            }
        }

        /// `x` in the rows `kept` of a zero matrix: `Q_kept·x` for
        /// `Q = I`.
        fn q_kept_times(
            qr: &PivotedQr<()>,
            kept: &[usize],
            x: &[Complex64],
            rank: usize,
        ) -> Vec<Complex64> {
            let rows = qr.r.rows();
            let mut product = vec![c(0.0, 0.0); rows * rank];
            for j in 0..rank {
                for (at, &i) in kept.iter().enumerate() {
                    product[i + rows * j] = x[at + kept.len() * j];
                }
            }
            product
        }
    }

    /// A 3 × 3 upper triangular `B`, factored with [`FakeKernels`], so
    /// that `B = R`. Its row 1, of norm `1.4e-20`, is within the trim's
    /// tolerance (`3ε` times row 0's norm, 1.15), and its rows 0 and 2
    /// are not. The factors give back `B` with row 1 zero, to rounding:
    /// the left vectors are the columns of `Q` of the rows kept, 0 and 2,
    /// times the rotations' right vectors, though a row is left out before
    /// the last row kept.
    #[test]
    fn gives_back_the_matrix_less_the_rows_the_trim_leaves_out() {
        let rows = [
            [c(1.0, 0.0), c(0.0, 0.5), c(0.25, 0.0)],
            [c(0.0, 0.0), c(1e-20, 0.0), c(1e-20, 0.0)],
            [c(0.0, 0.0), c(0.0, 0.0), c(0.5, 0.5)],
        ];
        let b = Matrix::from_fn(3, 3, |i, j| rows[i][j]);
        let svd = factor::<FakeKernels>(b.view()).expect("the rotations converge");
        let rank = svd.singular_values.len();
        assert_eq!(rank, 2, "precondition: the trim leaves out row 1 alone");
        let mut us = Matrix::new(3, rank, svd.left_vectors);
        us.multiply_columns_by(&svd.singular_values);
        let v = Matrix::new(3, rank, svd.right_vectors);
        let product = naive_product(us.view(), adjoint(&v).view());
        let kept = Matrix::from_fn(3, 3, |i, j| if i == 1 { c(0.0, 0.0) } else { rows[i][j] });
        assert!(distance(&product, &kept) < 1e-15, "{product:?}");
    }
}

/// The SVD answers at any scale, and fails on the matrices whose singular
/// values are too small or too large to store. The scaling of subnormal
/// entries and the phase of a subnormal entry are the QR's and the
/// kernels' rules, pinned in their tests.
mod scale {
    use crate::test_utils::{check_svd, check_svd_of, matrix, naive_product, times_power_of_two};
    use crate::{Matrix, Svd, SvdError};

    /// Matrices whose entries are near 1e-300, 1e-160, 1e160 or 1e300, as
    /// no MPS simulation meets but a caller's matrix may hold: their
    /// squared column norms would underflow or overflow. The SVD answers,
    /// and its factors, its values scaled back by the same power of two,
    /// are an SVD of the matrix at scale 1, with its values.
    #[test]
    fn decomposes_matrices_at_extreme_scales() {
        let rank_3 = naive_product(matrix(8, 3, 5).view(), matrix(3, 8, 6).view());
        for (case, a) in [matrix(6, 4, 1), matrix(4, 6, 2), rank_3]
            .iter()
            .enumerate()
        {
            let at_one = check_svd(a);
            for exponent in [-1000, -530, 530, 1000] {
                let values = values_scaled_back(a, exponent, &format!("case {case}"));
                assert_eq!(values.len(), at_one.len(), "case {case}, 2^{exponent}");
                for (value, expected) in values.iter().zip(&at_one) {
                    assert!(
                        (value - expected).abs() < 1e-13,
                        "case {case}, 2^{exponent}: {values:?} vs {at_one:?}"
                    );
                }
            }
        }
    }

    /// The singular values of `a` times `2^exponent`, scaled back by
    /// `2^-exponent`, once its factors and those values are checked to be
    /// an SVD of `a`.
    fn values_scaled_back(a: &Matrix, exponent: i32, case: &str) -> Vec<f64> {
        let svd = Svd::of(times_power_of_two(a, exponent).view())
            .unwrap_or_else(|_| panic!("{case}, 2^{exponent}: SVD"));
        let back = |s: &f64| s * 2_f64.powi(-exponent);
        check_svd_of(
            a,
            Svd {
                singular_values: svd.singular_values.iter().map(back).collect(),
                ..svd
            },
        )
    }

    /// The SVD fails rather than answer values it cannot store to the
    /// reconstruction's bound, on the 6 × 4 matrix scaled by a power of
    /// two. Scaled down by 2^1060, to deep subnormal entries (near 1e-319,
    /// of about 14 significant bits), its singular values, subnormal too,
    /// would give back the matrix only to 2^-14. Scaled up by 2^1024, to
    /// finite entries up to 2^1023 (about 9e307), its largest singular
    /// value, at least 2^1024, is above `f64::MAX`.
    #[test]
    fn fails_when_singular_values_cannot_be_stored() {
        let at_one = matrix(6, 4, 1);
        assert!(
            check_svd(&at_one)[0] >= 1.0,
            "precondition: a largest value of at least 1 at scale 1"
        );
        for exponent in [-1060, 1024] {
            let a = times_power_of_two(&at_one, exponent);
            assert!(
                a.view().elements().iter().all(|x| x.is_finite()),
                "2^{exponent}: precondition, finite entries"
            );
            let error = Svd::of(a.view()).expect_err("values it cannot store");
            assert_eq!(
                error,
                SvdError::FailedChecks { rows: 6, cols: 4 },
                "2^{exponent}"
            );
        }
    }
}

/// A truncated SVD keeps its first values and their vectors, and refuses
/// to keep none, or more than it has.
mod truncation {
    use crate::test_utils::{matrix, naive_product, panic_message};
    use crate::{Matrix, Svd};

    /// Truncated to each `k` of its 4 values, the SVD of a random 5 × 4
    /// matrix keeps `u`'s first `k` columns, the first `k` values and
    /// `v†`'s first `k` rows, each element as it was. So, to rounding,
    /// their product leaves out of the matrix `√(s_k² + ⋯ + s_3²)` in the
    /// Frobenius norm.
    #[test]
    fn keeps_the_first_values_and_their_vectors() {
        let a = matrix(5, 4, 1);
        let svd = Svd::of(a.view()).expect("a random matrix has a checked SVD");
        assert_eq!(
            svd.singular_values.len(),
            4,
            "a random matrix has full rank"
        );
        let (u, v_adjoint) = (svd.u.elements(), svd.v_adjoint.elements());
        for k in 1..=4 {
            let kept = svd.clone().truncated(k);
            assert_eq!(kept.singular_values, svd.singular_values[..k], "k = {k}");
            let first_columns = Matrix::from_fn(5, k, |i, j| u[i + 5 * j]);
            assert_eq!(kept.u, first_columns, "k = {k}: u");
            let first_rows = Matrix::from_fn(k, 4, |i, j| v_adjoint[i + 4 * j]);
            assert_eq!(kept.v_adjoint, first_rows, "k = {k}: v†");
            let mut us = kept.u.clone();
            us.multiply_columns_by(&kept.singular_values);
            let product = naive_product(us.view(), kept.v_adjoint.view());
            let left_out = a
                .elements()
                .iter()
                .zip(product.elements())
                .map(|(x, y)| (x - y).norm_sqr())
                .sum::<f64>()
                .sqrt();
            let dropped = svd.singular_values[k..].iter().map(|s| s * s).sum::<f64>();
            assert!((left_out - dropped.sqrt()).abs() < 1e-13, "k = {k}");
        }
    }

    /// Truncating the SVD of a random 5 × 4 matrix, of 4 values, to none or
    /// to 5 panics, naming both counts.
    #[test]
    fn truncating_to_no_value_or_more_than_it_has_panics() {
        for k in [0, 5] {
            let message = panic_message(|| {
                let svd = Svd::of(matrix(5, 4, 1).view()).expect("a checked SVD");
                let _ = svd.truncated(k);
            });
            assert!(message.contains("4 singular values"), "k = {k}: {message}");
            assert!(message.contains(&format!("to {k}")), "k = {k}: {message}");
        }
    }
}
