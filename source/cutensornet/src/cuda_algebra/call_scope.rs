// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`CallScope`]: what one call of a [`CudaAlgebra`](super::CudaAlgebra)
//! creates and lends, released before the call returns.
//!
//! A gate split or a move of the centre creates tensor descriptors, an SVD
//! configuration and report, and intermediate buffers, and allocates its
//! outputs. The scope records each as it is made, so that every exit of the
//! call, a failure included, releases them in one place
//! ([`CallScope::finish`]): the outputs survive only a call that succeeds.
//!
//! A failed call may leave device work pending, which may still write the
//! call's buffers: `finish` waits for it before releasing anything. When
//! that wait itself fails, nothing tells when the device is done with
//! them, so they are retired and the algebra is poisoned
//! (`algebra_session.rs`).

use super::AlgebraResources;
use super::algebra_session::{AlgebraSession, NativeObject, Retired};
use crate::device_session::DeviceSession;
use crate::native_calls::complex::{Complex64Abi, complex_bytes};
use crate::native_calls::error::{NativeError, combine_execution_and_cleanup, first_failure};
use crate::native_calls::tensor_calls::{SvdSetting, TensorModes};
use crate::native_calls::{NativeCalls, OpaqueHandle, Stream};
use num_complex::Complex64;

/// One call of the algebra: the session it shares with its tensors, the
/// resources the algebra holds across calls, and the native objects and
/// buffers this call made.
pub(super) struct CallScope<'a> {
    session: &'a AlgebraSession,
    pub(super) resources: &'a mut AlgebraResources,
    /// The call's own objects and intermediate buffers, released in the
    /// reverse order of their creation.
    owned: Vec<NativeObject>,
    /// The buffers the call returns, freed only if it fails.
    outputs: Vec<OpaqueHandle>,
    /// Whether the call copied, so that a failure also waits for the
    /// copies.
    copied: bool,
}

impl<'a> CallScope<'a> {
    pub(super) fn new(session: &'a AlgebraSession, resources: &'a mut AlgebraResources) -> Self {
        Self {
            session,
            resources,
            owned: Vec::new(),
            outputs: Vec::new(),
            copied: false,
        }
    }

