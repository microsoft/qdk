// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`CpuAlgebra`]: the reference algebra, the [`SiteAlgebra`] of the host
//! CPU.
//!
//! Each operation is computed directly from its contract; it passes the
//! conformance kit (`conformance`), and the engine's tests run on it. It
//! keeps each site tensor in a vector in the host's memory and computes
//! its two factorisations, the gate split and the move, with
//! `dense_linalg`'s product, thin QR and SVD (which checks its own
//! factors), on the calling thread. It needs no device, so it runs
//! wherever the QDK runs.
//!
//! A gate split builds the gated pair `θ` by a product and the gate's
//! loops, factors it by one SVD, and truncates it by the crate's rule
//! (`Truncation`): it drops the longest tail of values whose squares sum
//! to at most the cutoff's share, a tail of exactly that share included,
//! then keeps at most the cap. It multiplies the kept values, normalised,
//! into the centre's side. A move of the centre applies the one-site gate
//! to the leaving site, factors that site by one thin QR, and multiplies
//! the small factor into the other site.

use super::truncation::Truncation;
use super::{CentreSide, GateSplitCall, GateSplitOutcome, QrMoveCall, QrMoveOutcome, SiteAlgebra};
use crate::dense_linalg::{Matrix, MatrixRef, Qr, Svd, SvdError};
use num_complex::Complex64;
use std::borrow::Cow;

/// The reference algebra: the [`SiteAlgebra`] of the host CPU, each site
/// tensor a vector in the host's memory, factored by `dense_linalg`.
///
/// Only [`gate_split`](SiteAlgebra::gate_split) fails, when `dense_linalg`'s
/// SVD of the gated pair `θ` does ([`SvdError`]): its rotations do not
/// converge, `θ` holds a value that is not finite, or the factors fail the
/// checks the SVD makes of them. The other operations never fail.
#[derive(Clone, Copy, Debug, Default)]
pub struct CpuAlgebra;

impl SiteAlgebra for CpuAlgebra {
    type Tensor = Vec<Complex64>;
    type Error = SvdError;

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

    /// The gated pair, one SVD, the crate's truncation rule, and the kept
    /// values, normalised, multiplied into the centre's side.
    fn gate_split(
        &self,
        call: &GateSplitCall<'_, Self::Tensor>,
    ) -> Result<GateSplitOutcome<Self::Tensor>, Self::Error> {
        let rows = 2 * call.left_extent;
        let cols = 2 * call.right_extent;
        let svd = Svd::of(gated_pair(call).view())?;
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
