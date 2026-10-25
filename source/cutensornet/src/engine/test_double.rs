//! A CPU test double of the native API, so the engine's host tests run the
//! cuTensorNet path without a GPU.
//!
//! It keeps what cuTensorNet would keep, by id: buffers, workspaces, states
//! with the operators applied to them, network operators, expectations and
//! samplers. A compute replays a state's operators on
//! `tensornet::reference::ExactReference`, reading each matrix from its
//! buffer at that moment, as the library reads it at compute time; reads
//! use the computed state. It is strict about the order ④ established, so a
//! wrong order fails a test instead of passing silently:
//!
//! - `finalize_mps` once per state, before any configuration or prepare;
//! - a compute needs a prepare since the last operator, and scratch
//!   attached to the workspace;
//! - an expectation or sampler reads only a state computed since its last
//!   operator.
//!
//! An expectation returns the value and the squared norm both scaled by
//! [`NORM_SCALE`], as a truncated MPS's are, so a read that skips the
//! normalization is caught. It never truncates: a realized bond is the
//! exact Schmidt rank, capped by the target's. A call can be made to fail
//! with [`NativeDouble::fail`], to test cleanup.

use super::{
    EngineError, OpaqueHandle, Stream,
    execution::{
        MpsExecutionApi, MpsTarget, OutputMetadata, StateF64Attribute, StateU32Configuration,
    },
    ffi::Complex64Abi,
    memory_workspace::{MemorySpace, MemoryWorkspaceApi, WorkspaceKind, WorkspacePreference},
    resources::SessionApi,
    sampler::SamplerApi,
};
use num_complex::Complex64;
use std::{
    collections::BTreeMap,
    ffi::c_void,
    ptr::NonNull,
    sync::{Mutex, MutexGuard},
};
use tensornet::{
    Mps, MpsEngine, SampleSeed,
    reference::{ExactReference, ReferenceState},
};

/// What an expectation's value and squared norm are both multiplied by.
pub(super) const NORM_SCALE: f64 = 0.64;

/// The bytes `workspace_memory_size` asks for.
const WORKSPACE_BYTES: i64 = 1024;

#[derive(Default)]
pub(super) struct NativeDouble {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    next_id: usize,
    objects: BTreeMap<usize, Object>,
    events: Vec<&'static str>,
    calls: BTreeMap<&'static str, usize>,
    failure: Option<(&'static str, usize)>,
    /// Every operator applied, on any state: its modes and unitary flag.
    applied: Vec<(Vec<i32>, bool)>,
}

enum Object {
    Stream,
    Handle,
    Buffer(Vec<Complex64Abi>),
    Workspace {
        scratch: Option<usize>,
    },
    State(StateDouble),
    Operator(Vec<(Complex64, Vec<(usize, usize)>)>),
    Expectation {
        state: usize,
        operator: usize,
        prepared: bool,
    },
    Sampler(SamplerDouble),
}

struct StateDouble {
    sites: usize,
    operators: Vec<(Vec<usize>, usize)>,
    /// The target's bonds, once finalized.
    bonds: Option<Vec<usize>>,
    configurations: usize,
    /// How many operators the state had at its last prepare.
    prepared_at: Option<usize>,
    /// The state of the last compute, and how many operators it had.
    computed: Option<(ReferenceState, usize)>,
}

struct SamplerDouble {
    state: usize,
    modes: Vec<usize>,
    hyper_samples: Option<i32>,
    sample_seed: Option<i32>,
    prepared: bool,
}

fn failed(operation: &'static str, message: &str) -> EngineError {
    EngineError::NativeCallFailed {
        component: "test double",
        operation,
        status: 1,
        message: message.to_string(),
    }
}

fn pointer(id: usize) -> OpaqueHandle {
    NonNull::new(std::ptr::without_provenance_mut::<c_void>(id)).unwrap_or(NonNull::dangling())
}

fn id(handle: OpaqueHandle) -> usize {
    handle.addr().get()
}

fn to_usize(value: i64) -> Result<usize, EngineError> {
    usize::try_from(value).map_err(|_| failed("conversion", "negative value"))
}

impl NativeDouble {
    pub(super) fn new() -> Self {
        Self::default()
    }

