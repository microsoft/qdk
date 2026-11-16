// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Rules of `SiteAlgebra`'s gate split and move of the centre, each a
//! function of any algebra that asserts it on the shapes it is given,
//! against the reference algebra, `CpuAlgebra`, where the contract fixes
//! the answer; and the calls and host-side matrices they use.
//!
//! The CUDA algebra is checked on these twice, with the same code: on the
//! fake libraries, in the crate's unit tests (`gate_split.rs`,
//! `qr_move.rs`), and on a GPU, where `tests/cuda_algebra_on_the_gpu.rs`
//! includes this file. So it names nothing of the crate: only
//! `canonical_mps`, `dense_linalg` and `num_complex`.
//!
//! Products and shares are compared with the reference, never the factors
//! (an SVD's are unique only up to phases) nor the bond under a cutoff of
//! 0 (the libraries may keep values the reference drops as rounding).

use canonical_mps::CpuAlgebra;
use canonical_mps::algebra::{
    CentreSide, GateSplitCall, GateSplitOutcome, QrMoveCall, QrMoveOutcome, SiteAlgebra,
};
use dense_linalg::{Matrix, Qr};
use num_complex::Complex64;

/// The singular values of [`known_pair`]'s product.
const KNOWN_VALUES: [f64; 4] = [0.8, 0.5, 0.3, 0.1];

/// Random pairs of each of `shapes` (`l`, `m`, `r`) under a random gate,
/// each split without truncation, capped at 2 and under a cutoff, on both
/// sides: the product of the new sites is the reference algebra's, the
/// truncated SVD of the gated pair, normalised, with the same share kept,
/// within `tolerance`. A gate or a site read in another layout, or the
/// values absorbed on the wrong side, would change it.
pub fn splits_give_the_reference_algebras_truncated_svd<A: SiteAlgebra>(
    algebra: &A,
    shapes: &[(usize, usize, usize)],
    tolerance: f64,
) {
    for &(l, m, r) in shapes {
        let (a, b) = random_pair(l, m, r, 10);
        for (cutoff, max_extent) in [(0.0, usize::MAX), (0.0, 2), (0.05, usize::MAX)] {
            for centre in [CentreSide::Left, CentreSide::Right] {
                let split = Split {
                    a: &a,
                    b: &b,
                    l,
                    m,
                    r,
                    gate: random_gate(11),
                    cutoff,
                    max_extent,
                    centre,
                };
                let case =
                    format!("[{l}, {m}, {r}], cutoff {cutoff}, cap {max_extent}, {centre:?}");
                let on_algebra = split.run(algebra, &case);
                let on_cpu = split.run(&CpuAlgebra, &case);
                let error = distance(
                    &product(&on_algebra.left, &on_algebra.right),
                    &product(&on_cpu.left, &on_cpu.right),
                );
                assert!(error < tolerance, "{case}: {error:e}");
                let share = (on_algebra.retained_fraction - on_cpu.retained_fraction).abs();
                assert!(share < tolerance, "{case}: {share:e}");
            }
        }
    }
}

/// A random pair of each of `shapes` split with and without a cap of 2, on
/// both sides: the site away from the centre is an isometry, the left
/// site's columns or the right site's rows orthonormal within `tolerance`.
pub fn a_split_leaves_an_isometry_away_from_the_centre<A: SiteAlgebra>(
    algebra: &A,
    shapes: &[(usize, usize, usize)],
    tolerance: f64,
) {
    for &(l, m, r) in shapes {
        let (a, b) = random_pair(l, m, r, 12);
        for max_extent in [2, usize::MAX] {
            for centre in [CentreSide::Left, CentreSide::Right] {
                let split = Split {
                    a: &a,
                    b: &b,
                    l,
                    m,
                    r,
                    gate: random_gate(13),
                    cutoff: 0.0,
                    max_extent,
                    centre,
                };
                let case = format!("[{l}, {m}, {r}], cap {max_extent}, {centre:?}");
                let on_algebra = split.run(algebra, &case);
                let error = match centre {
                    CentreSide::Right => isometry_error(&on_algebra.left, true),
                    CentreSide::Left => isometry_error(&on_algebra.right, false),
                };
                assert!(error < tolerance, "{case}: {error:e}");
            }
        }
    }
}

