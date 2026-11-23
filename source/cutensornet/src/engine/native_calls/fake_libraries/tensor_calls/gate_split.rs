// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The fake's gate split: the SVD configuration it splits under
//! ([`SvdConfig`]), the labels it takes, and the truncated SVD it
//! factors θ by ([`split`]).
//!
//! A gate split's report and outputs follow cuTensorNet's, for the settings
//! `CudaAlgebra` makes, and only them: a discarded-weight cutoff, the kept
//! values normalized (L2), and absorbed into U or V; a split under other
//! settings is refused. The reduced extent is the lowest of the shared
//! mode's extent in U's descriptor, the count of singular values (the SVD
//! leaves out those of rounding's size) and the cutoff's rank, and U's and
//! V's descriptors are given it. The full extent is θ's smaller side,
//! whatever the SVD left out. The fake squares the singular values unscaled
//! for the weights and the normalization, so it models splits whose
//! singular values' squares neither overflow nor underflow an f64, as a
//! normalized state's do (a zero product is outside them). It writes no
//! singular value to their buffer. A test may replace the whole report, not
//! the outputs (`FakeLibraries::misreport_truncation`).

use super::super::{FakeRule, dense_tensor::DenseTensor, failed, refused};
use super::sides::Sides;
use crate::engine::CudaAlgebraError;
use canonical_mps::dense_linalg::{MatrixRef, Svd};
use num_complex::Complex64;

/// An SVD configuration's settings the fake acts on.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SvdConfig {
    /// Which factor absorbs the singular values; none returns them apart.
    pub(super) partition: Option<Partition>,
    pub(super) normalized: bool,
    /// The discarded-weight cutoff, if set: any value, 0 included, asks
    /// for a truncation by value.
    pub(super) cutoff: Option<f64>,
}

impl SvdConfig {
    /// The partition and the cutoff of settings `CudaAlgebra` makes, the
    /// only ones the fake splits under; other settings are refused.
    pub(super) fn the_algebras(
        self,
        operation: &'static str,
    ) -> Result<(Partition, f64), CudaAlgebraError> {
        let (Some(partition), Some(cutoff), true) = (self.partition, self.cutoff, self.normalized)
        else {
            return Err(failed(
                operation,
                "the fake splits only under a cutoff, normalized, and absorbed into U or V",
            ));
        };
        Ok((partition, cutoff))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Partition {
    Us,
    Sv,
}

impl Sides {
    /// Fails unless a gate split's `inputs` (A, B and the gate, in that
    /// order) form the graph cuTensorNet's gate split expects: each pair of
    /// them shares a label (a fully connected graph); every label is either
    /// shared by two or all three of them and in neither output, or in one
    /// of them and one output's other modes (no label is summed alone, and
    /// none is both contracted and kept); and a label of A alone is kept in
    /// U (the left output), one of B alone in V, one of the gate alone in
    /// either.
    pub(super) fn check_gate_split_labels(
        &self,
        inputs: &[&DenseTensor; 3],
        operation: &'static str,
    ) -> Result<(), CudaAlgebraError> {
        let connected = |[first, second]: [usize; 2]| {
            inputs[first]
                .modes
                .iter()
                .any(|mode| inputs[second].modes.contains(mode))
        };
        if ![[0, 1], [0, 2], [1, 2]].into_iter().all(connected) {
            return Err(refused(operation, FakeRule::GateSplitNotPairwiseConnected));
        }
        let on_left = |mode: &i32| self.left.0.contains(mode);
        let on_right = |mode: &i32| self.right.0.contains(mode);
        for (at, input) in inputs.iter().enumerate() {
            for mode in &input.modes {
                let count = inputs
                    .iter()
                    .filter(|other| other.modes.contains(mode))
                    .count();
                let fits = match (count, at) {
                    (1, 0) => on_left(mode),
                    (1, 1) => on_right(mode),
                    (1, _) => on_left(mode) || on_right(mode),
                    (2 | 3, _) => !on_left(mode) && !on_right(mode) && *mode != self.shared,
                    _ => false,
                };
                if !fits {
                    return Err(refused(operation, FakeRule::LabelOutsideGateSplitTopology));
                }
            }
        }
        Ok(())
    }
}

/// A gate split's factors: U (rows×reduced) and V (reduced×cols), both
/// column-major, one of them holding the kept singular values; and the
/// weight the truncation dropped.
pub(super) struct Split {
    pub(super) left: Vec<Complex64>,
    pub(super) right: Vec<Complex64>,
    pub(super) reduced: usize,
    pub(super) discarded_weight: f64,
}

/// Factors the `rows`×`cols` column-major `theta` into at most
/// `max_extent` singular values: drops the smallest whose squared sum is at
/// most `cutoff`'s fraction of the total, normalizes the kept ones, and
/// absorbs them into U or V, as `partition` says.
pub(super) fn split(
    theta: &[Complex64],
    rows: usize,
    cols: usize,
    max_extent: usize,
    cutoff: f64,
    partition: Partition,
) -> Result<Split, &'static str> {
    let Svd {
        u,
        singular_values: values,
        v_adjoint,
    } = Svd::of(MatrixRef::new(rows, cols, theta)).map_err(|_| "the SVD failed")?;
    let (left, right) = (u.elements(), v_adjoint.elements());
    let squares: Vec<f64> = values.iter().map(|value| value * value).collect();
    let total: f64 = squares.iter().sum();
    let rank = values.len();
    let mut discarded = 0.0;
    let dropped = squares
        .iter()
        .rev()
        .take_while(|&&square| {
            discarded += square;
            discarded <= cutoff * total
        })
        .count();
    let reduced = rank.min(max_extent).min((rank - dropped).max(1));
    // A fold from +0.0: the sum of no squares would be -0.0.
    let discarded_weight = squares[reduced..]
        .iter()
        .fold(0.0, |sum, square| sum + square)
        / total;
    let mut kept = values[..reduced].to_vec();
    let norm = kept.iter().map(|value| value * value).sum::<f64>().sqrt();
    for value in &mut kept {
        *value /= norm;
    }
    let mut u_matrix = left[..rows * reduced].to_vec();
    let mut v_matrix: Vec<Complex64> = (0..cols)
        .flat_map(|col| (0..reduced).map(move |row| (row, col)))
        .map(|(row, col)| right[row + col * rank])
        .collect();
    match partition {
        Partition::Us => {
            for (at, element) in u_matrix.iter_mut().enumerate() {
                *element *= kept[at / rows];
            }
        }
        Partition::Sv => {
            for (at, element) in v_matrix.iter_mut().enumerate() {
                *element *= kept[at % reduced];
            }
        }
    }
    Ok(Split {
        left: u_matrix,
        right: v_matrix,
        reduced,
        discarded_weight,
    })
}
