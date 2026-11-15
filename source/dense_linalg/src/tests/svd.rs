// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The tests of the SVD's guarantees (`svd`), through [`Svd::of`] where
//! they can be: accuracy on the matrices an MPS simulation meets, the trim
//! that leaves out only rounding, every scale, the routes, each giving a
//! checked SVD, with the fallback to the rotations, and the errors. The
//! checks and the rotations have their own files.
//!
//! [`Svd::of`]: crate::Svd::of

mod checks;
mod jacobi;

/// The SVD gives back the matrix with orthonormal factors and
/// nonincreasing values, on random matrices of every shape and on the
/// rank-deficient ones an MPS simulation meets all the time.
mod decomposition {
    use super::super::{SHAPES, check_svd, matrix, naive_product};
    use crate::{Matrix, Qr};
    use num_complex::Complex64;

    /// Random matrices, tall, wide, square, a single row, and up to
    /// 128 × 128: the SVD gives back each, with orthonormal U and V and
    /// nonincreasing values, one per column of the smaller side.
    #[test]
    fn decomposes_by_svd() {
        let larger = [(128, 128), (64, 128), (2, 2)];
        for (seed, &(rows, cols)) in (1..).zip(SHAPES.iter().chain(&larger)) {
            let values = check_svd(&matrix(rows, cols, seed));
            assert_eq!(values.len(), rows.min(cols), "{rows} × {cols}");
        }
    }

    /// Rank-deficient matrices, with repeated and zero singular values,
    /// as an MPS simulation meets them all the time: nalgebra 0.35's own
    /// SVD returns a wrong singular value on 8 of these, which a caller
    /// truncating by the values would act on. The SVD gives back each;
    /// on `U·diag(√½, …, √½, 0, …)·V`, it gives the value √½ `rank`
    /// times, and leaves out the zeros or gives them at rounding's size.
    /// The zero matrix gives one value, exactly 0: the trim would keep
    /// none of its rows.
    #[test]
    fn decomposes_rank_deficient_matrices_by_svd() {
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
            let (repeated, rest) = values.split_at(rank.min(values.len()));
            assert_eq!(repeated.len(), rank, "n {n}, rank {rank}: {values:?}");
            assert!(
                repeated.iter().all(|s| (s - half).abs() < 1e-13),
                "n {n}, rank {rank}: {values:?}"
            );
            assert!(
                rest.iter().all(|&s| s < 1e-13),
                "n {n}, rank {rank}: {values:?}"
            );
            let (rows, cols) = (n + case % 3, n + (case / 3) % 3);
            let product = naive_product(
                matrix(rows, rank, 3 * seed + 1).view(),
                matrix(rank, cols, 3 * seed + 2).view(),
            );
            let values = check_svd(&product);
            assert!(
                values.iter().skip(rank).all(|&s| s < 1e-13),
                "{rows} × {cols}, rank {rank}: {values:?}"
            );
        }
    }
}

/// What [`Svd::of`] refuses, and how its errors read.
///
/// [`Svd::of`]: crate::Svd::of
mod errors {
    use super::super::matrix;
    use crate::{Matrix, Svd, SvdError};
    use num_complex::Complex64;

    /// The errors name the matrix's shape and what went wrong, for a
    /// caller that reports them.
    #[test]
    fn errors_name_the_shape_and_the_failure() {
        let did_not_converge = SvdError::DidNotConverge { rows: 6, cols: 4 };
        let failed_checks = SvdError::FailedChecks { rows: 6, cols: 4 };
        assert_eq!(
            did_not_converge.to_string(),
            "the SVD of a 6 × 4 matrix did not converge"
        );
        assert_eq!(
            failed_checks.to_string(),
            "the SVD of a 6 × 4 matrix failed its accuracy checks"
        );
    }

    /// A matrix with no rows or no columns has no singular value: the
    /// SVD panics, naming the shape, rather than answer an empty one.
    #[test]
    #[should_panic(expected = "the SVD of an empty 0 × 3 matrix")]
    fn panics_on_an_empty_matrix() {
        let _ = Svd::of(Matrix::new(0, 3, Vec::new()).view());
    }

    /// The same, with no columns.
    #[test]
    #[should_panic(expected = "the SVD of an empty 3 × 0 matrix")]
    fn panics_on_a_matrix_with_no_columns() {
        let _ = Svd::of(Matrix::new(3, 0, Vec::new()).view());
    }

