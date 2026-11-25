// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`SiteAlgebra`]: the backend contract of the canonical-form MPS engine.
//! It holds the tensor operations the engine needs, wherever its site
//! tensors live.
//!
//! The engine ([`CanonicalMpsEngine`](crate::CanonicalMpsEngine)) keeps a
//! chain of site tensors in canonical form. One site, the orthogonality
//! centre, holds the state's norm. Every other site is an isometry towards
//! it. The engine's rules are written once over this trait (the crate's
//! `README.md` says why). So a backend implements only its operations: a
//! gate split and a move of the centre on two neighbouring sites, and the
//! storage of a site. Each of the two calls passes its inputs as one type
//! and gets its outputs as another: [`GateSplitCall`] and
//! [`GateSplitOutcome`], [`QrMoveCall`] and [`QrMoveOutcome`]. Each call
//! names the site that holds the centre afterwards as a [`CentreSide`].
//! [`SiteAlgebra::gate_split`] sets the bounds every truncation stays
//! within.
//!
//! The crate's `docs/site_algebra.md` writes each operation's rules as
//! formulas, with pictures of the layout and of each call, and says how
//! the conformance kit checks them. The crate README's Concepts define the
//! words this module uses (site, bond, isometry, SVD, truncation, QR), for
//! an implementer new to tensor networks.
//!
//! The module's parts:
//!
//! ```text
//! site_algebra.rs      SiteAlgebra: a site's storage, a gate split and a
//!                      move of the centre
//!   gate_split.rs      GateSplitCall, GateSplitOutcome: a split's inputs
//!                      and outputs
//!   qr_move.rs         QrMoveCall, QrMoveOutcome: a move's inputs and
//!                      outputs
//!   centre_side.rs     CentreSide: the site of the pair that holds the
//!                      centre after either call
//!   conformance.rs     the kit, which checks a backend against the
//!                      contract (feature `conformance`)
//!     storage.rs       the cases of upload, read and duplicate
//!     gate_split.rs    the cases of gate_split
//!     qr_move.rs       the cases of qr_move
//!     inputs.rs        the seeded pairs of sites and gates the cases use
//!     pair_tensors.rs  a pair uploaded before a call, read back after it
//!     formulas.rs      the contract's formulas the cases compare with
//! ```
//!
//! # The layout
//!
//! Every tensor holds complex doubles, its first mode fastest (Fortran
//! order). A site `T[a, s, b]` has extents `[left, 2, right]`: its left
//! bond, its qubit, its right bond. Its element `(a, s, b)` is at
//! `a + left·(s + 2·b)`. Read as a matrix, the same elements are the
//! column-major `(2·left) × right` matrix of rows `a + left·s`. They are
//! also the `left × (2·right)` matrix of columns `s + 2·b`.
//! `docs/site_algebra.md` draws both.
//!
//! The calls' outputs are packed: a site of extents `[left, 2, right]`
//! holds exactly `left·2·right` elements. So a read of one more is
//! refused.
//!
//! # Further reading
//!
//! - U. Schollwöck, "The density-matrix renormalization group in the age
//!   of matrix product states", arXiv:1008.3477
//!   (<https://arxiv.org/abs/1008.3477>), 2011, published in Ann. Phys.
//!   326(1):96–192.
//!   - §4.4, "Bringing a matrix product state into canonical form", covers
//!     the isometries, the centre and its moves by QR.
//!   - §7.3.2, "Time-evolving block decimation (TEBD)", covers a gate on
//!     two neighbours applied and truncated by one SVD, as a gate split
//!     does.

mod centre_side;
#[cfg(any(test, feature = "conformance"))]
pub mod conformance;
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
/// settings) before it calls. So an implementation may assume each is
/// valid. The exception is a read's length: [`read`](SiteAlgebra::read)
/// refuses one past the tensor's end. Tensors follow the
/// [module's layout](self).
///
/// [`gate_split`](SiteAlgebra::gate_split) and
/// [`qr_move`](SiteAlgebra::qr_move) leave their input tensors unchanged.
/// The caller may still hold them, as the sites of a state it forked.
///
/// # Errors
///
/// Each method fails, with [`SiteAlgebra::Error`], when the backend's own
/// computation fails (an allocation, a copy, a library call). It never
/// fails for arguments that meet the preconditions above, with one
/// exception. An implementation whose tensors belong to one instance may
/// refuse another instance's tensor. The caller cannot check that, as
/// tensors are opaque to it.
pub trait SiteAlgebra {
    /// Where one site tensor's elements live.
    type Tensor;

    /// Why a call failed. `'static`, so that the engine's error
    /// ([`CanonicalMpsError::Algebra`](crate::CanonicalMpsError::Algebra))
    /// can hold it as its source.
    type Error: std::error::Error + 'static;

