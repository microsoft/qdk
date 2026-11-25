// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A gate split on the GPU: the two-site gate applied to a pair of sites
//! and the result split again, in one cuTensorNet `GateSplit`, which
//! contracts A, B and the gate and factors the product by a truncated SVD.
//!
//! The call's tensors, their extents and their mode labels (`l`, `m` and
//! `r` the pair's left extent, bond and right extent, `left`, `bond` and
//! `right` in the code; `k` the largest bond the split may keep,
//! `largest_bond`). Each row is one [`TensorLayout`] of [`SplitLayouts`]:
//!
//! | tensor | extents        | modes                    |
//! | ------ | -------------- | ------------------------ |
//! | A      | `[l, 2, m]`    | `LEFT, P, BOND`          |
//! | B      | `[m, 2, r]`    | `BOND, Q, RIGHT`         |
//! | gate   | `[2, 2, 2, 2]` | `P_OUT, Q_OUT, P, Q`     |
//! | U      | `[l, 2, k]`    | `LEFT, P_OUT, NEW_BOND`  |
//! | V      | `[k, 2, r]`    | `NEW_BOND, Q_OUT, RIGHT` |
//!
//! The split runs cuTensorNet's reduced algorithm
//! (`CUTENSORNET_GATE_SPLIT_ALGO_REDUCED`, chosen in `settings.rs`). It first
//! factors A and B by QR, then applies the gate to the small core their
//! QRs leave, and splits only that core by the SVD. `k` is `max_extent`,
//! `2l`, `2r` or `4m`, whichever is least: the product is a `2l × 2r`
//! matrix of rank at most `4m` (the gate's four terms times the bond), and
//! the reduced algorithm accepts no more. Under the cutoff, the split keeps
//! the reduced extent its report gives, and U and V are packed at it.
//!
//! The descriptors, the SVD configuration and the report are made for each
//! call and released before it returns. Under a cutoff, the split lowers
//! U's and V's extents to the reduced one, so a descriptor would not serve
//! the next call.

use super::call_scope::CallScope;
use super::device_tensor::DeviceTensor;
use super::modes::{BOND, LEFT, NEW_BOND, P, P_OUT, Q, Q_OUT, RIGHT};
use super::settings;
use super::tensor_layout::TensorLayout;
use crate::engine::cuda_algebra_error::CudaAlgebraError;
use crate::engine::native_calls::memory_calls::DeviceBuffer;
use crate::engine::native_calls::tensor_calls::{
    GateSplitDescriptors, GateSplitOperands, SvdSetting, SvdTruncation, TensorOperand,
    TensorSvdConfig,
};
use canonical_mps::site_algebra::{CentreSide, GateSplitCall};
use num_complex::Complex64;

/// What a gate split returned, before its buffers become tensors: U and V,
/// allocated for the largest bond the split may keep and packed at the
/// `bond` kept.
pub(super) struct SplitFactors {
    /// U, the new left site.
    pub(super) left: DeviceBuffer,
    /// V, the new right site.
    pub(super) right: DeviceBuffer,
    /// The bond `k` the cutoff kept.
    pub(super) bond: usize,
    /// The share of the squared singular values kept.
    pub(super) retained_fraction: f64,
}

