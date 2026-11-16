// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`SiteAlgebra`]: the backend contract of the canonical-form MPS engine,
//! the tensor operations it needs, wherever its site tensors live.
//!
//! The planned engine will keep a chain of site tensors in canonical
//! form: one site, the orthogonality centre, holds the state's norm, and
//! every other site is an isometry towards it. Some of its rules will be
//! the same on every backend: where the centre moves, which pair a gate
//! splits, how the retained weight is kept, and the bounds every
//! truncation stays within (those of [`SiteAlgebra::gate_split`]); what
//! differs is where the tensors live, which library factors them, and
//! which values a truncation keeps within those bounds. So the engine is
//! to be written once over this trait, and a backend implements only its
//! operations: a gate split and a move of the centre on two neighbouring
//! sites, and the storage of a site.
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

mod cpu_algebra;
pub(crate) mod truncation;

pub use cpu_algebra::CpuAlgebra;

use num_complex::Complex64;
use std::borrow::Cow;

/// The tensor operations of the canonical-form MPS engine: the storage of
/// a site tensor, and a gate split and a move of the orthogonality centre
/// on two neighbouring sites.
///
/// The caller has checked every argument (extents, lengths, the call's
/// settings) before it calls, so an implementation may assume each is
/// valid. Tensors follow the [module's layout](self).
///
/// # Errors
///
/// Each method fails, with [`SiteAlgebra::Error`], only when the backend's
/// own computation fails (an allocation, a copy, a library call), never
/// for an argument.
pub trait SiteAlgebra {
    /// Where one site tensor's elements live.
    type Tensor;

    /// Why a call failed. `'static`, so that the planned engine's error
    /// can hold it as its source.
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
    /// [`GateSplitCall`] and `k` the outcome's `bond` (the call's `bond` is
    /// the one before the split), the outcome is, to rounding:
    ///
    /// - `left·right` is `θ`'s SVD truncated to its `k` largest singular
    ///   values, divided by the norm of those values;
    /// - `1 ≤ k ≤ min(max_extent, 2·left_extent, 2·right_extent)`: the
    ///   largest value is kept whatever the cutoff;
    /// - the site away from the centre is an isometry: `left`, read as a
    ///   `(2·left_extent) × k` matrix, has orthonormal columns when the
    ///   centre is on the right, and `right`, read as a
    ///   `k × (2·right_extent)` matrix, has orthonormal rows when it is
    ///   on the left;
    /// - the squares of the dropped values sum to at most `cutoff` of the
    ///   squares of all of them, unless `max_extent` cut the bond;
    /// - `retained_fraction` is the share of the squared values kept, and
    ///   exactly 1 when every dropped value is zero.
    ///
    /// The bond need not be the smallest that meets the cutoff, and a tail
    /// whose squares sum to exactly the cutoff's share may be kept or
    /// dropped: a backend whose library decides these keeps its own rule.
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
    /// - the outcome's `bond` is `min(call.bond, 2·left_extent)` when the
    ///   centre moves right, `min(call.bond, 2·right_extent)` when it moves
    ///   left;
    /// - the site the centre leaves is an isometry, as for
    ///   [`gate_split`](SiteAlgebra::gate_split).
    fn qr_move(
        &self,
        call: &QrMoveCall<'_, Self::Tensor>,
    ) -> Result<QrMoveOutcome<Self::Tensor>, Self::Error>;
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
/// `L` of extents `[left_extent, 2, k]` and `R` of extents
/// `[k, 2, right_extent]`, `k` the outcome's bond
/// ([`SiteAlgebra::gate_split`]).
///
/// `θ` is never zero: the caller's states have norm 1, and its gates are
/// unitary.
#[derive(Debug)]
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
    /// `[0, 1)`; 0 drops the zeros, and may drop values so small, against
    /// the largest, that their squares underflow.
    pub cutoff: f64,
    /// The site that absorbs the singular values, and so holds the centre
    /// afterwards.
    pub centre: CentreSide,
}

/// What one gate split returned ([`SiteAlgebra::gate_split`]).
#[derive(Debug)]
pub struct GateSplitOutcome<T> {
    /// The new left site, of extents `[left_extent, 2, bond]`, packed.
    pub left: T,
    /// The new right site, of extents `[bond, 2, right_extent]`, packed.
    pub right: T,
    /// The extent of the bond kept.
    pub bond: usize,
    /// The share of the squared singular values kept, in `(0, 1]`: the
    /// planned engine will multiply the state's retained weight by it.
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
#[derive(Debug)]
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
#[derive(Debug)]
pub struct QrMoveOutcome<T> {
    /// The new left site, of extents `[left_extent, 2, bond]`, packed.
    pub left: T,
    /// The new right site, of extents `[bond, 2, right_extent]`, packed.
    pub right: T,
    /// The extent of the bond after the move.
    pub bond: usize,
}
