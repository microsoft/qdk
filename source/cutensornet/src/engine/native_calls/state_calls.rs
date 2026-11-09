// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The cuTensorNet calls that hold an MPS: create a state, apply operators,
//! bound its bonds, compute it, capture it, start one from a computed MPS,
//! and read expectations of it; and the shapes and layouts they take.
//!
//! cuTensorNet's state API describes a quantum state as a network (an
//! initial MPS and the operators applied to it) and factorizes it into an
//! MPS only when asked: [`NativeCapacity`] is the chain a state may grow
//! to (the caller's capacity, as cuTensorNet's `cutensornetStateFinalizeMPS`
//! reads it), and [`MpsLayout`] the extents and strides a compute wrote,
//! which a new state reads back to start from that MPS.
//!
//! A caller keeps cuTensorNet's rules for these calls: a compute is
//! prepared since the state's last operator, a state is read only once
//! computed, and a computed MPS is captured before the next compute.

use super::{OpaqueHandle, Stream};
use crate::engine::EngineError;
use num_complex::Complex64;
use tensornet::{MpsShape, MpsShapeError};

/// The fewest sites cuTensorNet's MPS finalization accepts. NVIDIA's
/// documentation of `cutensornetStateFinalizeMPS` says: "As of cuTensorNet
/// v2.7.0, this API does not support MPS simulation when the number of
/// state modes is equal to 1".
const NATIVE_MINIMUM_SITES: usize = 2;

/// A state's capacity as cuTensorNet is asked for it
/// (`cutensornetStateFinalizeMPS`): the caller's capacity, each bond
/// lowered to what its cut can carry, a lone qubit padded to two sites.
///
/// The native shape is an [`MpsShape`]: extents only, since a capacity has
/// no layout. The `i64` copy and the pointers into it exist because the
/// library takes extents as an array of pointers, which must outlive the
/// call that reads them.
pub(crate) struct NativeCapacity {
    /// The capacity the state was prepared with: the chain the engine's
    /// callers see.
    requested: MpsShape,
    /// The chain the library computes: the capacity's, lowered, and padded
    /// to [`NATIVE_MINIMUM_SITES`].
    shape: MpsShape,
    extents: Vec<Box<[i64]>>,
    extent_pointers: Box<[*const i64]>,
}

