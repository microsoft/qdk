#![allow(
    dead_code,
    reason = "the private native adapter becomes live in the consumer integration iteration"
)]

#[path = "simulation/session.rs"]
mod session;

pub(crate) use session::Session;

use super::NativeApi;
use crate::bindings::{cudart_12, v2_13};
use crate::simulation::{
    Complex64Abi, MpsTarget, OpaqueHandle, OutputMetadata, ReplayApi, SimulationError,
    StateF64Attribute, StateU32Configuration, Stream,
};
use session::SessionApi;
use std::{
    ffi::{CStr, c_void},
    mem::size_of,
    ptr::NonNull,
};

impl NativeApi {
    fn cuda_message(&self, status: cudart_12::CudaError) -> String {
        // SAFETY: the pointer was resolved with the audited CUDA signature and
        // the returned library-owned string is copied before this call returns.
        let message = unsafe { (self.cuda_functions.get_error_string)(status) };
        copy_error_message(message, "CUDA returned a null error string")
    }

    fn cutensornet_message(&self, status: v2_13::cutensornetStatus_t) -> String {
        // SAFETY: both pointers were resolved with their audited signatures;
        // returned library-owned strings are copied immediately.
        let stable = unsafe { (self.cutensornet_functions.get_error_string)(status) };
        let mut message = copy_error_message(stable, "cuTensorNet returned a null error string");
        if let Some(get_last_error) = self.cutensornet_functions.get_last_error {
            // SAFETY: the optional pointer has its exact no-argument signature.
            let detail = copy_error_message(unsafe { get_last_error() }, "");
            if !detail.is_empty() {
                message.push_str(": ");
                message.push_str(&detail);
            }
        }
        message
    }

    fn check_cuda(
        &self,
        operation: &'static str,
        status: cudart_12::CudaError,
    ) -> Result<(), SimulationError> {
        if status == 0 {
            Ok(())
        } else {
            Err(SimulationError::NativeCallFailed {
                component: "CUDA Runtime",
                operation,
                status,
                message: self.cuda_message(status),
            })
        }
    }

    fn check_cutensornet(
        &self,
        operation: &'static str,
        status: v2_13::cutensornetStatus_t,
    ) -> Result<(), SimulationError> {
        if status == v2_13::cutensornetStatus_t_CUTENSORNET_STATUS_SUCCESS {
            Ok(())
        } else {
            Err(SimulationError::NativeCallFailed {
                component: "cuTensorNet",
                operation,
                status,
                message: self.cutensornet_message(status),
            })
        }
    }
}

impl SessionApi for NativeApi {
    fn device_count(&self) -> Result<i32, SimulationError> {
        let mut count = 0;
        // SAFETY: `count` is a valid writable CUDA `int` out-parameter.
        let status = unsafe { (self.cuda_functions.get_device_count)(&raw mut count) };
        self.check_cuda("cudaGetDeviceCount", status)?;
        Ok(count)
    }

    fn set_device(&self, ordinal: i32) -> Result<(), SimulationError> {
        // SAFETY: the session validates this ordinal against the device count.
        let status = unsafe { (self.cuda_functions.set_device)(ordinal) };
        self.check_cuda("cudaSetDevice", status)
    }

    fn create_stream(&self) -> Result<Stream, SimulationError> {
        let mut stream = std::ptr::null_mut();
        // SAFETY: `stream` is a valid writable out-pointer and the flags value
        // is the audited CUDA nonblocking-stream constant.
        let status = unsafe {
            (self.cuda_functions.stream_create_with_flags)(
                &raw mut stream,
                cudart_12::CUDA_STREAM_NON_BLOCKING,
            )
        };
        self.check_cuda("cudaStreamCreateWithFlags", status)?;
        NonNull::new(stream.cast()).ok_or(SimulationError::MissingNativeResource {
            operation: "cudaStreamCreateWithFlags",
            resource: "CUDA stream",
        })
    }

