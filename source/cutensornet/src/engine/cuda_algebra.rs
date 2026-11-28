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
//! buffer ([`DeviceTensor`]). It runs the two factorizations with
//! cuTensorNet's `GateSplit` and `TensorQR`, and the contractions around a
//! QR with cuTensorNet's contraction networks. Its native calls go through
//! `NativeCalls`, so the same code runs on the GPU and in host tests.
//!
//! ```text
//! CudaAlgebra ── holds ──► AlgebraSession     shared with its tensors:
//!                            DeviceSession (device, stream, handle;
//!                            device_session.rs), the poison
//!                            (algebra_session.rs)
//!             ── makes ──► DeviceTensor       one site in a device
//!                                             buffer (device_tensor.rs)
//!             ── holds ──► AlgebraResources   kept across calls
//!                            (resources.rs): workspace, Scratch
//!                            (scratch.rs), the gate and singular-value
//!                            buffers, PreparedContractionNetworks
//!                            (contraction.rs)
//! each call ──── runs in ─► CallScope         one call's objects and
//!                                             buffers (call_scope.rs)
//!   gate_split ──────────► gate_split.rs      one GateSplit
//!   qr_move ─────────────► qr_move.rs         a gate, a TensorQR, a
//!                                             contraction
//! tensor sizes             extents.rs
//! descriptors' mode labels modes.rs
//! fixed settings           settings.rs
//! failures                 CudaAlgebraError
//!                            (../cuda_algebra_error.rs)
//! ```
//!
//! Every call binds the algebra's device first, since CUDA's current
//! device is per thread and another algebra's call may have changed it.
//! Every call returns with the device idle: its stream synchronized, and
//! its copies too if it made any. Three things rely on it:
//!
//! - the copies run on CUDA's legacy default stream, which the algebra's
//!   stream does not wait for (`native_calls/memory_calls.rs`);
//! - the engine uses each tensor at once, often in the next call;
//! - the next call's growth may free this call's scratch.
//!
//! The one exception, a failed call whose wait for the device fails too,
//! poisons the algebra. [`CudaAlgebra`]'s doc, "When CUDA or cuTensorNet
//! fails", says what the algebra does then, and why.

mod algebra_session;
mod call_scope;
mod contraction;
mod device_session;
mod device_tensor;
mod extents;
mod gate_split;
mod modes;
mod qr_move;
mod resources;
mod scratch;
mod settings;
mod tensor_layout;

pub use device_tensor::DeviceTensor;

use crate::engine::cuda_algebra_error::{CudaAlgebraError, first_failure, report_failed_release};
use crate::engine::native_calls::complex::{Complex64Abi, complex_bytes};
use crate::engine::native_calls::{NativeCalls, memory_calls::DeviceBuffer};
use algebra_session::{AlgebraSession, Retired};
use call_scope::CallScope;
use canonical_mps::site_algebra::{
    GateSplitCall, GateSplitOutcome, QrMoveCall, QrMoveOutcome, SiteAlgebra,
};
use device_session::DeviceSession;
use num_complex::Complex64;
use resources::AlgebraResources;
use std::{borrow::Cow, cell::RefCell, rc::Rc, sync::Arc};

