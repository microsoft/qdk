// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! How the operators applied to a native state become the computed MPS every
//! read uses: applied when the engine calls `apply` or `project`, computed and
//! captured when a read needs them, and restarted from the last computed MPS
//! every `RESTART_PERIOD` computes.
//!
//! - **Computed on demand.** After an operator is applied, the next read
//!   computes the state again; `finalize_mps` (the native capacity's bond at
//!   each cut) is called once, before the first compute.
//! - **Captured after each compute** (`capture_mps`): the computed MPS becomes
//!   the state's start, and the library forgets the operators applied so far,
//!   so the next compute applies only the operators applied since. Without it
//!   cuTensorNet computes from |0…0⟩ with every operator, and a read after each
//!   of a shot's `M` measurements costs `O(M·G)` for `G` gates instead of
//!   `O(G)` (H₂: `M` ≈ 1,404, `G` ≈ 48,000). A compute's output buffers are
//!   freed once the next compute has been captured, since the captured MPS is
//!   read from them until then.
//! - **Operator tensors and modes** stay allocated as long as the native state
//!   they were applied to: `cutensornetStateApplyTensorOperator` says the
//!   tensor "must stay valid for the whole lifetime of the tensor network
//!   state", and `cutensornetStateCaptureMPS` deletes the operators but says
//!   nothing of their tensors. A restart ends the state and frees them.
//! - **Restarts.** Every `RESTART_PERIOD` computes, before the next operator,
//!   the native state is replaced by a new one on a cuTensorNet handle of its
//!   own, whose start is the last computed MPS, finalized and configured as the
//!   first one was; the old state, its operator tensors and its handle are
//!   released (never the session's). Capture is documented as "equivalent to
//!   creating a `cutensornetState_t` with the previous output MPS state as the
//!   initial MPS state", so a restart changes no value; it bounds the scratch a
//!   compute asks for over a long shot (`policy.rs`).

use super::NativeState;
use crate::engine::dev_tuning::SvdAlgorithm;
use crate::engine::{
    EngineError,
    error::{combine_execution_and_cleanup, first_failure},
    native_calls::{
        OpaqueHandle,
        complex::{Complex64Abi, complex_bytes},
        state_calls::{
            MpsLayout, StateF64Configuration, StateI32Configuration, StateU32Configuration,
            convert_layout, validate_realized_chain,
        },
    },
    policy::{DISCARDED_WEIGHT_CUTOFF, RESTART_PERIOD},
};
use std::{rc::Rc, time::Instant};
use tensornet::MpsShape;

impl NativeState {
    /// The chain of the computed state, computing it first if an operator
    /// was applied since the last compute.
    pub(super) fn ensure_computed(&mut self) -> Result<MpsShape, EngineError> {
        if let Some(realized) = &self.realized {
            return Ok(realized.clone());
        }
        self.poison_on_error(|native| {
            if !native.finalized {
                let (handle, state) = (native.handle(), native.state()?);
                native
                    .device_session
                    .calls()
                    .finalize_mps(handle, state, &native.capacity)?;
                native.configure(handle, state)?;
                native.finalized = true;
            }
            let realized = native.materialize()?;
            native.realized = Some(realized.clone());
            Ok(realized)
        })
    }