    /// Stores a site tensor on the backend, from its elements on the host.
    /// The engine uploads each site of a new state, and each site it
    /// computes on the host: a projected site, an MPO's multiplied sites.
    ///
    /// Returns a new tensor holding `values`. `values` holds at least one
    /// element (a site tensor, at least two).
    fn upload(&self, values: &[Complex64]) -> Result<Self::Tensor, Self::Error>;

    /// Reads a tensor's elements back to the host. The engine reads sites
    /// to compute on the host (a probability, an expectation, an MPO's
    /// span), so reads are frequent.
    ///
    /// Returns the first `len` elements of `tensor`, borrowed where they
    /// already live on the host: a backend whose tensors live there borrows
    /// them rather than copying. `tensor` holds at least `len` elements. A
    /// read of more is refused, with an error or a panic. It is never
    /// answered with elements that are not the tensor's. So a length the
    /// caller got wrong (a bond misread) fails rather than computes on other
    /// memory.
    fn read<'t>(
        &self,
        tensor: &'t Self::Tensor,
        len: usize,
    ) -> Result<Cow<'t, [Complex64]>, Self::Error>;

    /// Copies a site tensor, so that two states can change apart. The
    /// engine duplicates each site of a state it forks.
    ///
    /// Returns a new tensor holding the first `len` elements of `tensor`.
    /// Later calls on either leave the other unchanged. `tensor` holds at
    /// least `len` elements, and `len` is at least 1.
    fn duplicate(&self, tensor: &Self::Tensor, len: usize) -> Result<Self::Tensor, Self::Error>;

    /// Applies a two-site gate to a pair of neighbouring sites, and splits
    /// the result back into two sites, truncated. The engine calls it to
    /// apply a window of gates on two neighbouring qubits, with the centre
    /// on the pair. It also calls it to compress an MPO's span, one cut at
    /// a time, the identity as gate.
    ///
    /// The gated pair is split by one singular value decomposition,
    /// truncated as `call` says. The kept singular values are divided by
    /// their L2 norm and absorbed on the side `call.centre` names. Let `θ`
    /// be the gated pair of [`GateSplitCall`] and `k` the outcome's `bond`
    /// (the call's `bond` is the one before the split). Up to rounding, the
    /// outcome meets these rules:
    ///
    /// - `left·right` is `θ`'s SVD truncated to its `k` largest singular
    ///   values, divided by the norm of those values.
    /// - `1 ≤ k ≤ min(max_extent, 2·left_extent, 2·right_extent)`. The
    ///   largest value is kept whatever the cutoff.
    /// - The site away from the centre is an isometry. When the centre is
    ///   on the right, `left`, read as a `(2·left_extent) × k` matrix, has
    ///   orthonormal columns. When it is on the left, `right`, read as a
    ///   `k × (2·right_extent)` matrix, has orthonormal rows.
    /// - Unless `max_extent` cut the bond, the squares of the dropped
    ///   values sum to at most `cutoff` of the squares of all of them.
    /// - `retained_fraction` is the share of the squared values kept. It
    ///   is exactly 1 when every dropped value is zero.
    ///
    /// The bond need not be the smallest that meets the cutoff. A tail
    /// whose squares sum to exactly the cutoff's share may be kept or
    /// dropped. A backend whose library decides these keeps its own rule.
    fn gate_split(
        &self,
        call: &GateSplitCall<'_, Self::Tensor>,
    ) -> Result<GateSplitOutcome<Self::Tensor>, Self::Error>;

    /// Moves the orthogonality centre across the bond of a pair of
    /// neighbouring sites, exactly. The engine calls it to bring the centre
    /// to a gate or a read without truncating, applying a pending one-site
    /// gate on the way. It also calls it to compress an MPO's span exactly.
    ///
    /// It makes one QR decomposition of the site the centre leaves, its
    /// one-site `call.gate` applied first. That site keeps an isometry. The
    /// other absorbs the remaining factor and becomes the centre. Let `A`
    /// and `B` be the pair of [`QrMoveCall`], the gate applied to the site
    /// the centre leaves. Up to rounding, the outcome meets these rules:
    ///
    /// - `left·right` is `A·B`: nothing is truncated.
    /// - The outcome's `bond` is `min(call.bond, 2·left_extent)` when the
    ///   centre moves right, and `min(call.bond, 2·right_extent)` when it
    ///   moves left.
    /// - The site the centre leaves is an isometry, as for
    ///   [`gate_split`](SiteAlgebra::gate_split).
    fn qr_move(
        &self,
        call: &QrMoveCall<'_, Self::Tensor>,
    ) -> Result<QrMoveOutcome<Self::Tensor>, Self::Error>;
}