/// `canonical_mps`'s `SiteAlgebra` on one CUDA device, through
/// cuTensorNet: its tensors are device buffers ([`DeviceTensor`]).
///
/// Open one with `new`, on a discovery's [`Availability`](crate::Availability)
/// (Linux x86-64), and give it to `canonical_mps::CanonicalMpsEngine::new`,
/// or use [`CudaMpsEngine`](crate::CudaMpsEngine). Its settings (the
/// device, the SVD algorithm, the contractions' paths) are fixed
/// (`settings.rs`).
///
/// Across calls, it holds a workspace descriptor, the memory it lends
/// cuTensorNet (grown to the largest call's, never shrunk) and one
/// contraction network per shape seen, all released when they reach a
/// bound (`settings.rs`).
/// Dropping it releases them. Its tensors keep its device session alive
/// and free their own buffers.
///
/// Every call refuses a tensor another algebra made, before any native
/// call, with [`CudaAlgebraError::ForeignTensor`]: this algebra cannot wait
/// for another's device work.
///
/// It is `!Send`, so its stream and cuTensorNet handle stay on the thread
/// that created them. This does not compile:
///
/// ```compile_fail,E0277
/// fn require_send<T: Send>() {}
/// require_send::<qdk_cutensornet::CudaAlgebra>();
/// ```
///
/// # When CUDA or cuTensorNet fails
///
/// The algebra cannot prevent a failure of CUDA or cuTensorNet: an error
/// status, a GPU fault or reset, or the CUDA Runtime unloading as the
/// process exits. Its job is to report the failure and keep memory safe.
/// It never frees memory the device may still be using, since CUDA would
/// hand that memory to new work while the old work still writes into it.
/// And it frees memory only with the device that holds it selected, as
/// CUDA requires.
///
/// - **A method fails:** it waits for the device's pending work, frees
///   the method's buffers, then returns the failure.
/// - **That wait fails too:** nothing tells when the device will be done
///   with those buffers. The algebra is then *poisoned*, as a `Mutex` is
///   after a panic. It refuses every later call with
///   [`CudaAlgebraError::Poisoned`], before any other check. It keeps,
///   unfreed, every buffer the device may still use, the dropped tensors'
///   included. The algebra's drop waits again, and so does its last
///   tensor's; once a wait succeeds, they are freed (`algebra_session.rs`).
/// - **CUDA refuses a release, or the selection of the device:** what it
///   held is not freed, and the release is not retried. A method returns
///   that failure. A drop, which cannot return one, logs a warning. The
///   memory stays allocated until the process ends.
pub struct CudaAlgebra {
    /// The session the algebra shares with its tensors.
    session: Rc<AlgebraSession>,
    /// What the algebra keeps across its calls; each call borrows it.
    resources: RefCell<AlgebraResources>,
}