    /// Computes the state into new output buffers, captures it, and returns
    /// the chain the library realized, checked against the native capacity.
    fn materialize(&mut self) -> Result<MpsShape, EngineError> {
        let device_session = Rc::clone(&self.device_session);
        let (calls, handle, stream) = (
            device_session.calls(),
            self.handle(),
            device_session.stream(),
        );
        let state = self.state()?;
        let mark = self.allocations.len();
        let mut outputs = Vec::new();
        for elements in self.capacity.native_shape().element_counts() {
            outputs.push(self.allocate_complex(elements, "MPS output")?);
        }
        let workspace = self.workspace()?;
        let prepare_started = Instant::now();
        let available = self.available_workspace_bytes()?;
        calls.prepare_state(handle, state, available, workspace, stream)?;
        let bytes = calls.workspace_size(handle, workspace)?;
        if bytes <= 0 {
            return Err(EngineError::InvalidNativeResult {
                reason: format!("the compute's workspace size is {bytes}"),
            });
        }
        self.attach_scratch(workspace, bytes, "device workspace")?;
        device_session.record_dev_time(|times| times.state_prepare.add_since(prepare_started));
        let compute_started = Instant::now();
        calls.synchronize_copies()?;
        let mut layout = MpsLayout::new(&self.capacity);
        calls.compute_state(handle, state, workspace, &mut layout, &mut outputs, stream)?;
        calls.synchronize_stream(stream)?;
        device_session.record_dev_time(|times| times.state_compute.add_since(compute_started));
        let capture_started = Instant::now();
        // The library reports the shape it produced and, separately, the
        // strides it used to write it. Only the shape is part of the state.
        let realized =
            MpsShape::new(convert_layout("extent", &layout.extents)?).map_err(|error| {
                EngineError::InvalidNativeResult {
                    reason: format!("realized MPS is not a valid chain: {error}"),
                }
            })?;
        validate_realized_chain(&realized, self.capacity.native_shape())?;
        calls.capture_mps(handle, state)?;
        self.computes_since_restart += 1;
        self.computed_centre = self.canonical_centre;
        // The state now refers to the new buffers, as its computed MPS and,
        // captured, as its start; the stream has finished with the old
        // ones, and a restart or fork whose state read them has computed
        // and captured since.
        let fresh = self.allocations[mark..].to_vec();
        let previous = std::mem::replace(&mut self.computed_buffers, fresh);
        self.computed_layout = Some(layout);
        self.allocations
            .retain(|allocation| !previous.contains(allocation));
        let released = first_failure(
            previous
                .into_iter()
                .map(|old| self.release_buffer(old))
                .collect::<Vec<_>>(),
        );
        device_session.record_dev_time(|times| times.capture.add_since(capture_started));
        released.map(|()| realized)
    }

    /// The truncation and factorization every state of the engine is
    /// computed with (`policy.rs`).
    fn configure(&self, handle: OpaqueHandle, state: OpaqueHandle) -> Result<(), EngineError> {
        let calls = self.device_session.calls();
        calls.configure_state_f64(
            handle,
            state,
            StateF64Configuration::SvdDiscardedWeightCutoff(DISCARDED_WEIGHT_CUTOFF),
        )?;
        // No normalization is the library default, pinned so the norm stays
        // the retained weight whichever default a future release picks.
        calls.configure_state_u32(handle, state, StateU32Configuration::SvdNormalizationNone)?;
        let algorithm = match self.device_session.dev_tuning().svd_algorithm {
            SvdAlgorithm::Gesvd => StateU32Configuration::SvdAlgorithmGesvd,
            SvdAlgorithm::Gesvdj => StateU32Configuration::SvdAlgorithmGesvdj,
            SvdAlgorithm::Gesvdr => StateU32Configuration::SvdAlgorithmGesvdr,
        };
        calls.configure_state_u32(handle, state, algorithm)?;
        calls.configure_state_u32(handle, state, StateU32Configuration::MpsGaugeSimple)?;
        if let Some(centre) = self.canonical_centre {
            let centre = i32::try_from(centre).map_err(|_| EngineError::ResourceSizeOverflow {
                resource: "native mode",
            })?;
            calls.configure_state_i32(
                handle,
                state,
                StateI32Configuration::MpsCanonicalCentre(centre),
            )?;
        }
        Ok(())
    }

    /// Uploads `matrix` and applies it to `modes` of the state. The matrix
    /// is copied verbatim: cuTensorNet reads the row-major `M[out][in]`
    /// layout with default strides on the modes in order, first most
    /// significant, which is `MpsEngine::apply`'s convention.
    ///
    /// If `RESTART_PERIOD` computes have passed since the last restart, the
    /// native state is restarted first.
    pub(super) fn apply_operator(
        &mut self,
        modes: Box<[i32]>,
        matrix: &[Complex64Abi],
        unitary: bool,
    ) -> Result<(), EngineError> {
        let device_session = Rc::clone(&self.device_session);
        self.restart_if_due()?;
        let apply_started = Instant::now();
        let calls = device_session.calls();
        let bytes = complex_bytes(matrix.len(), "operator tensor")?;
        let tensor = self.take_buffer(bytes)?;
        self.operator_tensors.push(tensor);
        calls.copy_to_device(tensor, matrix)?;
        self.realized = None;
        self.last_probability_of_one = None;
        calls.apply_tensor_operator(self.handle(), self.state()?, &modes, tensor, unitary)?;
        self.operator_modes.push(modes);
        device_session.record_dev_time(|times| times.apply.add_since(apply_started));
        Ok(())
    }

