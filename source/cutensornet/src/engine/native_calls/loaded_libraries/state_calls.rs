// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! `StateCalls` on the loaded libraries: cuTensorNet's states, network
//! operators and expectations.

use crate::engine::{
    EngineError,
    native_calls::{
        OpaqueHandle, Stream,
        complex::Complex64Abi,
        state_calls::{
            MpsLayout, NativeCapacity, StateCalls, StateF64Configuration, StateU32Configuration,
        },
    },
};
use crate::libraries::{LoadedLibraries, cutensornet::v2_13};
use std::{ffi::c_void, ptr::NonNull};

impl StateCalls for LoadedLibraries {
    fn create_state(
        &self,
        handle: OpaqueHandle,
        mode_extents: &[i64],
    ) -> Result<OpaqueHandle, EngineError> {
        let mode_count =
            i32::try_from(mode_extents.len()).map_err(|_| EngineError::ResourceSizeOverflow {
                resource: "state mode count",
            })?;
        let mut state = std::ptr::null_mut();
        // SAFETY: the handle is live, the retained extent slice contains
        // `mode_count` entries, and `state` is a writable out-pointer.
        let status = unsafe {
            (self.cutensornet.functions.create_state)(
                handle.as_ptr(),
                v2_13::cutensornetStatePurity_t_CUTENSORNET_STATE_PURITY_PURE,
                mode_count,
                mode_extents.as_ptr(),
                v2_13::cudaDataType_t_CUDA_C_64F,
                &raw mut state,
            )
        };
        self.check_cutensornet("cutensornetCreateState", status)?;
        NonNull::new(state).ok_or(EngineError::MissingNativeResource {
            operation: "cutensornetCreateState",
            resource: "cuTensorNet state",
        })
    }

    fn destroy_state(&self, state: OpaqueHandle) -> Result<(), EngineError> {
        // SAFETY: `NativeState` (its `close`, or a restart) destroys each
        // state exactly once.
        let status = unsafe { (self.cutensornet.functions.destroy_state)(state.as_ptr()) };
        self.check_cutensornet("cutensornetDestroyState", status)
    }

