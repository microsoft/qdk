// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`CudaAlgebra`]: `canonical_mps`'s `SiteAlgebra` on one CUDA device,
//! through cuTensorNet's tensor API, so that the canonical-form MPS engine
//! runs on the GPU as [`CudaMpsEngine`](crate::CudaMpsEngine).
//!
//! The canonical-form engine keeps its chain of site tensors in canonical
//! form and asks its algebra for five operations: store a site, read it,
//! copy it, split a gated pair of sites by a truncated SVD, and move the
//! centre across a bond by a QR. This algebra holds each site in a device
//! buffer ([`DeviceTensor`]) and runs the two factorizations with
//! cuTensorNet's `GateSplit` and `TensorQR`, and the contractions around a
//! QR with cuTensorNet's contraction networks. Its native calls go through
//! `NativeCalls`, so the same code runs on the GPU and, in CI, on the fake
//! libraries.
//!
//! ```text
//! CudaAlgebra ── holds ──► AlgebraSession     shared with its tensors:
//!                            DeviceSession (device, stream, handle),
//!                            the poison (algebra_session.rs)
//!             ── holds ──► AlgebraResources   kept across calls:
//!                            workspace, Scratch (scratch.rs),
//!                            the gate and singular-value buffers,
//!                            Contractions (contractions.rs)
//! each call ──── runs in ─► CallScope         one call's objects and
//!                                             buffers (call_scope.rs)
//!   gate_split ──────────► gate_split.rs      one GateSplit
//!   qr_move ─────────────► qr_move.rs         a gate, a TensorQR, a
//!                                             contraction
//! settings                 policy.rs
//! ```
//!
//! Every call binds the algebra's device first, and returns with the
//! device idle: its stream synchronized, and its copies too if it made
//! any. Three things rely on it. The copies run on CUDA's legacy default
//! stream, which the algebra's stream does not wait for
//! (`native_calls/memory_calls.rs`); the engine uses each tensor at once,
//! often in the next call; and a call's scratch may be freed by the next
//! one's growth. The one exception is a failed call whose wait for the
//! device fails too: nothing then says when the device is done with the
//! call's buffers, its inputs or the algebra's memory, so every later
//! call returns [`NativeError::Poisoned`], and a buffer the device may
//! still use is freed only after a successful wait for the device (its
//! stream and its copies); otherwise it is kept until one succeeds, or
//! leaked (`algebra_session.rs`).
//!
//! That wait covers only the algebra's own device work, and a tensor is
//! freed through the session of the algebra that made it: so every call
//! refuses a tensor another algebra made, with
//! [`NativeError::ForeignTensor`] and before any native call
//! (`device_tensor.rs`), as the cuTensorNet engine refuses a state another
//! engine created.
//!
//! An algebra and its tensors are `!Send`, as the cuTensorNet engine is:
//! the stream and the handle stay on the thread that created them. Each
//! block below is a test that must fail to compile, one for the algebra
//! and one for the engine on it.
//!
//! ```compile_fail,E0277
//! fn require_send<T: Send>() {}
//! require_send::<qdk_cutensornet::CudaAlgebra>();
//! ```
//!
//! ```compile_fail,E0277
//! fn require_send<T: Send>() {}
//! require_send::<qdk_cutensornet::CudaMpsEngine>();
//! ```

mod algebra_session;
mod call_scope;
mod contractions;
mod device_tensor;
mod gate_split;
mod policy;
mod qr_move;
mod scratch;

pub use device_tensor::DeviceTensor;
#[cfg(test)]
pub(crate) use policy::NETWORKS_KEPT;

use crate::device_session::DeviceSession;
use crate::native_calls::complex::{Complex64Abi, complex_bytes};
use crate::native_calls::error::{NativeError, first_failure, report_failed_release};
use crate::native_calls::{NativeCalls, OpaqueHandle};
use algebra_session::{AlgebraSession, Retired};
use call_scope::CallScope;
use canonical_mps::algebra::{
    GateSplitCall, GateSplitOutcome, QrMoveCall, QrMoveOutcome, SiteAlgebra,
};
use contractions::Contractions;
use num_complex::Complex64;
use scratch::Scratch;
use std::{borrow::Cow, cell::RefCell, rc::Rc, sync::Arc};

