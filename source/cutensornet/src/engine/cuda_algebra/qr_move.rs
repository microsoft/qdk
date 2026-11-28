// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A move of the centre on the GPU: the site the centre leaves, gated if
//! the call says so, factored by one cuTensorNet QR, and its small factor
//! contracted into the other site.
//!
//! The QR's two factors are called the isometry and the small factor
//! here, since `Q` is the right site's qubit's mode.
//!
//! The call's tensors, their extents and their mode labels (`l`, `m` and
//! `r` the pair's left extent, bond and right extent, `left`, `bond` and
//! `right` in the code; `k` the new bond, `new_bond`, `min(m, 2l)` to the
//! right and `min(m, 2r)` to the left). Each row is one [`TensorLayout`]:
//!
//! | to the right                | extents     | modes                |
//! | --------------------------- | ----------- | -------------------- |
//! | gate (if any), on A         | `[2, 2]`    | `P_OUT, P`           |
//! | QR input, A                 | `[l, 2, m]` | `LEFT, P, BOND`      |
//! | the isometry, the new A     | `[l, 2, k]` | `LEFT, P, NEW_BOND`  |
//! | the small factor            | `[k, m]`    | `NEW_BOND, BOND`     |
//! | the new centre, factor · B  | `[k, 2, r]` | `NEW_BOND, Q, RIGHT` |
//!
//! | to the left                 | extents     | modes                |
//! | --------------------------- | ----------- | -------------------- |
//! | gate (if any), on B         | `[2, 2]`    | `Q_OUT, Q`           |
//! | QR input, B                 | `[m, 2, r]` | `BOND, Q, RIGHT`     |
//! | the isometry, the new B     | `[k, 2, r]` | `NEW_BOND, Q, RIGHT` |
//! | the small factor            | `[m, k]`    | `BOND, NEW_BOND`     |
//! | the new centre, A · factor  | `[l, 2, k]` | `LEFT, P, NEW_BOND`  |
//!
//! cuTensorNet's QR gives the isometry the input's modes that its
//! descriptor names, and the small factor the rest. So to the left, B is
//! factored as a `2r × m` matrix of rows `(q, r)`: its isometry, read as a
//! `k × 2r` matrix, has orthonormal rows, as the contract asks. The gate
//! and the small factor go through [`contraction`](super::contraction),
//! the QR through `tensor_qr`, each waited for before the next starts.

use super::call_scope::CallScope;
use super::contraction::{ContractionInput, contract};
use super::device_tensor::DeviceTensor;
use super::modes::{BOND, LEFT, NEW_BOND, P, P_OUT, Q, Q_OUT, RIGHT};
use super::tensor_layout::TensorLayout;
use crate::engine::cuda_algebra_error::CudaAlgebraError;
use crate::engine::native_calls::memory_calls::DeviceBuffer;
use crate::engine::native_calls::tensor_calls::TensorOperand;
use canonical_mps::site_algebra::{CentreSide, QrMoveCall};
use num_complex::Complex64;

/// What a move of the centre returned, before its buffers become tensors.
pub(super) struct MovedSites {
    /// The new left site.
    pub(super) left: DeviceBuffer,
    /// The new right site.
    pub(super) right: DeviceBuffer,
    /// The new bond, `k` in the module's tables.
    pub(super) bond: usize,
}

/// Runs `call` in `scope`, and returns the two new sites once the device
/// is done.
///
/// It checks that A and B hold their extents' values, then moves the
/// centre the way `call.centre` names ([`move_right`], [`move_left`]). It
/// fails if an input holds fewer values than its extents, or if a native
/// call fails.
pub(super) fn qr_move(
    scope: &mut CallScope<'_>,
    call: &QrMoveCall<'_, DeviceTensor>,
) -> Result<MovedSites, CudaAlgebraError> {
    let pair = PairLayouts::of(call);
    let a = call.left.buffer_holding(pair.a.elements()?)?;
    let b = call.right.buffer_holding(pair.b.elements()?)?;
    match call.centre {
        CentreSide::Right => move_right(scope, call, &pair, [a, b]),
        CentreSide::Left => move_left(scope, call, &pair, [a, b]),
    }
}