impl NativeCapacity {
    /// The native capacity of a state of `capacity`'s qubits, with each
    /// bond the capacity's, lowered to the largest bond the cut can carry
    /// (`min(2^left, 2^right)`; `docs/engine.md`, "Numerics").
    ///
    /// The capacity's sites are qubits: `prepare` refuses any other
    /// first, with `tensornet::checks::prepare`.
    ///
    /// A capacity of one qubit gets a second, idle site, because
    /// cuTensorNet's MPS finalization needs at least two sites (see
    /// [`NATIVE_MINIMUM_SITES`]) while a lone site is a valid `MpsShape` and
    /// one-qubit programs exist. The padding is the engine's business: the
    /// state refuses the idle site as out of range, so no operator, read or
    /// sample reaches it, and it stays |0⟩, a product factor that changes no
    /// probability, expectation or norm; [`logical`](Self::logical) drops it
    /// from the realized chain.
    pub(in crate::engine) fn new(capacity: &MpsShape) -> Result<Self, EngineError> {
        let qubit_count = capacity.site_count();
        let native_count = qubit_count.max(NATIVE_MINIMUM_SITES);
        let bonds = (0..native_count - 1)
            .map(|cut| {
                // The padding's bond asks for the full extent, 2, so a
                // padded capacity is the shape of any two-qubit chain, which
                // the library already runs; `logical` drops that bond.
                let requested = if cut + 1 < qubit_count {
                    capacity
                        .bond_dim(cut)
                        .and_then(|bond| i64::try_from(bond).ok())
                        .ok_or(EngineError::ResourceSizeOverflow {
                            resource: "MPS bond",
                        })?
                } else {
                    2
                };
                native_bond_extent(native_count, cut, requested)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let qubit_count = native_count;
        let mut extents = Vec::with_capacity(qubit_count);
        for site in 0..qubit_count {
            let shape = if site == 0 {
                vec![2, bonds[0]]
            } else if site + 1 == qubit_count {
                vec![bonds[site - 1], 2]
            } else {
                vec![bonds[site - 1], 2, bonds[site]]
            };
            extents.push(shape.into_boxed_slice());
        }
        let shape = MpsShape::new(convert_layout("native capacity extent", &extents)?).map_err(
            |error| {
                match error {
                    // A capacity whose bonds are large enough to overflow
                    // is a resource limit, not a malformed request.
                    MpsShapeError::ElementCountOverflow { .. } => {
                        EngineError::ResourceSizeOverflow {
                            resource: "MPS output",
                        }
                    }
                    other => EngineError::InvalidNativeCall {
                        reason: format!("native capacity is not a valid chain: {other}"),
                    },
                }
            },
        )?;
        let extent_pointers = extents
            .iter()
            .map(|shape| shape.as_ptr())
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Ok(Self {
            requested: capacity.clone(),
            shape,
            extents,
            extent_pointers,
        })
    }

    /// The capacity the state was prepared with.
    pub(crate) fn requested(&self) -> &MpsShape {
        &self.requested
    }

    /// How many sites the engine's callers see; the library's chain may
    /// have more.
    pub(crate) fn site_count(&self) -> usize {
        self.requested.site_count()
    }

    /// The chain the library is asked for, padding included.
    pub(crate) fn native_shape(&self) -> &MpsShape {
        &self.shape
    }

    /// The chain the callers see, from the one the library `realized`: the
    /// realized chain itself, or, for a padded lone site, that site alone.
    pub(crate) fn logical(&self, realized: MpsShape) -> Result<MpsShape, EngineError> {
        if realized.site_count() == self.site_count() {
            return Ok(realized);
        }
        let physical = realized
            .physical_dim(0)
            .ok_or_else(|| invalid_native_extents("realized MPS lacks the lone site"))?;
        MpsShape::new(vec![vec![physical]]).map_err(|error| EngineError::InvalidNativeResult {
            reason: format!("realized lone site is not a valid chain: {error}"),
        })
    }

    pub(crate) fn extent_pointers(&self) -> &[*const i64] {
        &self.extent_pointers
    }
}

/// The layout of a computed MPS in its buffers: each site's extents and
/// strides, as `cutensornetStateCompute` wrote them and as
/// `cutensornetStateInitializeMPS` reads them.
#[derive(Clone)]
pub(crate) struct MpsLayout {
    pub(crate) extents: Vec<Box<[i64]>>,
    pub(crate) strides: Vec<Box<[i64]>>,
}

impl MpsLayout {
    /// Tables for a compute to fill: per site of `capacity`'s native chain,
    /// an extent and a stride table of the site's rank, zeroed.
    pub(in crate::engine) fn new(capacity: &NativeCapacity) -> Self {
        let extents = capacity
            .extents
            .iter()
            .map(|shape| vec![0; shape.len()].into_boxed_slice())
            .collect();
        let strides = capacity
            .extents
            .iter()
            .map(|shape| vec![0; shape.len()].into_boxed_slice())
            .collect();
        Self { extents, strides }
    }
}

/// A state's `f64` configuration (`cutensornetStateConfigure`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum StateF64Configuration {
    /// The fraction of a cut's squared norm a truncation may discard
    /// (`policy::DISCARDED_WEIGHT_CUTOFF`).
    SvdDiscardedWeightCutoff(f64),
}

/// A state's 32-bit configuration (`cutensornetStateConfigure`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StateU32Configuration {
    /// Kept singular values are not rescaled after truncation, so the norm
    /// is the weight actually retained; every read divides by it.
    SvdNormalizationNone,
    /// cuSOLVER's GESVD, the QR-based SVD (`policy.rs` says why).
    SvdAlgorithmGesvd,
    /// The simple-update gauge (`policy.rs` says why).
    MpsGaugeSimple,
}

