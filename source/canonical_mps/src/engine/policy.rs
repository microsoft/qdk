// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The engine's default setting and its fixed threshold, with their
//! reasons.

/// The cutoff of [`CanonicalMpsEngine::new`](super::CanonicalMpsEngine::new):
/// a split drops singular values whose squares sum to at most `1e-14` of
/// all their squares, within the bounds
/// [`with_cutoff`](super::CanonicalMpsEngine::with_cutoff) states, which
/// also says what a smaller or larger cutoff trades.
///
/// A cutoff of zero keeps every value a split returns, those just above
/// rounding included, which grows the bonds and the time: on an Ising
/// quench of 16 qubits, depth 20, with `χ` 256, run by this engine's rules
/// on the CPU, the largest bond at rest measured 254 against 138 at this
/// cutoff, and the run took about 2.4 times as long.
pub const DEFAULT_CUTOFF: f64 = 1e-14;

/// The singular values a window's operator keeps when it is factored into
/// a matrix product operator ([`mpo`](super::mpo)): those above `1e-12`
/// times the largest. The factorisation is exact: the values below are
/// rounding's (the operator is a product of gates, each checked unitary),
/// and keeping them would grow the operator's bond, and with it every bond
/// of the span before its splits, for no weight. The dev line's MPO used
/// the same threshold, on the H₂ and polyene phase estimations.
pub(super) const OPERATOR_RANK_CUTOFF: f64 = 1e-12;
