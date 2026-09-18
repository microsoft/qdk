//! NVIDIA FFI implementations of the private resource and execution traits.
//!
//! Uses the audited cuTensorNet/CUDA ABI; runtime compatibility is checked by
//! discovery. Owners establish handle lifetimes, device affinity and buffer
//! sizes. Each unsafe call documents the remaining foreign-API obligations.

#![allow(
    dead_code,
    reason = "the private native adapter becomes live in the consumer integration iteration"
)]

#[path = "simulation/mps_session.rs"]
mod mps_session;

pub(crate) use mps_session::MpsSession;

use super::CuTensorNetApi;
use crate::bindings::{cudart_12, v2_13};
use crate::simulation::contraction::{
    NativeTensor, OptimizerEstimate, OptimizerSetting, SlicedMode,
};
use crate::simulation::resources::SessionApi;
use crate::simulation::{
    Complex64Abi, ContractionApi, MpsExecutionApi, MpsTarget, OpaqueHandle, OutputMetadata,
    SamplerApi, SimulationError, StateF64Attribute, StateU32Configuration, Stream,
};
use std::{
    ffi::{CStr, c_void},
    mem::size_of,
    ptr::NonNull,
};

impl CuTensorNetApi {
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

    fn configure_sampler_i32(
        &self,
        handle: OpaqueHandle,
        sampler: OpaqueHandle,
        attribute: v2_13::cutensornetSamplerAttributes_t,
        value: i32,
    ) -> Result<(), SimulationError> {
        // SAFETY: the attribute is paired with its audited int32 value and the
        // native API copies the call-local setting before returning.
        let status = unsafe {
            (self.cutensornet_functions.sampler_configure)(
                handle.as_ptr(),
                sampler.as_ptr(),
                attribute,
                (&raw const value).cast::<c_void>(),
                size_of::<i32>(),
            )
        };
        self.check_cutensornet("cutensornetSamplerConfigure", status)
    }
}

impl SamplerApi for CuTensorNetApi {
    fn create_sampler(
        &self,
        handle: OpaqueHandle,
        state: OpaqueHandle,
        modes_to_sample: &[i32],
    ) -> Result<OpaqueHandle, SimulationError> {
        if modes_to_sample.is_empty() {
            return Err(SimulationError::InvalidSamplerConfiguration {
                reason: "at least one state mode must be selected",
            });
        }
        let mode_count = i32::try_from(modes_to_sample.len()).map_err(|_| {
            SimulationError::ResourceSizeOverflow {
                resource: "sampler mode count",
            }
        })?;
        let mut sampler = std::ptr::null_mut();
        // SAFETY: handle/state are live, the mode slice has `mode_count`
        // entries, and `sampler` is a writable out-pointer.
        let status = unsafe {
            (self.cutensornet_functions.create_sampler)(
                handle.as_ptr(),
                state.as_ptr(),
                mode_count,
                modes_to_sample.as_ptr(),
                &raw mut sampler,
            )
        };
        self.check_cutensornet("cutensornetCreateSampler", status)?;
        NonNull::new(sampler).ok_or(SimulationError::MissingNativeResource {
            operation: "cutensornetCreateSampler",
            resource: "state sampler",
        })
    }

    fn destroy_sampler(&self, sampler: OpaqueHandle) -> Result<(), SimulationError> {
        // SAFETY: sampler cleanup consumes the owned sampler exactly once.
        let status = unsafe { (self.cutensornet_functions.destroy_sampler)(sampler.as_ptr()) };
        self.check_cutensornet("cutensornetDestroySampler", status)
    }

    fn configure_sampler_hyper_samples(
        &self,
        handle: OpaqueHandle,
        sampler: OpaqueHandle,
        hyper_samples: i32,
    ) -> Result<(), SimulationError> {
        require_positive(hyper_samples, "sampler hyper-samples must be positive")?;
        self.configure_sampler_i32(
            handle,
            sampler,
            v2_13::cutensornetSamplerAttributes_t_CUTENSORNET_SAMPLER_CONFIG_NUM_HYPER_SAMPLES,
            hyper_samples,
        )
    }