/// Runs `split` in `scope`, and returns its factors once the device is
/// done.
///
/// ```text
///  before:  ─l─ A ─m─ B ─r─      A, B: the pair; G: the two-site gate,
///               p     q          on their qubits p and q
///               └─ G ─┘
///  after:   ─l─ U ─k─ V ─r─      k: the bond the cutoff keeps; the kept
///               p     q          singular values go into the factor on
///                                the centre's side
/// ```
///
/// It describes the five tensors, configures the SVD, uploads the gate,
/// and sizes and lends the workspace. It then allocates U, V and the
/// singular values for the largest bond, runs the split and waits for
/// it. It fails if an input holds fewer values than its extents, if a
/// native call fails, or if the split's report is out of range
/// ([`checked_truncation`]).
pub(super) fn gate_split(
    scope: &mut CallScope<'_>,
    split: &GateSplitCall<'_, DeviceTensor>,
) -> Result<SplitFactors, CudaAlgebraError> {
    let layouts = SplitLayouts::of(split);
    let a_data = split.left.buffer_holding(layouts.a.elements()?)?;
    let b_data = split.right.buffer_holding(layouts.b.elements()?)?;
    let descriptors = describe(scope, &layouts)?;
    let config = svd_config(scope, split)?;
    let info = scope.svd_info()?;
    let gate_data = scope.upload_gate(&in_descriptor_layout(&split.gate))?;
    let (calls, handle, workspace) = (scope.calls(), scope.handle(), scope.workspace());
    calls.gate_split_workspace_sizes(handle, &descriptors, config, workspace)?;
    scope.lend_workspace()?;
    let operands = GateSplitOperands {
        a: TensorOperand {
            descriptor: descriptors.a,
            data: a_data,
        },
        b: TensorOperand {
            descriptor: descriptors.b,
            data: b_data,
        },
        gate: TensorOperand {
            descriptor: descriptors.gate,
            data: gate_data,
        },
        u: TensorOperand {
            descriptor: descriptors.u,
            data: scope.output(layouts.u.elements()?)?,
        },
        v: TensorOperand {
            descriptor: descriptors.v,
            data: scope.output(layouts.v.elements()?)?,
        },
        singular_values: scope.singular_values(layouts.largest_bond)?,
    };
    let stream = scope.stream();
    calls.gate_split(handle, &operands, config, info, workspace, stream)?;
    scope.synchronize()?;
    let truncation = calls.svd_truncation(handle, info)?;
    let (bond, discarded_weight) = checked_truncation(truncation, layouts.largest_bond)?;
    Ok(SplitFactors {
        left: operands.u.data,
        right: operands.v.data,
        bond,
        retained_fraction: 1.0 - discarded_weight,
    })
}

/// The layouts of a split's five tensors, the rows of the module's table,
/// and the largest bond the split may keep (`k` there). They are computed
/// once, since the descriptors, the input checks and the output buffers
/// all need them.
struct SplitLayouts {
    a: TensorLayout<3>,
    b: TensorLayout<3>,
    gate: TensorLayout<4>,
    u: TensorLayout<3>,
    v: TensorLayout<3>,
    largest_bond: usize,
}

impl SplitLayouts {
    /// The layouts of `split`'s tensors. The largest bond is the least of
    /// `max_extent`, `2l`, `2r` and `4m`, for the module header's reasons.
    fn of(split: &GateSplitCall<'_, DeviceTensor>) -> Self {
        let (left, bond, right) = (split.left_extent, split.bond, split.right_extent);
        let largest_bond = split
            .max_extent
            .min(left.saturating_mul(2))
            .min(right.saturating_mul(2))
            .min(bond.saturating_mul(4));
        Self {
            a: TensorLayout {
                extents: [left, 2, bond],
                modes: [LEFT, P, BOND],
            },
            b: TensorLayout {
                extents: [bond, 2, right],
                modes: [BOND, Q, RIGHT],
            },
            gate: TensorLayout {
                extents: [2, 2, 2, 2],
                modes: [P_OUT, Q_OUT, P, Q],
            },
            u: TensorLayout {
                extents: [left, 2, largest_bond],
                modes: [LEFT, P_OUT, NEW_BOND],
            },
            v: TensorLayout {
                extents: [largest_bond, 2, right],
                modes: [NEW_BOND, Q_OUT, RIGHT],
            },
            largest_bond,
        }
    }
}

