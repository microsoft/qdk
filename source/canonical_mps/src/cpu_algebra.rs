// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`CpuAlgebra`]: the [`SiteAlgebra`] of the host CPU, the backend that
//! runs the engine on any machine, without a GPU. CI has none, so the
//! engine's tests run on it.
//!
//! Each operation is computed directly from its contract. It passes the
//! conformance kit (`conformance`).
//!
//! - It keeps each site tensor in a vector in the host's memory.
//! - It computes its two factorisations, the gate split and the move, with
//!   `dense_linalg`'s product, thin QR and LQ, and SVD (which checks its
//!   own factors) and its truncation.
//!
//! Its parts:
//!
//! ```text
//! cpu_algebra.rs       CpuAlgebra: each SiteAlgebra operation
//!   site_matrices.rs   SiteMatrices: a site read as either of the layout's
//!                      matrices, and a one-qubit gate applied to it
//!   gate_split.rs      the gate split's steps, and the diagram of its
//!                      shapes
//!     truncation.rs    Truncation: which singular values a split keeps
//!   qr_move.rs         the move of the centre in either direction, with
//!                      its diagram
//! ```
//!
//! The Terms of `dense_linalg`'s `docs/algorithms.md` define the
//! linear-algebra words (SVD, QR, LQ, adjoint); the crate README's Concepts
//! define the gated pair, the split, the move, truncation and an isometry.

mod gate_split;
mod qr_move;
mod site_matrices;

use crate::site_algebra::{
    CentreSide, GateSplitCall, GateSplitOutcome, QrMoveCall, QrMoveOutcome, SiteAlgebra,
};
use dense_linalg::{Svd, SvdError};
use gate_split::truncation::Truncation;
use gate_split::{SplitSites, gated_pair};
use num_complex::Complex64;
use qr_move::{move_centre_left, move_centre_right};
use site_matrices::SiteMatrices;
use std::borrow::Cow;

/// The [`SiteAlgebra`] of the host CPU. Each site tensor is a vector in
/// the host's memory, factored by `dense_linalg`.
///
/// It needs no device, so it runs wherever the QDK runs. CI has no GPU, so
/// CI runs the engine's tests, the conformance kit and the circuits on it.
///
/// Only [`gate_split`](SiteAlgebra::gate_split) fails. It fails when
/// `dense_linalg`'s SVD of the gated pair `θ` does ([`SvdError`]): its
/// rotations do not converge, `θ` holds a value that is not finite, or the
/// factors fail the checks the SVD makes of them. The other operations
/// never fail. A read or a duplicate past a tensor's end panics, naming
/// both lengths. That is the refusal [`SiteAlgebra::read`] asks for.
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
        Ok(Cow::Borrowed(site_elements(tensor, len)))
    }

    fn duplicate(&self, tensor: &Self::Tensor, len: usize) -> Result<Self::Tensor, Self::Error> {
        Ok(site_elements(tensor, len).to_vec())
    }

    /// Factors the gated pair (the two sites merged and the gate applied)
    /// by one SVD, keeps its largest values by the truncation rule
    /// (`Truncation`), and absorbs them, normalised, into the centre's
    /// site. This algebra's `gate_split` module
    /// (`cpu_algebra/gate_split.rs`) draws each step's shapes.
    ///
    /// The SVD checks its factors against the gated pair before the
    /// truncation. When they do not give it back, the split fails with
    /// [`SvdError`] rather than spread wrong factors through the chain.
    fn gate_split(
        &self,
        call: &GateSplitCall<'_, Self::Tensor>,
    ) -> Result<GateSplitOutcome<Self::Tensor>, Self::Error> {
        let left = SiteMatrices::new(call.left, call.left_extent, call.bond);
        let right = SiteMatrices::new(call.right, call.bond, call.right_extent);
        let gated_pair = gated_pair(&left, &right, &call.gate);
        let svd = Svd::of(gated_pair.view())?;
        let truncation = Truncation::of(&svd.singular_values, call.cutoff, call.max_extent);
        let mut new_sites = SplitSites::keeping(svd, truncation.bond());
        new_sites.absorb_values_into_centre(&truncation.normalised_values, call.centre);
        Ok(new_sites.into_outcome(truncation.retained_fraction))
    }

    /// Factors the site the centre leaves, gated, by one QR (an LQ, its
    /// mirror image, when the centre moves left): the site keeps the
    /// isometry, and the small remainder (the other factor, which holds the
    /// site's norm) is multiplied into the other site, which becomes the
    /// centre. This algebra's `qr_move` module
    /// (`cpu_algebra/qr_move.rs`) draws both directions.
    ///
    /// The move is exact, dropping nothing, and never fails.
    fn qr_move(
        &self,
        call: &QrMoveCall<'_, Self::Tensor>,
    ) -> Result<QrMoveOutcome<Self::Tensor>, Self::Error> {
        let left = SiteMatrices::new(call.left, call.left_extent, call.bond);
        let right = SiteMatrices::new(call.right, call.bond, call.right_extent);
        let gate = call.gate.as_ref();
        Ok(match call.centre {
            CentreSide::Right => move_centre_right(&left.with_gate(gate), &right),
            CentreSide::Left => move_centre_left(&left, &right.with_gate(gate)),
        })
    }
}

/// The first `len` elements of `tensor`, which `read` and `duplicate` take.
/// A length past the tensor's end is the caller's bug (a bond misread). So
/// it panics with a message naming both lengths, rather than Rust's text
/// of an out-of-range slice.
fn site_elements(tensor: &[Complex64], len: usize) -> &[Complex64] {
    tensor.get(..len).unwrap_or_else(|| {
        panic!(
            "read of {len} values from a site tensor that holds {}: the caller's bond \
             dimensions do not match the tensor",
            tensor.len()
        )
    })
}

#[cfg(test)]
mod tests;
