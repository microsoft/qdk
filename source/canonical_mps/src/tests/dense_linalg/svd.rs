// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The tests of the SVD's guarantees (`svd`), through
//! [`Svd::of`](crate::dense_linalg::Svd::of) where they can be: accuracy on the
//! matrices an MPS simulation meets, the trim that leaves out only rounding,
//! every scale, and the errors. The checks and the rotations have their own
//! files.

mod checks;
mod jacobi;

/// The SVD gives back the matrix with orthonormal factors and
/// nonincreasing values, on random matrices of every shape and on the
/// rank-deficient ones an MPS simulation meets all the time.
mod decomposition {
    use super::super::{SHAPES, check_svd, matrix, naive_product};
    use crate::dense_linalg::{Matrix, Qr};
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
    /// as an MPS simulation meets them all the time: nalgebra 0.35's own
    /// SVD returns a wrong singular value on 8 of these, which a caller
    /// truncating by the values would act on. The SVD gives back each,
    /// with exactly `rank` values: on `U·diag(√½, …, √½, 0, …)·V`, the
    /// value √½ `rank` times, and on a product of random `rows × rank` and
    /// `rank × cols` factors, `rank` values; the trim leaves out the rows of
    /// `R` that the pivoting leaves at rounding's size. The zero matrix
    /// gives one value, exactly 0: the trim would keep none of its rows.
    #[test]
    fn gives_back_a_rank_deficient_matrix_with_exactly_its_rank_values() {
        for (rows, cols) in [(4, 3), (3, 4)] {
            let zero = Matrix::from_fn(rows, cols, |_, _| Complex64::new(0.0, 0.0));
            assert_eq!(check_svd(&zero), [0.0], "{rows} × {cols}");
        }
        let half = 0.5_f64.sqrt();
        for (seed, case) in (0_u64..60).zip(0_usize..) {
            let n = 2 + case % 15;
            let rank = 1 + (case / 15) % n;
            let u = Qr::of(matrix(n, n, 2 * seed + 1).view()).q;
            let v = Qr::of(matrix(n, n, 2 * seed + 2).view()).q;
            let diagonal = Matrix::from_fn(n, n, |i, j| {
                Complex64::new(if i == j && i < rank { half } else { 0.0 }, 0.0)
            });
            let degenerate =
                naive_product(naive_product(u.view(), diagonal.view()).view(), v.view());
            let values = check_svd(&degenerate);
            assert_eq!(values.len(), rank, "n {n}, rank {rank}: {values:?}");
            assert!(
                values.iter().all(|s| (s - half).abs() < 1e-13),
                "n {n}, rank {rank}: {values:?}"
            );
            let (rows, cols) = (n + case % 3, n + (case / 3) % 3);
            let product = naive_product(
                matrix(rows, rank, 3 * seed + 1).view(),
                matrix(rank, cols, 3 * seed + 2).view(),
            );
            let values = check_svd(&product);
            assert_eq!(
                values.len(),
                rank,
                "{rows} × {cols}, rank {rank}: {values:?}"
            );
        }
    }
}

/// What [`Svd::of`](crate::dense_linalg::Svd::of) refuses, and how its errors
/// read.
mod errors {
    use super::super::{matrix, panic_message};
    use crate::dense_linalg::{Matrix, Svd, SvdError};
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
                message.contains(&format!("the SVD of an empty {rows} × {cols} matrix")),
                "{message}"
            );
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
/// general SVDs have failed: degenerate, rank-deficient, graded.
mod accuracy {
    use super::super::{adjoint, check_svd, matrix, naive_product};
    use crate::dense_linalg::{Matrix, Qr};
    use num_complex::Complex64;

