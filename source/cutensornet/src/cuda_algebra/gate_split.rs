// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A gate split on the GPU: the two-site gate applied to a pair of sites
//! and the result split again, in one cuTensorNet `GateSplit`, which
//! contracts A, B and the gate and factors the product by a truncated SVD.
//!
//! The call's tensors, their extents and their mode labels (`l`, `m` and
//! `r` the pair's left extent, bond and right extent, `k` the largest
//! bond the split may keep):
//!
//! | tensor | extents        | modes                    |
//! | ------ | -------------- | ------------------------ |
//! | A      | `[l, 2, m]`    | `LEFT, P, BOND`          |
//! | B      | `[m, 2, r]`    | `BOND, Q, RIGHT`         |
//! | gate   | `[2, 2, 2, 2]` | `P_OUT, Q_OUT, P, Q`     |
//! | U      | `[l, 2, k]`    | `LEFT, P_OUT, NEW_BOND`  |
//! | V      | `[k, 2, r]`    | `NEW_BOND, Q_OUT, RIGHT` |
//!
//! `k` is `max_extent`, `2l`, `2r` or `4m`, whichever is least: the product
//! is a `2l × 2r` matrix of rank at most `4m` (the gate's four terms times
//! the bond), and cuTensorNet's reduced algorithm, which splits the core
//! left by a QR of A and of B, accepts no more. Under the cutoff, the split
//! keeps the reduced extent its report gives, and U and V are packed at it.
//!
//! The descriptors, the SVD configuration and the report are made for each
//! call and released before it returns: under a cutoff, the split lowers
//! U's and V's extents to the reduced one, so a descriptor would not serve
//! the next call.

use super::call_scope::{CallScope, elements};
use super::device_tensor::DeviceTensor;
use super::modes::{BOND, LEFT, NEW_BOND, P, P_OUT, Q, Q_OUT, RIGHT};
use super::policy;
use crate::native_calls::OpaqueHandle;
use crate::native_calls::error::NativeError;
use crate::native_calls::tensor_calls::{
    GateSplitDescriptors, GateSplitOperands, SvdSetting, SvdTruncation, TensorOperand,
};
use canonical_mps::algebra::{CentreSide, GateSplitCall};
use num_complex::Complex64;

/// What a gate split returned, before its buffers become tensors: U and V
/// hold `capacity` values of the bond, packed at the `bond` kept.
pub(super) struct SplitFactors {
    pub(super) left: OpaqueHandle,
    pub(super) right: OpaqueHandle,
    pub(super) capacity: usize,
    pub(super) bond: usize,
    pub(super) retained_fraction: f64,
}

/// Runs `split` in `scope`, and returns its factors once the device is
/// done.
#[expect(
    clippy::many_single_char_names,
    reason = "the extents as the module's table names them"
)]
pub(super) fn gate_split(
    scope: &mut CallScope<'_>,
    split: &GateSplitCall<'_, DeviceTensor>,
) -> Result<SplitFactors, NativeError> {
    let (l, m, r) = (split.left_extent, split.bond, split.right_extent);
    let k = split
        .max_extent
        .min(l.saturating_mul(2))
        .min(r.saturating_mul(2))
        .min(m.saturating_mul(4));
    let a = TensorOperand {
        descriptor: scope.descriptor(&[l, 2, m], &[LEFT, P, BOND])?,
        data: split.left.holding(elements(&[l, 2, m])?)?,
    };
    let b = TensorOperand {
        descriptor: scope.descriptor(&[m, 2, r], &[BOND, Q, RIGHT])?,
        data: split.right.holding(elements(&[m, 2, r])?)?,
    };
    let gate_descriptor = scope.descriptor(&[2, 2, 2, 2], &[P_OUT, Q_OUT, P, Q])?;
    let u_descriptor = scope.descriptor(&[l, 2, k], &[LEFT, P_OUT, NEW_BOND])?;
    let v_descriptor = scope.descriptor(&[k, 2, r], &[NEW_BOND, Q_OUT, RIGHT])?;
    let partition = match split.centre {
        CentreSide::Left => SvdSetting::PartitionUs,
        CentreSide::Right => SvdSetting::PartitionSv,
    };
    let config = scope.svd_config(&[
        policy::SVD_ALGORITHM,
        SvdSetting::NormalizationL2,
        partition,
        SvdSetting::DiscardedWeightCutoff(split.cutoff),
    ])?;
    let info = scope.svd_info()?;
    let gate = TensorOperand {
        descriptor: gate_descriptor,
        data: scope.upload_gate(&descriptor_gate(&split.gate))?,
    };
    let (calls, handle, workspace) = (scope.calls(), scope.handle(), scope.workspace());
    let descriptors = GateSplitDescriptors {
        a: a.descriptor,
        b: b.descriptor,
        gate: gate.descriptor,
        u: u_descriptor,
        v: v_descriptor,
    };
    calls.gate_split_workspace_sizes(handle, &descriptors, config, workspace)?;
    scope.lend_workspace()?;
    let u = TensorOperand {
        descriptor: u_descriptor,
        data: scope.output(elements(&[l, 2, k])?)?,
    };
    let v = TensorOperand {
        descriptor: v_descriptor,
        data: scope.output(elements(&[k, 2, r])?)?,
    };
    let singular_values = scope.singular_values(k)?;
    let operands = GateSplitOperands {
        a,
        b,
        gate,
        u,
        v,
        singular_values,
    };
    let stream = scope.stream();
    calls.gate_split(handle, &operands, config, info, workspace, stream)?;
    scope.synchronize()?;
    let truncation = calls.svd_truncation(handle, info)?;
    let (bond, discarded_weight) = checked(truncation, k)?;
    Ok(SplitFactors {
        left: u.data,
        right: v.data,
        capacity: k,
        bond,
        retained_fraction: 1.0 - discarded_weight,
    })
}

/// The gate in a descriptor's layout, `G[P, Q, p, q]` at
/// `P + 2Q + 4p + 8q`, from the call's row-major `(2P + Q)·4 + 2p + q`.
fn descriptor_gate(gate: &[Complex64; 16]) -> [Complex64; 16] {
    std::array::from_fn(|at| {
        let (big_p, big_q, p, q) = (at & 1, (at >> 1) & 1, (at >> 2) & 1, at >> 3);
        gate[(2 * big_p + big_q) * 4 + 2 * p + q]
    })
}

/// The bond and the discarded weight `truncation` reports, once checked
/// against what a split of at most `k` values can report: a bond in
/// `1..=k` and at most the full extent, and a weight in `[0, 1)`, so that
/// the retained fraction is in `(0, 1]`. Anything else is a library fault.
fn checked(truncation: SvdTruncation, k: usize) -> Result<(usize, f64), NativeError> {
    let SvdTruncation {
        reduced_extent,
        full_extent,
        discarded_weight,
    } = truncation;
    let invalid = |reason: String| NativeError::InvalidNativeResult { reason };
    let bond = usize::try_from(reduced_extent)
        .ok()
        .filter(|bond| (1..=k).contains(bond))
        .ok_or_else(|| {
            invalid(format!(
                "a gate split kept {reduced_extent} singular values, not 1 to {k}"
            ))
        })?;
    if reduced_extent > full_extent {
        return Err(invalid(format!(
            "a gate split kept {reduced_extent} singular values of {full_extent}"
        )));
    }
    if !(0.0..1.0).contains(&discarded_weight) {
        return Err(invalid(format!(
            "a gate split discarded a weight of {discarded_weight}, not in [0, 1)"
        )));
    }
    Ok((bond, discarded_weight))
}