/// Random pairs of each of `shapes`, at caps of 1, 2, 3 and none and
/// cutoffs from 0 to just below 1: the bond kept is at least 1, and at most
/// the cap and either side of the gated pair.
pub fn the_bond_of_a_split_lies_between_one_and_its_bounds<A: SiteAlgebra>(
    algebra: &A,
    shapes: &[(usize, usize, usize)],
) {
    for &(l, m, r) in shapes {
        let (a, b) = random_pair(l, m, r, 14);
        for max_extent in [1, 2, 3, usize::MAX] {
            for cutoff in [0.0, 0.5, 1.0 - f64::EPSILON] {
                let split = Split {
                    a: &a,
                    b: &b,
                    l,
                    m,
                    r,
                    gate: random_gate(15),
                    cutoff,
                    max_extent,
                    centre: CentreSide::Left,
                };
                let case = format!("[{l}, {m}, {r}], cap {max_extent}, cutoff {cutoff}");
                let bond = split.run(algebra, &case).bond;
                let largest = max_extent.min(2 * l).min(2 * r);
                assert!((1..=largest).contains(&bond), "{case}: {bond}");
            }
        }
    }
}

/// A pair whose product has the singular values 0.8, 0.5, 0.3 and 0.1,
/// split under a cutoff, a cap, and neither: the bond and the share of the
/// squared values kept are the truncation's, the share exactly 1 when
/// nothing is dropped.
pub fn a_split_returns_the_bond_and_share_it_kept<A: SiteAlgebra>(algebra: &A) {
    let (a, b) = known_pair();
    let total: f64 = KNOWN_VALUES.iter().map(|s| s * s).sum();
    for (cutoff, max_extent, bond, share) in [
        (0.1, usize::MAX, 3, 0.98 / total),
        (0.0, 2, 2, 0.89 / total),
        (0.0, usize::MAX, 4, 1.0),
    ] {
        let split = Split {
            a: &a,
            b: &b,
            l: 2,
            m: 4,
            r: 2,
            gate: identity_gate(),
            cutoff,
            max_extent,
            centre: CentreSide::Right,
        };
        let case = format!("cutoff {cutoff}, cap {max_extent}");
        let on_algebra = split.run(algebra, &case);
        assert_eq!(on_algebra.bond, bond, "{case}");
        let kept = on_algebra.retained_fraction;
        if bond == KNOWN_VALUES.len() {
            assert_eq!(kept.to_bits(), share.to_bits(), "{case}: {kept}");
        } else {
            assert!((kept - share).abs() < 1e-14, "{case}: {kept}");
        }
    }
}

/// Random pairs of each of `shapes` (the bond below, at and above either
/// side of the site the centre leaves, for some), moved each way with and
/// without a one-site gate: the new bond and the product of the new sites
/// are the reference algebra's, the gated pair, within `tolerance`. A gate
/// or a factor read in another layout, or a bond that keeps too few
/// columns, would change it.
pub fn moves_give_the_reference_algebras_product_and_bond<A: SiteAlgebra>(
    algebra: &A,
    shapes: &[(usize, usize, usize)],
    tolerance: f64,
) {
    for &(l, m, r) in shapes {
        let (a, b) = random_pair(l, m, r, 20);
        for centre in [CentreSide::Left, CentreSide::Right] {
            for gate in [None, Some(one_qubit_gate(21))] {
                let moved = Move {
                    a: &a,
                    b: &b,
                    l,
                    m,
                    r,
                    gate,
                    centre,
                };
                let case = format!("[{l}, {m}, {r}], {centre:?}, gate {}", gate.is_some());
                let on_algebra = moved.run(algebra, &case);
                let on_cpu = moved.run(&CpuAlgebra, &case);
                assert_eq!(on_algebra.bond, on_cpu.bond, "{case}");
                let error = distance(
                    &product(&on_algebra.left, &on_algebra.right),
                    &product(&on_cpu.left, &on_cpu.right),
                );
                assert!(error < tolerance, "{case}: {error:e}");
            }
        }
    }
}