    /// Degenerate pairs, as an MPS simulation meets them (its singular
    /// values come in degenerate pairs), here alone, coupled at the
    /// threshold's scale (`√n · ε` of the pair's norms, `n` the smaller
    /// side). The SVD gives each pair to rounding, and so with a phase on
    /// each row, which leaves the singular values as they are and makes the
    /// products of the pair's columns complex:
    /// - the upper bidiagonal `[[1, 0, 0], [0, d, ε], [0, 0, d + ulp]]`,
    ///   with `d` = 0.928…: a pair one ulp apart, coupled by `ε`. faer
    ///   0.24.4's QR algorithm reported that it does not converge on it,
    ///   and its divide and conquer took the same QR algorithm at this size;
    /// - the same pair in a 64 × 64 matrix, coupled by `30 ε`: above the
    ///   threshold (`8 ε`), so the pair is rotated. A threshold of `n · ε`
    ///   (`64 ε`) leaves it, and the values 15 ε off;
    /// - [`paired_by_a_rotation`]: a pair that only a rotation of two other
    ///   columns brings together, whose coupling is tested again once one
    ///   of its columns rotates. Skipping it because its other column has
    ///   not rotated leaves the values about 8 ε off.
    #[test]
    fn decomposes_degenerate_pairs_to_rounding() {
        let (d, d_next) = (9.282_350_173_388_935e-1, 9.282_350_173_388_936e-1);
        let pair = |n, coupling| {
            let entries = vec![(0, 0, 1.0), (1, 1, d), (1, 2, coupling), (2, 2, d_next)];
            let [larger, smaller] = upper_triangular_values(d, coupling, d_next);
            (n, entries, [1.0, larger, smaller])
        };
        let scenes = [
            pair(3, f64::EPSILON),
            pair(64, 30.0 * f64::EPSILON),
            paired_by_a_rotation(),
        ];
        for (n, entries, expected) in scenes {
            for phase in [0.0, 0.7] {
                let mut elements = vec![Complex64::new(0.0, 0.0); n * n];
                for &(i, j, x) in &entries {
                    #[expect(clippy::cast_precision_loss, reason = "a row index")]
                    let row_phase = Complex64::from_polar(1.0, phase * i as f64);
                    elements[i + n * j] = row_phase * x;
                }
                let values = check_svd(&Matrix::new(n, n, elements));
                assert_eq!(values.len(), 3, "n {n}, phase {phase}");
                for (value, expected) in values.iter().zip(expected) {
                    assert!(
                        (value - expected).abs() <= 2.0 * f64::EPSILON,
                        "n {n}, phase {phase}: {values:?} vs {expected}"
                    );
                }
            }
        }
    }

    /// The singular values of `[[a, b], [0, e]]`, larger first.
    fn upper_triangular_values(a: f64, b: f64, e: f64) -> [f64; 2] {
        let sum = (a + e).hypot(b);
        let difference = (a - e).hypot(b);
        [f64::midpoint(sum, difference), (sum - difference) / 2.0]
    }

    /// The entries `(row, column, value)` of a 64 × 64 matrix of rank 3,
    /// and its values. The pivoted QR keeps its rows, up to a phase each,
    /// and the rotations see them as columns:
    /// - row 0 is `e₀` but for `20 ε` and `−20 ε` in columns 1 and 2: it
    ///   couples to rows 1 and 2 by `15 ε` and `−10 ε`, each below the
    ///   threshold, and is never rotated in the first sweep;
    /// - rows 1 and 2, of squared norms 4 and 1.75, couple by 1.5, and
    ///   their rotation leaves a column of squared norm 1, as row 0's, to
    ///   which row 0 couples by `35 ε / √5`, above the threshold.
    ///
    /// The pair's values are `√(1 ± 35 ε / √5)`, up to terms of order `ε²`.
    fn paired_by_a_rotation() -> (usize, Vec<(usize, usize, f64)>, [f64; 3]) {
        let (r01, r02) = (20.0 * f64::EPSILON, -20.0 * f64::EPSILON);
        let mut entries = vec![
            (0, 0, 1.0),
            (0, 1, r01),
            (0, 2, r02),
            (1, 1, 0.75),
            (2, 2, 0.5),
        ];
        entries.extend((3..27).flat_map(|j| [(1, j, 0.25), (2, j, 0.25)]));
        entries.extend((27..34).map(|j| (1, j, 0.5)));
        entries.extend((34..37).map(|j| (1, j, 0.25)));
        let coupling = (0.75 * r01 - 2.0 * 0.5 * r02).abs() / 5_f64.sqrt();
        let values = [
            4.75_f64.sqrt(),
            (1.0 + coupling).sqrt(),
            (1.0 - coupling).sqrt(),
        ];
        (64, entries, values)
    }

