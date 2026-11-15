// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`FakeAlgebra`]: a [`SiteAlgebra`] on the host, for the crate's tests.
//!
//! The planned engine's rules will be tested in CI with no backend crate:
//! the engine will run on this algebra, whose tensors are vectors and
//! whose maths is `dense_linalg`'s (its products, its QR, and its SVD,
//! which checks its own factors). It truncates by the rule of
//! [`Truncation`]. It is test support, not a test: it sits beside the
//! trait it implements, and is compiled only with the tests, which live
//! under `src/tests` and check it against the trait's contract, since the
//! planned engine's tests will stand on its answers.

use super::truncation::Truncation;
use super::{CentreSide, GateSplitCall, GateSplitOutcome, QrMoveCall, QrMoveOutcome, SiteAlgebra};
use dense_linalg::{Matrix, MatrixRef, Qr, Svd};
use num_complex::Complex64;
use std::borrow::Cow;
use std::convert::Infallible;

/// The [`SiteAlgebra`] of the crate's tests: each tensor a vector on the
/// host. It never fails.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct FakeAlgebra;

impl SiteAlgebra for FakeAlgebra {
    type Tensor = Vec<Complex64>;
    type Error = Infallible;

    fn upload(&self, values: &[Complex64]) -> Result<Self::Tensor, Self::Error> {
        Ok(values.to_vec())
    }

    fn read<'t>(
        &self,
        tensor: &'t Self::Tensor,
        len: usize,
    ) -> Result<Cow<'t, [Complex64]>, Self::Error> {
        Ok(Cow::Borrowed(&tensor[..len]))
    }

    fn duplicate(&self, tensor: &Self::Tensor, len: usize) -> Result<Self::Tensor, Self::Error> {
        Ok(tensor[..len].to_vec())
    }

    /// The gated pair, one SVD, [`Truncation`]'s truncation, and the kept
    /// values, normalised, multiplied into the centre's side.
    fn gate_split(
        &self,
        call: &GateSplitCall<'_, Self::Tensor>,
    ) -> Result<GateSplitOutcome<Self::Tensor>, Self::Error> {
        let rows = 2 * call.left_extent;
        let cols = 2 * call.right_extent;
        let svd = thin_svd(gated_pair(call).view());
        let rank = svd.singular_values.len();
        let truncation = Truncation::of(&svd.singular_values, call.cutoff, call.max_extent);
        let values = &truncation.normalised_values;
        let bond = values.len();
        // The first `bond` columns of `u`, and rows of `v†`.
        let mut left = svd.u.into_elements();
        left.truncate(rows * bond);
        let v_adjoint = svd.v_adjoint.elements();
        let mut right: Vec<Complex64> = (0..cols)
            .flat_map(|j| (0..bond).map(move |i| v_adjoint[i + rank * j]))
            .collect();
        match call.centre {
            CentreSide::Left => {
                for (column, &value) in left.chunks_exact_mut(rows).zip(values) {
                    for x in column {
                        *x *= value;
                    }
                }
            }
            CentreSide::Right => {
                for column in right.chunks_exact_mut(bond) {
                    for (x, &value) in column.iter_mut().zip(values) {
                        *x *= value;
                    }
                }
            }
        }
        Ok(GateSplitOutcome {
            left,
            right,
            bond,
            retained_fraction: truncation.retained_fraction,
        })
    }

    /// The leaving site, gated, factored by `dense_linalg`'s thin QR, and
    /// the small factor multiplied into the other site.
    #[expect(
        clippy::many_single_char_names,
        reason = "the contract's extents l, m, r and sites A, B"
    )]
    fn qr_move(
        &self,
        call: &QrMoveCall<'_, Self::Tensor>,
    ) -> Result<QrMoveOutcome<Self::Tensor>, Self::Error> {
        let (l, m, r) = (call.left_extent, call.bond, call.right_extent);
        let mut a = call.left[..2 * l * m].to_vec();
        let mut b = call.right[..m * 2 * r].to_vec();
        Ok(match call.centre {
            CentreSide::Right => {
                if let Some(gate) = &call.gate {
                    apply_one(&mut a, l, m, gate);
                }
                // A = Q·R; A becomes Q, B becomes R·B.
                let Qr { q, r: factor } = Qr::of(MatrixRef::new(2 * l, m, &a));
                let merged = Matrix::product(factor.view(), MatrixRef::new(m, 2 * r, &b));
                QrMoveOutcome {
                    bond: q.cols(),
                    left: q.into_elements(),
                    right: merged.into_elements(),
                }
            }
            CentreSide::Left => {
                if let Some(gate) = &call.gate {
                    apply_one(&mut b, m, r, gate);
                }
                // B† = Q·R, so B = R†·Q†; B becomes Q†, A becomes A·R†.
                let b_adjoint = MatrixRef::new(m, 2 * r, &b).adjoint();
                let Qr { q, r: factor } = Qr::of(b_adjoint.view());
                let merged =
                    Matrix::product(MatrixRef::new(2 * l, m, &a), factor.view().adjoint().view());
                QrMoveOutcome {
                    bond: q.cols(),
                    left: merged.into_elements(),
                    right: q.view().adjoint().into_elements(),
                }
            }
        })
    }
}

/// The thin SVD of `a`, the gated pair a split truncates.
///
/// # Panics
///
/// When `dense_linalg`'s SVD fails: the fake has no error to report, and
/// the engine's tests give it finite matrices, which that SVD answers.
fn thin_svd(a: MatrixRef<'_>) -> Svd {
    Svd::of(a).expect("the engine's tests give the SVD finite matrices, which it answers")
}

/// The gated pair `θ` of `call`, as [`GateSplitCall`] defines it: the
/// product of the two sites over their bond, then the gate on the two
/// qubits, a `(2·left) × (2·right)` matrix.
fn gated_pair(call: &GateSplitCall<'_, Vec<Complex64>>) -> Matrix {
    let (l, m, r) = (call.left_extent, call.bond, call.right_extent);
    let rows = 2 * l;
    let pair = Matrix::product(
        MatrixRef::new(rows, m, &call.left[..rows * m]),
        MatrixRef::new(m, 2 * r, &call.right[..m * 2 * r]),
    );
    let pair = pair.elements();
    let mut theta = vec![Complex64::ZERO; rows * 2 * r];
    for b in 0..r {
        for (s, t, p, q) in qubit_indices() {
            let g = call.gate[(2 * s + t) * 4 + 2 * p + q];
            let from = rows * (q + 2 * b) + l * p;
            let to = rows * (t + 2 * b) + l * s;
            for a in 0..l {
                theta[to + a] += g * pair[from + a];
            }
        }
    }
    Matrix::new(rows, 2 * r, theta)
}

/// Every `(s, t, p, q)` in `{0, 1}⁴`.
fn qubit_indices() -> impl Iterator<Item = (usize, usize, usize, usize)> {
    (0..16).map(|i| (i >> 3 & 1, i >> 2 & 1, i >> 1 & 1, i & 1))
}

/// Applies the one-site `gate` (`G[s, p]` at `2s + p`) to the site of
/// extents `[left, 2, right]`.
fn apply_one(site: &mut [Complex64], left: usize, right: usize, gate: &[Complex64; 4]) {
    for b in 0..right {
        for a in 0..left {
            let at = |s: usize| a + left * (s + 2 * b);
            let (zero, one) = (site[at(0)], site[at(1)]);
            site[at(0)] = gate[0] * zero + gate[1] * one;
            site[at(1)] = gate[2] * zero + gate[3] * one;
        }
    }
}
