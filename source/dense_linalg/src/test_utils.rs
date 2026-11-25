// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The helpers the tests of several modules share: deterministic
//! matrices, naive products and adjoints written apart from the code they
//! check, and the SVD's checks and scenes.

use crate::{Matrix, MatrixRef, Svd};
use num_complex::Complex64;

/// The complex number `re + i·im`.
pub(crate) fn c(re: f64, im: f64) -> Complex64 {
    Complex64::new(re, im)
}

/// The message of the panic `f` makes; it fails when `f` returns.
pub(crate) fn panic_message(f: impl FnOnce() + std::panic::UnwindSafe) -> String {
    let payload = std::panic::catch_unwind(f).expect_err("a panic");
    match payload.downcast::<String>() {
        Ok(message) => *message,
        Err(payload) => (*payload
            .downcast::<&str>()
            .expect("a panic message is a string"))
        .to_owned(),
    }
}

/// A deterministic pseudo-random `rows × cols` matrix, drawn from
/// [`SplitMix64`] seeded by `seed`.
pub(crate) fn matrix(rows: usize, cols: usize, seed: u64) -> Matrix {
    let mut stream = SplitMix64::new(seed);
    let mut next = move || stream.next_uniform() - 0.5;
    Matrix::from_fn(rows, cols, |_, _| c(next(), next()))
}

/// The reference `SplitMix64` (Steele, Lea and Flood, 2014; Vigna's
/// `splitmix64.c`): the stream of 64-bit values, and of uniforms in
/// [0, 1), that a `u64` seed gives. Copied, so that the tests of a linear
/// algebra crate depend on no simulator crate; the paper fixes the
/// stream, so the copies cannot drift.
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    /// The start of the stream that `seed` gives; any `u64` is a seed.
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// The next 64-bit value of the stream, any `u64`.
    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// The next value's top 53 bits as a multiple of 2⁻⁵³: a uniform in
    /// [0, 1), whose every value an `f64` holds exactly.
    fn next_uniform(&mut self) -> f64 {
        #[expect(clippy::cast_precision_loss, reason = "53 bits fit an f64")]
        let uniform = (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64;
        uniform
    }
}

/// `a·b` by the textbook triple loop.
pub(crate) fn naive_product(a: MatrixRef<'_>, b: MatrixRef<'_>) -> Matrix {
    Matrix::from_fn(a.rows(), b.cols(), |i, j| {
        (0..a.cols())
            .map(|k| a.elements()[i + a.rows() * k] * b.elements()[k + b.rows() * j])
            .sum()
    })
}

/// The largest modulus of the entries of `a − b`; NaN when any is NaN.
pub(crate) fn distance(a: &Matrix, b: &Matrix) -> f64 {
    assert_eq!((a.rows(), a.cols()), (b.rows(), b.cols()));
    a.elements()
        .iter()
        .zip(b.elements())
        .map(|(x, y)| (x - y).norm())
        // Not `f64::max`, which drops a NaN: a NaN entry must fail the
        // caller's `< tolerance` check.
        .fold(0.0, |m, d| {
            if m.is_nan() || d.is_nan() {
                f64::NAN
            } else {
                m.max(d)
            }
        })
}

/// The conjugate transpose of `m`.
pub(crate) fn adjoint(m: &Matrix) -> Matrix {
    let e = m.elements();
    Matrix::from_fn(m.cols(), m.rows(), |i, j| e[j + m.rows() * i].conj())
}

/// The `n × n` identity.
pub(crate) fn identity(n: usize) -> Matrix {
    Matrix::from_fn(n, n, |i, j| c(if i == j { 1.0 } else { 0.0 }, 0.0))
}

/// Tall, wide, square and single-row shapes.
pub(crate) const SHAPES: [(usize, usize); 4] = [(6, 4), (4, 6), (5, 5), (1, 3)];

/// Checks that [`Svd::of`] gives a thin SVD of `a`, as [`check_svd_of`]
/// does, and returns its values.
pub(crate) fn check_svd(a: &Matrix) -> Vec<f64> {
    let svd = Svd::of(a.view()).expect("a finite, nonempty matrix has a checked SVD");
    check_svd_of(a, svd)
}