    fn synchronize_stream(&self, stream: Stream) -> Result<(), SimulationError> {
        // SAFETY: `stream` is owned by the live session and has not been destroyed.
        let status = unsafe { (self.cuda_functions.stream_synchronize)(stream.as_ptr().cast()) };
        self.check_cuda("cudaStreamSynchronize", status)
    }

    fn destroy_stream(&self, stream: Stream) -> Result<(), SimulationError> {
        // SAFETY: the session consumes this owned stream exactly once.
        let status = unsafe { (self.cuda_functions.stream_destroy)(stream.as_ptr().cast()) };
        self.check_cuda("cudaStreamDestroy", status)
    }

    fn create_handle(&self) -> Result<OpaqueHandle, SimulationError> {
        let mut handle = std::ptr::null_mut();
        // SAFETY: `handle` is a valid writable cuTensorNet out-pointer.
        let status = unsafe { (self.cutensornet_functions.create)(&raw mut handle) };
        self.check_cutensornet("cutensornetCreate", status)?;
        NonNull::new(handle).ok_or(SimulationError::MissingNativeResource {
            operation: "cutensornetCreate",
            resource: "cuTensorNet handle",
        })
    }

    fn destroy_handle(&self, handle: OpaqueHandle) -> Result<(), SimulationError> {
        // SAFETY: the session consumes this handle exactly once after all children.
        let status = unsafe { (self.cutensornet_functions.destroy)(handle.as_ptr()) };
        self.check_cutensornet("cutensornetDestroy", status)
    }
}

impl ReplayApi for NativeApi {
    fn memory_info(&self) -> Result<(usize, usize), SimulationError> {
        let mut free = 0;
        let mut total = 0;
        // SAFETY: both arguments are valid writable `size_t` out-pointers.
        let status = unsafe { (self.cuda_functions.mem_get_info)(&raw mut free, &raw mut total) };
        self.check_cuda("cudaMemGetInfo", status)?;
        Ok((free, total))
    }

    fn allocate(&self, bytes: usize) -> Result<OpaqueHandle, SimulationError> {
        let mut allocation = std::ptr::null_mut();
        // SAFETY: `allocation` is a valid writable out-pointer and the caller
        // has checked that `bytes` is positive and addressable.
        let status = unsafe { (self.cuda_functions.malloc)(&raw mut allocation, bytes) };
        self.check_cuda("cudaMalloc", status)?;
        NonNull::new(allocation).ok_or(SimulationError::MissingNativeResource {
            operation: "cudaMalloc",
            resource: "device allocation",
        })
    }

    fn free(&self, allocation: OpaqueHandle) -> Result<(), SimulationError> {
        // SAFETY: the replay consumes each successful cudaMalloc allocation once.
        let status = unsafe { (self.cuda_functions.free)(allocation.as_ptr()) };
        self.check_cuda("cudaFree", status)
    }

    fn copy_to_device(
        &self,
        destination: OpaqueHandle,
        source: &[Complex64Abi],
    ) -> Result<(), SimulationError> {
        let bytes = complex_bytes(source.len())?;
        // SAFETY: the retained destination allocation is at least `bytes`
        // long, and `source` is a live host slice for this synchronous copy.
        let status = unsafe {
            (self.cuda_functions.memcpy)(
                destination.as_ptr(),
                source.as_ptr().cast(),
                bytes,
                cudart_12::CUDA_MEMCPY_HOST_TO_DEVICE,
            )
        };
        self.check_cuda("cudaMemcpy(H2D)", status)
    }

    fn copy_from_device(
        &self,
        source: OpaqueHandle,
        destination: &mut [Complex64Abi],
    ) -> Result<(), SimulationError> {
        let bytes = complex_bytes(destination.len())?;
        // SAFETY: the retained source allocation is at least `bytes` long,
        // and `destination` is a writable host slice for this synchronous copy.
        let status = unsafe {
            (self.cuda_functions.memcpy)(
                destination.as_mut_ptr().cast(),
                source.as_ptr(),
                bytes,
                cudart_12::CUDA_MEMCPY_DEVICE_TO_HOST,
            )
        };
        self.check_cuda("cudaMemcpy(D2H)", status)
    }