/// The layouts of the pair's two sites before the move: A and B, the QR
/// input on one side and the site the small factor goes into on the
/// other. Both directions need both, and the input checks come first.
struct PairLayouts {
    a: TensorLayout<3>,
    b: TensorLayout<3>,
}

impl PairLayouts {
    /// The layouts of `call`'s two sites, the tables' rows for A and B.
    fn of(call: &QrMoveCall<'_, DeviceTensor>) -> Self {
        let (left, bond, right) = (call.left_extent, call.bond, call.right_extent);
        Self {
            a: TensorLayout {
                extents: [left, 2, bond],
                modes: [LEFT, P, BOND],
            },
            b: TensorLayout {
                extents: [bond, 2, right],
                modes: [BOND, Q, RIGHT],
            },
        }
    }
}

/// The move to the right, the module's first table: A, gated, is factored
/// into the new A and a small factor, which B absorbs into the new centre.
///
/// ```text
///  before:  ─l─ A ─m─ B ─r─       A: the centre, its gate pending
///  QR:      A = I · F             I: the isometry, [l, 2, k];
///                                 F: the small factor, [k, m]
///  after:   ─l─ I ─k─ F·B ─r─     F·B: the new centre
/// ```
fn move_right(
    scope: &mut CallScope<'_>,
    call: &QrMoveCall<'_, DeviceTensor>,
    pair: &PairLayouts,
    [a, b]: [DeviceBuffer; 2],
) -> Result<MovedSites, CudaAlgebraError> {
    let (left, bond, right) = (call.left_extent, call.bond, call.right_extent);
    let new_bond = bond.min(left.saturating_mul(2));
    let qr = QrLayouts {
        input: pair.a,
        isometry: TensorLayout {
            extents: [left, 2, new_bond],
            modes: [LEFT, P, NEW_BOND],
        },
        small_factor: TensorLayout {
            extents: [new_bond, bond],
            modes: [NEW_BOND, BOND],
        },
    };
    let centre = TensorLayout {
        extents: [new_bond, 2, right],
        modes: [NEW_BOND, Q, RIGHT],
    };
    let site = gated(scope, a, &pair.a, call.gate.as_ref())?;
    let (isometry, small_factor) = factor(scope, site, &qr)?;
    let centre_data = scope.output(centre.elements()?)?;
    contract(
        scope,
        &[
            ContractionInput::new(small_factor, &qr.small_factor),
            ContractionInput::new(b, &pair.b),
        ],
        &centre.modes,
        centre_data,
    )?;
    Ok(MovedSites {
        left: isometry,
        right: centre_data,
        bond: new_bond,
    })
}

/// The move to the left, the module's second table: B, gated, is factored
/// into the new B and a small factor, which A absorbs into the new centre.
///
/// ```text
///  before:  ─l─ A ─m─ B ─r─       B: the centre, its gate pending
///  QR:      B = F · I             I: the isometry, [k, 2, r];
///                                 F: the small factor, [m, k]
///  after:   ─l─ A·F ─k─ I ─r─     A·F: the new centre
/// ```
fn move_left(
    scope: &mut CallScope<'_>,
    call: &QrMoveCall<'_, DeviceTensor>,
    pair: &PairLayouts,
    [a, b]: [DeviceBuffer; 2],
) -> Result<MovedSites, CudaAlgebraError> {
    let (left, bond, right) = (call.left_extent, call.bond, call.right_extent);
    let new_bond = bond.min(right.saturating_mul(2));
    let qr = QrLayouts {
        input: pair.b,
        isometry: TensorLayout {
            extents: [new_bond, 2, right],
            modes: [NEW_BOND, Q, RIGHT],
        },
        small_factor: TensorLayout {
            extents: [bond, new_bond],
            modes: [BOND, NEW_BOND],
        },
    };
    let centre = TensorLayout {
        extents: [left, 2, new_bond],
        modes: [LEFT, P, NEW_BOND],
    };
    let site = gated(scope, b, &pair.b, call.gate.as_ref())?;
    let (isometry, small_factor) = factor(scope, site, &qr)?;
    let centre_data = scope.output(centre.elements()?)?;
    contract(
        scope,
        &[
            ContractionInput::new(a, &pair.a),
            ContractionInput::new(small_factor, &qr.small_factor),
        ],
        &centre.modes,
        centre_data,
    )?;
    Ok(MovedSites {
        left: centre_data,
        right: isometry,
        bond: new_bond,
    })
}