    /// A matrix with a NaN or an infinite entry, in its real or imaginary
    /// part, has no SVD to rounding: the SVD fails, its error saying that
    /// the factors failed the checks, rather than answer factors that are
    /// not finite. Tall, wide, square, routed to the fast route and 1 × 1,
    /// and zero but for the entry, whose largest finite entry is 0.
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
    use crate::{Matrix, Qr};
    use num_complex::Complex64;

    /// The upper bidiagonal `[[1, 0, 0], [0, d, ε], [0, 0, d + ulp]]`, with
    /// `d` = 0.928…: a degenerate pair one ulp apart, coupled by `ε`, as an
    /// MPS simulation meets them (its singular values come in degenerate
    /// pairs), here alone. faer 0.24.4's QR algorithm reported that it does
    /// not converge on it, and its divide and conquer took the same QR
    /// algorithm at this size. The SVD
    /// gives the pair to rounding, and so with a phase on each row, which
    /// leaves the singular values as they are and makes the products of
    /// the pair's columns complex.
    #[test]
    fn decomposes_a_degenerate_pair_one_ulp_apart() {
        let (d, d_next) = (9.282_350_173_388_935e-1, 9.282_350_173_388_936e-1);
        for phase in [0.0, 0.7] {
            let a = Matrix::from_fn(3, 3, |i, j| {
                let x = match (i, j) {
                    (0, 0) => 1.0,
                    (1, 1) => d,
                    (1, 2) => f64::EPSILON,
                    (2, 2) => d_next,
                    _ => 0.0,
                };
                #[expect(clippy::cast_precision_loss, reason = "a row index")]
                let row_phase = Complex64::from_polar(1.0, phase * i as f64);
                row_phase * x
            });
            let values = check_svd(&a);
            assert_eq!(values.len(), 3, "phase {phase}");
            for (value, expected) in values.iter().zip([1.0, d_next, d]) {
                assert!(
                    (value - expected).abs() <= 2.0 * f64::EPSILON,
                    "phase {phase}"
                );
            }
        }
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
    use crate::{Matrix, Svd};
    use num_complex::Complex64;

    /// A 4 × 4 matrix from an MPS simulation of H₂: two columns equal
    /// to rounding, and two of norm 4e-6 that differ by about 1e-9.
    /// t4a-faer's pivoted QR, as faer 0.24.4's did, puts the pivot of
    /// rounding's size (2e-16) before the one of 1e-9, so a trim that cut
    /// the rows of `R` at the first small diagonal entry would drop the
    /// singular value 9.4e-10. The SVD keeps it: the factors give back the
    /// matrix to 1e-13.
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

    /// A 6 × 5 matrix with four singular values of 1 and small ones near
    /// the trim's tolerance, `max(m, n) · ε` times the largest (here
    /// `6ε`): the SVD leaves out the smallest values only while their
    /// joint norm stays within it. One value of 0.9 times the tolerance is
    /// left out, one of 1.1 times is kept, and of two of 0.8 times each,
    /// whose joint norm is 1.13 times, one is kept.
    #[test]
    fn leaves_out_small_values_only_while_their_joint_norm_is_rounding() {
        let tolerance = 6.0 * f64::EPSILON;
        for (small, kept) in [(&[0.9][..], 4), (&[1.1][..], 5), (&[0.8, 0.8][..], 4)] {
            let diagonal: Vec<f64> = [1.0; 5]
                .iter()
                .take(5 - small.len())
                .copied()
                .chain(small.iter().map(|s| s * tolerance))
                .collect();
            let a = Matrix::from_fn(6, 5, |i, j| {
                Complex64::new(if i == j { diagonal[i] } else { 0.0 }, 0.0)
            });
            let values = check_svd(&a);
            assert_eq!(values.len(), kept, "small values {small:?}: {values:?}");
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
    fn gives_a_value_kept_behind_a_row_left_out_its_own_left_vector() {
        let a = a_row_left_out_before_a_kept_one();
        let svd = Svd::of(a.view()).expect("SVD");
        assert_eq!(svd.singular_values.len(), 2, "{:?}", svd.singular_values);
        let u = svd.u.view().elements();
        let column = &u[7..14];
        let overlap = (column[2] + column[3]).norm() * std::f64::consts::FRAC_1_SQRT_2;
        assert!((overlap - 1.0).abs() < 1e-12, "overlap {overlap}");
    }

    /// The trim's tolerance scales with the matrix: a 6 × 5 diagonal of
    /// four values of `c` and one of 0.9 times `6ε·c` gives four values,
    /// and one of 1.1 times gives five, whether `c` is 1e-100 or 1e100.
    #[test]
    fn leaves_out_the_same_small_values_at_any_scale() {
        let tolerance = 6.0 * f64::EPSILON;
        for scale in [1e-100, 1e100] {
            for (small, kept) in [(0.9, 4), (1.1, 5)] {
                let a = Matrix::from_fn(6, 5, |i, j| {
                    let value = match (i == j, i < 4) {
                        (false, _) => 0.0,
                        (true, true) => scale,
                        (true, false) => small * tolerance * scale,
                    };
                    Complex64::new(value, 0.0)
                });
                let values = Svd::of(a.view()).expect("SVD").singular_values;
                assert_eq!(
                    values.len(),
                    kept,
                    "scale {scale}, small {small}: {values:?}"
                );
            }
        }
    }
}

/// The SVD answers at any scale, down to subnormal entries, and fails on
/// the matrices whose singular values are too small or too large to
/// store.
mod scale {
    use super::super::{check_svd, check_svd_of, matrix, naive_product, times_power_of_two};
    use crate::{Svd, SvdError};

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
    }

    /// A matrix of subnormal entries, just below the smallest normal
    /// number (near 2^-1025, so of about 49 significant bits): scaling it
    /// to 1 takes 2^1025, which overflows as one factor. The SVD answers,
    /// and its values are those of the matrix scaled up to 1 (exactly),
    /// scaled down.
    #[test]
    fn decomposes_a_matrix_of_subnormal_entries() {
        let a = times_power_of_two(&matrix(6, 4, 1), -1025);
        let expected = check_svd(&times_power_of_two(&a, 1025));
        let svd = Svd::of(a.view()).expect("SVD");
        assert_eq!(svd.singular_values.len(), expected.len());
        for (value, expected) in svd.singular_values.iter().zip(&expected) {
            let value = value * 2_f64.powi(512) * 2_f64.powi(513);
            assert!((value - expected).abs() < 1e-13, "{value} vs {expected}");
        }
    }

    /// A matrix of deep subnormal entries (near 1e-319, of about 14
    /// significant bits): its singular values, subnormal too, cannot be
    /// stored to the reconstruction's bound, so the SVD fails rather than
    /// answer factors that give back the matrix only to 2^-14.
    #[test]
    fn fails_on_a_matrix_whose_singular_values_are_too_small_to_store() {
        let a = times_power_of_two(&matrix(6, 4, 1), -1060);
        let error = Svd::of(a.view()).expect_err("values of 14 bits");
        assert_eq!(error, SvdError::FailedChecks { rows: 6, cols: 4 });
    }

    /// A matrix of entries up to 2^1023 (about 9e307), the 6 × 4 matrix
    /// of the subnormal tests scaled up by 2^1024: its largest singular
    /// value, at least 2^1024, is above `f64::MAX`, so the SVD fails
    /// rather than answer an infinite value.
    #[test]
    fn fails_on_a_matrix_whose_singular_values_are_too_large_to_store() {
        let at_one = matrix(6, 4, 1);
        assert!(
            check_svd(&at_one)[0] >= 1.0,
            "a largest value of at least 1 at scale 1"
        );
        let a = times_power_of_two(&at_one, 1024);
        assert!(
            a.view().elements().iter().all(|x| x.is_finite()),
            "finite entries"
        );
        let error = Svd::of(a.view()).expect_err("a value above f64::MAX");
        assert_eq!(error, SvdError::FailedChecks { rows: 6, cols: 4 });
    }
}

/// Every route gives a checked SVD: the fast route on large matrices far
/// from orthogonal, the rotations otherwise and whenever the fast route
/// fails or answers factors that are not finite.
mod routes {
    use super::super::{check_svd, check_svd_of, matrix};
    use crate::{
        Matrix, SvdError,
        kernels::Shipped,
        svd::{
            ColumnMajorSvd,
            checks::checked,
            jacobi::{MAX_SWEEPS, one_sided},
            trimmed_thin_svd,
        },
    };
    use num_complex::Complex64;

    /// Full-rank matrices of rank 32 and more, far from orthogonal, as the
    /// splits of deep circuits are: the SVD factors them by the fast route
    /// rather than by rotations, and gives an SVD, tall, square or wide,
    /// real or complex, with one value per column of the smaller side.
    #[test]
    fn decomposes_full_rank_matrices_far_from_orthogonal() {
        for (rows, cols, seed) in [(32, 32, 1), (64, 64, 2), (96, 48, 3), (48, 96, 4)] {
            let a = matrix(rows, cols, seed);
            let values = check_svd(&a);
            assert_eq!(values.len(), rows.min(cols), "{rows} × {cols}");
            let real = Matrix::from_fn(rows, cols, |i, j| {
                Complex64::new(a.elements()[i + rows * j].re, 0.0)
            });
            let values = check_svd(&real);
            assert_eq!(values.len(), rows.min(cols), "{rows} × {cols}, real");
        }
    }

    /// When the fast route reports a failure, as faer 0.24.4's QR algorithm
    /// did on a degenerate pair one ulp apart, the rotations decompose the
    /// matrix instead: a matrix the route takes still gets its SVD.
    #[test]
    fn falls_back_to_the_rotations_when_the_fast_route_fails() {
        let a = matrix(64, 64, 2);
        let fails = |_: usize, _: usize, _: &[Complex64]| None;
        let svd = trimmed_thin_svd::<Shipped>(a.view(), &fails).expect("the rotations answer");
        assert_eq!(check_svd_of(&a, svd).len(), 64);
    }

    /// A correct SVD of `w` standing for the fast route's, so that these
    /// tests do not depend on the library's: the rotations'.
    fn correct(rows: usize, cols: usize, w: &[Complex64]) -> Option<ColumnMajorSvd> {
        let columns = w
            .chunks_exact(rows)
            .flat_map(|column| {
                column
                    .iter()
                    .map(|x| x.re)
                    .chain(column.iter().map(|x| x.im))
            })
            .collect();
        one_sided(rows, cols, columns, MAX_SWEEPS)
    }

    /// The fast route's factors of `w` with a NaN in the first entry of
    /// one factor (U, the singular values, or V, by `factor`), in its real
    /// or `imaginary` part.
    fn spoiled(
        rows: usize,
        cols: usize,
        w: &[Complex64],
        factor: usize,
        imaginary: bool,
    ) -> Option<ColumnMajorSvd> {
        let (mut u, mut values, mut v) = correct(rows, cols, w)?;
        let nan = if imaginary {
            Complex64::new(0.0, f64::NAN)
        } else {
            Complex64::new(f64::NAN, 0.0)
        };
        match factor {
            0 => u[0] += nan,
            1 => values[0] = f64::NAN,
            _ => v[0] += nan,
        }
        Some((u, values, v))
    }

    /// When the fast route returns factors that are not finite, which the
    /// checks would refuse, the rotations decompose the matrix instead: a
    /// NaN in U, the values or V, in a real or an imaginary part, still
    /// gives an SVD.
    #[test]
    fn falls_back_to_the_rotations_when_the_fast_route_is_not_finite() {
        let a = matrix(64, 64, 2);
        for (factor, imaginary) in [(0, false), (0, true), (1, false), (2, false), (2, true)] {
            let spoiled = |rows: usize, cols: usize, w: &[Complex64]| {
                spoiled(rows, cols, w, factor, imaginary)
            };
            let svd = trimmed_thin_svd::<Shipped>(a.view(), &spoiled)
                .unwrap_or_else(|| panic!("factor {factor}, imaginary {imaginary}"));
            assert_eq!(check_svd_of(&a, svd).len(), 64);
        }
    }

    /// The fast route's factors pass the same checks as the rotations':
    /// finite factors whose product is not the matrix, values 1% too
    /// large, fail the SVD.
    #[test]
    fn refuses_the_fast_routes_factors_that_do_not_give_back_the_matrix() {
        let a = matrix(64, 64, 2);
        let off = |rows: usize, cols: usize, w: &[Complex64]| {
            let (u, values, v) = correct(rows, cols, w)?;
            Some((u, values.iter().map(|s| s * 1.01).collect(), v))
        };
        let answer = trimmed_thin_svd::<Shipped>(a.view(), &off);
        let error = checked::<Shipped>(a.view(), answer).expect_err("values 1% off");
        assert_eq!(error, SvdError::FailedChecks { rows: 64, cols: 64 });
    }
}
