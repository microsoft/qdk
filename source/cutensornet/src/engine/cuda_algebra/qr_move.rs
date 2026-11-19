// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A move of the centre on the GPU: the site the centre leaves, gated if
//! the call says so, factored by one cuTensorNet QR, and its small factor
//! contracted into the other site.
//!
//! The call's tensors, their extents and their mode labels (`l`, `m` and
//! `r` the pair's left extent, bond and right extent; `k` the new bond,
//! `min(m, 2l)` to the right and `min(m, 2r)` to the left):
//!
//! The QR's two factors are called the isometry and the small factor
//! here, since `Q` is the right site's qubit's mode.
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
//! descriptor names and the small factor the rest, so to the left B is
//! factored as a `2r × m` matrix, of rows `(q, r)`: its isometry, read as
//! a `k × 2r` matrix, has orthonormal rows, as the contract asks. The gate
//! and the small factor go through
//! [`contraction`](super::contraction), the QR through `tensor_qr`, each
//! waited for before the next starts.

use super::call_scope::{CallScope, element_count};
use super::contraction::{ContractionInput, contract};
use super::device_tensor::DeviceTensor;
use super::modes::{BOND, LEFT, NEW_BOND, P, P_OUT, Q, Q_OUT, RIGHT};
use crate::engine::cuda_algebra_error::CudaAlgebraError;
use crate::engine::native_calls::OpaqueHandle;
use crate::engine::native_calls::tensor_calls::TensorOperand;
use canonical_mps::site_algebra::{CentreSide, QrMoveCall};
use num_complex::Complex64;

/// What a move of the centre returned, before its buffers become tensors.
pub(super) struct MovedSites {
    pub(super) left: OpaqueHandle,
    pub(super) right: OpaqueHandle,
    pub(super) bond: usize,
}

/// Runs `call` in `scope`, and returns the two new sites once the device
/// is done.
#[expect(
    clippy::many_single_char_names,
    reason = "the extents as the module's table names them"
)]
pub(super) fn qr_move(
    scope: &mut CallScope<'_>,
    call: &QrMoveCall<'_, DeviceTensor>,
) -> Result<MovedSites, CudaAlgebraError> {
    let (l, m, r) = (call.left_extent, call.bond, call.right_extent);
    let a = call.left.buffer_holding(element_count(&[l, 2, m])?)?;
    let b = call.right.buffer_holding(element_count(&[m, 2, r])?)?;
    match call.centre {
        CentreSide::Right => {
            let k = m.min(l.saturating_mul(2));
            let site = gated(scope, a, &[l, 2, m], [LEFT, P, BOND], call.gate.as_ref())?;
            let (isometry, small_factor) = factor(
                scope,
                (site, &[l, 2, m], &[LEFT, P, BOND]),
                (&[l, 2, k], &[LEFT, P, NEW_BOND]),
                (&[k, m], &[NEW_BOND, BOND]),
            )?;
            let centre = scope.output(element_count(&[k, 2, r])?)?;
            contract(
                scope,
                &[
                    ContractionInput {
                        data: small_factor,
                        extents: &[k, m],
                        modes: &[NEW_BOND, BOND],
                    },
                    ContractionInput {
                        data: b,
                        extents: &[m, 2, r],
                        modes: &[BOND, Q, RIGHT],
                    },
                ],
                &[NEW_BOND, Q, RIGHT],
                centre,
            )?;
            Ok(MovedSites {
                left: isometry,
                right: centre,
                bond: k,
            })
        }
        CentreSide::Left => {
            let k = m.min(r.saturating_mul(2));
            let site = gated(scope, b, &[m, 2, r], [BOND, Q, RIGHT], call.gate.as_ref())?;
            let (isometry, small_factor) = factor(
                scope,
                (site, &[m, 2, r], &[BOND, Q, RIGHT]),
                (&[k, 2, r], &[NEW_BOND, Q, RIGHT]),
                (&[m, k], &[BOND, NEW_BOND]),
            )?;
            let centre = scope.output(element_count(&[l, 2, k])?)?;
            contract(
                scope,
                &[
                    ContractionInput {
                        data: a,
                        extents: &[l, 2, m],
                        modes: &[LEFT, P, BOND],
                    },
                    ContractionInput {
                        data: small_factor,
                        extents: &[m, k],
                        modes: &[BOND, NEW_BOND],
                    },
                ],
                &[LEFT, P, NEW_BOND],
                centre,
            )?;
            Ok(MovedSites {
                left: centre,
                right: isometry,
                bond: k,
            })
        }
    }
}

/// `site`, of `extents` and `modes`, with the one-site `gate` applied to
/// its qubit (the middle mode), in a buffer of the call; `site` itself
/// when there is no gate.
fn gated(
    scope: &mut CallScope<'_>,
    site: OpaqueHandle,
    extents: &[usize; 3],
    modes: [i32; 3],
    gate: Option<&[Complex64; 4]>,
) -> Result<OpaqueHandle, CudaAlgebraError> {
    let Some(gate) = gate else {
        return Ok(site);
    };
    let qubit_out = if modes[1] == P { P_OUT } else { Q_OUT };
    let gate = scope.upload_gate(&in_descriptor_layout(gate))?;
    let out = scope.intermediate(element_count(extents)?)?;
    contract(
        scope,
        &[
            ContractionInput {
                data: gate,
                extents: &[2, 2],
                modes: &[qubit_out, modes[1]],
            },
            ContractionInput {
                data: site,
                extents,
                modes: &modes,
            },
        ],
        &[modes[0], qubit_out, modes[2]],
        out,
    )?;
    Ok(out)
}

/// The one-site gate in a descriptor's layout, `G[S, s]` at `S + 2s`, from
/// the call's row-major `2S + s`.
fn in_descriptor_layout(gate: &[Complex64; 4]) -> [Complex64; 4] {
    std::array::from_fn(|at| gate[2 * (at & 1) + (at >> 1)])
}

/// The QR of `input` (its buffer, extents and labels) into an isometry of
/// `isometry`'s extents and labels, returned by the call, and a small
/// factor of `small_factor`'s, freed when it returns; waited for.
fn factor(
    scope: &mut CallScope<'_>,
    input: (OpaqueHandle, &[usize], &[i32]),
    isometry: (&[usize], &[i32]),
    small_factor: (&[usize], &[i32]),
) -> Result<(OpaqueHandle, OpaqueHandle), CudaAlgebraError> {
    let input = TensorOperand {
        descriptor: scope.descriptor(input.1, input.2)?,
        data: input.0,
    };
    let isometry_descriptor = scope.descriptor(isometry.0, isometry.1)?;
    let small_factor_descriptor = scope.descriptor(small_factor.0, small_factor.1)?;
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
        data: scope.output(element_count(isometry.0)?)?,
    };
    let small_factor = TensorOperand {
        descriptor: small_factor_descriptor,
        data: scope.intermediate(element_count(small_factor.0)?)?,
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