/// A random pair of each of `shapes` moved each way, with and without a
/// gate: the site the centre leaves is an isometry, the left site's columns
/// or the right site's rows orthonormal within `tolerance`.
pub fn a_move_leaves_an_isometry_behind<A: SiteAlgebra>(
    algebra: &A,
    shapes: &[(usize, usize, usize)],
    tolerance: f64,
) {
    for &(l, m, r) in shapes {
        let (a, b) = random_pair(l, m, r, 22);
        for centre in [CentreSide::Left, CentreSide::Right] {
            for gate in [None, Some(one_qubit_gate(23))] {
                let moved = Move {
                    a: &a,
                    b: &b,
                    l,
                    m,
                    r,
                    gate,
                    centre,
                };
                let case = format!("[{l}, {m}, {r}], {centre:?}, gate {}", gate.is_some());
                let on_algebra = moved.run(algebra, &case);
                let error = match centre {
                    CentreSide::Right => isometry_error(&on_algebra.left, true),
                    CentreSide::Left => isometry_error(&on_algebra.right, false),
                };
                assert!(error < tolerance, "{case}: {error:e}");
            }
        }
    }
}

/// A gate split of `a` and `b`, of extents `[l, 2, m]` and `[m, 2, r]`,
/// with `gate`, `cutoff`, `max_extent` and `centre`, run on any algebra.
pub struct Split<'v> {
    pub a: &'v [Complex64],
    pub b: &'v [Complex64],
    pub l: usize,
    pub m: usize,
    pub r: usize,
    pub gate: [Complex64; 16],
    pub cutoff: f64,
    pub max_extent: usize,
    pub centre: CentreSide,
}

/// A move of the centre across the bond of `a` and `b`, of extents
/// `[l, 2, m]` and `[m, 2, r]`, run on any algebra.
pub struct Move<'v> {
    pub a: &'v [Complex64],
    pub b: &'v [Complex64],
    pub l: usize,
    pub m: usize,
    pub r: usize,
    pub gate: Option<[Complex64; 4]>,
    pub centre: CentreSide,
}

/// A split's or a move's outcome on the host: the new sites as matrices,
/// the left `(2l) × k` and the right `k × (2r)`, the bond `k`, and the
/// retained fraction (1 for a move).
pub struct OnHost {
    pub left: Matrix,
    pub right: Matrix,
    pub bond: usize,
    pub retained_fraction: f64,
}

impl Split<'_> {
    /// The split on `algebra`, its pair uploaded and its sites read back.
    pub fn on<A: SiteAlgebra>(&self, algebra: &A) -> Result<OnHost, A::Error> {
        let (a, b) = (algebra.upload(self.a)?, algebra.upload(self.b)?);
        let outcome: GateSplitOutcome<A::Tensor> = algebra.gate_split(&self.call(&a, &b))?;
        on_host(
            algebra,
            self.l,
            self.r,
            &outcome.left,
            &outcome.right,
            outcome.bond,
        )
        .map(|sites| OnHost {
            retained_fraction: outcome.retained_fraction,
            ..sites
        })
    }

    /// The call on the tensors `a` and `b`.
    pub fn call<'t, T>(&self, a: &'t T, b: &'t T) -> GateSplitCall<'t, T> {
        GateSplitCall {
            left: a,
            right: b,
            left_extent: self.l,
            bond: self.m,
            right_extent: self.r,
            gate: self.gate,
            max_extent: self.max_extent,
            cutoff: self.cutoff,
            centre: self.centre,
        }
    }

    fn run<A: SiteAlgebra>(&self, algebra: &A, case: &str) -> OnHost {
        self.on(algebra)
            .unwrap_or_else(|error| panic!("{case}: the split fails: {error}"))
    }
}

impl Move<'_> {
    /// The move on `algebra`, its pair uploaded and its sites read back.
    pub fn on<A: SiteAlgebra>(&self, algebra: &A) -> Result<OnHost, A::Error> {
        let (a, b) = (algebra.upload(self.a)?, algebra.upload(self.b)?);
        let outcome: QrMoveOutcome<A::Tensor> = algebra.qr_move(&self.call(&a, &b))?;
        on_host(
            algebra,
            self.l,
            self.r,
            &outcome.left,
            &outcome.right,
            outcome.bond,
        )
    }

    /// The call on the tensors `a` and `b`.
    pub fn call<'t, T>(&self, a: &'t T, b: &'t T) -> QrMoveCall<'t, T> {
        QrMoveCall {
            left: a,
            right: b,
            left_extent: self.l,
            bond: self.m,
            right_extent: self.r,
            gate: self.gate,
            centre: self.centre,
        }
    }

    fn run<A: SiteAlgebra>(&self, algebra: &A, case: &str) -> OnHost {
        self.on(algebra)
            .unwrap_or_else(|error| panic!("{case}: the move fails: {error}"))
    }
}