    fn configure_sampler_path_seed(
        &self,
        handle: OpaqueHandle,
        sampler: OpaqueHandle,
        seed: i32,
    ) -> Result<(), SimulationError> {
        require_positive(seed, "sampler pathfinding seed must be positive")?;
        self.configure_sampler_i32(
            handle,
            sampler,
            v2_13::cutensornetSamplerAttributes_t_CUTENSORNET_SAMPLER_CONFIG_DETERMINISTIC,
            seed,
        )
    }

    fn prepare_sampler(
        &self,
        handle: OpaqueHandle,
        sampler: OpaqueHandle,
        maximum_workspace_bytes: usize,
        workspace: OpaqueHandle,
        stream: Stream,
    ) -> Result<(), SimulationError> {
        // SAFETY: every native owner is live and the workspace limit is passed
        // by value from the validated execution policy.
        let status = unsafe {
            (self.cutensornet_functions.sampler_prepare)(
                handle.as_ptr(),
                sampler.as_ptr(),
                maximum_workspace_bytes,
                workspace.as_ptr(),
                stream.as_ptr().cast(),
            )
        };
        self.check_cutensornet("cutensornetSamplerPrepare", status)
    }

    fn configure_sampler_sample_seed(
        &self,
        handle: OpaqueHandle,
        sampler: OpaqueHandle,
        seed: i32,
    ) -> Result<(), SimulationError> {
        require_positive(seed, "sampler sample seed must be positive")?;
        self.configure_sampler_i32(
            handle,
            sampler,
            v2_13::cutensornetSamplerAttributes_t_CUTENSORNET_SAMPLER_CONFIG_DETERMINISTIC,
            seed,
        )
    }

    fn sample(
        &self,
        handle: OpaqueHandle,
        sampler: OpaqueHandle,
        shots: i64,
        workspace: OpaqueHandle,
        output: &mut [i64],
        stream: Stream,
    ) -> Result<(), SimulationError> {
        require_positive(shots, "sampler shot count must be positive")?;
        if output.is_empty() {
            return Err(SimulationError::InvalidSamplerConfiguration {
                reason: "sampler output buffer must not be empty",
            });
        }
        // SAFETY: every native owner is live and the nonempty host output
        // buffer remains writable for the synchronous sampler call.
        let status = unsafe {
            (self.cutensornet_functions.sampler_sample)(
                handle.as_ptr(),
                sampler.as_ptr(),
                shots,
                workspace.as_ptr(),
                output.as_mut_ptr(),
                stream.as_ptr().cast(),
            )
        };
        self.check_cutensornet("cutensornetSamplerSample", status)
    }
}

