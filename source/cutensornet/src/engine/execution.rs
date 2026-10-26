//! The cuTensorNet State/MPS operations the engine needs, and the shape a
//! state asks the library to produce.
//!
//! [`MpsExecutionApi`] is the injection seam: `library::engine` implements
//! it with cuTensorNet, and host tests with the test double in
//! `engine::test_double`. [`MpsTarget`] turns a state's capacity, a
//! `tensornet::MpsShape`, into the extents `cutensornetStateFinalizeMPS` reads.
//! Both are ④'s (`library/simulation/mps_execution.rs` in M0), moved; the
//! target is now built from a capacity rather than from a qubit count and a
//! bond cap; and `capture_mps`, ④'s test-only full-state capture, now
//! follows every compute.

use super::{
    EngineError, OpaqueHandle, Stream,
    memory_workspace::WorkspacePreference,
    policy::{MpsGauge, SvdAlgorithm},
};
use num_complex::Complex64;
use tensornet::{MpsShape, MpsShapeError};

/// The fewest sites cuTensorNet's MPS finalization accepts
/// (`bindings/v2_13.rs:338`).
const NATIVE_MINIMUM_SITES: usize = 2;

/// The MPS shape a state asks the library to produce, and the FFI
/// scaffolding that shape needs.
///
/// The shape itself is a [`MpsShape`]: extents only, since a target describes a
/// capacity rather than a buffer and so has no layout. The `i64` copy and the
/// pointers into it exist purely because the library takes extents as an array
/// of pointers, and they must outlive the call that reads them.
pub(crate) struct MpsTarget {
    /// The capacity the state was prepared with: the chain the engine's
    /// callers see.
    capacity: MpsShape,
    /// The chain the library computes: the capacity's, lowered, and padded
    /// to [`NATIVE_MINIMUM_SITES`].
    shape: MpsShape,
    extents: Vec<Box<[i64]>>,
    extent_pointers: Box<[*const i64]>,
}