/// `site`, of `layout`, with the one-site `gate` applied to its qubit (the
/// middle mode), in a buffer of the call; `site` itself when there is no
/// gate.
fn gated(
    scope: &mut CallScope<'_>,
    site: DeviceBuffer,
    layout: &TensorLayout<3>,
    gate: Option<&[Complex64; 4]>,
) -> Result<DeviceBuffer, CudaAlgebraError> {
    let Some(gate) = gate else {
        return Ok(site);
    };
    let [outer_left, qubit, outer_right] = layout.modes;
    let qubit_out = if qubit == P { P_OUT } else { Q_OUT };
    let gate_layout = TensorLayout {
        extents: [2, 2],
        modes: [qubit_out, qubit],
    };
    let gate = scope.upload_gate(&in_descriptor_layout(gate))?;
    let out = scope.intermediate(layout.elements()?)?;
    contract(
        scope,
        &[
            ContractionInput::new(gate, &gate_layout),
            ContractionInput::new(site, layout),
        ],
        &[outer_left, qubit_out, outer_right],
        out,
    )?;
    Ok(out)
}

/// The one-site gate in a descriptor's layout, `G[S, s]` at `S + 2s`, from
/// the call's row-major `2S + s`.
fn in_descriptor_layout(gate: &[Complex64; 4]) -> [Complex64; 4] {
    std::array::from_fn(|at| gate[2 * (at & 1) + (at >> 1)])
}

/// The layouts of a QR's three tensors: its input, the isometry and the
/// small factor, rows of one of the module's tables. They differ by
/// direction, and [`factor`] takes them as one value.
struct QrLayouts {
    input: TensorLayout<3>,
    isometry: TensorLayout<3>,
    small_factor: TensorLayout<2>,
}

/// The QR of `input`, of `layouts.input`, into an isometry, returned by
/// the call, and a small factor, freed when it returns; waited for. Both
/// directions run the same QR on their own layouts.
///
/// It describes the three tensors, sizes and lends the workspace,
/// allocates the two factors, runs cuTensorNet's QR and waits for it. It
/// fails if a native call fails.
fn factor(
    scope: &mut CallScope<'_>,
    input: DeviceBuffer,
    layouts: &QrLayouts,
) -> Result<(DeviceBuffer, DeviceBuffer), CudaAlgebraError> {
    let input = TensorOperand {
        descriptor: scope.descriptor(&layouts.input)?,
        data: input,
    };
    let isometry_descriptor = scope.descriptor(&layouts.isometry)?;
    let small_factor_descriptor = scope.descriptor(&layouts.small_factor)?;
    let (calls, handle, workspace) = (scope.calls(), scope.handle(), scope.workspace());
    calls.qr_workspace_sizes(
        handle,
        input.descriptor,
        isometry_descriptor,
        small_factor_descriptor,
        workspace,
    )?;
    scope.lend_workspace()?;
    let isometry = TensorOperand {
        descriptor: isometry_descriptor,
        data: scope.output(layouts.isometry.elements()?)?,
    };
    let small_factor = TensorOperand {
        descriptor: small_factor_descriptor,
        data: scope.intermediate(layouts.small_factor.elements()?)?,
    };
    calls.tensor_qr(
        handle,
        input,
        isometry,
        small_factor,
        workspace,
        scope.stream(),
    )?;
    scope.synchronize()?;
    Ok((isometry.data, small_factor.data))
}