/// `canonical_mps`'s `SiteAlgebra` on one CUDA device, through
/// cuTensorNet: its tensors are device buffers ([`DeviceTensor`]).
///
/// Open one with `new`, on a discovery's [`Availability`](crate::Availability)
/// (Linux x86-64), and give it to `canonical_mps::CanonicalMpsEngine::new`,
/// or use [`CudaMpsEngine`](crate::CudaMpsEngine). Its settings (the
/// device, the SVD algorithm, the contractions' paths) are fixed, not
/// tunable (`policy.rs`).
///
/// It holds, across calls, a workspace descriptor and the memory it lends
/// cuTensorNet, which grows to the largest call's and never shrinks, and
/// the contraction networks of up to a few hundred shapes. Dropping it
/// releases them, unless it cannot select its device, which leaks them;
/// its tensors keep the session alive and free their own buffers. A
/// poisoned algebra frees nothing the device may still use until a wait
/// for it succeeds (`algebra_session.rs`). It accepts only the tensors it
/// made, refusing another algebra's with [`NativeError::ForeignTensor`].
pub struct CudaAlgebra {
    session: Rc<AlgebraSession>,
    resources: RefCell<AlgebraResources>,
}

/// What an algebra keeps across its calls.
#[derive(Default)]
struct AlgebraResources {
    /// The one workspace descriptor every call is sized for in turn.
    workspace: Option<OpaqueHandle>,
    /// The memory lent to the workspace.
    scratch: Scratch,
    /// A device buffer of 16 complex doubles: each call's gate.
    gate: Option<OpaqueHandle>,
    /// A device buffer for a gate split's singular values, and the complex
    /// doubles it holds.
    singular_values: Option<(OpaqueHandle, usize)>,
    contractions: Contractions,
}

impl AlgebraResources {
    /// Releases everything, the workspace's memory once the descriptor is
    /// destroyed; attempts every release and returns the first failure.
    fn release(mut self, calls: &dyn NativeCalls) -> Result<(), NativeError> {
        let mut released = vec![self.contractions.release(calls)];
        if let Some(workspace) = self.workspace.take() {
            released.push(calls.destroy_workspace(workspace));
        }
        released.push(self.scratch.release(calls));
        let buffers = [
            self.singular_values.take().map(|(buffer, _)| buffer),
            self.gate.take(),
        ];
        released.extend(
            buffers
                .into_iter()
                .flatten()
                .map(|buffer| calls.free(buffer)),
        );
        first_failure(released)
    }
}

impl CudaAlgebra {
    /// An algebra on the first CUDA device the process sees (device 0), on
    /// the libraries `availability` loaded and checked, as
    /// [`CuTensorNetEngine::new`](crate::CuTensorNetEngine::new) opens an
    /// engine. The algebra keeps the libraries loaded for as long as it or
    /// any of its tensors lives.
    ///
    /// # Errors
    ///
    /// Fails if the loaded cuTensorNet lacks a function of its tensor API
    /// (each is named), which discovery does not require; if no CUDA device
    /// is available; or if creating the device's stream, the cuTensorNet
    /// handle, the workspace descriptor or the gate's buffer fails.
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    pub fn new(availability: &crate::Availability) -> Result<Self, NativeError> {
        Self::with_native_calls(availability.libraries().clone())
    }

    /// An algebra on device 0 that makes its native calls through
    /// `native_calls`; a failure releases what was created.
    #[cfg_attr(
        not(all(target_os = "linux", target_arch = "x86_64")),
        allow(
            dead_code,
            reason = "only host tests create an algebra off Linux x86-64"
        )
    )]
    pub(crate) fn with_native_calls(
        native_calls: Arc<dyn NativeCalls>,
    ) -> Result<Self, NativeError> {
        native_calls.check_tensor_functions()?;
        let device_session = DeviceSession::new(native_calls, policy::DEVICE_ORDINAL)?;
        let algebra = Self {
            session: Rc::new(AlgebraSession::new(device_session)),
            resources: RefCell::new(AlgebraResources::default()),
        };
        {
            let device_session = algebra.session.device_session();
            let calls = device_session.calls();
            let mut resources = algebra.resources.borrow_mut();
            resources.workspace = Some(calls.create_workspace(device_session.handle())?);
            resources.gate = Some(calls.allocate(complex_bytes(16, "gate")?)?);
        }
        Ok(algebra)
    }

    /// Runs `body` in a new call scope, with the device bound, and ends it
    /// ([`CallScope::finish`]). A poisoned algebra runs nothing, not even
    /// the checks of the arguments that each method makes in `body`; then
    /// `operands`, the tensors the call reads, are refused if another
    /// algebra made any, before any native call.
    fn run<T>(
        &self,
        operands: &[&DeviceTensor],
        body: impl FnOnce(&mut CallScope<'_>) -> Result<T, NativeError>,
    ) -> Result<T, NativeError> {
        let mut resources = self.resources.borrow_mut();
        self.session.check_usable()?;
        for operand in operands {
            operand.check_made_by(&self.session)?;
        }
        self.session.device_session().bind_device()?;
        let mut scope = CallScope::new(&self.session, &mut resources);
        let result = body(&mut scope);
        scope.finish(result)
    }

    fn tensor(&self, buffer: OpaqueHandle, elements: usize) -> DeviceTensor {
        DeviceTensor::new(buffer, elements, &self.session)
    }

    /// Releases everything the algebra holds, with its device bound;
    /// attempts every release and returns the first failure, or leaks them
    /// all if the device cannot be bound. A poisoned algebra's memory is
    /// retired, and released with the rest of what was retired if a wait
    /// for the device now succeeds (`algebra_session.rs`).
    fn close(&mut self) -> Result<(), NativeError> {
        let resources = std::mem::take(self.resources.get_mut());
        let released = self.session.release(Retired::Resources(resources));
        first_failure([released, self.session.settle()])
    }
}