/// Checks that `svd` is a thin SVD of `a`, with `r` values,
/// `1 ≤ r ≤ min(m, n)`, and returns them.
pub(crate) fn check_svd_of(a: &Matrix, svd: Svd) -> Vec<f64> {
    let (rows, cols) = (a.rows(), a.cols());
    let Svd {
        u,
        singular_values,
        v_adjoint,
    } = svd;
    let r = singular_values.len();
    assert!((1..=rows.min(cols)).contains(&r), "{r} values");
    assert!(singular_values.windows(2).all(|w| w[0] >= w[1]));
    let us = Matrix::from_fn(rows, r, |i, j| {
        u.elements()[i + rows * j] * singular_values[j]
    });
    assert!(distance(&naive_product(us.view(), v_adjoint.view()), a) < 1e-13);
    assert!(distance(&naive_product(adjoint(&u).view(), u.view()), &identity(r)) < 1e-13);
    let rows_gram = naive_product(v_adjoint.view(), adjoint(&v_adjoint).view());
    assert!(distance(&rows_gram, &identity(r)) < 1e-13);
    singular_values
}

/// `m` times `2^exponent`, in two steps so that each factor is finite;
/// rounded where the product is subnormal, exact otherwise.
pub(crate) fn times_power_of_two(m: &Matrix, exponent: i32) -> Matrix {
    let factors = [
        2_f64.powi(exponent / 2),
        2_f64.powi(exponent - exponent / 2),
    ];
    let elements = m.elements().iter();
    let scaled = elements.map(|x| x * factors[0] * factors[1]).collect();
    Matrix::new(m.rows(), m.cols(), scaled)
}

/// A random matrix of each of [`SHAPES`] times `2^exponent`.
fn random_of_each_shape(exponent: i32) -> Vec<Matrix> {
    (1..)
        .zip(&SHAPES)
        .map(|(seed, &(rows, cols))| times_power_of_two(&matrix(rows, cols, seed), exponent))
        .collect()
}

/// The zero matrix of each of [`SHAPES`].
fn zero_of_each_shape() -> Vec<Matrix> {
    SHAPES
        .iter()
        .map(|&(rows, cols)| Matrix::new(rows, cols, vec![c(0.0, 0.0); rows * cols]))
        .collect()
}

/// The scenes, each named, that a QR must decompose to cover the whole
/// `f64` range, as it scales by the largest entry. Each but the last holds
/// a matrix of each of [`SHAPES`]:
///
/// - random entries;
/// - the zero matrix, which has no largest entry to scale by, and
///   decomposes with `R = 0`;
/// - random entries with one column `2⁻⁵³³` times the others, which needs
///   no scaling, but whose reflection's `|v₀|²` is subnormal. A reflector
///   whose `τ` was the textbook `2/(1 + ‖x‖²/|v₀|²)`, from the unscaled
///   tail `x`, gave back neither the matrix nor an orthonormal `Q`;
/// - entries all subnormal, about `2⁻¹⁰²⁶`;
/// - entries about `2¹⁰¹⁹`, whose squares overflow;
/// - `[t + it, 1]ᵀ`, `t` the smallest subnormal `f64`, which needs no
///   scaling. A reflector that took the phase of `t + it` to be `1 + i`
///   gave back `4/3` in place of the `1`.
pub(crate) fn scenes_at_each_scale() -> [(&'static str, Vec<Matrix>); 6] {
    let t = f64::from_bits(1);
    let beside_one = vec![Matrix::new(2, 1, vec![c(t, t), c(1.0, 0.0)])];
    [
        ("random entries", random_of_each_shape(0)),
        ("the zero matrix", zero_of_each_shape()),
        ("a column 2⁻⁵³³ times the others", with_a_column_apart()),
        ("entries about 2⁻¹⁰²⁶", random_of_each_shape(-1025)),
        ("entries about 2¹⁰¹⁹", random_of_each_shape(1020)),
        ("[t + it, 1]ᵀ", beside_one),
    ]
}

/// A random matrix of each of [`SHAPES`] with its column 1 `2⁻⁵³³` times
/// the others.
fn with_a_column_apart() -> Vec<Matrix> {
    let small = 2_f64.powi(-533);
    (1..)
        .zip(&SHAPES)
        .map(|(seed, &(rows, cols))| {
            let a = matrix(rows, cols, seed);
            Matrix::from_fn(rows, cols, |i, j| {
                let x = a.elements()[i + rows * j];
                if j == 1 { x * small } else { x }
            })
        })
        .collect()
}
