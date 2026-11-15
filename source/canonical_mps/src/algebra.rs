// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`SiteAlgebra`]: the backend contract of the canonical-form MPS engine,
//! the tensor operations it needs, wherever its site tensors live.
//!
//! The engine keeps a chain of site tensors in canonical form: one site,
//! the orthogonality centre, holds the state's norm, and every other site is
//! an isometry towards it. Its rules (where the centre moves, which pair a
//! gate splits, what weight a truncation keeps) are the same on every
//! backend; what differs is where the tensors live and which library
//! factors them. So the engine is written once over this trait, and a
//! backend implements only its operations: a gate split and a move of the
//! centre on two neighbouring sites, the storage of a site, and a product
//! and an SVD of small host matrices for the engine's reads.
//!
//! The engine checks every argument before it calls the backend, so an
//! implementation computes, and reports only its own failures, in its
//! [`SiteAlgebra::Error`].
//!
//! # The layout
//!
//! Every tensor holds complex doubles, its first mode fastest (Fortran
//! order). A site `T[a, s, b]`, of extents `[left, 2, right]` (its left
//! bond, its qubit, its right bond), has element `(a, s, b)` at
//! `a + left·(s + 2·b)`. Read as a matrix, the same elements are the
//! column-major `(2·left) × right` matrix of rows `a + left·s`, or the
//! `left × (2·right)` matrix of columns `s + 2·b`. The calls' outputs are
//! packed: a site of extents `[left, 2, right]` holds exactly
//! `left·2·right` elements.
//!
//! The host matrices of [`SiteAlgebra::multiply`] and [`SiteAlgebra::svd`]
//! are `dense_linalg`'s [`Matrix`] and [`MatrixRef`], column-major like
//! the sites; [`truncation`] holds the rule host backends use to truncate
//! a split.

pub mod truncation;

#[cfg(test)]
pub(crate) mod fake_algebra;

use dense_linalg::{Matrix, MatrixRef, Svd};
use num_complex::Complex64;
use std::borrow::Cow;

/// The tensor operations of the canonical-form MPS engine: the storage of
/// a site tensor, a gate split and a move of the orthogonality centre on
/// two neighbouring sites, and a product and an SVD of host matrices.
///
/// The engine has checked every argument (extents, lengths, the call's
/// settings) before it calls, so an implementation reports only what fails
/// in its own computation. Tensors follow the [module's layout](self).
pub trait SiteAlgebra {
    /// Where one site tensor's elements live.
    type Tensor;

    /// Why a call failed. `'static`, so that the engine's error can hold it
    /// as its source.
    type Error: std::error::Error + 'static;

    /// A new tensor holding `values`.
    fn upload(&self, values: &[Complex64]) -> Result<Self::Tensor, Self::Error>;

    /// The first `len` elements of `tensor`, borrowed where they already
    /// live on the host. `tensor` holds at least `len` elements.
    fn read<'t>(
        &self,
        tensor: &'t Self::Tensor,
        len: usize,
    ) -> Result<Cow<'t, [Complex64]>, Self::Error>;

    /// A new tensor holding the first `len` elements of `tensor`, which
    /// later calls on either leave the other unchanged. `tensor` holds at
    /// least `len` elements.
    fn duplicate(&self, tensor: &Self::Tensor, len: usize) -> Result<Self::Tensor, Self::Error>;

    /// Applies a two-site gate to a pair of neighbouring sites and splits
    /// the result again by one singular value decomposition, truncated as
    /// `call` says, the kept singular values divided by their L2 norm and
    /// absorbed on the side `call.centre` names. With `θ` the gated pair of
    /// [`GateSplitCall`], the outcome is, to rounding:
    ///
    /// - `left·right` is `θ`'s SVD truncated to its `bond` largest singular
    ///   values, divided by the norm of those values;
    /// - `1 ≤ bond ≤ min(max_extent, 2·left_extent, 2·right_extent)`: the
    ///   largest value is kept whatever the cutoff;
    /// - the site away from the centre is an isometry: `left`, read as a
    ///   `(2·left_extent) × bond` matrix, has orthonormal columns when the
    ///   centre is on the right, and `right`, read as a
    ///   `bond × (2·right_extent)` matrix, has orthonormal rows when it is
    ///   on the left;
    /// - the squares of the dropped values sum to at most `cutoff` of the
    ///   squares of all of them, unless `max_extent` cut the bond;
    /// - `retained_fraction` is the share of the squared values kept, and
    ///   exactly 1 when every dropped value is zero.
    ///
    /// The bond need not be the smallest that meets the cutoff, and a tail
    /// whose squares sum to exactly the cutoff's share may be kept or
    /// dropped: a backend whose library decides these keeps its own rule.
    /// A host backend uses [`truncation::Truncation`].
    fn gate_split(
        &self,
        call: &GateSplitCall<'_, Self::Tensor>,
    ) -> Result<GateSplitOutcome<Self::Tensor>, Self::Error>;

    /// Moves the orthogonality centre across the bond of a pair of
    /// neighbouring sites, exactly, by one QR decomposition of the site it
    /// leaves (its one-site `call.gate` applied first): that site keeps an
    /// isometry, and the other absorbs the remaining factor and becomes the
    /// centre. With `A` and `B` the pair of [`QrMoveCall`] and the gate
    /// applied to the site the centre leaves, the outcome is, to rounding:
    ///
    /// - `left·right` is `A·B`: nothing is truncated;
    /// - `bond` is `min(bond, 2·left_extent)` when the centre moves right,
    ///   `min(bond, 2·right_extent)` when it moves left;
    /// - the site the centre leaves is an isometry, as for
    ///   [`gate_split`](SiteAlgebra::gate_split).
    fn qr_move(
        &self,
        call: &QrMoveCall<'_, Self::Tensor>,
    ) -> Result<QrMoveOutcome<Self::Tensor>, Self::Error>;

    /// The product `a·b` of two host matrices. `a.cols()` equals
    /// `b.rows()`.
    fn multiply(&self, a: MatrixRef<'_>, b: MatrixRef<'_>) -> Matrix;

    /// The thin singular value decomposition `a = u·diag(s)·v†` of a host
    /// matrix, to rounding: `s` holds `r` values in nonincreasing order,
    /// with `1 ≤ r ≤ min(rows, cols)`, `u` is `rows × r` with orthonormal
    /// columns, and `v†` is `r × cols` with orthonormal rows. Values left
    /// out (`r < min(rows, cols)`) are at most rounding's, and the zero
    /// matrix gives one value, zero.
    ///
    /// It must be accurate on rank-deficient matrices, with repeated and
    /// zero singular values: the engine factors states whose Schmidt rank
    /// is below the matrix's size all the time, and trusts the values.
    fn svd(&self, a: MatrixRef<'_>) -> Result<Svd, Self::Error>;
}