impl Drop for CudaAlgebra {
    fn drop(&mut self) {
        report_failed_release("the CUDA algebra", self.close());
    }
}

impl std::fmt::Debug for CudaAlgebra {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CudaAlgebra").finish_non_exhaustive()
    }
}

impl SiteAlgebra for CudaAlgebra {
    type Tensor = DeviceTensor;
    type Error = NativeError;

    fn upload(&self, values: &[Complex64]) -> Result<DeviceTensor, NativeError> {
        let buffer = self.run(&[], |scope| {
            let buffer = scope.output(values.len())?;
            scope.upload_to(buffer, values)?;
            Ok(buffer)
        })?;
        Ok(self.tensor(buffer, values.len()))
    }

    fn read<'t>(
        &self,
        tensor: &'t DeviceTensor,
        len: usize,
    ) -> Result<Cow<'t, [Complex64]>, NativeError> {
        let values = self.run(&[tensor], |scope| {
            let source = tensor.holding(len)?;
            if len == 0 {
                // A copy of no bytes is not a valid native call.
                return Ok(Vec::new());
            }
            let mut values = vec![Complex64Abi::default(); len];
            scope.calls().copy_to_host(&mut values, source)?;
            Ok(values)
        })?;
        Ok(Cow::Owned(
            values.into_iter().map(Complex64::from).collect(),
        ))
    }

    fn duplicate(&self, tensor: &DeviceTensor, len: usize) -> Result<DeviceTensor, NativeError> {
        let buffer = self.run(&[tensor], |scope| {
            let source = tensor.holding(len)?;
            let buffer = scope.output(len)?;
            scope.copy_on_device(buffer, source, complex_bytes(len, "device tensor")?)?;
            Ok(buffer)
        })?;
        Ok(self.tensor(buffer, len))
    }

    fn gate_split(
        &self,
        call: &GateSplitCall<'_, DeviceTensor>,
    ) -> Result<GateSplitOutcome<DeviceTensor>, NativeError> {
        let split = self.run(&[call.left, call.right], |scope| {
            gate_split::gate_split(scope, call)
        })?;
        // The buffers' capacities: each factor is packed at the kept bond.
        let k = split.capacity;
        Ok(GateSplitOutcome {
            left: self.tensor(split.left, call.left_extent * 2 * k),
            right: self.tensor(split.right, k * 2 * call.right_extent),
            bond: split.bond,
            retained_fraction: split.retained_fraction,
        })
    }

    fn qr_move(
        &self,
        call: &QrMoveCall<'_, DeviceTensor>,
    ) -> Result<QrMoveOutcome<DeviceTensor>, NativeError> {
        let moved = self.run(&[call.left, call.right], |scope| {
            qr_move::qr_move(scope, call)
        })?;
        let k = moved.bond;
        Ok(QrMoveOutcome {
            left: self.tensor(moved.left, call.left_extent * 2 * k),
            right: self.tensor(moved.right, k * 2 * call.right_extent),
            bond: k,
        })
    }
}

/// The mode labels of the calls' descriptors. Two tensors' modes of one
/// label are one index; a call's output keeps the labels it names.
mod modes {
    /// A pair's left extent.
    pub(super) const LEFT: i32 = 1;
    /// The left site's qubit.
    pub(super) const P: i32 = 2;
    /// The bond the pair shares before the call.
    pub(super) const BOND: i32 = 3;
    /// The right site's qubit.
    pub(super) const Q: i32 = 4;
    /// A pair's right extent.
    pub(super) const RIGHT: i32 = 5;
    /// The left qubit once gated.
    pub(super) const P_OUT: i32 = 6;
    /// The right qubit once gated.
    pub(super) const Q_OUT: i32 = 7;
    /// The bond the call returns.
    pub(super) const NEW_BOND: i32 = 8;
}