impl CudaAlgebra {
    /// An algebra on the first CUDA device the process sees (device 0), on
    /// the libraries `availability` loaded and checked. The algebra keeps
    /// the libraries loaded for as long as it or any of its tensors lives.
    ///
    /// # Errors
    ///
    /// Fails if no CUDA device is available, or if creating the device's
    /// stream, the cuTensorNet handle, the workspace descriptor or the
    /// gate's buffer fails.
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    pub fn new(availability: &crate::Availability) -> Result<Self, CudaAlgebraError> {
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
    pub(super) fn with_native_calls(
        native_calls: Arc<dyn NativeCalls>,
    ) -> Result<Self, CudaAlgebraError> {
        let device_session = DeviceSession::new(native_calls, settings::DEVICE_ORDINAL)?;
        let algebra = Self {
            session: Rc::new(AlgebraSession::new(device_session)),
            resources: RefCell::new(AlgebraResources::default()),
        };
        algebra.create_workspace_and_gate_buffer()?;
        Ok(algebra)
    }

    /// Creates what every call uses: the workspace descriptor and the
    /// gate's buffer. It runs once the algebra exists, so that a failure
    /// drops the algebra, which releases what was created.
    fn create_workspace_and_gate_buffer(&self) -> Result<(), CudaAlgebraError> {
        let device_session = self.session.device_session();
        let calls = device_session.calls();
        let mut resources = self.resources.borrow_mut();
        resources.workspace = Some(calls.create_workspace(device_session.handle())?);
        resources.gate = Some(calls.allocate(complex_bytes(16, "gate")?)?);
        Ok(())
    }

    /// Runs `body` in a new call scope, with the device bound, and ends it
    /// ([`CallScope::finish`]). A poisoned algebra runs nothing, not even
    /// the argument checks each method makes in `body`. Otherwise
    /// `operands`, the tensors the call reads, are refused if another
    /// algebra made any, before any native call. The scope refuses an
    /// allocation that returns one of their buffers.
    fn in_call_scope<T>(
        &self,
        operands: &[&DeviceTensor],
        body: impl FnOnce(&mut CallScope<'_>) -> Result<T, CudaAlgebraError>,
    ) -> Result<T, CudaAlgebraError> {
        let mut resources = self.resources.borrow_mut();
        self.session.check_usable()?;
        for operand in operands {
            operand.check_made_by(&self.session)?;
        }
        self.session.device_session().bind_device()?;
        let mut scope = CallScope::new(&self.session, &mut resources, operands);
        let result = body(&mut scope);
        scope.finish(result)
    }

    /// The tensor of the `elements` complex doubles of `buffer`, which it
    /// now owns.
    fn tensor(&self, buffer: DeviceBuffer, elements: usize) -> DeviceTensor {
        DeviceTensor::new(buffer, elements, &self.session)
    }

    /// Releases everything the algebra holds, with its device bound. It
    /// attempts every release and returns the first failure; if the device
    /// cannot be bound, it leaks them all. A poisoned algebra's memory is
    /// retired, and released with the rest of what was retired if a wait
    /// for the device now succeeds (`algebra_session.rs`).
    fn close(&mut self) -> Result<(), CudaAlgebraError> {
        let resources = std::mem::take(self.resources.get_mut());
        let released = self.session.release(Retired::Resources(resources));
        first_failure([released, self.session.wait_then_release_retired()])
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

/// Each method runs through `in_call_scope`. It refuses another algebra's
/// tensors, then selects the algebra's device. Its native calls write only
/// the buffers the call allocates and the algebra's own (`call_scope.rs`),
/// so the input tensors stay unchanged. It returns with the device idle, so
/// the engine may use the outputs at once.
///
/// The native calls go through `NativeCalls` (`native_calls.rs`): the CUDA
/// Runtime and cuTensorNet on a GPU, fakes in host tests. Each method's
/// doc names our call, then the library function it maps to.
impl SiteAlgebra for CudaAlgebra {
    type Tensor = DeviceTensor;
    type Error = CudaAlgebraError;

    /// Stores `values` as a new site on the device. It allocates the
    /// site's buffer (`MemoryCalls::allocate`, `cudaMalloc`), then copies
    /// the values from the host (`copy_to_device`, `cudaMemcpy`).
    fn upload(&self, values: &[Complex64]) -> Result<DeviceTensor, CudaAlgebraError> {
        let buffer = self.in_call_scope(&[], |scope| {
            let buffer = scope.output(values.len())?;
            scope.upload_to(buffer, values)?;
            Ok(buffer)
        })?;
        Ok(self.tensor(buffer, values.len()))
    }

    /// Copies the first `len` elements of `tensor` into a new host vector
    /// (`MemoryCalls::copy_to_host`, `cudaMemcpy`). `SiteAlgebra` returns
    /// a `Cow` so that a backend on the host can lend its tensors without
    /// a copy. The host cannot borrow device memory, so this one always
    /// copies (`Cow::Owned`). Each tensor knows its buffer's length, so a
    /// read past it fails with [`CudaAlgebraError::InvalidNativeCall`]
    /// before any copy.
    fn read<'t>(
        &self,
        tensor: &'t DeviceTensor,
        len: usize,
    ) -> Result<Cow<'t, [Complex64]>, CudaAlgebraError> {
        let values = self.in_call_scope(&[tensor], |scope| {
            let source = tensor.buffer_holding(len)?;
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

    /// Copies the first `len` elements of `tensor` into a new device
    /// buffer (`MemoryCalls::copy_on_device`, `cudaMemcpy` from device to
    /// device): no round trip through the host. The two tensors own
    /// separate buffers, so later calls on either leave the other
    /// unchanged. A copy past the tensor's end is refused, as for
    /// [`read`](SiteAlgebra::read).
    fn duplicate(
        &self,
        tensor: &DeviceTensor,
        len: usize,
    ) -> Result<DeviceTensor, CudaAlgebraError> {
        let buffer = self.in_call_scope(&[tensor], |scope| {
            let source = tensor.buffer_holding(len)?;
            let buffer = scope.output(len)?;
            scope.copy_on_device(buffer, source, complex_bytes(len, "device tensor")?)?;
            Ok(buffer)
        })?;
        Ok(self.tensor(buffer, len))
    }

    /// Applies the gate and splits the pair in one cuTensorNet call
    /// (`TensorCalls::gate_split`, `cutensornetGateSplit`), set up in
    /// `gate_split.rs`. Its SVD settings (`TensorCalls::configure_svd`,
    /// `cutensornetTensorSVDConfigSetAttribute`) carry the contract's
    /// rules:
    ///
    /// - **truncation:** the call's `cutoff` is cuTensorNet's
    ///   discarded-weight cutoff. `max_extent` caps the new bond: the
    ///   factors' descriptors give it the largest bond the split may keep.
    /// - **norm:** the kept singular values are divided by their L2 norm.
    /// - **centre:** the values go into the factor on the centre's side
    ///   (partition `US` or `SV`), so the other factor is an isometry.
    ///
    /// cuTensorNet reports the bond it kept and the weight it discarded. A
    /// report outside the contract's bounds fails the call with
    /// [`CudaAlgebraError::InvalidNativeResult`], so the engine never sees
    /// it.
    fn gate_split(
        &self,
        call: &GateSplitCall<'_, DeviceTensor>,
    ) -> Result<GateSplitOutcome<DeviceTensor>, CudaAlgebraError> {
        let split = self.in_call_scope(&[call.left, call.right], |scope| {
            gate_split::gate_split(scope, call)
        })?;
        // Each factor holds its site packed at the bond kept, the first
        // elements of a buffer allocated for the largest bond the split
        // could keep: a read past the site is refused.
        let bond = split.bond;
        Ok(GateSplitOutcome {
            left: self.tensor(split.left, call.left_extent * 2 * bond),
            right: self.tensor(split.right, bond * 2 * call.right_extent),
            bond,
            retained_fraction: split.retained_fraction,
        })
    }

    /// Moves the centre in up to three cuTensorNet calls, each waited for
    /// before the next (`qr_move.rs`):
    ///
    /// 1. the one-site gate, if any, contracted into the site the centre
    ///    leaves (`TensorCalls::contract`, `cutensornetNetworkContract`);
    /// 2. a QR of that site, which keeps the isometry
    ///    (`TensorCalls::tensor_qr`, `cutensornetTensorQR`);
    /// 3. the QR's small factor contracted into the other site, the new
    ///    centre (`contract` again).
    ///
    /// A QR truncates nothing, so the move is exact. The contractions run
    /// on cuTensorNet contraction networks. A network costs a path search
    /// (`optimize_contraction`, `cutensornetContractionOptimize`) and a
    /// preparation (`prepare_contraction`,
    /// `cutensornetNetworkPrepareContraction`), so the algebra keeps one
    /// per shape and reuses it (`contraction.rs`): a run's moves repeat
    /// their shapes.
    fn qr_move(
        &self,
        call: &QrMoveCall<'_, DeviceTensor>,
    ) -> Result<QrMoveOutcome<DeviceTensor>, CudaAlgebraError> {
        let moved = self.in_call_scope(&[call.left, call.right], |scope| {
            qr_move::qr_move(scope, call)
        })?;
        let bond = moved.bond;
        Ok(QrMoveOutcome {
            left: self.tensor(moved.left, call.left_extent * 2 * bond),
            right: self.tensor(moved.right, bond * 2 * call.right_extent),
            bond,
        })
    }
}

#[cfg(test)]
mod tests;