fn require_positive<T>(value: T, reason: &'static str) -> Result<(), SimulationError>
where
    T: Copy + PartialOrd + From<u8>,
{
    if value > T::from(0) {
        Ok(())
    } else {
        Err(SimulationError::InvalidSamplerConfiguration { reason })
    }
}

impl ContractionApi for CuTensorNetApi {
    fn append_tensor(
        &self,
        handle: OpaqueHandle,
        network: OpaqueHandle,
        tensor: &NativeTensor,
    ) -> Result<i64, SimulationError> {
        let rank = contraction_count(tensor.modes.len())?;
        if tensor.extents.len() != tensor.modes.len() {
            return Err(SimulationError::InvalidContractionConfiguration {
                reason: "tensor modes and extents have different lengths",
            });
        }
        let mut id = 0;
        // SAFETY: topology conversion checked rank and extents; both arrays
        // contain rank entries. NULL qualifiers select the SDK defaults.
        let status = unsafe {
            (self.cutensornet_functions.network_append_tensor)(
                handle.as_ptr(),
                network.as_ptr(),
                rank,
                tensor.extents.as_ptr(),
                tensor.modes.as_ptr(),
                std::ptr::null(),
                v2_13::cudaDataType_t_CUDA_C_64F,
                &raw mut id,
            )
        };
        self.check_cutensornet("cutensornetNetworkAppendTensor", status)?;
        Ok(id)
    }

    fn set_output(
        &self,
        handle: OpaqueHandle,
        network: OpaqueHandle,
        modes: &[i32],
    ) -> Result<(), SimulationError> {
        let rank = contraction_count(modes.len())?;
        // SAFETY: modes contains rank initialized labels; the query validated
        // their membership and order. No output coefficient memory is needed.
        let status = unsafe {
            (self.cutensornet_functions.network_set_output_tensor)(
                handle.as_ptr(),
                network.as_ptr(),
                rank,
                modes.as_ptr(),
                v2_13::cudaDataType_t_CUDA_C_64F,
            )
        };
        self.check_cutensornet("cutensornetNetworkSetOutputTensor", status)
    }

    fn set_compute_f64(
        &self,
        handle: OpaqueHandle,
        network: OpaqueHandle,
    ) -> Result<(), SimulationError> {
        let compute = v2_13::cutensornetComputeType_t_CUTENSORNET_COMPUTE_64F;
        // SAFETY: this attribute takes exactly a cutensornetComputeType_t.
        let status = unsafe {
            (self.cutensornet_functions.network_set_attribute)(
                handle.as_ptr(),
                network.as_ptr(),
                v2_13::cutensornetNetworkAttributes_t_CUTENSORNET_NETWORK_COMPUTE_TYPE,
                (&raw const compute).cast(),
                size_of::<v2_13::cutensornetComputeType_t>(),
            )
        };
        self.check_cutensornet("cutensornetNetworkSetAttribute(COMPUTE_TYPE)", status)
    }

    fn configure_optimizer(
        &self,
        handle: OpaqueHandle,
        config: OpaqueHandle,
        setting: OptimizerSetting,
        value: i32,
    ) -> Result<(), SimulationError> {
        let attribute = match setting {
            OptimizerSetting::HyperSamples => v2_13::cutensornetContractionOptimizerConfigAttributes_t_CUTENSORNET_CONTRACTION_OPTIMIZER_CONFIG_HYPER_NUM_SAMPLES,
            OptimizerSetting::Threads => v2_13::cutensornetContractionOptimizerConfigAttributes_t_CUTENSORNET_CONTRACTION_OPTIMIZER_CONFIG_HYPER_NUM_THREADS,
            OptimizerSetting::Seed => v2_13::cutensornetContractionOptimizerConfigAttributes_t_CUTENSORNET_CONTRACTION_OPTIMIZER_CONFIG_SEED,
            OptimizerSetting::ReconfigurationIterations => v2_13::cutensornetContractionOptimizerConfigAttributes_t_CUTENSORNET_CONTRACTION_OPTIMIZER_CONFIG_RECONFIG_NUM_ITERATIONS,
            OptimizerSetting::DisableRankSimplification => v2_13::cutensornetContractionOptimizerConfigAttributes_t_CUTENSORNET_CONTRACTION_OPTIMIZER_CONFIG_SIMPLIFICATION_DISABLE_DR,
            OptimizerSetting::DisableSlicing => v2_13::cutensornetContractionOptimizerConfigAttributes_t_CUTENSORNET_CONTRACTION_OPTIMIZER_CONFIG_SLICER_DISABLE_SLICING,
        };
        // SAFETY: all six selected attributes have int32_t payloads.
        let status = unsafe {
            (self.cutensornet_functions.optimizer_config_set_attribute)(
                handle.as_ptr(),
                config.as_ptr(),
                attribute,
                (&raw const value).cast(),
                size_of::<i32>(),
            )
        };
        self.check_cutensornet("cutensornetContractionOptimizerConfigSetAttribute", status)
    }

    fn optimize(
        &self,
        handle: OpaqueHandle,
        network: OpaqueHandle,
        config: OpaqueHandle,
        workspace_constraint: u64,
        info: OpaqueHandle,
    ) -> Result<(), SimulationError> {
        // SAFETY: the owner established topology before info creation and keeps
        // all objects live. The constraint is a byte budget, not an allocation.
        let status = unsafe {
            (self.cutensornet_functions.contraction_optimize)(
                handle.as_ptr(),
                network.as_ptr(),
                config.as_ptr(),
                workspace_constraint,
                info.as_ptr(),
            )
        };
        self.check_cutensornet("cutensornetContractionOptimize", status)
    }

    fn set_path(
        &self,
        handle: OpaqueHandle,
        info: OpaqueHandle,
        path: &[[i32; 2]],
    ) -> Result<(), SimulationError> {
        let mut pairs: Vec<_> = path
            .iter()
            .map(|&[first, second]| v2_13::cutensornetNodePair_t { first, second })
            .collect();
        let payload = v2_13::cutensornetContractionPath_t {
            numContractions: contraction_count(pairs.len())?,
            data: pairs.as_mut_ptr(),
        };
        // SAFETY: PATH takes this payload and copies the live pair array.
        unsafe {
            self.set_optimizer_info(
            handle, info,
            v2_13::cutensornetContractionOptimizerInfoAttributes_t_CUTENSORNET_CONTRACTION_OPTIMIZER_INFO_PATH,
            &payload,
        )
        }
    }

    fn set_slicing(
        &self,
        handle: OpaqueHandle,
        info: OpaqueHandle,
        slicing: &[SlicedMode],
    ) -> Result<(), SimulationError> {
        let mut pairs: Vec<_> = slicing
            .iter()
            .map(|slice| v2_13::cutensornetSliceInfoPair_t {
                slicedMode: slice.mode,
                slicedExtent: slice.extent,
            })
            .collect();
        let payload = v2_13::cutensornetSlicingConfig_t {
            numSlicedModes: u32::try_from(pairs.len()).map_err(|_| {
                SimulationError::ResourceSizeOverflow {
                    resource: "sliced mode count",
                }
            })?,
            data: pairs.as_mut_ptr(),
        };
        // SAFETY: SLICING_CONFIG takes this payload and copies its live array.
        unsafe {
            self.set_optimizer_info(
            handle, info,
            v2_13::cutensornetContractionOptimizerInfoAttributes_t_CUTENSORNET_CONTRACTION_OPTIMIZER_INFO_SLICING_CONFIG,
            &payload,
        )
        }
    }

    fn attach_optimizer_info(
        &self,
        handle: OpaqueHandle,
        network: OpaqueHandle,
        info: OpaqueHandle,
    ) -> Result<(), SimulationError> {
        // SAFETY: the matching network and populated info remain live together.
        let status = unsafe {
            (self.cutensornet_functions.network_set_optimizer_info)(
                handle.as_ptr(),
                network.as_ptr(),
                info.as_ptr(),
            )
        };
        self.check_cutensornet("cutensornetNetworkSetOptimizerInfo", status)
    }

    fn read_path(
        &self,
        handle: OpaqueHandle,
        info: OpaqueHandle,
        path: &mut [[i32; 2]],
    ) -> Result<i32, SimulationError> {
        let mut pairs = vec![
            v2_13::cutensornetNodePair_t {
                first: -1,
                second: -1
            };
            path.len()
        ];
        let mut payload = [v2_13::cutensornetContractionPath_t {
            numContractions: contraction_count(path.len())?,
            data: pairs.as_mut_ptr(),
        }];
        // SAFETY: PATH writes into caller storage for numInputs - 1 pairs,
        // as in the pinned SDK's OptimizerInfoInterface.path getter.
        unsafe {
            self.get_optimizer_info(
            handle, info,
            v2_13::cutensornetContractionOptimizerInfoAttributes_t_CUTENSORNET_CONTRACTION_OPTIMIZER_INFO_PATH,
            &mut payload,
        )?;
        }
        if payload[0].data != pairs.as_mut_ptr() {
            return Err(invalid_metadata(
                "native path getter changed the caller's data pointer",
            ));
        }
        for (target, pair) in path.iter_mut().zip(pairs) {
            *target = [pair.first, pair.second];
        }
        Ok(payload[0].numContractions)
    }

    fn num_sliced_modes(
        &self,
        handle: OpaqueHandle,
        info: OpaqueHandle,
    ) -> Result<i32, SimulationError> {
        let mut value = [-1_i32];
        // SAFETY: NUM_SLICED_MODES is one int32_t.
        unsafe {
            self.get_optimizer_info(
            handle, info,
            v2_13::cutensornetContractionOptimizerInfoAttributes_t_CUTENSORNET_CONTRACTION_OPTIMIZER_INFO_NUM_SLICED_MODES,
            &mut value,
        )?;
        }
        Ok(value[0])
    }

    fn read_slicing(
        &self,
        handle: OpaqueHandle,
        info: OpaqueHandle,
        slicing: &mut [SlicedMode],
    ) -> Result<u32, SimulationError> {
        let mut pairs = vec![
            v2_13::cutensornetSliceInfoPair_t {
                slicedMode: -1,
                slicedExtent: 0,
            };
            slicing.len()
        ];
        let mut payload = [v2_13::cutensornetSlicingConfig_t {
            numSlicedModes: u32::try_from(slicing.len()).map_err(|_| {
                SimulationError::ResourceSizeOverflow {
                    resource: "sliced mode count",
                }
            })?,
            data: pairs.as_mut_ptr(),
        }];
        // SAFETY: the preceding count getter sized the owned array; this
        // payload is the audited SLICING_CONFIG ABI.
        unsafe {
            self.get_optimizer_info(
            handle, info,
            v2_13::cutensornetContractionOptimizerInfoAttributes_t_CUTENSORNET_CONTRACTION_OPTIMIZER_INFO_SLICING_CONFIG,
            &mut payload,
        )?;
        }
        if payload[0].data != pairs.as_mut_ptr() {
            return Err(invalid_metadata(
                "native slicing getter changed the caller's data pointer",
            ));
        }
        for (target, pair) in slicing.iter_mut().zip(pairs) {
            *target = SlicedMode {
                mode: pair.slicedMode,
                extent: pair.slicedExtent,
            };
        }
        Ok(payload[0].numSlicedModes)
    }

    fn num_slices(&self, handle: OpaqueHandle, info: OpaqueHandle) -> Result<i64, SimulationError> {
        let mut value = [-1_i64];
        // SAFETY: NUM_SLICES is one int64_t.
        unsafe {
            self.get_optimizer_info(
            handle, info,
            v2_13::cutensornetContractionOptimizerInfoAttributes_t_CUTENSORNET_CONTRACTION_OPTIMIZER_INFO_NUM_SLICES,
            &mut value,
        )?;
        }
        Ok(value[0])
    }

    fn intermediate_mode_counts(
        &self,
        handle: OpaqueHandle,
        info: OpaqueHandle,
        counts: &mut [i32],
    ) -> Result<(), SimulationError> {
        // SAFETY: NUM_INTERMEDIATE_MODES writes numInputs - 1 int32_t ranks.
        unsafe {
            self.get_optimizer_info(
            handle, info,
            v2_13::cutensornetContractionOptimizerInfoAttributes_t_CUTENSORNET_CONTRACTION_OPTIMIZER_INFO_NUM_INTERMEDIATE_MODES,
            counts,
        )
        }
    }

    fn intermediate_modes(
        &self,
        handle: OpaqueHandle,
        info: OpaqueHandle,
        modes: &mut [i32],
    ) -> Result<(), SimulationError> {
        // SAFETY: the owner sized this int32_t array from validated native ranks.
        unsafe {
            self.get_optimizer_info(
            handle, info,
            v2_13::cutensornetContractionOptimizerInfoAttributes_t_CUTENSORNET_CONTRACTION_OPTIMIZER_INFO_INTERMEDIATE_MODES,
            modes,
        )
        }
    }

    fn estimate(
        &self,
        handle: OpaqueHandle,
        info: OpaqueHandle,
        estimate: OptimizerEstimate,
    ) -> Result<f64, SimulationError> {
        let attribute = match estimate {
            OptimizerEstimate::FlopCount => v2_13::cutensornetContractionOptimizerInfoAttributes_t_CUTENSORNET_CONTRACTION_OPTIMIZER_INFO_FLOP_COUNT,
            OptimizerEstimate::LargestTensor => v2_13::cutensornetContractionOptimizerInfoAttributes_t_CUTENSORNET_CONTRACTION_OPTIMIZER_INFO_LARGEST_TENSOR,
        };
        let mut value = [f64::NAN];
        // SAFETY: both selected estimates have one double payload.
        unsafe {
            self.get_optimizer_info(handle, info, attribute, &mut value)?;
        }
        Ok(value[0])
    }

    fn create_network(&self, handle: OpaqueHandle) -> Result<OpaqueHandle, SimulationError> {
        let mut network = std::ptr::null_mut();
        // SAFETY: `handle` is a live cuTensorNet context and `network` is a
        // valid writable out-pointer.
        let status = unsafe {
            (self.cutensornet_functions.create_network)(handle.as_ptr(), &raw mut network)
        };
        self.check_cutensornet("cutensornetCreateNetwork", status)?;
        NonNull::new(network).ok_or(SimulationError::MissingNativeResource {
            operation: "cutensornetCreateNetwork",
            resource: "tensor network descriptor",
        })
    }

    fn destroy_network(&self, network: OpaqueHandle) -> Result<(), SimulationError> {
        // SAFETY: the owning resources consume this descriptor exactly once,
        // after every object derived from it has been destroyed.
        let status = unsafe { (self.cutensornet_functions.destroy_network)(network.as_ptr()) };
        self.check_cutensornet("cutensornetDestroyNetwork", status)
    }

    fn create_optimizer_config(
        &self,
        handle: OpaqueHandle,
    ) -> Result<OpaqueHandle, SimulationError> {
        let mut config = std::ptr::null_mut();
        // SAFETY: `handle` is a live cuTensorNet context and `config` is a
        // valid writable out-pointer.
        let status = unsafe {
            (self.cutensornet_functions.create_optimizer_config)(handle.as_ptr(), &raw mut config)
        };
        self.check_cutensornet("cutensornetCreateContractionOptimizerConfig", status)?;
        NonNull::new(config).ok_or(SimulationError::MissingNativeResource {
            operation: "cutensornetCreateContractionOptimizerConfig",
            resource: "contraction optimizer config",
        })
    }

    fn destroy_optimizer_config(&self, config: OpaqueHandle) -> Result<(), SimulationError> {
        // SAFETY: the owning resources consume this config exactly once.
        let status =
            unsafe { (self.cutensornet_functions.destroy_optimizer_config)(config.as_ptr()) };
        self.check_cutensornet("cutensornetDestroyContractionOptimizerConfig", status)
    }

    fn create_optimizer_info(
        &self,
        handle: OpaqueHandle,
        network: OpaqueHandle,
    ) -> Result<OpaqueHandle, SimulationError> {
        let mut info = std::ptr::null_mut();
        // SAFETY: `handle` and `network` are live, `network` outlives the
        // returned info because the owning resources destroy it first, and
        // `info` is a valid writable out-pointer.
        let status = unsafe {
            (self.cutensornet_functions.create_optimizer_info)(
                handle.as_ptr(),
                network.as_ptr(),
                &raw mut info,
            )
        };
        self.check_cutensornet("cutensornetCreateContractionOptimizerInfo", status)?;
        NonNull::new(info).ok_or(SimulationError::MissingNativeResource {
            operation: "cutensornetCreateContractionOptimizerInfo",
            resource: "contraction optimizer info",
        })
    }

    fn destroy_optimizer_info(&self, info: OpaqueHandle) -> Result<(), SimulationError> {
        // SAFETY: the owning resources consume this info exactly once, before
        // the network descriptor it was created from is destroyed.
        let status = unsafe { (self.cutensornet_functions.destroy_optimizer_info)(info.as_ptr()) };
        self.check_cutensornet("cutensornetDestroyContractionOptimizerInfo", status)
    }

    fn create_slice_group_from_id_range(
        &self,
        handle: OpaqueHandle,
        start: i64,
        stop: i64,
        increment: i64,
    ) -> Result<OpaqueHandle, SimulationError> {
        let mut slice_group = std::ptr::null_mut();
        // SAFETY: `handle` is live, the caller has rejected a zero increment, and
        // `slice_group` is a valid writable out-pointer.
        let status = unsafe {
            (self.cutensornet_functions.create_slice_group_from_id_range)(
                handle.as_ptr(),
                start,
                stop,
                increment,
                &raw mut slice_group,
            )
        };
        self.check_cutensornet("cutensornetCreateSliceGroupFromIDRange", status)?;
        NonNull::new(slice_group).ok_or(SimulationError::MissingNativeResource {
            operation: "cutensornetCreateSliceGroupFromIDRange",
            resource: "contraction slice group",
        })
    }

    fn destroy_slice_group(&self, slice_group: OpaqueHandle) -> Result<(), SimulationError> {
        // SAFETY: the owning slice group consumes this object exactly once.
        let status =
            unsafe { (self.cutensornet_functions.destroy_slice_group)(slice_group.as_ptr()) };
        self.check_cutensornet("cutensornetDestroySliceGroup", status)
    }
}

fn contraction_count(count: usize) -> Result<i32, SimulationError> {
    i32::try_from(count).map_err(|_| SimulationError::ResourceSizeOverflow {
        resource: "contraction metadata count",
    })
}

fn invalid_metadata(reason: &'static str) -> SimulationError {
    SimulationError::InvalidNativeResult {
        reason: reason.to_string(),
    }
}

impl CuTensorNetApi {
    /// Reads one optimizer-info attribute into caller-owned storage.
    ///
    /// # Safety
    /// The context and info must be live and associated. `T`, its alignment and
    /// the slice length must match the attribute's ABI. Nested output buffers
    /// must be writable, non-aliasing and large enough for the native counts;
    /// all buffers must remain live throughout the call.
    unsafe fn get_optimizer_info<T>(
        &self,
        handle: OpaqueHandle,
        info: OpaqueHandle,
        attribute: v2_13::cutensornetContractionOptimizerInfoAttributes_t,
        values: &mut [T],
    ) -> Result<(), SimulationError> {
        // SAFETY: the caller guarantees the payload and nested-buffer contract.
        let status = unsafe {
            (self.cutensornet_functions.optimizer_info_get_attribute)(
                handle.as_ptr(),
                info.as_ptr(),
                attribute,
                values.as_mut_ptr().cast(),
                std::mem::size_of_val(values),
            )
        };
        self.check_cutensornet("cutensornetContractionOptimizerInfoGetAttribute", status)
    }

    /// Copies one attribute payload into the native optimizer-info object.
    ///
    /// # Safety
    /// The context and info must be live and associated. `T` must match the
    /// attribute's ABI. Nested pointers must address initialized input arrays
    /// of the declared lengths, retained for the call. Only attributes that
    /// copy their input, rather than retaining its pointers, may use this helper.
    unsafe fn set_optimizer_info<T>(
        &self,
        handle: OpaqueHandle,
        info: OpaqueHandle,
        attribute: v2_13::cutensornetContractionOptimizerInfoAttributes_t,
        value: &T,
    ) -> Result<(), SimulationError> {
        // SAFETY: the caller guarantees the payload and nested-buffer contract.
        let status = unsafe {
            (self.cutensornet_functions.optimizer_info_set_attribute)(
                handle.as_ptr(),
                info.as_ptr(),
                attribute,
                std::ptr::from_ref(value).cast(),
                size_of::<T>(),
            )
        };
        self.check_cutensornet("cutensornetContractionOptimizerInfoSetAttribute", status)
    }
}

impl SessionApi for CuTensorNetApi {
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

impl MpsExecutionApi for CuTensorNetApi {
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
        // cuTensorNet declares the two pointer tables as mutable even though it
        // only reads them, so they are passed as `*mut` over owned local tables.
        let status = unsafe {
            (self.cutensornet_functions.append_product)(
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