    fn create_state(
        &self,
        handle: OpaqueHandle,
        mode_extents: &[i64],
    ) -> Result<OpaqueHandle, SimulationError> {
        let mode_count = i32::try_from(mode_extents.len()).map_err(|_| {
            SimulationError::ResourceSizeOverflow {
                resource: "state mode count",
            }
        })?;
        let mut state = std::ptr::null_mut();
        // SAFETY: the handle is live, the retained extent slice contains
        // `mode_count` entries, and `state` is a writable out-pointer.
        let status = unsafe {
            (self.cutensornet_functions.create_state)(
                handle.as_ptr(),
                v2_13::cutensornetStatePurity_t_CUTENSORNET_STATE_PURITY_PURE,
                mode_count,
                mode_extents.as_ptr(),
                v2_13::cudaDataType_t_CUDA_C_64F,
                &raw mut state,
            )
        };
        self.check_cutensornet("cutensornetCreateState", status)?;
        NonNull::new(state).ok_or(SimulationError::MissingNativeResource {
            operation: "cutensornetCreateState",
            resource: "cuTensorNet state",
        })
    }

    fn destroy_state(&self, state: OpaqueHandle) -> Result<(), SimulationError> {
        // SAFETY: the replay consumes this state exactly once.
        let status = unsafe { (self.cutensornet_functions.destroy_state)(state.as_ptr()) };
        self.check_cutensornet("cutensornetDestroyState", status)
    }

    fn apply_tensor_operator(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        modes: &[i32],
        tensor: OpaqueHandle,
        unitary: bool,
    ) -> Result<(), SimulationError> {
        let mode_count =
            i32::try_from(modes.len()).map_err(|_| SimulationError::ResourceSizeOverflow {
                resource: "operator mode count",
            })?;
        let mut tensor_id = 0;
        // SAFETY: handle/state and device tensor are live; the caller retains
        // `modes` and tensor storage through state destruction. Null strides
        // select the audited textbook row-major gate interpretation.
        let status = unsafe {
            (self.cutensornet_functions.apply_tensor_operator)(
                handle.as_ptr(),
                state.as_ptr(),
                mode_count,
                modes.as_ptr(),
                tensor.as_ptr(),
                std::ptr::null(),
                1,
                0,
                i32::from(unitary),
                &raw mut tensor_id,
            )
        };
        self.check_cutensornet("cutensornetStateApplyTensorOperator", status)
    }

    fn finalize_mps(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        target: &MpsTarget,
    ) -> Result<(), SimulationError> {
        // SAFETY: the target owns two retained extent arrays and their pointer
        // table through state destruction; null strides request native layout.
        let status = unsafe {
            (self.cutensornet_functions.finalize_mps)(
                handle.as_ptr(),
                state.as_ptr(),
                v2_13::cutensornetBoundaryCondition_t_CUTENSORNET_BOUNDARY_CONDITION_OPEN,
                target.extent_pointers().as_ptr(),
                std::ptr::null(),
            )
        };
        self.check_cutensornet("cutensornetStateFinalizeMPS", status)
    }

    fn capture_mps(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
    ) -> Result<(), SimulationError> {
        // SAFETY: handle and state are live. This call deletes all registered
        // tensor operators from the state, so the caller must ensure retained
        // tensors are safe across this call.
        let status =
            unsafe { (self.cutensornet_functions.capture_mps)(handle.as_ptr(), state.as_ptr()) };
        self.check_cutensornet("cutensornetStateCaptureMPS", status)
    }