/// The site that holds the orthogonality centre after a call: the other
/// site of the pair is an isometry towards it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CentreSide {
    /// The pair's left site.
    Left,
    /// The pair's right site.
    Right,
}

/// One gate split of the pair `A[a, p, j]`, `B[j, q, b]`: the gated pair
/// `θ[a, s, t, b] = Σ G[(s, t), (p, q)] · A[a, p, j] · B[j, q, b]`, read as
/// the column-major `(2·left_extent) × (2·right_extent)` matrix of rows
/// `a + left_extent·s` and columns `t + 2·b`, is split as `θ ≈ L·R`, with
/// `L` of extents `[left_extent, 2, bond]` and `R` of extents
/// `[bond, 2, right_extent]` ([`SiteAlgebra::gate_split`]).
///
/// `θ` is never zero: the engine's states have norm 1, and its gates are
/// unitary.
pub struct GateSplitCall<'t, T> {
    /// `A`, of extents `[left_extent, 2, bond]`.
    pub left: &'t T,
    /// `B`, of extents `[bond, 2, right_extent]`.
    pub right: &'t T,
    /// `A`'s left extent.
    pub left_extent: usize,
    /// The extent of the bond the pair shares before the split.
    pub bond: usize,
    /// `B`'s right extent.
    pub right_extent: usize,
    /// The gate `G`, row-major, the left site the most significant bit:
    /// `G[(s, t), (p, q)]` at `(2s + t)·4 + 2p + q`.
    pub gate: [Complex64; 16],
    /// The largest bond the split may keep, at least 1; `usize::MAX`: no
    /// cap.
    pub max_extent: usize,
    /// The share of the squared singular values the split may drop, in
    /// `[0, 1)`; 0 drops only zeros.
    pub cutoff: f64,
    /// The site that absorbs the singular values, and so holds the centre
    /// afterwards.
    pub centre: CentreSide,
}

/// What one gate split returned ([`SiteAlgebra::gate_split`]).
pub struct GateSplitOutcome<T> {
    /// The new left site, of extents `[left_extent, 2, bond]`, packed.
    pub left: T,
    /// The new right site, of extents `[bond, 2, right_extent]`, packed.
    pub right: T,
    /// The extent of the bond kept.
    pub bond: usize,
    /// The share of the squared singular values kept, in `(0, 1]`: the
    /// engine multiplies the state's retained weight by it.
    pub retained_fraction: f64,
}

/// One move of the centre across the bond of the pair `A[a, p, j]`,
/// `B[j, q, b]` ([`SiteAlgebra::qr_move`]).
///
/// To the right ([`CentreSide::Right`]): `A`, gated, read as the
/// `(2·left_extent) × bond` matrix of rows `(a, p)`, is `Q·R`; `A` becomes
/// `Q` and `B` becomes `R·B`. To the left ([`CentreSide::Left`]): `B`,
/// gated, read as the `bond × (2·right_extent)` matrix of columns `(q, b)`,
/// is `L·Q` with `Q`'s rows orthonormal; `B` becomes `Q` and `A` becomes
/// `A·L`.
pub struct QrMoveCall<'t, T> {
    /// `A`, of extents `[left_extent, 2, bond]`.
    pub left: &'t T,
    /// `B`, of extents `[bond, 2, right_extent]`.
    pub right: &'t T,
    /// `A`'s left extent.
    pub left_extent: usize,
    /// The extent of the bond the pair shares before the move.
    pub bond: usize,
    /// `B`'s right extent.
    pub right_extent: usize,
    /// A one-site gate `G` to apply to the site the centre leaves before
    /// its QR, row-major: `G[s, p]` at `2s + p`. `None`: no gate.
    pub gate: Option<[Complex64; 4]>,
    /// The site that holds the centre afterwards: [`CentreSide::Right`]
    /// moves it right, [`CentreSide::Left`] left.
    pub centre: CentreSide,
}

/// What one move of the centre returned ([`SiteAlgebra::qr_move`]).
pub struct QrMoveOutcome<T> {
    /// The new left site, of extents `[left_extent, 2, bond]`, packed.
    pub left: T,
    /// The new right site, of extents `[bond, 2, right_extent]`, packed.
    pub right: T,
    /// The extent of the bond after the move.
    pub bond: usize,
}
