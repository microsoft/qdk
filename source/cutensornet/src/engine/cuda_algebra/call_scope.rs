// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`CallScope`]: what one call of a [`CudaAlgebra`](super::CudaAlgebra)
//! creates and lends, released before the call returns.
//!
//! A gate split or a move of the centre creates tensor descriptors, an SVD
//! configuration and report, and intermediate buffers, and allocates its
//! outputs. The scope records each as it is made, so every exit of the
//! call, a failure included, releases them in one place
//! ([`CallScope::finish`]). The outputs survive only a call that succeeds.
//!
//! A failed call may leave device work pending, which may still write the
//! call's buffers. `finish` waits for it before releasing anything; a
//! failed wait poisons the algebra (`algebra_session.rs`).
//!
//! Every method leaves its input tensors unchanged, as `SiteAlgebra`'s
//! contract asks. It writes only the buffers it allocates and the
//! algebra's own (its scratch, the gate's buffer). Only a faulty allocator
//! returns an input's buffer as a new one. So the scope compares each
//! buffer allocated during the call (an output, an intermediate, a grown
//! scratch or singular-values buffer) with the inputs' buffers, and refuses
//! the call on a match. The input keeps its buffer, which the call does
//! not free.

use super::algebra_session::{AlgebraSession, NativeObject, Retired};
use super::device_session::DeviceSession;
use super::device_tensor::DeviceTensor;
use super::extents::native_extents;
use super::resources::AlgebraResources;
use super::tensor_layout::TensorLayout;
use crate::engine::cuda_algebra_error::{
    CudaAlgebraError, combine_execution_and_cleanup, first_failure,
};
use crate::engine::native_calls::complex::{Complex64Abi, complex_bytes};
use crate::engine::native_calls::tensor_calls::{SvdSetting, TensorModes};
use crate::engine::native_calls::{
    NativeCalls,
    memory_calls::{DeviceBuffer, WorkspaceDescriptor},
    session_calls::{CuTensorNetHandle, CudaStream},
    tensor_calls::{TensorDescriptor, TensorSvdConfig, TensorSvdInfo},
};
use num_complex::Complex64;

/// One call of the algebra: the session it shares with its tensors, the
/// resources the algebra holds across calls, the tensors it reads, and the
/// native objects and buffers this call made.
pub(super) struct CallScope<'a> {
    /// The algebra's session, shared with its tensors: the device, stream
    /// and cuTensorNet handle every step uses, and the poison a failed wait
    /// for the device sets (`poison`).
    session: &'a AlgebraSession,
    /// What the algebra keeps across calls (its workspace, its scratch, its
    /// buffers, its prepared networks), which the call may grow or add to.
    pub(super) resources: &'a mut AlgebraResources,
    /// The tensors the call reads, which it leaves unchanged.
    inputs: &'a [&'a DeviceTensor],
    /// The call's own objects and intermediate buffers, released in the
    /// reverse order of their creation.
    owned: Vec<NativeObject>,
    /// The buffers the call returns, freed only if it fails.
    outputs: Vec<DeviceBuffer>,
    /// Whether the call copied, so that a failure also waits for the
    /// copies.
    copied: bool,
}