/// The sites `left` (`[l, 2, k]`) and `right` (`[k, 2, r]`) read back as
/// matrices.
fn on_host<A: SiteAlgebra>(
    algebra: &A,
    l: usize,
    r: usize,
    left: &A::Tensor,
    right: &A::Tensor,
    k: usize,
) -> Result<OnHost, A::Error> {
    let left = algebra.read(left, 2 * l * k)?.into_owned();
    let right = algebra.read(right, 2 * k * r)?.into_owned();
    Ok(OnHost {
        left: Matrix::new(2 * l, k, left),
        right: Matrix::new(k, 2 * r, right),
        bond: k,
        retained_fraction: 1.0,
    })
}

/// `len` complex numbers whose parts lie in `[-0.5, 0.5)`, from a linear
/// congruential generator seeded by `seed`.
pub fn random(len: usize, seed: u64) -> Vec<Complex64> {
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
    (0..len)
        .map(|_| {
            let re = next();
            Complex64::new(re, next())
        })
        .collect()
}

/// A pair `A`, `B` of extents `[l, 2, m]` and `[m, 2, r]`, random from
/// `seed`, as host vectors.
pub fn random_pair(l: usize, m: usize, r: usize, seed: u64) -> (Vec<Complex64>, Vec<Complex64>) {
    (random(l * 2 * m, seed), random(m * 2 * r, seed + 1))
}

/// A random two-site gate: not unitary, nor symmetric under the exchange
/// of its qubits, nor equal to its transpose, so that a gate read in
/// another layout shows.
pub fn random_gate(seed: u64) -> [Complex64; 16] {
    random(16, seed).try_into().expect("16 entries")
}

/// A random one-site gate, row-major.
pub fn one_qubit_gate(seed: u64) -> [Complex64; 4] {
    random(4, seed).try_into().expect("4 entries")
}

/// The two-qubit identity, row-major.
pub fn identity_gate() -> [Complex64; 16] {
    std::array::from_fn(|i| Complex64::new(if i % 5 == 0 { 1.0 } else { 0.0 }, 0.0))
}

/// The product `a·b`.
pub fn product(a: &Matrix, b: &Matrix) -> Matrix {
    Matrix::product(a.view(), b.view())
}

/// How far `site`, read as a matrix, is from having orthonormal columns
/// (`columns`) or rows.
pub fn isometry_error(site: &Matrix, columns: bool) -> f64 {
    let adjoint = site.view().adjoint();
    let identity = |n| {
        Matrix::from_fn(n, n, |i, j| {
            Complex64::new(f64::from(u8::from(i == j)), 0.0)
        })
    };
    if columns {
        distance(&product(&adjoint, site), &identity(site.cols()))
    } else {
        distance(&product(site, &adjoint), &identity(site.rows()))
    }
}

/// The largest difference between two matrices of one shape, element by
/// element; infinite when a difference is not finite.
pub fn distance(a: &Matrix, b: &Matrix) -> f64 {
    assert_eq!((a.rows(), a.cols()), (b.rows(), b.cols()), "one shape");
    a.elements()
        .iter()
        .zip(b.elements())
        .map(|(x, y)| (x - y).norm())
        .map(|d| if d.is_finite() { d } else { f64::INFINITY })
        .fold(0.0, f64::max)
}

/// A pair `[2, 2, 4]`, `[4, 2, 2]` whose product, under the identity gate,
/// is `u·diag(KNOWN_VALUES)·v†` with `u` and `v` random unitaries:
/// `A = u·diag(KNOWN_VALUES)` and `B = v†`.
fn known_pair() -> (Vec<Complex64>, Vec<Complex64>) {
    let unitary = |seed| Qr::of(Matrix::new(4, 4, random(16, seed)).view()).q;
    let (u, v) = (unitary(18), unitary(19));
    let a = Matrix::from_fn(4, 4, |i, j| u.elements()[i + 4 * j] * KNOWN_VALUES[j]);
    (a.into_elements(), v.view().adjoint().into_elements())
}
