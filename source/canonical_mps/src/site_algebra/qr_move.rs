// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`QrMoveCall`] and [`QrMoveOutcome`]: what the engine passes to
//! [`SiteAlgebra::qr_move`](super::SiteAlgebra::qr_move), and what it
//! gets back.

use super::CentreSide;
use num_complex::Complex64;

/// One move of the centre across the bond of the pair `A[a, p, j]`,
/// `B[j, q, b]` ([`SiteAlgebra::qr_move`](super::SiteAlgebra::qr_move)).
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

/// What one move of the centre returned
/// ([`SiteAlgebra::qr_move`](super::SiteAlgebra::qr_move)).
#[derive(Debug)]
pub struct QrMoveOutcome<T> {
    /// The new left site, of extents `[left_extent, 2, bond]`, packed.
    pub left: T,
    /// The new right site, of extents `[bond, 2, right_extent]`, packed.
    pub right: T,
    /// The extent of the bond after the move.
    pub bond: usize,
}