    /// Makes the `occurrence`-th call (from 1) to `operation` fail.
    pub(super) fn fail(&self, operation: &'static str, occurrence: usize) {
        self.lock().failure = Some((operation, occurrence));
    }

    pub(super) fn events(&self) -> Vec<&'static str> {
        self.lock().events.clone()
    }

    pub(super) fn count(&self, operation: &'static str) -> usize {
        self.lock().calls.get(operation).copied().unwrap_or(0)
    }

    /// Every native object not yet released.
    pub(super) fn live_objects(&self) -> usize {
        self.lock().objects.len()
    }

    /// The modes and unitary flag of every operator applied.
    pub(super) fn applied(&self) -> Vec<(Vec<i32>, bool)> {
        self.lock().applied.clone()
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Records a call to `operation` and fails it if asked to.
    fn call(&self, operation: &'static str) -> Result<MutexGuard<'_, Inner>, EngineError> {
        let mut inner = self.lock();
        inner.events.push(operation);
        let count = inner.calls.entry(operation).or_insert(0);
        *count += 1;
        let count = *count;
        if inner.failure == Some((operation, count)) {
            return Err(failed(operation, "injected failure"));
        }
        Ok(inner)
    }
}

impl Inner {
    fn create(&mut self, object: Object) -> OpaqueHandle {
        self.next_id += 1;
        let id = self.next_id * 256;
        self.objects.insert(id, object);
        pointer(id)
    }

    fn destroy(
        &mut self,
        handle: OpaqueHandle,
        operation: &'static str,
        is_kind: fn(&Object) -> bool,
    ) -> Result<(), EngineError> {
        match self.objects.get(&id(handle)) {
            Some(object) if is_kind(object) => {
                self.objects.remove(&id(handle));
                Ok(())
            }
            _ => Err(failed(operation, "no such object")),
        }
    }

    fn require(
        &self,
        handle: OpaqueHandle,
        operation: &'static str,
        is_kind: fn(&Object) -> bool,
    ) -> Result<(), EngineError> {
        match self.objects.get(&id(handle)) {
            Some(object) if is_kind(object) => Ok(()),
            _ => Err(failed(operation, "no such object")),
        }
    }

    fn buffer(&self, id: usize, operation: &'static str) -> Result<&[Complex64Abi], EngineError> {
        match self.objects.get(&id) {
            Some(Object::Buffer(buffer)) => Ok(buffer),
            _ => Err(failed(operation, "no such buffer")),
        }
    }

    fn state(
        &mut self,
        handle: OpaqueHandle,
        operation: &'static str,
    ) -> Result<&mut StateDouble, EngineError> {
        match self.objects.get_mut(&id(handle)) {
            Some(Object::State(state)) => Ok(state),
            _ => Err(failed(operation, "no such state")),
        }
    }

    /// The state `state` was last computed as, if no operator was applied
    /// since.
    fn computed(
        &self,
        state: usize,
        operation: &'static str,
    ) -> Result<&ReferenceState, EngineError> {
        match self.objects.get(&state) {
            Some(Object::State(StateDouble {
                operators,
                computed: Some((computed, at)),
                ..
            })) if *at == operators.len() => Ok(computed),
            Some(Object::State(_)) => Err(failed(operation, "the state is not computed")),
            _ => Err(failed(operation, "no such state")),
        }
    }

    /// A copy of `state` with the matrix in buffer `tensor` applied to
    /// `sites`.
    fn apply(
        &self,
        state: &mut ReferenceState,
        sites: &[usize],
        tensor: usize,
        operation: &'static str,
    ) -> Result<(), EngineError> {
        let elements = 1 << (2 * sites.len());
        let matrix: Vec<Complex64> = self
            .buffer(tensor, operation)?
            .get(..elements)
            .ok_or_else(|| failed(operation, "the operator buffer is too small"))?
            .iter()
            .copied()
            .map(Complex64::from)
            .collect();
        ExactReference
            .apply(state, sites, &matrix)
            .map_err(|error| failed(operation, &error.to_string()))
    }
}

fn inner_product(bra: &ReferenceState, ket: &ReferenceState) -> Complex64 {
    (0..1_usize << bra.sites())
        .map(|i| bra.amplitude(i).conj() * ket.amplitude(i))
        .sum()
}

impl SessionApi for NativeDouble {
    fn device_count(&self) -> Result<i32, EngineError> {
        drop(self.call("device_count")?);
        Ok(1)
    }

    fn set_device(&self, ordinal: i32) -> Result<(), EngineError> {
        drop(self.call("set_device")?);
        if ordinal == 0 {
            Ok(())
        } else {
            Err(failed("set_device", "no such device"))
        }
    }

    fn create_stream(&self) -> Result<Stream, EngineError> {
        Ok(self.call("create_stream")?.create(Object::Stream))
    }

    fn synchronize_stream(&self, stream: Stream) -> Result<(), EngineError> {
        self.call("synchronize_stream")?
            .require(stream, "synchronize_stream", |o| {
                matches!(o, Object::Stream)
            })
    }

    fn destroy_stream(&self, stream: Stream) -> Result<(), EngineError> {
        self.call("destroy_stream")?
            .destroy(stream, "destroy_stream", |o| matches!(o, Object::Stream))
    }

    fn create_handle(&self) -> Result<OpaqueHandle, EngineError> {
        Ok(self.call("create_handle")?.create(Object::Handle))
    }

    fn destroy_handle(&self, handle: OpaqueHandle) -> Result<(), EngineError> {
        self.call("destroy_handle")?
            .destroy(handle, "destroy_handle", |o| matches!(o, Object::Handle))
    }
}

impl MemoryWorkspaceApi for NativeDouble {
    fn memory_info(&self) -> Result<(usize, usize), EngineError> {
        drop(self.call("memory_info")?);
        Ok((1 << 40, 1 << 40))
    }

    fn allocate(&self, bytes: usize) -> Result<OpaqueHandle, EngineError> {
        let elements = bytes.div_ceil(size_of::<Complex64Abi>());
        Ok(self
            .call("allocate")?
            .create(Object::Buffer(vec![Complex64Abi::default(); elements])))
    }

    fn free(&self, allocation: OpaqueHandle) -> Result<(), EngineError> {
        self.call("free")?
            .destroy(allocation, "free", |o| matches!(o, Object::Buffer(_)))
    }

    fn copy_to_device(
        &self,
        destination: OpaqueHandle,
        source: &[Complex64Abi],
    ) -> Result<(), EngineError> {
        let mut inner = self.call("copy_to_device")?;
        match inner.objects.get_mut(&id(destination)) {
            Some(Object::Buffer(buffer)) if buffer.len() >= source.len() => {
                buffer[..source.len()].copy_from_slice(source);
                Ok(())
            }
            _ => Err(failed("copy_to_device", "no such buffer of that size")),
        }
    }

    fn create_workspace(&self, _handle: OpaqueHandle) -> Result<OpaqueHandle, EngineError> {
        Ok(self
            .call("create_workspace")?
            .create(Object::Workspace { scratch: None }))
    }

    fn destroy_workspace(&self, workspace: OpaqueHandle) -> Result<(), EngineError> {
        self.call("destroy_workspace")?
            .destroy(workspace, "destroy_workspace", |o| {
                matches!(o, Object::Workspace { .. })
            })
    }

    fn workspace_memory_size(
        &self,
        _handle: OpaqueHandle,
        workspace: OpaqueHandle,
        _preference: WorkspacePreference,
        _space: MemorySpace,
        _kind: WorkspaceKind,
    ) -> Result<i64, EngineError> {
        self.call("workspace_size")?
            .require(workspace, "workspace_size", |o| {
                matches!(o, Object::Workspace { .. })
            })?;
        Ok(WORKSPACE_BYTES)
    }

    fn set_workspace_memory(
        &self,
        _handle: OpaqueHandle,
        workspace: OpaqueHandle,
        _space: MemorySpace,
        _kind: WorkspaceKind,
        allocation: Option<OpaqueHandle>,
        bytes: i64,
    ) -> Result<(), EngineError> {
        let mut inner = self.call("set_workspace")?;
        let allocation = allocation.map(id);
        let size = match allocation.map(|allocation| inner.buffer(allocation, "set_workspace")) {
            Some(buffer) => size_of_val(buffer?),
            None => 0,
        };
        if size < to_usize(bytes)? {
            return Err(failed(
                "set_workspace",
                "the scratch is smaller than its size",
            ));
        }
        match inner.objects.get_mut(&id(workspace)) {
            Some(Object::Workspace { scratch }) => {
                *scratch = allocation;
                Ok(())
            }
            _ => Err(failed("set_workspace", "no such workspace")),
        }
    }
}

impl MpsExecutionApi for NativeDouble {
    fn create_state(
        &self,
        _handle: OpaqueHandle,
        mode_extents: &[i64],
    ) -> Result<OpaqueHandle, EngineError> {
        let mut inner = self.call("create_state")?;
        if mode_extents.iter().any(|&extent| extent != 2) {
            return Err(failed("create_state", "a mode is not a qubit"));
        }
        Ok(inner.create(Object::State(StateDouble {
            sites: mode_extents.len(),
            operators: Vec::new(),
            bonds: None,
            configurations: 0,
            prepared_at: None,
            computed: None,
        })))
    }

    fn destroy_state(&self, state: OpaqueHandle) -> Result<(), EngineError> {
        self.call("destroy_state")?
            .destroy(state, "destroy_state", |o| matches!(o, Object::State(_)))
    }

    fn apply_tensor_operator(
        &self,
        _handle: OpaqueHandle,
        state: OpaqueHandle,
        modes: &[i32],
        tensor: OpaqueHandle,
        unitary: bool,
    ) -> Result<(), EngineError> {
        const OPERATION: &str = "apply_tensor_operator";
        let mut inner = self.call(OPERATION)?;
        inner.buffer(id(tensor), OPERATION)?;
        let sites = modes
            .iter()
            .map(|&mode| usize::try_from(mode).map_err(|_| failed(OPERATION, "negative mode")))
            .collect::<Result<Vec<_>, _>>()?;
        inner.applied.push((modes.to_vec(), unitary));
        let target = inner.state(state, OPERATION)?;
        if sites.iter().any(|&site| site >= target.sites) {
            return Err(failed(OPERATION, "a mode outside the state"));
        }
        target.operators.push((sites, id(tensor)));
        Ok(())
    }

    fn finalize_mps(
        &self,
        _handle: OpaqueHandle,
        state: OpaqueHandle,
        target: &MpsTarget,
    ) -> Result<(), EngineError> {
        let mut inner = self.call("finalize_mps")?;
        let state = inner.state(state, "finalize_mps")?;
        if state.bonds.is_some() {
            return Err(failed("finalize_mps", "the state is already finalized"));
        }
        let shape = target.shape();
        if shape.site_count() != state.sites {
            return Err(failed("finalize_mps", "the target has another site count"));
        }
        state.bonds = Some(
            (0..state.sites - 1)
                .map(|cut| shape.bond_dim(cut).unwrap_or(1))
                .collect(),
        );
        Ok(())
    }

    fn configure_state_f64(
        &self,
        _handle: OpaqueHandle,
        state: OpaqueHandle,
        _attribute: StateF64Attribute,
        value: f64,
    ) -> Result<(), EngineError> {
        let mut inner = self.call("configure_state_f64")?;
        let state = inner.state(state, "configure_state_f64")?;
        if state.bonds.is_none() || !value.is_finite() {
            return Err(failed("configure_state_f64", "configured before finalize"));
        }
        state.configurations += 1;
        Ok(())
    }

    fn configure_state_u32(
        &self,
        _handle: OpaqueHandle,
        state: OpaqueHandle,
        _configuration: StateU32Configuration,
    ) -> Result<(), EngineError> {
        let mut inner = self.call("configure_state_u32")?;
        let state = inner.state(state, "configure_state_u32")?;
        if state.bonds.is_none() {
            return Err(failed("configure_state_u32", "configured before finalize"));
        }
        state.configurations += 1;
        Ok(())
    }

    fn prepare_state(
        &self,
        _handle: OpaqueHandle,
        state: OpaqueHandle,
        _maximum_workspace_bytes: usize,
        workspace: OpaqueHandle,
        _stream: Stream,
    ) -> Result<(), EngineError> {
        let mut inner = self.call("prepare_state")?;
        inner.require(workspace, "prepare_state", |o| {
            matches!(o, Object::Workspace { .. })
        })?;
        let state = inner.state(state, "prepare_state")?;
        if state.bonds.is_none() {
            return Err(failed("prepare_state", "prepared before finalize"));
        }
        state.prepared_at = Some(state.operators.len());
        Ok(())
    }

    fn compute_state(
        &self,
        _handle: OpaqueHandle,
        state: OpaqueHandle,
        workspace: OpaqueHandle,
        metadata: &mut OutputMetadata,
        outputs: &mut [OpaqueHandle],
        _stream: Stream,
    ) -> Result<(), EngineError> {
        const OPERATION: &str = "compute_state";
        let mut inner = self.call(OPERATION)?;
        match inner.objects.get(&id(workspace)) {
            Some(Object::Workspace {
                scratch: Some(scratch),
            }) => {
                inner.buffer(*scratch, OPERATION)?;
            }
            _ => return Err(failed(OPERATION, "no scratch attached")),
        }
        for output in outputs.iter() {
            inner.buffer(id(*output), OPERATION)?;
        }
        let (sites, operators, bonds) = {
            let state = inner.state(state, OPERATION)?;
            if state.prepared_at != Some(state.operators.len()) {
                return Err(failed(OPERATION, "not prepared since the last operator"));
            }
            let Some(bonds) = state.bonds.clone() else {
                return Err(failed(OPERATION, "not finalized"));
            };
            (state.sites, state.operators.clone(), bonds)
        };
        if outputs.len() != sites || metadata.extents.len() != sites {
            return Err(failed(OPERATION, "one output per site is needed"));
        }
        let capacity = tensornet::conformance::exact_capacity(sites)
            .map_err(|error| failed(OPERATION, &error.to_string()))?;
        let mut computed = ExactReference
            .prepare(&capacity)
            .map_err(|error| failed(OPERATION, &error.to_string()))?;
        for (sites, tensor) in &operators {
            inner.apply(&mut computed, sites, *tensor, OPERATION)?;
        }
        let exact = ExactReference
            .shape(&computed)
            .map_err(|error| failed(OPERATION, &error.to_string()))?;
        let bond = |cut: usize| -> Result<i64, EngineError> {
            let realized = exact.bond_dim(cut).unwrap_or(1).min(bonds[cut]);
            i64::try_from(realized).map_err(|_| failed(OPERATION, "bond overflow"))
        };
        for site in 0..sites {
            let extents = if site == 0 {
                vec![2, bond(0)?]
            } else if site + 1 == sites {
                vec![bond(site - 1)?, 2]
            } else {
                vec![bond(site - 1)?, 2, bond(site)?]
            };
            if metadata.extents[site].len() != extents.len()
                || metadata.strides[site].len() != extents.len()
            {
                return Err(failed(OPERATION, "metadata of the wrong rank"));
            }
            let mut stride = 1;
            for (axis, &extent) in extents.iter().enumerate() {
                metadata.strides[site][axis] = stride;
                stride *= extent;
            }
            metadata.extents[site].copy_from_slice(&extents);
        }
        let at = operators.len();
        inner.state(state, OPERATION)?.computed = Some((computed, at));
        Ok(())
    }

    fn create_network_operator(
        &self,
        _handle: OpaqueHandle,
        _mode_extents: &[i64],
    ) -> Result<OpaqueHandle, EngineError> {
        Ok(self
            .call("create_network_operator")?
            .create(Object::Operator(Vec::new())))
    }

    fn destroy_network_operator(&self, operator: OpaqueHandle) -> Result<(), EngineError> {
        self.call("destroy_network_operator")?
            .destroy(operator, "destroy_network_operator", |o| {
                matches!(o, Object::Operator(_))
            })
    }

    fn append_product(
        &self,
        _handle: OpaqueHandle,
        operator: OpaqueHandle,
        coefficient: Complex64,
        factor_modes: &[Box<[i32]>],
        factor_tensors: &[OpaqueHandle],
    ) -> Result<(), EngineError> {
        const OPERATION: &str = "append_product";
        let mut inner = self.call(OPERATION)?;
        if factor_modes.len() != factor_tensors.len() {
            return Err(failed(OPERATION, "modes and tensors differ in number"));
        }
        let mut factors = Vec::with_capacity(factor_modes.len());
        for (modes, tensor) in factor_modes.iter().zip(factor_tensors) {
            inner.buffer(id(*tensor), OPERATION)?;
            let [mode] = **modes else {
                return Err(failed(OPERATION, "the double takes one-mode factors"));
            };
            let mode = usize::try_from(mode).map_err(|_| failed(OPERATION, "negative mode"))?;
            factors.push((mode, id(*tensor)));
        }
        match inner.objects.get_mut(&id(operator)) {
            Some(Object::Operator(products)) => {
                products.push((coefficient, factors));
                Ok(())
            }
            _ => Err(failed(OPERATION, "no such operator")),
        }
    }

    fn create_expectation(
        &self,
        _handle: OpaqueHandle,
        state: OpaqueHandle,
        operator: OpaqueHandle,
    ) -> Result<OpaqueHandle, EngineError> {
        let mut inner = self.call("create_expectation")?;
        inner.require(state, "create_expectation", |o| {
            matches!(o, Object::State(_))
        })?;
        inner.require(operator, "create_expectation", |o| {
            matches!(o, Object::Operator(_))
        })?;
        Ok(inner.create(Object::Expectation {
            state: id(state),
            operator: id(operator),
            prepared: false,
        }))
    }

    fn destroy_expectation(&self, expectation: OpaqueHandle) -> Result<(), EngineError> {
        self.call("destroy_expectation")?
            .destroy(expectation, "destroy_expectation", |o| {
                matches!(o, Object::Expectation { .. })
            })
    }

    fn configure_expectation_hyper_samples(
        &self,
        _handle: OpaqueHandle,
        expectation: OpaqueHandle,
        hyper_samples: i32,
    ) -> Result<(), EngineError> {
        let inner = self.call("configure_expectation_hyper_samples")?;
        inner.require(expectation, "configure_expectation_hyper_samples", |o| {
            matches!(o, Object::Expectation { .. })
        })?;
        if hyper_samples > 0 {
            Ok(())
        } else {
            Err(failed(
                "configure_expectation_hyper_samples",
                "not positive",
            ))
        }
    }

    fn prepare_expectation(
        &self,
        _handle: OpaqueHandle,
        expectation: OpaqueHandle,
        _maximum_workspace_bytes: usize,
        workspace: OpaqueHandle,
        _stream: Stream,
    ) -> Result<(), EngineError> {
        let mut inner = self.call("prepare_expectation")?;
        inner.require(workspace, "prepare_expectation", |o| {
            matches!(o, Object::Workspace { .. })
        })?;
        match inner.objects.get_mut(&id(expectation)) {
            Some(Object::Expectation { prepared, .. }) => {
                *prepared = true;
                Ok(())
            }
            _ => Err(failed("prepare_expectation", "no such expectation")),
        }
    }

    fn compute_expectation(
        &self,
        _handle: OpaqueHandle,
        expectation: OpaqueHandle,
        _workspace: OpaqueHandle,
        _stream: Stream,
    ) -> Result<(Complex64, Complex64), EngineError> {
        const OPERATION: &str = "compute_expectation";
        let inner = self.call(OPERATION)?;
        let Some(Object::Expectation {
            state,
            operator,
            prepared: true,
        }) = inner.objects.get(&id(expectation))
        else {
            return Err(failed(OPERATION, "no prepared expectation"));
        };
        let Some(Object::Operator(products)) = inner.objects.get(operator) else {
            return Err(failed(OPERATION, "no such operator"));
        };
        let computed = inner.computed(*state, OPERATION)?;
        let mut value = Complex64::new(0.0, 0.0);
        for (coefficient, factors) in products {
            let mut ket = computed.clone();
            for &(mode, tensor) in factors {
                inner.apply(&mut ket, &[mode], tensor, OPERATION)?;
            }
            value += coefficient * inner_product(computed, &ket);
        }
        let squared_norm = inner_product(computed, computed);
        Ok((value * NORM_SCALE, squared_norm * NORM_SCALE))
    }
}

impl SamplerApi for NativeDouble {
    fn create_sampler(
        &self,
        _handle: OpaqueHandle,
        state: OpaqueHandle,
        modes_to_sample: &[i32],
    ) -> Result<OpaqueHandle, EngineError> {
        let mut inner = self.call("create_sampler")?;
        inner.require(state, "create_sampler", |o| matches!(o, Object::State(_)))?;
        let modes = modes_to_sample
            .iter()
            .map(|&mode| {
                usize::try_from(mode).map_err(|_| failed("create_sampler", "negative mode"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(inner.create(Object::Sampler(SamplerDouble {
            state: id(state),
            modes,
            hyper_samples: None,
            sample_seed: None,
            prepared: false,
        })))
    }

    fn destroy_sampler(&self, sampler: OpaqueHandle) -> Result<(), EngineError> {
        self.call("destroy_sampler")?
            .destroy(sampler, "destroy_sampler", |o| {
                matches!(o, Object::Sampler(_))
            })
    }

    fn configure_sampler_hyper_samples(
        &self,
        _handle: OpaqueHandle,
        sampler: OpaqueHandle,
        hyper_samples: i32,
    ) -> Result<(), EngineError> {
        let mut inner = self.call("configure_sampler_hyper_samples")?;
        match inner.objects.get_mut(&id(sampler)) {
            Some(Object::Sampler(sampler)) => {
                sampler.hyper_samples = Some(hyper_samples);
                Ok(())
            }
            _ => Err(failed("configure_sampler_hyper_samples", "no such sampler")),
        }
    }

    fn configure_sampler_path_seed(
        &self,
        _handle: OpaqueHandle,
        sampler: OpaqueHandle,
        _seed: i32,
    ) -> Result<(), EngineError> {
        self.call("configure_sampler_path_seed")?.require(
            sampler,
            "configure_sampler_path_seed",
            |o| matches!(o, Object::Sampler(_)),
        )
    }

    fn prepare_sampler(
        &self,
        _handle: OpaqueHandle,
        sampler: OpaqueHandle,
        _maximum_workspace_bytes: usize,
        workspace: OpaqueHandle,
        _stream: Stream,
    ) -> Result<(), EngineError> {
        let mut inner = self.call("prepare_sampler")?;
        inner.require(workspace, "prepare_sampler", |o| {
            matches!(o, Object::Workspace { .. })
        })?;
        match inner.objects.get_mut(&id(sampler)) {
            Some(Object::Sampler(sampler)) if sampler.hyper_samples.is_some() => {
                sampler.prepared = true;
                Ok(())
            }
            _ => Err(failed("prepare_sampler", "no configured sampler")),
        }
    }

    fn configure_sampler_sample_seed(
        &self,
        _handle: OpaqueHandle,
        sampler: OpaqueHandle,
        seed: i32,
    ) -> Result<(), EngineError> {
        let mut inner = self.call("configure_sampler_sample_seed")?;
        match inner.objects.get_mut(&id(sampler)) {
            Some(Object::Sampler(sampler)) => {
                sampler.sample_seed = Some(seed);
                Ok(())
            }
            _ => Err(failed("configure_sampler_sample_seed", "no such sampler")),
        }
    }

    fn sample(
        &self,
        _handle: OpaqueHandle,
        sampler: OpaqueHandle,
        shots: i64,
        _workspace: OpaqueHandle,
        output: &mut [i64],
        _stream: Stream,
    ) -> Result<(), EngineError> {
        const OPERATION: &str = "sample";
        let inner = self.call(OPERATION)?;
        let Some(Object::Sampler(SamplerDouble {
            state,
            modes,
            sample_seed: Some(seed),
            prepared: true,
            ..
        })) = inner.objects.get(&id(sampler))
        else {
            return Err(failed(OPERATION, "no prepared, seeded sampler"));
        };
        let shots = to_usize(shots)?;
        if output.len() != shots * modes.len() {
            return Err(failed(OPERATION, "the output is not shots × modes"));
        }
        let seed = u64::try_from(*seed)
            .ok()
            .and_then(|seed| SampleSeed::try_from(seed).ok())
            .ok_or_else(|| failed(OPERATION, "the seed is not positive"))?;
        let computed = inner.computed(*state, OPERATION)?;
        let values = ExactReference
            .sample(computed, modes, shots, seed)
            .map_err(|error| failed(OPERATION, &error.to_string()))?;
        for (out, value) in output.iter_mut().zip(values) {
            *out = i64::from(value);
        }
        Ok(())
    }
}

/// The capacity of `sites` qubits with every bond `bond`.
pub(super) fn uniform_capacity(sites: usize, bond: usize) -> Mps {
    let shape = (0..sites)
        .map(|site| match site {
            0 => vec![2, bond],
            _ if site + 1 == sites => vec![bond, 2],
            _ => vec![bond, 2, bond],
        })
        .collect();
    Mps::new(shape).unwrap_or_else(|error| panic!("a valid capacity: {error}"))
}
