// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`ContractedPair`]: two neighbouring sites contracted into one block,
//! which a gate on them applies to, and [`Truncation`], what its split
//! discards.
//!
//! A gate on two neighbours contracts their sites into one block and applies
//! to the block; with fusion, the gates that follow on the same pair apply to
//! it too, until another operation splits it by one SVD (`engine::gates`,
//! `engine::split`). The block remembers how its split is to truncate, set by
//! the last two-site operation applied.

use super::c;
use num_complex::Complex64;

/// The contracted block of the sites `cut` and `cut + 1`, with the gates
/// applied to it since it was contracted: `θ[a, s, t, b]` of extents
/// `[left, 2, 2, right]`, laid out at `a + left·(s + 2·(t + 2·b))`. It holds
/// the state's norm, as the centre does.
#[derive(Clone, Debug)]
pub(crate) struct ContractedPair {
    pub(crate) cut: usize,
    pub(crate) left: usize,
    pub(crate) right: usize,
    pub(crate) block: Vec<Complex64>,
    /// What its split discards, set by the last two-site operation applied.
    pub(crate) truncation: Truncation,
}

impl ContractedPair {
    /// Applies the two-site gate `m` (row-major, site `cut` the most
    /// significant bit).
    pub(crate) fn apply(&mut self, m: &[Complex64]) {
        let (l, theta) = (self.left, &mut self.block);
        for b in 0..self.right {
            for a in 0..l {
                let at = |s: usize, t: usize| a + l * (s + 2 * (t + 2 * b));
                let index = [at(0, 0), at(0, 1), at(1, 0), at(1, 1)];
                let v = index.map(|i| theta[i]);
                for (row, &i) in index.iter().enumerate() {
                    let m = &m[4 * row..4 * row + 4];
                    theta[i] = m[0] * v[0] + m[1] * v[1] + m[2] * v[2] + m[3] * v[3];
                }
            }
        }
    }

    /// Applies the one-site gate `m` (row-major) to the pair's site at
    /// chain position `at`, `cut` or `cut + 1`.
    pub(crate) fn apply_one(&mut self, at: usize, m: &[Complex64]) {
        let gate = if self.cut == at {
            kron(m, &IDENTITY)
        } else {
            kron(&IDENTITY, m)
        };
        self.apply(&gate);
    }
}

/// What the split of a [`ContractedPair`] discards: the tail below the
/// cutoff, then what exceeds the capacity's bond at its cut; or, for a pair
/// whose last operation was an outbound SWAP of a route, the tail below the
/// cutoff alone; or, while `MeetHalfway` holds the chain out of the caller's
/// order, the tail below the cutoff, then what exceeds twice the capacity's
/// largest bond (see Routing on [`CpuMpsEngine`](crate::CpuMpsEngine)).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[expect(
    clippy::enum_variant_names,
    reason = "every split applies the cutoff; the names say what else"
)]
pub(crate) enum Truncation {
    /// A split at rest, after a gate or a return SWAP, in the caller's
    /// order.
    CutoffAndCapacity,
    /// A split after an outbound SWAP of `swap-back-2x`, whose bond is at
    /// most twice the capacity's bond at the cut below.
    CutoffOnly,
    /// A split of `MeetHalfway` while the chain is out of the caller's
    /// order: the capacity's cut is deferred until the order is restored.
    CutoffAndTwiceCapacity,
}

/// The one-site identity, row-major.
const IDENTITY: [Complex64; 4] = [c(1.0), c(0.0), c(0.0), c(1.0)];

/// `a ⊗ b` of two one-site gates (row-major), with `a` on the most
/// significant bit.
fn kron(a: &[Complex64], b: &[Complex64]) -> [Complex64; 16] {
    std::array::from_fn(|at| {
        let (row, col) = (at / 4, at % 4);
        a[2 * (row >> 1) + (col >> 1)] * b[2 * (row & 1) + (col & 1)]
    })
}