    /// Development only (`with_dev_wide_operators`): uploads each MPO
    /// tensor (its extents and its column-major elements), appends them to
    /// a new network operator as one open-boundary MPO on `modes`, and
    /// applies it to the state as a unitary. The operator and its tensors
    /// are kept as long as the native state, as a tensor operator's are.
    pub(super) fn apply_mpo(
        &mut self,
        modes: Box<[i32]>,
        tensors: Vec<(Box<[i64]>, Vec<Complex64Abi>)>,
    ) -> Result<(), EngineError> {
        let device_session = Rc::clone(&self.device_session);
        self.restart_if_due()?;
        let apply_started = Instant::now();
        let calls = device_session.calls();
        let mut buffers = Vec::with_capacity(tensors.len());
        let mut extents = Vec::with_capacity(tensors.len());
        for (tensor_extents, elements) in tensors {
            let bytes = complex_bytes(elements.len(), "MPO tensor")?;
            let buffer = self.take_buffer(bytes)?;
            self.operator_tensors.push(buffer);
            calls.copy_to_device(buffer, &elements)?;
            buffers.push(buffer);
            extents.push(tensor_extents);
        }
        let handle = self.handle();
        let operator = calls.create_network_operator(handle, &self.state_extents)?;
        self.mpo_operators.push(operator);
        self.realized = None;
        self.last_probability_of_one = None;
        let first = self.mpo_extents.len();
        self.mpo_extents.extend(extents);
        calls.append_mpo(
            handle,
            operator,
            &modes,
            &self.mpo_extents[first..],
            &buffers,
        )?;
        calls.apply_network_operator(handle, self.state()?, operator, true)?;
        self.operator_modes.push(modes);
        device_session.record_dev_time(|times| times.apply.add_since(apply_started));
        Ok(())
    }

    /// Restarts the native state if `RESTART_PERIOD` computes have passed
    /// since the last restart and it is computed.
    fn restart_if_due(&mut self) -> Result<(), EngineError> {
        if self.realized.is_some() && self.computes_since_restart >= RESTART_PERIOD {
            let device_session = Rc::clone(&self.device_session);
            let restart_started = Instant::now();
            self.restart()?;
            device_session.record_dev_time(|times| times.restart.add_since(restart_started));
        }
        Ok(())
    }

    /// Replaces the native state, which must have been computed and
    /// captured since its last operator, by a new one on a new handle
    /// whose start is that computed MPS, finalized and configured as the
    /// first one, with a new workspace descriptor; then releases the old
    /// state, its workspace, its operator tensors and, unless it is the
    /// session's, its handle. The new state reads the MPS from
    /// `computed_buffers`, which the next compute frees once it has
    /// captured its own.
    fn restart(&mut self) -> Result<(), EngineError> {
        let device_session = Rc::clone(&self.device_session);
        let calls = device_session.calls();
        let layout = self
            .computed_layout
            .as_ref()
            .expect("a restart follows a compute, which leaves its layout");
        let handle = calls.create_handle()?;
        let started = calls
            .create_state(handle, self.state_extents.as_ref())
            .and_then(|fresh| {
                let initialized = self
                    .start_from(handle, fresh, layout, &self.computed_buffers)
                    .and_then(|()| calls.create_workspace(handle));
                match initialized {
                    Ok(workspace) => Ok((fresh, workspace)),
                    Err(error) => {
                        combine_execution_and_cleanup(Err(error), calls.destroy_state(fresh))
                    }
                }
            });
        let (fresh, workspace) = match started {
            Ok(started) => started,
            Err(error) => {
                return combine_execution_and_cleanup(Err(error), calls.destroy_handle(handle));
            }
        };
        let mut released = Vec::new();
        if let Some(previous) = self.state.replace(fresh) {
            released.push(calls.destroy_state(previous));
        }
        if let Some(previous) = self.workspace.replace(workspace) {
            released.push(calls.destroy_workspace(previous));
        }
        for operator in std::mem::take(&mut self.mpo_operators) {
            released.push(calls.destroy_network_operator(operator));
        }
        self.mpo_extents.clear();
        self.operator_modes.clear();
        for tensor in std::mem::take(&mut self.operator_tensors) {
            released.push(self.release_buffer(tensor));
        }
        if let Some(previous) = self.own_handle.replace(handle) {
            released.push(calls.destroy_handle(previous));
        }
        self.computes_since_restart = 0;
        first_failure(released)
    }

    /// Makes the MPS in `tensors`, laid out as `layout` says, the start of
    /// the native `state` on `handle`, then finalizes and configures it as
    /// every state of the engine.
    pub(super) fn start_from(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        layout: &MpsLayout,
        tensors: &[OpaqueHandle],
    ) -> Result<(), EngineError> {
        let calls = self.device_session.calls();
        calls.initialize_mps(handle, state, layout, tensors)?;
        calls.finalize_mps(handle, state, &self.capacity)?;
        self.configure(handle, state)
    }
}
