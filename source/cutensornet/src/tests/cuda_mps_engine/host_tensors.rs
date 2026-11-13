// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Development only (not for review): [`HostTensors`], the CUDA MPS
//! engine's tensor calls on the host, so that its rules run in the unit
//! tests without a GPU. Its `GateSplit` is what cuTensorNet's documents:
//! contract, apply the gate, one SVD (t4a-faer's), truncate by the cutoff
//! then the largest extent, normalise the kept values to an L2 norm of 1,
//! absorb them on one side. Its QR move is the CPU engine's: t4a-faer's
//! thin QR, then one product.

use crate::CudaMpsError;
use crate::cuda_mps_engine::{
    GateSplitCall, Partition, QrMoveCall, QrMoveOutcome, SplitOutcome, TensorCalls,
};
use num_complex::Complex64;
use t4a_faer::{
    Accum, Auto, MatMut, MatRef, Par, ParShape,
    diag::Diag,
    dyn_stack::{MemBuffer, MemStack},
    linalg::{
        matmul::matmul,
        svd::{ComputeSvdVectors, SvdParams, svd, svd_scratch},
    },
};

/// The tensor calls on the host: each buffer a vector.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct HostTensors;

impl TensorCalls for HostTensors {
    type Buffer = Vec<Complex64>;

    fn upload(&self, values: &[Complex64]) -> Result<Self::Buffer, CudaMpsError> {
        Ok(values.to_vec())
    }

    fn download(
        &self,
        buffer: &Self::Buffer,
        count: usize,
    ) -> Result<Vec<Complex64>, CudaMpsError> {
        Ok(buffer[..count].to_vec())
    }

    fn duplicate(&self, buffer: &Self::Buffer, count: usize) -> Result<Self::Buffer, CudaMpsError> {
        Ok(buffer[..count].to_vec())
    }

    fn gate_split(
        &self,
        call: &GateSplitCall<'_, Self::Buffer>,
    ) -> Result<SplitOutcome<Self::Buffer>, CudaMpsError> {
        let theta = gated_block(call);
        let (rows, cols) = (2 * call.left_extent, 2 * call.right_extent);
        let (u, values, v) = thin_svd(rows, cols, &theta)?;
        let full = values.len();
        let need = match call.cutoff {
            Some(cutoff) => kept(&values, cutoff),
            None => full,
        };
        let k = need.min(call.max_extent).max(1);
        let largest = values[0];
        let square = |s: f64| (s / largest) * (s / largest);
        let total: f64 = values.iter().map(|&s| square(s)).sum();
        let dropped: f64 = values[k..].iter().map(|&s| square(s)).sum();
        let kept_norm = values[..k].iter().map(|&s| s * s).sum::<f64>().sqrt();
        let normalised: Vec<f64> = values[..k].iter().map(|&s| s / kept_norm).collect();
        let mut left = u[..rows * k].to_vec();
        let mut right = vec![Complex64::ZERO; k * cols];
        for column in 0..cols {
            for row in 0..k {
                right[row + k * column] = v[column + cols * row].conj();
            }
        }
        match call.partition {
            Partition::Us => {
                for (column, &value) in left.chunks_exact_mut(rows).zip(&normalised) {
                    for x in column {
                        *x *= value;
                    }
                }
            }
            Partition::Sv => {
                for column in right.chunks_exact_mut(k) {
                    for (x, &value) in column.iter_mut().zip(&normalised) {
                        *x *= value;
                    }
                }
            }
        }
        Ok(SplitOutcome {
            u: left,
            v: right,
            reduced: k,
            full,
            discarded_weight: dropped / total,
            singular_values: if call.read_values {
                normalised
            } else {
                Vec::new()
            },
        })
    }

    /// As the CPU engine's moves: t4a-faer's thin QR, then one product.
    #[expect(
        clippy::many_single_char_names,
        reason = "the extents as the trait's docs name them: l, m, r, and the new bond k"
    )]
    fn qr_move(
        &self,
        call: &QrMoveCall<'_, Self::Buffer>,
    ) -> Result<QrMoveOutcome<Self::Buffer>, CudaMpsError> {
        let (l, m, r) = (call.left_extent, call.bond, call.right_extent);
        let mut left = call.left[..l * 2 * m].to_vec();
        let mut right = call.right[..m * 2 * r].to_vec();
        Ok(match call.partition {
            Partition::Sv => {
                if let Some(gate) = &call.gate {
                    apply_one(&mut left, l, m, gate);
                }
                let (q, factor, k) = thin_qr(2 * l, m, &left);
                let merged = product(&factor, &right, k, m, 2 * r);
                QrMoveOutcome {
                    left: q,
                    right: merged,
                    bond: k,
                }
            }
            Partition::Us => {
                if let Some(gate) = &call.gate {
                    apply_one(&mut right, m, r, gate);
                }
                let (q, factor, k) = thin_qr(2 * r, m, &adjoint(&right, m, 2 * r));
                let merged = product(&left, &adjoint(&factor, k, m), 2 * l, m, k);
                QrMoveOutcome {
                    left: merged,
                    right: adjoint(&q, 2 * r, k),
                    bond: k,
                }
            }
        })
    }
}

/// Applies the one-site `gate` (row-major) to the site `[left, 2, right]`.
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