    fn device_session(&self) -> &'a DeviceSession<dyn NativeCalls> {
        self.session.device_session()
    }

    pub(super) fn calls(&self) -> &'a dyn NativeCalls {
        self.device_session().calls()
    }

    pub(super) fn handle(&self) -> OpaqueHandle {
        self.device_session().handle()
    }

    /// The algebra's stream, which every call's device work is queued on.
    pub(super) fn stream(&self) -> Stream {
        self.device_session().stream()
    }

    /// The algebra's workspace descriptor.
    pub(super) fn workspace(&self) -> OpaqueHandle {
        self.resources
            .workspace
            .expect("a live algebra holds its workspace")
    }

    /// Lends the workspace, sized for the call about to run, the memory it
    /// asks for.
    pub(super) fn lend_workspace(&mut self) -> Result<(), NativeError> {
        let workspace = self.workspace();
        let device_session = self.device_session();
        self.resources.scratch.lend(device_session, workspace)
    }

    /// Waits for the work queued on the algebra's stream.
    pub(super) fn synchronize(&self) -> Result<(), NativeError> {
        self.calls().synchronize_stream(self.stream())
    }

    /// A tensor descriptor of these extents and mode labels, destroyed when
    /// the call returns.
    pub(super) fn descriptor(
        &mut self,
        extents: &[usize],
        modes: &[i32],
    ) -> Result<OpaqueHandle, NativeError> {
        let extents = native_extents(extents)?;
        let descriptor = self.calls().create_tensor_descriptor(
            self.handle(),
            TensorModes {
                extents: &extents,
                modes,
            },
        )?;
        self.owned.push(NativeObject::Descriptor(descriptor));
        Ok(descriptor)
    }

    /// An SVD configuration of `settings`, destroyed when the call returns.
    pub(super) fn svd_config(
        &mut self,
        settings: &[SvdSetting],
    ) -> Result<OpaqueHandle, NativeError> {
        let (calls, handle) = (self.calls(), self.handle());
        let config = calls.create_svd_config(handle)?;
        self.owned.push(NativeObject::SvdConfig(config));
        for &setting in settings {
            calls.configure_svd(handle, config, setting)?;
        }
        Ok(config)
    }

    /// An SVD report, destroyed when the call returns.
    pub(super) fn svd_info(&mut self) -> Result<OpaqueHandle, NativeError> {
        let info = self.calls().create_svd_info(self.handle())?;
        self.owned.push(NativeObject::SvdInfo(info));
        Ok(info)
    }

    /// A buffer of `elements` complex doubles, freed when the call returns.
    pub(super) fn intermediate(&mut self, elements: usize) -> Result<OpaqueHandle, NativeError> {
        let buffer = self.allocate(elements)?;
        self.owned.push(NativeObject::Buffer(buffer));
        Ok(buffer)
    }

    /// A buffer of `elements` complex doubles that the call returns, freed
    /// only if it fails.
    pub(super) fn output(&mut self, elements: usize) -> Result<OpaqueHandle, NativeError> {
        let buffer = self.allocate(elements)?;
        self.outputs.push(buffer);
        Ok(buffer)
    }

    /// Copies `values` into `destination`, and waits for the copy.
    pub(super) fn upload_to(
        &mut self,
        destination: OpaqueHandle,
        values: &[Complex64],
    ) -> Result<(), NativeError> {
        let values: Vec<Complex64Abi> = values.iter().copied().map(Complex64Abi::from).collect();
        self.copied = true;
        self.calls().copy_to_device(destination, &values)?;
        self.calls().synchronize_copies()
    }

    /// Copies `bytes` from `source` into `destination` on the device, and
    /// waits for the copy.
    pub(super) fn copy_on_device(
        &mut self,
        destination: OpaqueHandle,
        source: OpaqueHandle,
        bytes: usize,
    ) -> Result<(), NativeError> {
        self.copied = true;
        self.calls().copy_on_device(destination, source, bytes)?;
        self.calls().synchronize_copies()
    }

    /// Copies `gate`, in a descriptor's layout, into the algebra's gate
    /// buffer, waits for the copy, and returns the buffer.
    pub(super) fn upload_gate(&mut self, gate: &[Complex64]) -> Result<OpaqueHandle, NativeError> {
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
    pub(super) fn singular_values(&mut self, count: usize) -> Result<OpaqueHandle, NativeError> {
        let elements = count.div_ceil(2);
        match self.resources.singular_values {
            Some((buffer, held)) if held >= elements => Ok(buffer),
            previous => {
                let grown = self
                    .calls()
                    .allocate(complex_bytes(elements, "singular values")?)?;
                self.resources.singular_values = Some((grown, elements));
                match previous {
                    Some((old, _)) => self.calls().free(old).map(|()| grown),
                    None => Ok(grown),
                }
            }
        }
    }

    /// Ends the call with `result`: waits for the device if the call failed,
    /// then releases the call's own objects and buffers, and its outputs
    /// unless it succeeded. A failed release fails the call; a failure to
    /// wait does too, retires the call's objects and buffers and poisons
    /// the algebra. Both failures are kept.
    pub(super) fn finish<T>(self, result: Result<T, NativeError>) -> Result<T, NativeError> {
        let calls = self.calls();
        if result.is_err() {
            let stream = self.synchronize();
            let copies = self.copied.then(|| calls.synchronize_copies());
            if let Err(error) = first_failure(std::iter::once(stream).chain(copies)) {
                let reason = format!(
                    "a failed call could not wait for the device, which may still use \
                     its buffers: {error}"
                );
                let outputs = self.outputs.into_iter().map(NativeObject::Buffer);
                let retired = self.owned.into_iter().chain(outputs);
                self.session.poison(reason, retired.map(Retired::Object));
                return combine_execution_and_cleanup(result, Err(error));
            }
        }
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

    fn allocate(&self, elements: usize) -> Result<OpaqueHandle, NativeError> {
        self.calls()
            .allocate(complex_bytes(elements, "device tensor")?)
    }
}

/// `extents` as a descriptor takes them.
pub(super) fn native_extents(extents: &[usize]) -> Result<Vec<i64>, NativeError> {
    extents
        .iter()
        .map(|&extent| {
            i64::try_from(extent).map_err(|_| NativeError::ResourceSizeOverflow {
                resource: "tensor extent",
            })
        })
        .collect()
}

/// The element count of a tensor of `extents`.
pub(super) fn elements(extents: &[usize]) -> Result<usize, NativeError> {
    extents
        .iter()
        .try_fold(1_usize, |count, &extent| count.checked_mul(extent))
        .ok_or(NativeError::ResourceSizeOverflow {
            resource: "device tensor",
        })
}