impl MpsTarget {
    /// The target of a state of `capacity`'s qubits, with each bond the
    /// capacity's, lowered to the largest bond the cut can carry
    /// (`min(2^left, 2^right)`), as ④ asked for.
    ///
    /// A capacity of one qubit gets a second, idle site, because
    /// cuTensorNet's MPS finalization needs at least two sites
    /// (`bindings/v2_13.rs:338`) while a lone site is a valid `MpsShape` and
    /// one-qubit programs exist. The padding is the engine's business: the
    /// state refuses the idle site as out of range, so no operator, read or
    /// sample reaches it, and it stays |0⟩, a product factor that changes no
    /// probability, expectation or norm; [`logical`](Self::logical) drops it
    /// from the realized chain.
    pub(super) fn from_capacity(capacity: &MpsShape) -> Result<Self, EngineError> {
        let qubit_count = capacity.site_count();
        if let Some(site) = (0..qubit_count).find(|&site| capacity.physical_dim(site) != Some(2)) {
            return Err(EngineError::InvalidRequest {
                reason: format!("site {site} of the capacity is not a qubit"),
            });
        }
        let native_count = qubit_count.max(NATIVE_MINIMUM_SITES);
        let bonds = (0..native_count - 1)
            .map(|cut| {
                // The padding's bond asks for the full extent, 2, so a
                // padded target is the shape of any two-qubit chain, which
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
                target_bond_extent(native_count, cut, requested)
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
        let shape = MpsShape::new(convert_layout("target extent", &extents)?).map_err(|error| {
            match error {
                // A bond cap large enough to overflow is a resource limit,
                // not a malformed request.
                MpsShapeError::ElementCountOverflow { .. } => EngineError::ResourceSizeOverflow {
                    resource: "MPS output",
                },
                other => EngineError::InvalidRequest {
                    reason: format!("target MPS is not a valid chain: {other}"),
                },
            }
        })?;
        let extent_pointers = extents
            .iter()
            .map(|shape| shape.as_ptr())
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Ok(Self {
            capacity: capacity.clone(),
            shape,
            extents,
            extent_pointers,
        })
    }

    /// The capacity the state was prepared with.
    pub(crate) fn capacity(&self) -> &MpsShape {
        &self.capacity
    }

    /// How many sites the engine's callers see; the library's chain may
    /// have more.
    pub(crate) fn site_count(&self) -> usize {
        self.capacity.site_count()
    }

    /// The chain the library is asked for, padding included.
    pub(crate) fn shape(&self) -> &MpsShape {
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

pub(crate) struct OutputMetadata {
    pub(crate) extents: Vec<Box<[i64]>>,
    pub(crate) strides: Vec<Box<[i64]>>,
}

impl OutputMetadata {
    pub(super) fn new(target: &MpsTarget) -> Self {
        let extents = target
            .extents
            .iter()
            .map(|shape| vec![0; shape.len()].into_boxed_slice())
            .collect();
        let strides = target
            .extents
            .iter()
            .map(|shape| vec![0; shape.len()].into_boxed_slice())
            .collect();
        Self { extents, strides }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StateF64Attribute {
    SvdAbsoluteCutoff,
    SvdRelativeCutoff,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StateU32Configuration {
    /// Kept singular values are not rescaled after truncation, so the norm
    /// is the weight actually retained; every read divides by it.
    SvdNormalizationNone,
    /// The policy's SVD algorithm.
    SvdAlgorithm(SvdAlgorithm),
    /// The policy's MPS gauge.
    MpsGauge(MpsGauge),
}

/// Injected cuTensorNet State/MPS operations, not a backend-neutral execution API.
///
/// Callers keep the context, stream and child handles live and device-compatible.
/// Registered tensor buffers must outlive the native objects retaining them;
/// asynchronous results require stream synchronization before host access.
/// Stream synchronization is `SessionApi`'s: every engine API implements
/// both, so ④'s copy of it here is left out.
pub(crate) trait MpsExecutionApi: super::memory_workspace::MemoryWorkspaceApi {
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
        target: &MpsTarget,
    ) -> Result<(), EngineError>;
    /// `cutensornetStateInitializeMPS`: the MPS in `tensors`, laid out as
    /// `layout` says (a compute's realized extents and strides), becomes the
    /// state's start. The tensors must stay allocated until the state has
    /// been computed and captured again, or destroyed.
    fn initialize_mps(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        layout: &OutputMetadata,
        tensors: &[OpaqueHandle],
    ) -> Result<(), EngineError>;
    /// `cutensornetStateCaptureMPS`: the last computed MPS becomes the
    /// state's start, and the operators applied so far are deleted from the
    /// state (their ids with them); the finalize target and configuration
    /// stay. The next compute applies only the operators applied after it.
    fn capture_mps(&self, handle: OpaqueHandle, state: OpaqueHandle) -> Result<(), EngineError>;
    fn configure_state_f64(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        attribute: StateF64Attribute,
        value: f64,
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
    fn workspace_size(
        &self,
        handle: OpaqueHandle,
        workspace: OpaqueHandle,
        preference: WorkspacePreference,
    ) -> Result<i64, EngineError> {
        use super::memory_workspace::{MemorySpace, WorkspaceKind};
        self.workspace_memory_size(
            handle,
            workspace,
            preference,
            MemorySpace::Device,
            WorkspaceKind::Scratch,
        )
    }
    fn set_workspace(
        &self,
        handle: OpaqueHandle,
        workspace: OpaqueHandle,
        allocation: OpaqueHandle,
        bytes: i64,
    ) -> Result<(), EngineError> {
        use super::memory_workspace::{MemorySpace, WorkspaceKind};
        self.set_workspace_memory(
            handle,
            workspace,
            MemorySpace::Device,
            WorkspaceKind::Scratch,
            Some(allocation),
            bytes,
        )
    }
    fn compute_state(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        workspace: OpaqueHandle,
        metadata: &mut OutputMetadata,
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

pub(super) fn convert_layout(
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
/// left are two demands this engine makes on its own behalf. The library must
/// not have exceeded the capacity it was given, and every site must be a
/// qubit, because the engine's sites are qubits. Neither belongs to matrix
/// product states in general.
pub(super) fn validate_realized_chain(
    realized: &MpsShape,
    target: &MpsShape,
) -> Result<(), EngineError> {
    if !realized.fits_within(target) {
        return Err(invalid_native_extents(
            "realized extent exceeds target capacity",
        ));
    }
    if (0..realized.site_count()).any(|site| realized.physical_dim(site) != Some(2)) {
        return Err(invalid_native_extents(
            "realized physical extent is not two",
        ));
    }
    Ok(())
}

fn invalid_native_extents(reason: &'static str) -> EngineError {
    EngineError::InvalidNativeResult {
        reason: reason.to_string(),
    }
}

fn target_bond_extent(
    qubit_count: usize,
    cut: usize,
    requested_cap: i64,
) -> Result<i64, EngineError> {
    if requested_cap <= 0 {
        return Err(EngineError::InvalidRequest {
            reason: "invalid MPS bond request".to_string(),
        });
    }
    let Some(left_mode_count) = cut.checked_add(1) else {
        return Err(EngineError::InvalidRequest {
            reason: "invalid MPS bond request".to_string(),
        });
    };
    let Some(right_mode_count) = qubit_count
        .checked_sub(left_mode_count)
        .filter(|count| *count > 0)
    else {
        return Err(EngineError::InvalidRequest {
            reason: "invalid MPS bond request".to_string(),
        });
    };
    let left = saturating_power_of_two(left_mode_count);
    let right = saturating_power_of_two(right_mode_count);
    Ok(requested_cap.min(left).min(right))
}

fn saturating_power_of_two(exponent: usize) -> i64 {
    match u32::try_from(exponent) {
        Ok(exponent) if exponent < i64::BITS - 1 => 1_i64 << exponent,
        _ => i64::MAX,
    }
}