    fn configure_state_f64(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        attribute: StateF64Attribute,
        value: f64,
    ) -> Result<(), SimulationError> {
        let attribute = match attribute {
            StateF64Attribute::SvdAbsoluteCutoff => {
                v2_13::cutensornetStateAttributes_t_CUTENSORNET_STATE_CONFIG_MPS_SVD_ABS_CUTOFF
            }
            StateF64Attribute::SvdRelativeCutoff => {
                v2_13::cutensornetStateAttributes_t_CUTENSORNET_STATE_CONFIG_MPS_SVD_REL_CUTOFF
            }
        };
        // SAFETY: the attribute is paired with its audited f64 representation,
        // and cuTensorNet copies the call-local value before returning.
        let status = unsafe {
            (self.cutensornet_functions.state_configure)(
                handle.as_ptr(),
                state.as_ptr(),
                attribute,
                (&raw const value).cast(),
                size_of::<f64>(),
            )
        };
        self.check_cutensornet("cutensornetStateConfigure(f64)", status)
    }

    fn configure_state_u32(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        configuration: StateU32Configuration,
    ) -> Result<(), SimulationError> {
        let (attribute, value) = match configuration {
            StateU32Configuration::SvdAlgorithmGesvd => (
                v2_13::cutensornetStateAttributes_t_CUTENSORNET_STATE_CONFIG_MPS_SVD_ALGO,
                v2_13::cutensornetTensorSVDAlgo_t_CUTENSORNET_TENSOR_SVD_ALGO_GESVD,
            ),
            StateU32Configuration::MpsGaugeSimple => (
                v2_13::cutensornetStateAttributes_t_CUTENSORNET_STATE_CONFIG_MPS_GAUGE_OPTION,
                v2_13::cutensornetStateMPSGaugeOption_t_CUTENSORNET_STATE_MPS_GAUGE_SIMPLE,
            ),
        };
        // SAFETY: the attribute is paired with its audited 32-bit enum
        // representation, and cuTensorNet copies the value before returning.
        let status = unsafe {
            (self.cutensornet_functions.state_configure)(
                handle.as_ptr(),
                state.as_ptr(),
                attribute,
                (&raw const value).cast(),
                size_of::<u32>(),
            )
        };
        self.check_cutensornet("cutensornetStateConfigure(u32)", status)
    }

    fn create_workspace(&self, handle: OpaqueHandle) -> Result<OpaqueHandle, SimulationError> {
        let mut workspace = std::ptr::null_mut();
        // SAFETY: the handle is live and `workspace` is a writable out-pointer.
        let status = unsafe {
            (self.cutensornet_functions.create_workspace)(handle.as_ptr(), &raw mut workspace)
        };
        self.check_cutensornet("cutensornetCreateWorkspaceDescriptor", status)?;
        NonNull::new(workspace).ok_or(SimulationError::MissingNativeResource {
            operation: "cutensornetCreateWorkspaceDescriptor",
            resource: "workspace descriptor",
        })
    }

    fn destroy_workspace(&self, workspace: OpaqueHandle) -> Result<(), SimulationError> {
        // SAFETY: the replay consumes this descriptor exactly once.
        let status = unsafe { (self.cutensornet_functions.destroy_workspace)(workspace.as_ptr()) };
        self.check_cutensornet("cutensornetDestroyWorkspaceDescriptor", status)
    }

    fn prepare_state(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        maximum_workspace_bytes: usize,
        workspace: OpaqueHandle,
        stream: Stream,
    ) -> Result<(), SimulationError> {
        // SAFETY: every native owner is live and the workspace ceiling is a
        // by-value snapshot from cudaMemGetInfo.
        let status = unsafe {
            (self.cutensornet_functions.state_prepare)(
                handle.as_ptr(),
                state.as_ptr(),
                maximum_workspace_bytes,
                workspace.as_ptr(),
                stream.as_ptr().cast(),
            )
        };
        self.check_cutensornet("cutensornetStatePrepare", status)
    }