/// The cuTensorNet calls on a native state (the State/MPS API), as the
/// engine makes them; cuTensorNet's own, not a backend-neutral interface.
///
/// Callers keep the handle, the stream and the native objects live and on
/// the engine's device. Registered tensor buffers must outlive the native
/// objects retaining them; results are on the host only after the stream
/// is synchronized, which is a `SessionCalls` call: every implementer
/// implements both.
pub(crate) trait StateCalls: super::memory_calls::MemoryCalls {
    fn create_state(
        &self,
        handle: OpaqueHandle,
        mode_extents: &[i64],
    ) -> Result<OpaqueHandle, EngineError>;
    fn destroy_state(&self, state: OpaqueHandle) -> Result<(), EngineError>;
    fn apply_tensor_operator(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        modes: &[i32],
        tensor: OpaqueHandle,
        unitary: bool,
    ) -> Result<(), EngineError>;
    fn finalize_mps(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        capacity: &NativeCapacity,
    ) -> Result<(), EngineError>;
    /// `cutensornetStateInitializeMPS`: the MPS in `tensors`, laid out as
    /// `layout` says (a compute's realized extents and strides), becomes the
    /// state's start. The tensors must stay allocated until the state has
    /// been computed and captured again, or destroyed.
    fn initialize_mps(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        layout: &MpsLayout,
        tensors: &[OpaqueHandle],
    ) -> Result<(), EngineError>;
    /// `cutensornetStateCaptureMPS`: the last computed MPS becomes the
    /// state's start, and the operators applied so far are deleted from the
    /// state (their ids with them); the finalize capacity and configuration
    /// stay. The next compute applies only the operators applied after it.
    fn capture_mps(&self, handle: OpaqueHandle, state: OpaqueHandle) -> Result<(), EngineError>;
    fn configure_state_f64(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        configuration: StateF64Configuration,
    ) -> Result<(), EngineError>;
    fn configure_state_u32(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        configuration: StateU32Configuration,
    ) -> Result<(), EngineError>;
    fn prepare_state(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        maximum_workspace_bytes: usize,
        workspace: OpaqueHandle,
        stream: Stream,
    ) -> Result<(), EngineError>;
    fn compute_state(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        workspace: OpaqueHandle,
        layout: &mut MpsLayout,
        outputs: &mut [OpaqueHandle],
        stream: Stream,
    ) -> Result<(), EngineError>;
    fn create_network_operator(
        &self,
        handle: OpaqueHandle,
        mode_extents: &[i64],
    ) -> Result<OpaqueHandle, EngineError>;
    fn destroy_network_operator(&self, operator: OpaqueHandle) -> Result<(), EngineError>;
    fn append_product(
        &self,
        handle: OpaqueHandle,
        operator: OpaqueHandle,
        coefficient: Complex64,
        factor_modes: &[Box<[i32]>],
        factor_tensors: &[OpaqueHandle],
    ) -> Result<(), EngineError>;
    fn create_expectation(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        operator: OpaqueHandle,
    ) -> Result<OpaqueHandle, EngineError>;
    fn destroy_expectation(&self, expectation: OpaqueHandle) -> Result<(), EngineError>;
    fn configure_expectation_hyper_samples(
        &self,
        handle: OpaqueHandle,
        expectation: OpaqueHandle,
        hyper_samples: i32,
    ) -> Result<(), EngineError>;
    fn prepare_expectation(
        &self,
        handle: OpaqueHandle,
        expectation: OpaqueHandle,
        maximum_workspace_bytes: usize,
        workspace: OpaqueHandle,
        stream: Stream,
    ) -> Result<(), EngineError>;
    fn compute_expectation(
        &self,
        handle: OpaqueHandle,
        expectation: OpaqueHandle,
        workspace: OpaqueHandle,
        stream: Stream,
    ) -> Result<(Complex64, Complex64), EngineError>;
}

/// The extent or stride tables `values` as `usize`s, for an `MpsShape` or
/// a check; a value that is not positive, or does not fit, is an invalid
/// native result named by `label`.
pub(in crate::engine) fn convert_layout(
    label: &str,
    values: &[Box<[i64]>],
) -> Result<Vec<Vec<usize>>, EngineError> {
    let mut converted = Vec::with_capacity(values.len());
    for shape in values {
        let mut converted_shape = Vec::with_capacity(shape.len());
        for value in shape {
            if *value <= 0 {
                return Err(EngineError::InvalidNativeResult {
                    reason: format!("MPS {label} is not positive: {values:?}"),
                });
            }
            converted_shape.push(usize::try_from(*value).map_err(|_| {
                EngineError::InvalidNativeResult {
                    reason: format!("MPS {label} does not fit usize: {values:?}"),
                }
            })?);
        }
        converted.push(converted_shape);
    }
    Ok(converted)
}

/// Checks a realized chain against the shape this engine asked for.
///
/// `MpsShape` has already established that the chain is well formed, so what is
/// left is a demand this engine makes on its own behalf: the library must
/// not have exceeded the capacity it was given. Fitting the capacity also
/// keeps every site a qubit, as the engine's sites are: the capacity's
/// physical extents are all two, and a chain fits it only with the same.
/// The demand does not belong to matrix product states in general.
pub(in crate::engine) fn validate_realized_chain(
    realized: &MpsShape,
    target: &MpsShape,
) -> Result<(), EngineError> {
    if !realized.fits_within(target) {
        return Err(invalid_native_extents(
            "realized chain does not fit the capacity",
        ));
    }
    Ok(())
}

fn invalid_native_extents(reason: &'static str) -> EngineError {
    EngineError::InvalidNativeResult {
        reason: reason.to_string(),
    }
}

fn native_bond_extent(
    qubit_count: usize,
    cut: usize,
    requested_bond: i64,
) -> Result<i64, EngineError> {
    if requested_bond <= 0 {
        return Err(EngineError::InvalidNativeCall {
            reason: "invalid MPS bond request".to_string(),
        });
    }
    let Some(left_mode_count) = cut.checked_add(1) else {
        return Err(EngineError::InvalidNativeCall {
            reason: "invalid MPS bond request".to_string(),
        });
    };
    let Some(right_mode_count) = qubit_count
        .checked_sub(left_mode_count)
        .filter(|count| *count > 0)
    else {
        return Err(EngineError::InvalidNativeCall {
            reason: "invalid MPS bond request".to_string(),
        });
    };
    let left = saturating_power_of_two(left_mode_count);
    let right = saturating_power_of_two(right_mode_count);
    Ok(requested_bond.min(left).min(right))
}

fn saturating_power_of_two(exponent: usize) -> i64 {
    match u32::try_from(exponent) {
        Ok(exponent) if exponent < i64::BITS - 1 => 1_i64 << exponent,
        _ => i64::MAX,
    }
}
