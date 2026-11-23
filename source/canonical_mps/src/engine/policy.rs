// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The engine's default cutoff and its operator rank cutoff, with their
//! reasons.

/// The cutoff of [`CanonicalMpsEngine::new`](super::CanonicalMpsEngine::new):
/// a split drops singular values whose squares sum to at most this
/// fraction of all their squares, within the bounds
/// [`with_cutoff`](super::CanonicalMpsEngine::with_cutoff) states, which
/// also says what a smaller or larger cutoff trades.
///
/// A cutoff of zero keeps every value above rounding's size, those just
/// above it included, which grows the bonds and the time: on an Ising
/// quench of 16 qubits, depth 20, with `χ` 256, run with fusion (as
/// [`new`](super::CanonicalMpsEngine::new) makes the engine) on the CPU,
/// the largest bond at rest measured 254 against 138 at this cutoff, and
/// the run took about 2.7 times as long.
pub const DEFAULT_CUTOFF: f64 = 1e-14;

/// The singular values a window's operator keeps when it is factored into a
/// matrix product operator ([`mpo`](super::mpo)): those above this fraction of
/// the largest. [`Svd::of`](crate::dense_linalg::Svd::of) already trims the
/// values its rank-revealing QR finds to be rounding; this cutoff drops what
/// remains at or below that relative size: rounding the trim kept, because a
/// pivoted QR is not perfectly rank-revealing, or a genuine part of the
/// operator that small (a two-qubit rotation by an angle below about `1e-12`),
/// which the retained weight does not count. Keeping them would grow the
/// operator's bond, and with it every bond of the span before its splits, for
/// at most that part. The threshold sits about four decades above
/// `f64::EPSILON` (about `2.2e-16`), so values of rounding, a few times that
/// relative to the largest, fall well below it.
pub(super) const OPERATOR_RANK_CUTOFF: f64 = 1e-12;