    fn workspace_size(
        &self,
        handle: OpaqueHandle,
        workspace: OpaqueHandle,
    ) -> Result<i64, SimulationError> {
        let mut bytes = 0;
        // SAFETY: handle/workspace are live and `bytes` is a writable out-pointer.
        let status = unsafe {
            (self.cutensornet_functions.workspace_get_memory_size)(
                handle.as_ptr(),
                workspace.as_ptr(),
                v2_13::cutensornetWorksizePref_t_CUTENSORNET_WORKSIZE_PREF_RECOMMENDED,
                v2_13::cutensornetMemspace_t_CUTENSORNET_MEMSPACE_DEVICE,
                v2_13::cutensornetWorkspaceKind_t_CUTENSORNET_WORKSPACE_SCRATCH,
                &raw mut bytes,
            )
        };
        self.check_cutensornet("cutensornetWorkspaceGetMemorySize", status)?;
        Ok(bytes)
    }

    fn set_workspace(
        &self,
        handle: OpaqueHandle,
        workspace: OpaqueHandle,
        allocation: OpaqueHandle,
        bytes: i64,
    ) -> Result<(), SimulationError> {
        // SAFETY: the retained cudaMalloc allocation is at least `bytes` long
        // and remains live until after descriptor destruction.
        let status = unsafe {
            (self.cutensornet_functions.workspace_set_memory)(
                handle.as_ptr(),
                workspace.as_ptr(),
                v2_13::cutensornetMemspace_t_CUTENSORNET_MEMSPACE_DEVICE,
                v2_13::cutensornetWorkspaceKind_t_CUTENSORNET_WORKSPACE_SCRATCH,
                allocation.as_ptr(),
                bytes,
            )
        };
        self.check_cutensornet("cutensornetWorkspaceSetMemory", status)
    }

    fn compute_state(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        workspace: OpaqueHandle,
        metadata: &mut OutputMetadata,
        outputs: &mut [OpaqueHandle],
        stream: Stream,
    ) -> Result<(), SimulationError> {
        let mut extent_pointers = metadata
            .extents
            .iter_mut()
            .map(|extents| extents.as_mut_ptr())
            .collect::<Vec<_>>();
        let mut stride_pointers = metadata
            .strides
            .iter_mut()
            .map(|strides| strides.as_mut_ptr())
            .collect::<Vec<_>>();
        let expected_output_pointers = outputs
            .iter()
            .map(|output| output.as_ptr())
            .collect::<Vec<_>>();
        let mut output_pointers = expected_output_pointers.clone();
        // SAFETY: all native owners and output allocations are live. Each
        // pointer table has one retained writable entry per state mode.
        let status = unsafe {
            (self.cutensornet_functions.state_compute)(
                handle.as_ptr(),
                state.as_ptr(),
                workspace.as_ptr(),
                extent_pointers.as_mut_ptr(),
                stride_pointers.as_mut_ptr(),
                output_pointers.as_mut_ptr(),
                stream.as_ptr().cast(),
            )
        };
        self.check_cutensornet("cutensornetStateCompute", status)?;
        validate_output_pointers(&expected_output_pointers, &output_pointers)
    }

    fn synchronize_stream(&self, stream: Stream) -> Result<(), SimulationError> {
        SessionApi::synchronize_stream(self, stream)
    }

    fn create_network_operator(
        &self,
        handle: OpaqueHandle,
        mode_extents: &[i64],
    ) -> Result<OpaqueHandle, SimulationError> {
        let mode_count = i32::try_from(mode_extents.len()).map_err(|_| {
            SimulationError::ResourceSizeOverflow {
                resource: "network operator mode count",
            }
        })?;
        let mut operator = std::ptr::null_mut();
        // SAFETY: the handle is live, the extent slice has `mode_count`
        // entries, and `operator` is a writable out-pointer.
        let status = unsafe {
            (self.cutensornet_functions.create_network_operator)(
                handle.as_ptr(),
                mode_count,
                mode_extents.as_ptr(),
                v2_13::cudaDataType_t_CUDA_C_64F,
                &raw mut operator,
            )
        };
        self.check_cutensornet("cutensornetCreateNetworkOperator", status)?;
        NonNull::new(operator).ok_or(SimulationError::MissingNativeResource {
            operation: "cutensornetCreateNetworkOperator",
            resource: "network operator",
        })
    }