/// The thin QR `W = Q·R` of the column-major `rows × cols` `w`: `Q`
/// (`rows × k`), `R` (`k × cols`) and `k = min(rows, cols)`.
fn thin_qr(rows: usize, cols: usize, w: &[Complex64]) -> (Vec<Complex64>, Vec<Complex64>, usize) {
    let qr = MatRef::from_column_major_slice(w, rows, cols).qr(Par::Seq);
    let q = qr.compute_thin_Q(Par::Seq);
    let r = qr.thin_R();
    let k = q.ncols();
    let q = (0..k)
        .flat_map(|j| (0..rows).map(move |i| (i, j)))
        .map(|(i, j)| q[(i, j)])
        .collect();
    let r = (0..cols)
        .flat_map(|j| (0..k).map(move |i| (i, j)))
        .map(|(i, j)| r[(i, j)])
        .collect();
    (q, r, k)
}

/// `A (rows × inner) · B (inner × cols)`, column-major.
fn product(
    a: &[Complex64],
    b: &[Complex64],
    rows: usize,
    inner: usize,
    cols: usize,
) -> Vec<Complex64> {
    let mut out = vec![Complex64::ZERO; rows * cols];
    matmul(
        MatMut::from_column_major_slice_mut(&mut out, rows, cols),
        Accum::Replace,
        MatRef::from_column_major_slice(a, rows, inner),
        MatRef::from_column_major_slice(b, inner, cols),
        Complex64::ONE,
        Par::Seq,
    );
    out
}

/// The adjoint of the column-major `rows × cols` `m`.
fn adjoint(m: &[Complex64], rows: usize, cols: usize) -> Vec<Complex64> {
    let mut out = vec![Complex64::ZERO; rows * cols];
    for j in 0..cols {
        for i in 0..rows {
            out[j + cols * i] = m[i + rows * j].conj();
        }
    }
    out
}

/// `θ[(a, s), (t, b)] = Σ G[(s, t), (p, q)] · A[a, p, j] · B[j, q, b]`,
/// column-major `2·left × 2·right`, the row `a + left·s` and the column
/// `t + 2·b`.
#[allow(
    clippy::many_single_char_names,
    reason = "the indices of the formula above"
)]
fn gated_block(call: &GateSplitCall<'_, Vec<Complex64>>) -> Vec<Complex64> {
    let (l, m, r) = (call.left_extent, call.bond, call.right_extent);
    let zero = Complex64::ZERO;
    let mut x = vec![zero; 2 * l * 2 * r];
    matmul(
        MatMut::from_column_major_slice_mut(&mut x, 2 * l, 2 * r),
        Accum::Replace,
        MatRef::from_column_major_slice(&call.left[..2 * l * m], 2 * l, m),
        MatRef::from_column_major_slice(&call.right[..m * 2 * r], m, 2 * r),
        Complex64::ONE,
        Par::Seq,
    );
    let rows = 2 * l;
    let mut theta = vec![zero; rows * 2 * r];
    for b in 0..r {
        for s in 0..2 {
            for t in 0..2 {
                for p in 0..2 {
                    for q in 0..2 {
                        let g = call.gate[(2 * s + t) * 4 + 2 * p + q];
                        if g == zero {
                            continue;
                        }
                        let from = rows * (q + 2 * b) + l * p;
                        let to = rows * (t + 2 * b) + l * s;
                        for a in 0..l {
                            theta[to + a] += g * x[from + a];
                        }
                    }
                }
            }
        }
    }
    theta
}

/// How many of `values` (nonincreasing) the cutoff keeps, as
/// `qdk_cpu_mps`'s `kept`: drop the tail whose squares sum to at most
/// `cutoff` of their total, never the first value.
fn kept(values: &[f64], cutoff: f64) -> usize {
    let largest = values[0];
    let square = |s: f64| (s / largest) * (s / largest);
    let total: f64 = values.iter().map(|&s| square(s)).sum();
    let allowed = cutoff * total;
    let mut keep = values.len();
    let mut discarded = 0.0;
    while keep > 1 {
        let next = discarded + square(values[keep - 1]);
        if next > allowed {
            break;
        }
        discarded = next;
        keep -= 1;
    }
    keep
}

/// `U`, the singular values and `V` of a thin SVD.
type ThinSvd = (Vec<Complex64>, Vec<f64>, Vec<Complex64>);

/// The thin SVD `W = U·diag(S)·V†` of the column-major `rows × cols` `w`:
/// `U` (`rows × k`), the values nonincreasing, `V` (`cols × k`), with
/// `k = min(rows, cols)`.
fn thin_svd(rows: usize, cols: usize, w: &[Complex64]) -> Result<ThinSvd, CudaMpsError> {
    let k = rows.min(cols);
    let params = <SvdParams as Auto<Complex64>>::auto();
    let mut values = Diag::<Complex64>::zeros(k);
    let mut u = vec![Complex64::ZERO; rows * k];
    let mut v = vec![Complex64::ZERO; cols * k];
    let mut buffer = MemBuffer::new(svd_scratch::<Complex64>(
        rows,
        cols,
        ComputeSvdVectors::Thin,
        ComputeSvdVectors::Thin,
        ParShape::Seq,
        params.into(),
    ));
    svd(
        MatRef::from_column_major_slice(w, rows, cols),
        values.as_mut(),
        Some(MatMut::from_column_major_slice_mut(&mut u, rows, k)),
        Some(MatMut::from_column_major_slice_mut(&mut v, cols, k)),
        Par::Seq,
        MemStack::new(&mut buffer),
        params.into(),
    )
    .map_err(|error| CudaMpsError::TensorCallFailed(format!("faer's SVD: {error:?}")))?;
    let values = values.column_vector();
    Ok((u, (0..k).map(|i| values[i].re).collect(), v))
}