    fn apply_tensor_operator(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        modes: &[i32],
        tensor: OpaqueHandle,
        unitary: bool,
    ) -> Result<(), EngineError> {
        let mode_count =
            i32::try_from(modes.len()).map_err(|_| EngineError::ResourceSizeOverflow {
                resource: "operator mode count",
            })?;
        let mut tensor_id = 0;
        // SAFETY: handle/state and device tensor are live; the caller retains
        // `modes` and tensor storage through state destruction. Null strides
        // select the audited textbook row-major gate interpretation.
        let status = unsafe {
            (self.cutensornet.functions.apply_tensor_operator)(
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
        capacity: &NativeCapacity,
    ) -> Result<(), EngineError> {
        // SAFETY: the capacity owns two retained extent arrays and their
        // pointer table through state destruction; null strides request
        // native layout.
        let status = unsafe {
            (self.cutensornet.functions.finalize_mps)(
                handle.as_ptr(),
                state.as_ptr(),
                v2_13::cutensornetBoundaryCondition_t_CUTENSORNET_BOUNDARY_CONDITION_OPEN,
                capacity.extent_pointers().as_ptr(),
                std::ptr::null(),
            )
        };
        self.check_cutensornet("cutensornetStateFinalizeMPS", status)
    }

    fn initialize_mps(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        layout: &MpsLayout,
        tensors: &[OpaqueHandle],
    ) -> Result<(), EngineError> {
        if layout.extents.len() != tensors.len() || layout.strides.len() != tensors.len() {
            return Err(EngineError::InvalidNativeCall {
                reason: "an initial MPS needs one extent, stride and tensor per site".to_string(),
            });
        }
        let extent_pointers = layout
            .extents
            .iter()
            .map(|extents| extents.as_ptr())
            .collect::<Vec<_>>();
        let stride_pointers = layout
            .strides
            .iter()
            .map(|strides| strides.as_ptr())
            .collect::<Vec<_>>();
        let mut tensor_pointers = tensors
            .iter()
            .map(|tensor| tensor.as_ptr())
            .collect::<Vec<_>>();
        // SAFETY: handle and state are live; each table has one entry per
        // state mode and outlives the call. The tensors are device
        // allocations the caller keeps until the state has been computed and
        // captured again (or destroyed), since the library may read them at
        // the next compute; whether it copies them is not documented.
        let status = unsafe {
            (self.cutensornet.functions.initialize_mps)(
                handle.as_ptr(),
                state.as_ptr(),
                v2_13::cutensornetBoundaryCondition_t_CUTENSORNET_BOUNDARY_CONDITION_OPEN,
                extent_pointers.as_ptr(),
                stride_pointers.as_ptr(),
                tensor_pointers.as_mut_ptr(),
            )
        };
        self.check_cutensornet("cutensornetStateInitializeMPS", status)
    }

    fn capture_mps(&self, handle: OpaqueHandle, state: OpaqueHandle) -> Result<(), EngineError> {
        // SAFETY: handle and state are live and the state has been computed
        // into output buffers the caller keeps until the next compute after
        // this one. The call deletes the state's tensor operators; their
        // device tensors and modes stay allocated until the state is
        // destroyed, so no pointer the library may still hold dangles.
        let status =
            unsafe { (self.cutensornet.functions.capture_mps)(handle.as_ptr(), state.as_ptr()) };
        self.check_cutensornet("cutensornetStateCaptureMPS", status)
    }

    fn configure_state_f64(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        configuration: StateF64Configuration,
    ) -> Result<(), EngineError> {
        let (attribute, value) = match configuration {
            StateF64Configuration::SvdDiscardedWeightCutoff(value) => (
                v2_13::cutensornetStateAttributes_t_CUTENSORNET_STATE_CONFIG_MPS_SVD_DISCARDED_WEIGHT_CUTOFF,
                value,
            ),
        };
        // SAFETY: the attribute is paired with its audited f64 representation,
        // and cuTensorNet copies the call-local value before returning.
        let status = unsafe {
            (self.cutensornet.functions.state_configure)(
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
    ) -> Result<(), EngineError> {
        let (attribute, value) = match configuration {
            StateU32Configuration::SvdNormalizationNone => (
                v2_13::cutensornetStateAttributes_t_CUTENSORNET_STATE_CONFIG_MPS_SVD_S_NORMALIZATION,
                v2_13::cutensornetTensorSVDNormalization_t_CUTENSORNET_TENSOR_SVD_NORMALIZATION_NONE,
            ),
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
            (self.cutensornet.functions.state_configure)(
                handle.as_ptr(),
                state.as_ptr(),
                attribute,
                (&raw const value).cast(),
                size_of::<u32>(),
            )
        };
        self.check_cutensornet("cutensornetStateConfigure(u32)", status)
    }

    fn prepare_state(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        maximum_workspace_bytes: usize,
        workspace: OpaqueHandle,
        stream: Stream,
    ) -> Result<(), EngineError> {
        // SAFETY: every native owner is live and the workspace ceiling is a
        // by-value snapshot from cudaMemGetInfo.
        let status = unsafe {
            (self.cutensornet.functions.state_prepare)(
                handle.as_ptr(),
                state.as_ptr(),
                maximum_workspace_bytes,
                workspace.as_ptr(),
                stream.as_ptr().cast(),
            )
        };
        self.check_cutensornet("cutensornetStatePrepare", status)
    }

    fn compute_state(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        workspace: OpaqueHandle,
        layout: &mut MpsLayout,
        outputs: &mut [OpaqueHandle],
        stream: Stream,
    ) -> Result<(), EngineError> {
        let mut extent_pointers = layout
            .extents
            .iter_mut()
            .map(|extents| extents.as_mut_ptr())
            .collect::<Vec<_>>();
        let mut stride_pointers = layout
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
            (self.cutensornet.functions.state_compute)(
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

    fn create_network_operator(
        &self,
        handle: OpaqueHandle,
        mode_extents: &[i64],
    ) -> Result<OpaqueHandle, EngineError> {
        let mode_count =
            i32::try_from(mode_extents.len()).map_err(|_| EngineError::ResourceSizeOverflow {
                resource: "network operator mode count",
            })?;
        let mut operator = std::ptr::null_mut();
        // SAFETY: the handle is live, the extent slice has `mode_count`
        // entries, and `operator` is a writable out-pointer.
        let status = unsafe {
            (self.cutensornet.functions.create_network_operator)(
                handle.as_ptr(),
                mode_count,
                mode_extents.as_ptr(),
                v2_13::cudaDataType_t_CUDA_C_64F,
                &raw mut operator,
            )
        };
        self.check_cutensornet("cutensornetCreateNetworkOperator", status)?;
        NonNull::new(operator).ok_or(EngineError::MissingNativeResource {
            operation: "cutensornetCreateNetworkOperator",
            resource: "network operator",
        })
    }

    fn destroy_network_operator(&self, operator: OpaqueHandle) -> Result<(), EngineError> {
        // SAFETY: `NativeState`'s query cleanup destroys each operator
        // exactly once.
        let status =
            unsafe { (self.cutensornet.functions.destroy_network_operator)(operator.as_ptr()) };
        self.check_cutensornet("cutensornetDestroyNetworkOperator", status)
    }

    fn append_product(
        &self,
        handle: OpaqueHandle,
        operator: OpaqueHandle,
        coefficient: num_complex::Complex64,
        factor_modes: &[Box<[i32]>],
        factor_tensors: &[OpaqueHandle],
    ) -> Result<(), EngineError> {
        if factor_modes.is_empty() || factor_modes.len() != factor_tensors.len() {
            return Err(EngineError::InvalidNativeCall {
                reason: "expectation product has no factor, or factors and modes differ in number"
                    .to_string(),
            });
        }
        let coefficient = v2_13::double2 {
            x: coefficient.re,
            y: coefficient.im,
        };
        let factor_count =
            i32::try_from(factor_modes.len()).map_err(|_| EngineError::ResourceSizeOverflow {
                resource: "expectation factor count",
            })?;
        let mode_counts = factor_modes
            .iter()
            .map(|modes| {
                i32::try_from(modes.len()).map_err(|_| EngineError::ResourceSizeOverflow {
                    resource: "expectation factor mode count",
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut mode_pointers = factor_modes
            .iter()
            .map(|modes| modes.as_ptr())
            .collect::<Vec<_>>();
        let mut tensor_pointers = factor_tensors
            .iter()
            .map(|tensor| tensor.as_ptr().cast_const())
            .collect::<Vec<_>>();
        let mut component_id = 0;
        // SAFETY: all pointer tables contain `factor_count` retained entries;
        // operator tensors remain allocated until operator destruction. Null
        // strides request default layout for each single-mode Pauli factor.
        // cuTensorNet declares the two pointer tables as mutable even though
        // it only reads them, so they are passed as `*mut` over owned local
        // tables.
        let status = unsafe {
            (self.cutensornet.functions.append_product)(
                handle.as_ptr(),
                operator.as_ptr(),
                coefficient,
                factor_count,
                mode_counts.as_ptr(),
                mode_pointers.as_mut_ptr(),
                std::ptr::null_mut(),
                tensor_pointers.as_mut_ptr(),
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
    ) -> Result<OpaqueHandle, EngineError> {
        let mut expectation = std::ptr::null_mut();
        // SAFETY: handle/state/operator are live and `expectation` is writable.
        let status = unsafe {
            (self.cutensornet.functions.create_expectation)(
                handle.as_ptr(),
                state.as_ptr(),
                operator.as_ptr(),
                &raw mut expectation,
            )
        };
        self.check_cutensornet("cutensornetCreateExpectation", status)?;
        NonNull::new(expectation).ok_or(EngineError::MissingNativeResource {
            operation: "cutensornetCreateExpectation",
            resource: "state expectation",
        })
    }

    fn destroy_expectation(&self, expectation: OpaqueHandle) -> Result<(), EngineError> {
        // SAFETY: `NativeState`'s query cleanup destroys each expectation
        // exactly once.
        let status =
            unsafe { (self.cutensornet.functions.destroy_expectation)(expectation.as_ptr()) };
        self.check_cutensornet("cutensornetDestroyExpectation", status)
    }

    fn configure_expectation_hyper_samples(
        &self,
        handle: OpaqueHandle,
        expectation: OpaqueHandle,
        hyper_samples: i32,
    ) -> Result<(), EngineError> {
        // SAFETY: the attribute is paired with its audited int32 value and the
        // native API copies the call-local setting before returning.
        let status = unsafe {
            (self.cutensornet.functions.expectation_configure)(
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
    ) -> Result<(), EngineError> {
        // SAFETY: every native owner is live and the workspace limit is passed
        // by value.
        let status = unsafe {
            (self.cutensornet.functions.expectation_prepare)(
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
    ) -> Result<(num_complex::Complex64, num_complex::Complex64), EngineError> {
        let mut value = Complex64Abi::default();
        let mut norm = Complex64Abi::default();
        // SAFETY: native owners are live and both host result buffers are valid
        // writable complex-f64 values for the duration of the call, which
        // returns only once the stream is synchronized. NVIDIA documents
        // this call as synchronous only "in the current release": "the
        // execution of this API function will synchronize the provided CUDA
        // stream. This restriction may be released in the future". The
        // exact-version policy admits only 2.13.0; re-check this when it
        // admits another version.
        let status = unsafe {
            (self.cutensornet.functions.expectation_compute)(
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
) -> Result<(), EngineError> {
    if actual == expected {
        Ok(())
    } else {
        Err(EngineError::InvalidNativeResult {
            reason: "cutensornetStateCompute replaced caller-owned output pointers".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{EngineError, validate_output_pointers};
    use std::ffi::c_void;

    #[test]
    fn state_compute_must_preserve_caller_owned_output_pointers() {
        let first = 0x100_usize as *mut c_void;
        let second = 0x200_usize as *mut c_void;
        let expected = [first, second];

        validate_output_pointers(&expected, &expected)
            .expect("unchanged caller-owned pointers should be accepted");
        assert!(matches!(
            validate_output_pointers(&expected, &[first, first]),
            Err(EngineError::InvalidNativeResult { .. })
        ));
        assert!(matches!(
            validate_output_pointers(&expected, &[first]),
            Err(EngineError::InvalidNativeResult { .. })
        ));
    }
}