    /// A 128 × 128 matrix of rank 64, the size of a split of an MPS at
    /// bond 64: faer 0.24.4's default SVD, which diagonalizes by divide and
    /// conquer from size 128, returned on it a spurious singular value of
    /// 2.4e-4 where the matrix has 0, and an entry error of 4e-5, without
    /// reporting a failure. The SVD gives its 64 values and no spurious
    /// one.
    #[test]
    fn decomposes_a_128_by_128_matrix_of_rank_64() {
        let a = naive_product(matrix(128, 64, 1).view(), matrix(64, 128, 101).view());
        let values = check_svd(&a);
        assert!(values[63] > 1.0, "the 64 values of the rank");
        assert!(values.iter().skip(64).all(|&s| s < 1e-12), "{values:?}");
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

/// The pivoted-QR trim leaves out only the values of rounding, measured
/// by the rows' norms against the largest row, at any scale.
mod trim {
    use super::super::{check_svd, check_svd_of};
    use crate::dense_linalg::{Matrix, Svd};
    use num_complex::Complex64;

    /// A 4 × 4 matrix from an MPS simulation of H₂: two columns equal
    /// to rounding, and two of norm 4e-6 that differ by about 1e-9. A
    /// blocked pivoted QR (faer 0.24.4's, t4a-faer's) put the pivot of
    /// rounding's size (2e-16) before the one of 1e-9, so a trim that cut
    /// the rows of `R` at the first small diagonal entry would drop the
    /// singular value 9.4e-10. The SVD keeps it, whatever the pivoted QR's
    /// order: the factors give back the matrix to 1e-13.
    #[test]
    fn keeps_a_singular_value_behind_a_pivot_of_rounding_size() {
        let entries = [
            (-2.826_477_255_239_069e-1, 2.826_477_255_239_07e-1),
            (3.997_242_468_098_175_5e-1, 3.606_189_232_763_299e-17),
            (-2.334_936_804_251_045e-19, -4.568_220_746_422_801e-21),
            (9.281_104_830_607_539e-21, -5.464_237_877_849_814e-21),
            (-2.826_477_255_239_068e-1, 2.826_477_255_239_07e-1),
            (3.997_242_468_098_178e-1, -1.026_293_516_740_142_1e-16),
            (-2.334_936_804_247_072e-19, -4.568_220_745_640_912_4e-21),
            (9.281_104_830_607_248e-21, -5.464_237_877_849_605e-21),
            (5.485_030_873_338_035e-20, 2.611_838_808_297_573_3e-19),
            (6.930_070_926_668_269e-19, -7.256_210_936_041_421e-19),
            (2.818_261_032_938_185e-6, -2.799_256_216_118_589_3e-6),
            (-4.897_464_638_578_369e-7, 4.857_963_813_784_236e-7),
            (-2.377_931_200_857_292e-20, -1.003_349_300_101_549_2e-18),
            (-1.468_714_090_873_190_5e-19, 2.225_998_373_728_427_6e-19),
            (2.818_231_819_215_572_3e-6, -2.792_004_717_411_87e-6),
            (-4.898_151_985_876_091e-7, 4.858_801_243_624_121e-7),
        ];
        let a = Matrix::new(
            4,
            4,
            entries
                .iter()
                .map(|&(re, im)| Complex64::new(re, im))
                .collect(),
        );
        let values = check_svd_of(&a, Svd::of(a.view()).expect("SVD"));
        assert_eq!(values.len(), 3, "{values:?}");
        assert!(values[2] > 9e-10, "{values:?}");
    }

    /// A diagonal matrix of values `c` and small ones near the trim's
    /// tolerance, `max(m, n) · ε` times the largest: the SVD leaves out
    /// the smallest values only while their joint norm stays within it.
    /// Each row: the rows of a matrix of 5 columns, the small values in
    /// units of the tolerance, the scale `c` as a power of ten, and the
    /// values kept; at scale 1, the factors are checked too. On a
    /// 6 × 5 matrix (a tolerance of `6ε`), one value of 0.9 times the
    /// tolerance is left out, one of 1.1 times is kept, and of two of 0.8
    /// times each, whose joint norm is 1.13 times, one is kept. On an 8 × 5
    /// one, a value of `8ε`, the tolerance exactly (`8ε = 2⁻⁴⁹`, so the
    /// rows' norms and their squares are exact): a joint norm equal to the
    /// tolerance is within it, and the value is left out. The tolerance
    /// scales with the matrix: the same at a scale of 1e-100 or 1e100.
    #[test]
    fn leaves_out_small_values_only_within_the_tolerance() {
        let rows: [(usize, &[f64], i32, usize); 8] = [
            (6, &[0.9], 0, 4),
            (6, &[1.1], 0, 5),
            (6, &[0.8, 0.8], 0, 4),
            (8, &[1.0], 0, 4),
            (6, &[0.9], -100, 4),
            (6, &[1.1], -100, 5),
            (6, &[0.9], 100, 4),
            (6, &[1.1], 100, 5),
        ];
        for (rows, small, exponent, kept) in rows {
            let scale = 10f64.powi(exponent);
            #[expect(clippy::cast_precision_loss, reason = "a small count")]
            let tolerance = rows as f64 * f64::EPSILON;
            let diagonal: Vec<f64> = std::iter::repeat_n(1.0, 5 - small.len())
                .chain(small.iter().map(|s| s * tolerance))
                .map(|value| value * scale)
                .collect();
            let a = Matrix::from_fn(rows, 5, |i, j| {
                Complex64::new(if i == j { diagonal[i] } else { 0.0 }, 0.0)
            });
            let values = if exponent == 0 {
                check_svd(&a)
            } else {
                Svd::of(a.view()).expect("SVD").singular_values
            };
            assert_eq!(
                values.len(),
                kept,
                "{rows} × 5, small values {small:?}, scale {scale}: {values:?}"
            );
        }
    }

    /// A 6 × 5 matrix whose pivoted QR has a first row of norm 1 and a
    /// second, larger, of norm `0.9·√3`: columns `e₀`, three of `0.9·e₁`,
    /// and a small `s·e₂`. The trim's tolerance is `6ε` times the largest
    /// row's norm, so `s = 1.3 · 6ε`, above `6ε` times the first row's norm
    /// but below `6ε` times the largest, is left out.
    #[test]
    fn measures_the_trims_tolerance_against_the_largest_row() {
        let small = 1.3 * 6.0 * f64::EPSILON;
        let a = Matrix::from_fn(6, 5, |i, j| {
            let value = match (i, j) {
                (0, 0) => 1.0,
                (1, 1..=3) => 0.9,
                (2, 4) => small,
                _ => 0.0,
            };
            Complex64::new(value, 0.0)
        });
        let values = check_svd(&a);
        assert_eq!(values.len(), 2, "{values:?}");
    }

    /// A 7 × 5 matrix with values 1, `d·√3` and `d·1.1`, `d = 5ε`: columns
    /// `e₀`, `1.1d·e₁`, and three of `d·(e₂ + e₃)/√2`. The pivoted QR's
    /// second row, `1.1d` on its diagonal alone, is smaller than its third,
    /// `d` on the diagonal and `d` twice beside it: the trim measures each
    /// row by its norm, so it leaves out the value `1.1d` and keeps
    /// `d·√3`, though `d` is the smaller pivot.
    fn a_row_left_out_before_a_kept_one() -> Matrix {
        let d = 5.0 * f64::EPSILON;
        let half = std::f64::consts::FRAC_1_SQRT_2 * d;
        Matrix::from_fn(7, 5, |i, j| {
            let value = match (i, j) {
                (0, 0) => 1.0,
                (1, 1) => 1.1 * d,
                (2 | 3, 2..=4) => half,
                _ => 0.0,
            };
            Complex64::new(value, 0.0)
        })
    }

    /// The trim measures a row of `R` by its norm from the diagonal on,
    /// not by its diagonal entry: on `a_row_left_out_before_a_kept_one`
    /// it keeps the value `d·√3` and leaves out `1.1d`.
    #[test]
    fn leaves_out_the_rows_of_smallest_norm_not_of_smallest_pivot() {
        let a = a_row_left_out_before_a_kept_one();
        let values = check_svd(&a);
        let kept = 5.0 * f64::EPSILON * 3f64.sqrt();
        assert_eq!(values.len(), 2, "{values:?}");
        assert!((values[1] - kept).abs() < 1e-3 * kept, "{values:?}");
    }

    /// On `a_row_left_out_before_a_kept_one`, whose kept rows of `R` are
    /// the first and the third, the left singular vector of the value
    /// `d·√3` kept behind the row left out is its own, `(e₂ + e₃)/√2`, to
    /// rounding: the reflectors of `Q` are applied up to the last kept
    /// row, not one per kept row.
    #[test]
    fn a_value_kept_after_a_row_left_out_has_its_own_left_vector() {
        let a = a_row_left_out_before_a_kept_one();
        let svd = Svd::of(a.view()).expect("SVD");
        assert_eq!(svd.singular_values.len(), 2, "{:?}", svd.singular_values);
        let u = svd.u.view().elements();
        let column = &u[7..14];
        let overlap = (column[2] + column[3]).norm() * std::f64::consts::FRAC_1_SQRT_2;
        assert!((overlap - 1.0).abs() < 1e-12, "overlap {overlap}");
    }
}

/// The SVD answers at any scale, down to subnormal entries, and fails on
/// the matrices whose singular values are too small or too large to
/// store.
mod scale {
    use super::super::{check_svd, check_svd_of, matrix, naive_product, times_power_of_two};
    use crate::dense_linalg::{Svd, SvdError};

    /// Matrices whose entries are near 1e-300, 1e-160, 1e160 or 1e300, as
    /// no MPS simulation meets but a caller's matrix may hold: their
    /// squared column norms would underflow or overflow. The SVD answers,
    /// and its factors, its values scaled back by the same power of two,
    /// are an SVD of the matrix at scale 1, with its values. And a matrix
    /// of subnormal entries, just below the smallest normal number (near
    /// 2^-1025, so of about 49 significant bits): scaling it to 1 takes
    /// 2^1025, which overflows as one factor. The SVD answers, and its
    /// values are those of the matrix scaled up to 1 (exactly), scaled
    /// down.
    #[test]
    fn decomposes_matrices_at_extreme_scales() {
        let rank_3 = naive_product(matrix(8, 3, 5).view(), matrix(3, 8, 6).view());
        for (case, a) in [matrix(6, 4, 1), matrix(4, 6, 2), rank_3]
            .iter()
            .enumerate()
        {
            let at_one = check_svd(a);
            for exponent in [-1000, -530, 530, 1000] {
                let svd = Svd::of(times_power_of_two(a, exponent).view())
                    .unwrap_or_else(|_| panic!("case {case}, 2^{exponent}: SVD"));
                let back = |s: &f64| s * 2_f64.powi(-exponent);
                let values = check_svd_of(
                    a,
                    Svd {
                        singular_values: svd.singular_values.iter().map(back).collect(),
                        ..svd
                    },
                );
                assert_eq!(values.len(), at_one.len(), "case {case}, 2^{exponent}");
                for (value, expected) in values.iter().zip(&at_one) {
                    assert!(
                        (value - expected).abs() < 1e-13,
                        "case {case}, 2^{exponent}: {values:?} vs {at_one:?}"
                    );
                }
            }
        }
        let subnormal = times_power_of_two(&matrix(6, 4, 1), -1025);
        let expected = check_svd(&times_power_of_two(&subnormal, 1025));
        let svd = Svd::of(subnormal.view()).expect("SVD");
        assert_eq!(svd.singular_values.len(), expected.len());
        for (value, expected) in svd.singular_values.iter().zip(&expected) {
            let value = value * 2_f64.powi(512) * 2_f64.powi(513);
            assert!(
                (value - expected).abs() < 1e-13,
                "subnormal: {value} vs {expected}"
            );
        }
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