/// The descriptors of the split's five tensors, destroyed when the call
/// returns. Made apart from the buffers, since cuTensorNet sizes the
/// workspace from the descriptors before the outputs are allocated.
fn describe(
    scope: &mut CallScope<'_>,
    layouts: &SplitLayouts,
) -> Result<GateSplitDescriptors, CudaAlgebraError> {
    Ok(GateSplitDescriptors {
        a: scope.descriptor(&layouts.a)?,
        b: scope.descriptor(&layouts.b)?,
        gate: scope.descriptor(&layouts.gate)?,
        u: scope.descriptor(&layouts.u)?,
        v: scope.descriptor(&layouts.v)?,
    })
}

/// The SVD configuration of `split`, destroyed when the call returns: the
/// algorithm `settings.rs` sets, the kept values normalized, absorbed into
/// the factor on the centre's side, and truncated at the call's cutoff.
/// These are the only settings the algebra makes, so they stand in one
/// place.
fn svd_config(
    scope: &mut CallScope<'_>,
    split: &GateSplitCall<'_, DeviceTensor>,
) -> Result<TensorSvdConfig, CudaAlgebraError> {
    let partition = match split.centre {
        CentreSide::Left => SvdSetting::PartitionUs,
        CentreSide::Right => SvdSetting::PartitionSv,
    };
    scope.svd_config(&[
        settings::SVD_ALGORITHM,
        SvdSetting::NormalizationL2,
        partition,
        SvdSetting::DiscardedWeightCutoff(split.cutoff),
    ])
}

/// The gate in a descriptor's layout, `G[P, Q, p, q]` at
/// `P + 2Q + 4p + 8q`, from the call's row-major `(2P + Q)·4 + 2p + q`.
fn in_descriptor_layout(gate: &[Complex64; 16]) -> [Complex64; 16] {
    std::array::from_fn(|at| {
        let (big_p, big_q, p, q) = (at & 1, (at >> 1) & 1, (at >> 2) & 1, at >> 3);
        gate[(2 * big_p + big_q) * 4 + 2 * p + q]
    })
}

/// The bond and the discarded weight `truncation` reports, once checked
/// against what a split of at most `largest_bond` values can report. The
/// bond is in `1..=largest_bond` and at most the full extent. The weight
/// is in `[0, 1)`, or below zero by at most
/// `settings::DISCARDED_WEIGHT_TOLERANCE` and read as 0, so the retained
/// fraction is in `(0, 1]`. Anything else is a library fault.
///
/// The upper end is strict. cuTensorNet keeps the largest singular values,
/// at least one of them (as the bond's check asks), and the largest square
/// is at least `1/n` of the sum of `n`. So a true weight is at most
/// `1 - 1/n`, and a weight of 1 or more, or NaN, is a fault.
fn checked_truncation(
    truncation: SvdTruncation,
    largest_bond: usize,
) -> Result<(usize, f64), CudaAlgebraError> {
    let SvdTruncation {
        reduced_extent,
        full_extent,
        discarded_weight,
    } = truncation;
    let invalid = |reason: String| CudaAlgebraError::InvalidNativeResult { reason };
    let bond = usize::try_from(reduced_extent)
        .ok()
        .filter(|bond| (1..=largest_bond).contains(bond))
        .ok_or_else(|| {
            invalid(format!(
                "a gate split kept {reduced_extent} singular values, not 1 to {largest_bond}"
            ))
        })?;
    if reduced_extent > full_extent {
        return Err(invalid(format!(
            "a gate split kept {reduced_extent} singular values of {full_extent}"
        )));
    }
    if !(-settings::DISCARDED_WEIGHT_TOLERANCE..1.0).contains(&discarded_weight) {
        return Err(invalid(format!(
            "a gate split discarded a weight of {discarded_weight}, not in [0, 1) \
             (nor a rounding of 0 within {:e})",
            settings::DISCARDED_WEIGHT_TOLERANCE
        )));
    }
    Ok((bond, discarded_weight.max(0.0)))
}

#[cfg(test)]
mod tests;