impl<'a> CallScope<'a> {
    /// The scope of a call that reads `inputs`, before it makes anything.
    /// Every method of the algebra opens one, so that what it makes is
    /// released in one place.
    pub(super) fn new(
        session: &'a AlgebraSession,
        resources: &'a mut AlgebraResources,
        inputs: &'a [&'a DeviceTensor],
    ) -> Self {
        Self {
            session,
            resources,
            inputs,
            owned: Vec::new(),
            outputs: Vec::new(),
            copied: false,
        }
    }

    /// The device, stream and cuTensorNet handle every call shares.
    fn device_session(&self) -> &'a DeviceSession {
        self.session.device_session()
    }

    /// The native calls the algebra was opened on, which every step of a
    /// call goes through.
    pub(super) fn calls(&self) -> &'a dyn NativeCalls {
        self.device_session().calls()
    }

    /// The algebra's cuTensorNet handle, which owns every descriptor,
    /// network and SVD object a call makes.
    pub(super) fn handle(&self) -> CuTensorNetHandle {
        self.device_session().handle()
    }

    /// The algebra's stream, which every call's device work is queued on.
    pub(super) fn stream(&self) -> CudaStream {
        self.device_session().stream()
    }

    /// The algebra's workspace descriptor.
    pub(super) fn workspace(&self) -> WorkspaceDescriptor {
        self.resources
            .workspace
            .expect("a live algebra holds its workspace")
    }

    /// Lends the workspace, sized for the call about to run, the memory it
    /// asks for.
    pub(super) fn lend_workspace(&mut self) -> Result<(), CudaAlgebraError> {
        let workspace = self.workspace();
        let device_session = self.device_session();
        let inputs = self.inputs;
        self.resources
            .scratch
            .lend(device_session, workspace, &|buffer| {
                not_an_input_buffer(inputs, buffer)
            })
    }

    /// Waits for the work queued on the algebra's stream.
    pub(super) fn synchronize(&self) -> Result<(), CudaAlgebraError> {
        self.calls().synchronize_stream(self.stream())
    }

    /// A tensor descriptor of `layout`, destroyed when the call returns.
    pub(super) fn descriptor<const RANK: usize>(
        &mut self,
        layout: &TensorLayout<RANK>,
    ) -> Result<TensorDescriptor, CudaAlgebraError> {
        let extents = native_extents(&layout.extents)?;
        let descriptor = self.calls().create_tensor_descriptor(
            self.handle(),
            TensorModes {
                extents: &extents,
                modes: &layout.modes,
            },
        )?;
        self.owned.push(NativeObject::Descriptor(descriptor));
        Ok(descriptor)
    }

    /// An SVD configuration of `settings`, destroyed when the call returns.
    pub(super) fn svd_config(
        &mut self,
        settings: &[SvdSetting],
    ) -> Result<TensorSvdConfig, CudaAlgebraError> {
        let (calls, handle) = (self.calls(), self.handle());
        let config = calls.create_svd_config(handle)?;
        self.owned.push(NativeObject::SvdConfig(config));
        for &setting in settings {
            calls.configure_svd(handle, config, setting)?;
        }
        Ok(config)
    }

    /// An SVD report, destroyed when the call returns.
    pub(super) fn svd_info(&mut self) -> Result<TensorSvdInfo, CudaAlgebraError> {
        let info = self.calls().create_svd_info(self.handle())?;
        self.owned.push(NativeObject::SvdInfo(info));
        Ok(info)
    }

    /// A buffer of `elements` complex doubles, freed when the call returns.
    pub(super) fn intermediate(
        &mut self,
        elements: usize,
    ) -> Result<DeviceBuffer, CudaAlgebraError> {
        let buffer = self.allocate(elements)?;
        self.owned.push(NativeObject::Buffer(buffer));
        Ok(buffer)
    }

    /// A buffer of `elements` complex doubles that the call returns, freed
    /// only if it fails.
    pub(super) fn output(&mut self, elements: usize) -> Result<DeviceBuffer, CudaAlgebraError> {
        let buffer = self.allocate(elements)?;
        self.outputs.push(buffer);
        Ok(buffer)
    }

    /// Copies `values` into `destination`, and waits for the copy.
    pub(super) fn upload_to(
        &mut self,
        destination: DeviceBuffer,
        values: &[Complex64],
    ) -> Result<(), CudaAlgebraError> {
        let values: Vec<Complex64Abi> = values.iter().copied().map(Complex64Abi::from).collect();
        self.copied = true;
        self.calls().copy_to_device(destination, &values)?;
        self.calls().synchronize_copies()
    }

    /// Copies `bytes` from `source` into `destination` on the device, and
    /// waits for the copy.
    pub(super) fn copy_on_device(
        &mut self,
        destination: DeviceBuffer,
        source: DeviceBuffer,
        bytes: usize,
    ) -> Result<(), CudaAlgebraError> {
        self.copied = true;
        self.calls().copy_on_device(destination, source, bytes)?;
        self.calls().synchronize_copies()
    }

    /// Copies `gate`, in a descriptor's layout, into the algebra's gate
    /// buffer, waits for the copy, and returns the buffer.
    pub(super) fn upload_gate(
        &mut self,
        gate: &[Complex64],
    ) -> Result<DeviceBuffer, CudaAlgebraError> {
        let buffer = self
            .resources
            .gate
            .expect("a live algebra holds its gate buffer");
        self.upload_to(buffer, gate)?;
        Ok(buffer)
    }

    /// The algebra's buffer for a gate split's singular values, grown to
    /// hold `count` doubles if it is smaller. No device work may be
    /// pending: a growth frees the smaller buffer, and a failed free is
    /// returned, not retried.
    pub(super) fn singular_values(
        &mut self,
        count: usize,
    ) -> Result<DeviceBuffer, CudaAlgebraError> {
        let elements = count.div_ceil(2);
        match self.resources.singular_values {
            Some((buffer, held)) if held >= elements => Ok(buffer),
            previous => self.grow_singular_values(elements, previous.map(|(old, _)| old)),
        }
    }

    /// Grows the algebra's singular-values buffer to `elements` complex
    /// doubles: allocates the new buffer, keeps it in place of `previous`,
    /// then frees `previous`. Keeping the new one first means a failed free
    /// leaks only the old buffer, never the one the algebra holds.
    fn grow_singular_values(
        &mut self,
        elements: usize,
        previous: Option<DeviceBuffer>,
    ) -> Result<DeviceBuffer, CudaAlgebraError> {
        let grown = not_an_input_buffer(
            self.inputs,
            self.calls()
                .allocate(complex_bytes(elements, "singular values")?)?,
        )?;
        self.resources.singular_values = Some((grown, elements));
        match previous {
            Some(old) => self.calls().free(old).map(|()| grown),
            None => Ok(grown),
        }
    }

    /// Ends the call with `result`. If the call failed, it first waits for
    /// the device. It then releases the call's own objects and buffers, and
    /// its outputs unless the call succeeded. A failed release fails the
    /// call and leaks its object, without poisoning the algebra. A failed
    /// wait fails the call too, retires the call's objects and buffers, and
    /// poisons the algebra. Both failures are kept.
    ///
    /// ```text
    ///  the call   its wait     its objects        its outputs
    ///  succeeded  not needed   released, newest   returned (freed if a
    ///                          first              release failed)
    ///  failed     succeeded    released           freed
    ///  failed     failed       retired            retired; the algebra
    ///                                             is poisoned
    /// ```
    pub(super) fn finish<T>(
        self,
        result: Result<T, CudaAlgebraError>,
    ) -> Result<T, CudaAlgebraError> {
        if result.is_err()
            && let Err(error) = self.wait_for_the_device()
        {
            return self.poison(result, error);
        }
        let calls = self.calls();
        let released = first_failure(
            self.owned
                .into_iter()
                .rev()
                .map(|owned| owned.release(calls)),
        );
        match combine_execution_and_cleanup(result, released) {
            Ok(value) => Ok(value),
            Err(error) => {
                let freed =
                    first_failure(self.outputs.into_iter().map(|output| calls.free(output)));
                combine_execution_and_cleanup(Err(error), freed)
            }
        }
    }

    /// Waits for the work the call queued on the stream, and for its
    /// copies if it made any. A failed call needs it: its work may still
    /// write the buffers `finish` is about to free.
    fn wait_for_the_device(&self) -> Result<(), CudaAlgebraError> {
        let stream = self.synchronize();
        let copies = self.copied.then(|| self.calls().synchronize_copies());
        first_failure(std::iter::once(stream).chain(copies))
    }

    /// Ends a failed call whose wait for the device failed with `error`:
    /// the device may still use the call's objects and buffers, so they
    /// are retired, unreleased, and the algebra poisoned
    /// (`algebra_session.rs`). Returns the call's failure and `error`.
    fn poison<T>(
        self,
        result: Result<T, CudaAlgebraError>,
        error: CudaAlgebraError,
    ) -> Result<T, CudaAlgebraError> {
        let reason = format!(
            "a failed call could not wait for the device, which may still use its buffers: \
             {error}"
        );
        let outputs = self.outputs.into_iter().map(NativeObject::Buffer);
        let retired = self.owned.into_iter().chain(outputs);
        self.session.poison(reason, retired.map(Retired::Object));
        combine_execution_and_cleanup(result, Err(error))
    }

    /// A buffer of `elements` complex doubles, refused if the allocator
    /// returned an input's buffer. `intermediate` and `output` share it,
    /// and differ only in who frees the buffer.
    fn allocate(&self, elements: usize) -> Result<DeviceBuffer, CudaAlgebraError> {
        not_an_input_buffer(
            self.inputs,
            self.calls()
                .allocate(complex_bytes(elements, "device tensor")?)?,
        )
    }
}

/// `buffer`, just allocated, unless it is one of `inputs`': the call would
/// then write its input. The buffer stays the input's, and the call does
/// not free it.
fn not_an_input_buffer(
    inputs: &[&DeviceTensor],
    buffer: DeviceBuffer,
) -> Result<DeviceBuffer, CudaAlgebraError> {
    if inputs.iter().any(|input| input.uses_buffer(buffer)) {
        return Err(CudaAlgebraError::InvalidNativeResult {
            reason: "an allocation returned the buffer of an input tensor, which the call \
                     must leave unchanged"
                .to_string(),
        });
    }
    Ok(buffer)
}

#[cfg(test)]
mod tests;
