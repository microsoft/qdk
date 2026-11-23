// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`SiteAlgebra`]: the backend contract of the canonical-form MPS engine,
//! the tensor operations it needs, wherever its site tensors live.
//!
//! The engine ([`CanonicalMpsEngine`](crate::CanonicalMpsEngine)) keeps a
//! chain of site tensors in canonical form: one site, the orthogonality
//! centre, holds the state's norm, and every other site is an isometry
//! towards it. The engine's rules are written once over this trait, so a
//! backend implements only its operations (the crate's `README.md` says
//! why): a gate split and a move of the centre on two neighbouring sites,
//! and the storage of a site. Each of the two calls passes its inputs as
//! one type and gets its outputs as another: [`GateSplitCall`] and
//! [`GateSplitOutcome`], [`QrMoveCall`] and [`QrMoveOutcome`], each call
//! naming the site that holds the centre afterwards as a [`CentreSide`].
//! The bounds every truncation stays within are those of
//! [`SiteAlgebra::gate_split`].
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

mod centre_side;
#[cfg(any(test, feature = "conformance"))]
pub mod conformance;
pub(crate) mod cpu_algebra;
mod gate_split;
mod qr_move;

pub use centre_side::CentreSide;
pub use gate_split::{GateSplitCall, GateSplitOutcome};
pub use qr_move::{QrMoveCall, QrMoveOutcome};

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
/// [`gate_split`](SiteAlgebra::gate_split) and
/// [`qr_move`](SiteAlgebra::qr_move) leave their input tensors unchanged:
/// the caller may still hold them, as the sites of a state it forked.
///
/// # Errors
///
/// Each method fails, with [`SiteAlgebra::Error`], when the backend's own
/// computation fails (an allocation, a copy, a library call), never for
/// arguments that meet the preconditions above, except that an
/// implementation whose tensors belong to one instance may refuse another
/// instance's tensor, which the caller cannot check, as tensors are opaque
/// to it.
pub trait SiteAlgebra {
    /// Where one site tensor's elements live.
    type Tensor;

    /// Why a call failed. `'static`, so that the engine's error
    /// ([`CanonicalMpsError::Algebra`](crate::CanonicalMpsError::Algebra))
    /// can hold it as its source.
    type Error: std::error::Error + 'static;

    /// A new tensor holding `values`. `values` holds at least one element
    /// (a site tensor, at least two). The engine uploads a new state's
    /// sites, and each site it computes on the host: a projected site, an
    /// MPO's multiplied sites.
    fn upload(&self, values: &[Complex64]) -> Result<Self::Tensor, Self::Error>;

    /// The first `len` elements of `tensor`, borrowed where they already
    /// live on the host. `tensor` holds at least `len` elements. The engine
    /// reads sites to compute on the host (a probability, an expectation,
    /// an MPO's span), so reads are frequent: a backend whose tensors live
    /// on the host borrows them rather than copying.
    fn read<'t>(
        &self,
        tensor: &'t Self::Tensor,
        len: usize,
    ) -> Result<Cow<'t, [Complex64]>, Self::Error>;

    /// A new tensor holding the first `len` elements of `tensor`, which
    /// later calls on either leave the other unchanged. `tensor` holds at
    /// least `len` elements, and `len` is at least 1. A fork of a state
    /// duplicates each of its sites, and the two states then change apart.
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
    ///
    /// The engine calls it to apply a window of gates on two neighbouring
    /// qubits, with the centre on the pair, and to compress an MPO's span,
    /// one cut at a time, the identity as gate.
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
    ///
    /// The engine calls it to bring the centre to a gate or a read without
    /// truncating, applying a pending one-site gate on the way, and to
    /// sweep an MPO's span exactly.
    fn qr_move(
        &self,
        call: &QrMoveCall<'_, Self::Tensor>,
    ) -> Result<QrMoveOutcome<Self::Tensor>, Self::Error>;
}