    fn destroy_network_operator(&self, operator: OpaqueHandle) -> Result<(), SimulationError> {
        // SAFETY: replay cleanup consumes the owned operator exactly once.
        let status =
            unsafe { (self.cutensornet_functions.destroy_network_operator)(operator.as_ptr()) };
        self.check_cutensornet("cutensornetDestroyNetworkOperator", status)
    }

    fn append_product(
        &self,
        handle: OpaqueHandle,
        operator: OpaqueHandle,
        coefficient: num_complex::Complex64,
        factor_modes: &[Box<[i32]>],
        factor_tensors: &[OpaqueHandle],
    ) -> Result<(), SimulationError> {
        if factor_modes.is_empty() || factor_modes.len() != factor_tensors.len() {
            return Err(SimulationError::InvalidCircuit {
                reason: "Query product factors do not match".to_string(),
            });
        }
        let coefficient = v2_13::double2 {
            x: coefficient.re,
            y: coefficient.im,
        };
        let factor_count = i32::try_from(factor_modes.len()).map_err(|_| {
            SimulationError::ResourceSizeOverflow {
                resource: "Query product factor count",
            }
        })?;
        let mode_counts = factor_modes
            .iter()
            .map(|modes| {
                i32::try_from(modes.len()).map_err(|_| SimulationError::ResourceSizeOverflow {
                    resource: "Query product mode count",
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mode_pointers = factor_modes
            .iter()
            .map(|modes| modes.as_ptr())
            .collect::<Vec<_>>();
        let tensor_pointers = factor_tensors
            .iter()
            .map(|tensor| tensor.as_ptr().cast_const())
            .collect::<Vec<_>>();
        let mut component_id = 0;
        // SAFETY: all pointer tables contain `factor_count` retained entries;
        // operator tensors remain allocated until operator destruction. Null
        // strides request default layout for each single-mode Pauli factor.
        let status = unsafe {
            (self.cutensornet_functions.append_product)(
                handle.as_ptr(),
                operator.as_ptr(),
                coefficient,
                factor_count,
                mode_counts.as_ptr(),
                mode_pointers.as_ptr(),
                std::ptr::null(),
                tensor_pointers.as_ptr(),
                &raw mut component_id,
            )
        };
        self.check_cutensornet("cutensornetNetworkOperatorAppendProduct", status)
    }

    fn create_expectation(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        operator: OpaqueHandle,
    ) -> Result<OpaqueHandle, SimulationError> {
        let mut expectation = std::ptr::null_mut();
        // SAFETY: handle/state/operator are live and `expectation` is writable.
        let status = unsafe {
            (self.cutensornet_functions.create_expectation)(
                handle.as_ptr(),
                state.as_ptr(),
                operator.as_ptr(),
                &raw mut expectation,
            )
        };
        self.check_cutensornet("cutensornetCreateExpectation", status)?;
        NonNull::new(expectation).ok_or(SimulationError::MissingNativeResource {
            operation: "cutensornetCreateExpectation",
            resource: "state expectation",
        })
    }

    fn destroy_expectation(&self, expectation: OpaqueHandle) -> Result<(), SimulationError> {
        // SAFETY: replay cleanup consumes the owned expectation exactly once.
        let status =
            unsafe { (self.cutensornet_functions.destroy_expectation)(expectation.as_ptr()) };
        self.check_cutensornet("cutensornetDestroyExpectation", status)
    }

    fn configure_expectation_hyper_samples(
        &self,
        handle: OpaqueHandle,
        expectation: OpaqueHandle,
        hyper_samples: i32,
    ) -> Result<(), SimulationError> {
        // SAFETY: the attribute is paired with its audited int32 value and the
        // native API copies the call-local setting before returning.
        let status = unsafe {
            (self.cutensornet_functions.expectation_configure)(
                handle.as_ptr(),
                expectation.as_ptr(),
                v2_13::cutensornetExpectationAttributes_t_CUTENSORNET_EXPECTATION_CONFIG_NUM_HYPER_SAMPLES,
                (&raw const hyper_samples).cast(),
                size_of::<i32>(),
            )
        };
        self.check_cutensornet("cutensornetExpectationConfigure", status)
    }

    fn prepare_expectation(
        &self,
        handle: OpaqueHandle,
        expectation: OpaqueHandle,
        maximum_workspace_bytes: usize,
        workspace: OpaqueHandle,
        stream: Stream,
    ) -> Result<(), SimulationError> {
        // SAFETY: every native owner is live and the workspace limit is passed
        // by value from the validated engine qualification policy.
        let status = unsafe {
            (self.cutensornet_functions.expectation_prepare)(
                handle.as_ptr(),
                expectation.as_ptr(),
                maximum_workspace_bytes,
                workspace.as_ptr(),
                stream.as_ptr().cast(),
            )
        };
        self.check_cutensornet("cutensornetExpectationPrepare", status)
    }

    fn compute_expectation(
        &self,
        handle: OpaqueHandle,
        expectation: OpaqueHandle,
        workspace: OpaqueHandle,
        stream: Stream,
    ) -> Result<(num_complex::Complex64, num_complex::Complex64), SimulationError> {
        let mut value = Complex64Abi::default();
        let mut norm = Complex64Abi::default();
        // SAFETY: native owners are live and both host result buffers are valid
        // writable complex-f64 values for the duration of the call.
        let status = unsafe {
            (self.cutensornet_functions.expectation_compute)(
                handle.as_ptr(),
                expectation.as_ptr(),
                workspace.as_ptr(),
                (&raw mut value).cast(),
                (&raw mut norm).cast(),
                stream.as_ptr().cast(),
            )
        };
        self.check_cutensornet("cutensornetExpectationCompute", status)?;
        Ok((value.into(), norm.into()))
    }
}

fn validate_output_pointers(
    expected: &[*mut c_void],
    actual: &[*mut c_void],
) -> Result<(), SimulationError> {
    if actual == expected {
        Ok(())
    } else {
        Err(SimulationError::InvalidNativeResult {
            reason: "cutensornetStateCompute replaced caller-owned output pointers".to_string(),
        })
    }
}

fn complex_bytes(elements: usize) -> Result<usize, SimulationError> {
    if elements == 0 {
        return Err(SimulationError::InvalidNativeResult {
            reason: "complex tensor copy requires at least one element".to_string(),
        });
    }
    elements
        .checked_mul(size_of::<Complex64Abi>())
        .ok_or(SimulationError::ResourceSizeOverflow {
            resource: "complex tensor",
        })
}

fn copy_error_message(pointer: *const std::ffi::c_char, null_message: &str) -> String {
    if pointer.is_null() {
        null_message.to_string()
    } else {
        // SAFETY: callers pass non-null pointers returned by CUDA or
        // cuTensorNet error APIs, both documented as null-terminated strings.
        unsafe { CStr::from_ptr(pointer) }
            .to_string_lossy()
            .into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::{SimulationError, complex_bytes, validate_output_pointers};
    use std::ffi::c_void;

    #[test]
    fn complex_tensor_size_rejects_zero_and_overflow() {
        assert!(matches!(
            complex_bytes(0),
            Err(SimulationError::InvalidNativeResult { .. })
        ));
        assert!(matches!(
            complex_bytes(usize::MAX),
            Err(SimulationError::ResourceSizeOverflow {
                resource: "complex tensor"
            })
        ));
    }

    #[test]
    fn state_compute_must_preserve_caller_owned_output_pointers() {
        let first = 0x100_usize as *mut c_void;
        let second = 0x200_usize as *mut c_void;
        let expected = [first, second];

        validate_output_pointers(&expected, &expected)
            .expect("unchanged caller-owned pointers should be accepted");
        assert!(matches!(
            validate_output_pointers(&expected, &[first, first]),
            Err(SimulationError::InvalidNativeResult { .. })
        ));
        assert!(matches!(
            validate_output_pointers(&expected, &[first]),
            Err(SimulationError::InvalidNativeResult { .. })
        ));
    }
}
