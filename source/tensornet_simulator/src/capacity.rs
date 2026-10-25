// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The capacity of a chain of qubits under a bond cap χ.

use tensornet::{MpsShape, MpsShapeError};

/// The capacity of `qubits` qubits with every bond capped at `max_bond`
/// (χ): the bond at the cut after site `k` is `min(χ, 2^(k+1), 2^(n-k-1))`,
/// the cap or the smaller side's full dimension, whichever is less, as M0's
/// `MpsTarget` sized each bond. A chain grows no larger than this, so it is
/// what [`MpsEngine::prepare`](tensornet::MpsEngine::prepare) takes.
///
/// For example `mps_capacity(4, 3)` is `[[2, 2], [2, 2, 3], [3, 2, 2], [2, 2]]`
/// and `mps_capacity(1, 5)` is `[[2]]`.
///
/// # Errors
///
/// [`MpsShapeError::Empty`] for no qubits; [`MpsShapeError::ZeroExtent`]
/// for a cap of 0 on two or more qubits.
pub fn mps_capacity(qubits: usize, max_bond: usize) -> Result<MpsShape, MpsShapeError> {
    let bond = |cut: usize| {
        let smaller = (cut + 1).min(qubits - cut - 1);
        // 2^smaller, saturated: past 2^63 the cap is always the smaller.
        let full = 1usize
            .checked_shl(u32::try_from(smaller).unwrap_or(u32::MAX))
            .unwrap_or(usize::MAX);
        full.min(max_bond)
    };
    let chain = (0..qubits)
        .map(|site| {
            let left = site.checked_sub(1).map(bond);
            let right = (site + 1 < qubits).then(|| bond(site));
            left.into_iter().chain([2]).chain(right).collect()
        })
        .collect();
    MpsShape::new(chain)
}
