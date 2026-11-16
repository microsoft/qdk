// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! `TensorCalls` on the loaded libraries: cuTensorNet's tensor API, through
//! its optional table (`CuTensorNetFunctions::tensor`).
//!
//! Every call first takes that table, so on a cuTensorNet that lacks a
//! function of it, every call fails with
//! `NativeError::MissingNativeFunctions`, naming each missing one. The
//! descriptors, and the tensors of networks and their buffers, take null
//! strides (first mode fastest) and no qualifiers; cuTensorNet copies their
//! extents and labels, so the caller's slices need live only during the
//! call. A network computes in `CUTENSORNET_COMPUTE_64F`, the default
//! `cutensornetCreateNetwork` documents for `CUDA_C_64F` tensors.

use crate::libraries::cutensornet::v2_13;
use crate::libraries::{LoadedLibraries, cutensornet::tensor::CuTensorNetTensorFunctions};
use crate::native_calls::error::{NativeError, combine_execution_and_cleanup};
use crate::native_calls::{
    OpaqueHandle, Stream,
    tensor_calls::{
        ContractionNetwork, GateSplitDescriptors, GateSplitOperands, SvdSetting, SvdTruncation,
        TensorCalls, TensorModes, TensorOperand,
    },
};
use std::{ffi::c_void, ptr::NonNull};

impl TensorCalls for LoadedLibraries {
    fn check_tensor_functions(&self) -> Result<(), NativeError> {
        self.tensor_functions().map(|_| ())
    }

    fn create_tensor_descriptor(
        &self,
        handle: OpaqueHandle,
        tensor: TensorModes<'_>,
    ) -> Result<OpaqueHandle, NativeError> {
        let functions = self.tensor_functions()?;
        let mode_count = mode_count(tensor)?;
        let mut descriptor = std::ptr::null_mut();
        // SAFETY: the handle is live; `extents` and `modes` hold `mode_count`
        // entries each, which cuTensorNet copies; null strides mean the
        // first mode is fastest; `descriptor` is a writable out-pointer.
        let status = unsafe {
            (functions.create_tensor_descriptor)(
                handle.as_ptr(),
                mode_count,
                tensor.extents.as_ptr(),
                std::ptr::null(),
                tensor.modes.as_ptr(),
                v2_13::cudaDataType_t_CUDA_C_64F,
                &raw mut descriptor,
            )
        };
        self.check_cutensornet("cutensornetCreateTensorDescriptor", status)?;
        created(
            descriptor,
            "cutensornetCreateTensorDescriptor",
            "tensor descriptor",
        )
    }

    fn destroy_tensor_descriptor(&self, descriptor: OpaqueHandle) -> Result<(), NativeError> {
        let functions = self.tensor_functions()?;
        // SAFETY: its owner destroys each descriptor exactly once.
        let status = unsafe { (functions.destroy_tensor_descriptor)(descriptor.as_ptr()) };
        self.check_cutensornet("cutensornetDestroyTensorDescriptor", status)
    }

    fn create_svd_config(&self, handle: OpaqueHandle) -> Result<OpaqueHandle, NativeError> {
        let functions = self.tensor_functions()?;
        let mut config = std::ptr::null_mut();
        // SAFETY: the handle is live and `config` is a writable out-pointer.
        let status = unsafe { (functions.create_svd_config)(handle.as_ptr(), &raw mut config) };
        self.check_cutensornet("cutensornetCreateTensorSVDConfig", status)?;
        created(
            config,
            "cutensornetCreateTensorSVDConfig",
            "SVD configuration",
        )
    }

    fn configure_svd(
        &self,
        handle: OpaqueHandle,
        config: OpaqueHandle,
        setting: SvdSetting,
    ) -> Result<(), NativeError> {
        let functions = self.tensor_functions()?;
        let set = |attribute, value: *const c_void, bytes| {
            // SAFETY: the handle and the config are live, and `value` points
            // to `bytes` bytes of the attribute's declared type.
            let status = unsafe {
                (functions.svd_config_set_attribute)(
                    handle.as_ptr(),
                    config.as_ptr(),
                    attribute,
                    value,
                    bytes,
                )
            };
            self.check_cutensornet("cutensornetTensorSVDConfigSetAttribute", status)
        };
        let choice = |attribute, value: std::os::raw::c_uint| {
            set(attribute, (&raw const value).cast(), size_of_val(&value))
        };
        match setting {
            SvdSetting::AlgorithmGesvd => choice(
                v2_13::cutensornetTensorSVDConfigAttributes_t_CUTENSORNET_TENSOR_SVD_CONFIG_ALGO,
                v2_13::cutensornetTensorSVDAlgo_t_CUTENSORNET_TENSOR_SVD_ALGO_GESVD,
            ),
            SvdSetting::NormalizationL2 => choice(
                v2_13::cutensornetTensorSVDConfigAttributes_t_CUTENSORNET_TENSOR_SVD_CONFIG_S_NORMALIZATION,
                v2_13::cutensornetTensorSVDNormalization_t_CUTENSORNET_TENSOR_SVD_NORMALIZATION_L2,
            ),
            SvdSetting::PartitionUs => choice(
                v2_13::cutensornetTensorSVDConfigAttributes_t_CUTENSORNET_TENSOR_SVD_CONFIG_S_PARTITION,
                v2_13::cutensornetTensorSVDPartition_t_CUTENSORNET_TENSOR_SVD_PARTITION_US,
            ),
            SvdSetting::PartitionSv => choice(
                v2_13::cutensornetTensorSVDConfigAttributes_t_CUTENSORNET_TENSOR_SVD_CONFIG_S_PARTITION,
                v2_13::cutensornetTensorSVDPartition_t_CUTENSORNET_TENSOR_SVD_PARTITION_SV,
            ),
            SvdSetting::DiscardedWeightCutoff(cutoff) => set(
                v2_13::cutensornetTensorSVDConfigAttributes_t_CUTENSORNET_TENSOR_SVD_CONFIG_DISCARDED_WEIGHT_CUTOFF,
                (&raw const cutoff).cast(),
                size_of_val(&cutoff),
            ),
        }
    }

    fn destroy_svd_config(&self, config: OpaqueHandle) -> Result<(), NativeError> {
        let functions = self.tensor_functions()?;
        // SAFETY: its owner destroys each configuration exactly once.
        let status = unsafe { (functions.destroy_svd_config)(config.as_ptr()) };
        self.check_cutensornet("cutensornetDestroyTensorSVDConfig", status)
    }

    fn create_svd_info(&self, handle: OpaqueHandle) -> Result<OpaqueHandle, NativeError> {
        let functions = self.tensor_functions()?;
        let mut info = std::ptr::null_mut();
        // SAFETY: the handle is live and `info` is a writable out-pointer.
        let status = unsafe { (functions.create_svd_info)(handle.as_ptr(), &raw mut info) };
        self.check_cutensornet("cutensornetCreateTensorSVDInfo", status)?;
        created(info, "cutensornetCreateTensorSVDInfo", "SVD info")
    }

    fn svd_truncation(
        &self,
        handle: OpaqueHandle,
        info: OpaqueHandle,
    ) -> Result<SvdTruncation, NativeError> {
        let functions = self.tensor_functions()?;
        let read = |attribute, value: *mut c_void, bytes| {
            // SAFETY: the handle and the info are live, and `value` points
            // to `bytes` writable bytes of the attribute's declared type.
            let status = unsafe {
                (functions.svd_info_get_attribute)(
                    handle.as_ptr(),
                    info.as_ptr(),
                    attribute,
                    value,
                    bytes,
                )
            };
            self.check_cutensornet("cutensornetTensorSVDInfoGetAttribute", status)
        };
        let mut reduced_extent = 0_i64;
        let mut full_extent = 0_i64;
        let mut discarded_weight = 0_f64;
        read(
            v2_13::cutensornetTensorSVDInfoAttributes_t_CUTENSORNET_TENSOR_SVD_INFO_REDUCED_EXTENT,
            (&raw mut reduced_extent).cast(),
            size_of::<i64>(),
        )?;
        read(
            v2_13::cutensornetTensorSVDInfoAttributes_t_CUTENSORNET_TENSOR_SVD_INFO_FULL_EXTENT,
            (&raw mut full_extent).cast(),
            size_of::<i64>(),
        )?;
        read(
            v2_13::cutensornetTensorSVDInfoAttributes_t_CUTENSORNET_TENSOR_SVD_INFO_DISCARDED_WEIGHT,
            (&raw mut discarded_weight).cast(),
            size_of::<f64>(),
        )?;
        Ok(SvdTruncation {
            reduced_extent,
            full_extent,
            discarded_weight,
        })
    }

    fn destroy_svd_info(&self, info: OpaqueHandle) -> Result<(), NativeError> {
        let functions = self.tensor_functions()?;
        // SAFETY: its owner destroys each info exactly once.
        let status = unsafe { (functions.destroy_svd_info)(info.as_ptr()) };
        self.check_cutensornet("cutensornetDestroyTensorSVDInfo", status)
    }

    fn gate_split_workspace_sizes(
        &self,
        handle: OpaqueHandle,
        descriptors: &GateSplitDescriptors,
        config: OpaqueHandle,
        workspace: OpaqueHandle,
    ) -> Result<(), NativeError> {
        let functions = self.tensor_functions()?;
        // SAFETY: the handle, the five descriptors, the config and the
        // workspace descriptor are live.
        let status = unsafe {
            (functions.gate_split_sizes)(
                handle.as_ptr(),
                descriptors.a.as_ptr(),
                descriptors.b.as_ptr(),
                descriptors.gate.as_ptr(),
                descriptors.u.as_ptr(),
                descriptors.v.as_ptr(),
                v2_13::cutensornetGateSplitAlgo_t_CUTENSORNET_GATE_SPLIT_ALGO_REDUCED,
                config.as_ptr(),
                v2_13::cutensornetComputeType_t_CUTENSORNET_COMPUTE_64F,
                workspace.as_ptr(),
            )
        };
        self.check_cutensornet("cutensornetWorkspaceComputeGateSplitSizes", status)
    }

    fn gate_split(
        &self,
        handle: OpaqueHandle,
        operands: &GateSplitOperands,
        config: OpaqueHandle,
        info: OpaqueHandle,
        workspace: OpaqueHandle,
        stream: Stream,
    ) -> Result<(), NativeError> {
        let functions = self.tensor_functions()?;
        // SAFETY: the handle, the descriptors, the config, the info, the
        // workspace and the stream are live; each buffer holds its
        // descriptor's extents (U and V at the extent before truncation, S
        // that many doubles), and the workspace was sized for this split and
        // lent the memory it asked for. Their owners keep them until the
        // stream is synchronized.
        let status = unsafe {
            (functions.gate_split)(
                handle.as_ptr(),
                operands.a.descriptor.as_ptr(),
                operands.a.data.as_ptr(),
                operands.b.descriptor.as_ptr(),
                operands.b.data.as_ptr(),
                operands.gate.descriptor.as_ptr(),
                operands.gate.data.as_ptr(),
                operands.u.descriptor.as_ptr(),
                operands.u.data.as_ptr(),
                operands.singular_values.as_ptr(),
                operands.v.descriptor.as_ptr(),
                operands.v.data.as_ptr(),
                v2_13::cutensornetGateSplitAlgo_t_CUTENSORNET_GATE_SPLIT_ALGO_REDUCED,
                config.as_ptr(),
                v2_13::cutensornetComputeType_t_CUTENSORNET_COMPUTE_64F,
                info.as_ptr(),
                workspace.as_ptr(),
                stream.as_ptr().cast(),
            )
        };
        self.check_cutensornet("cutensornetGateSplit", status)
    }

    fn qr_workspace_sizes(
        &self,
        handle: OpaqueHandle,
        input: OpaqueHandle,
        q: OpaqueHandle,
        r: OpaqueHandle,
        workspace: OpaqueHandle,
    ) -> Result<(), NativeError> {
        let functions = self.tensor_functions()?;
        // SAFETY: the handle, the three descriptors and the workspace
        // descriptor are live.
        let status = unsafe {
            (functions.qr_sizes)(
                handle.as_ptr(),
                input.as_ptr(),
                q.as_ptr(),
                r.as_ptr(),
                workspace.as_ptr(),
            )
        };
        self.check_cutensornet("cutensornetWorkspaceComputeQRSizes", status)
    }

    fn tensor_qr(
        &self,
        handle: OpaqueHandle,
        input: TensorOperand,
        q: TensorOperand,
        r: TensorOperand,
        workspace: OpaqueHandle,
        stream: Stream,
    ) -> Result<(), NativeError> {
        let functions = self.tensor_functions()?;
        // SAFETY: the handle, the descriptors, the workspace and the stream
        // are live; each buffer holds its descriptor's extents, and the
        // workspace was sized for this QR and lent the memory it asked for.
        // Their owners keep them until the stream is synchronized.
        let status = unsafe {
            (functions.tensor_qr)(
                handle.as_ptr(),
                input.descriptor.as_ptr(),
                input.data.as_ptr(),
                q.descriptor.as_ptr(),
                q.data.as_ptr(),
                r.descriptor.as_ptr(),
                r.data.as_ptr(),
                workspace.as_ptr(),
                stream.as_ptr().cast(),
            )
        };
        self.check_cutensornet("cutensornetTensorQR", status)
    }

    fn create_contraction_network(
        &self,
        handle: OpaqueHandle,
        inputs: [TensorModes<'_>; 2],
        output_modes: &[i32],
    ) -> Result<ContractionNetwork, NativeError> {
        let functions = self.tensor_functions()?;
        let input_mode_counts = [mode_count(inputs[0])?, mode_count(inputs[1])?];
        let output_mode_count =
            i32::try_from(output_modes.len()).map_err(|_| NativeError::ResourceSizeOverflow {
                resource: "tensor mode count",
            })?;
        let mut network = std::ptr::null_mut();
        // SAFETY: the handle is live and `network` is a writable out-pointer.
        let status = unsafe { (functions.create_network)(handle.as_ptr(), &raw mut network) };
        self.check_cutensornet("cutensornetCreateNetwork", status)?;
        let network = created(network, "cutensornetCreateNetwork", "network")?;
        let described = self.describe_network(
            functions,
            handle,
            network,
            inputs,
            input_mode_counts,
            (output_mode_count, output_modes),
        );
        match described {
            Ok(input_ids) => Ok(ContractionNetwork {
                descriptor: network,
                input_ids,
            }),
            Err(error) => {
                // SAFETY: the network was created above, and is destroyed
                // once, since the caller never receives it.
                let status = unsafe { (functions.destroy_network)(network.as_ptr()) };
                let destroyed = self.check_cutensornet("cutensornetDestroyNetwork", status);
                combine_execution_and_cleanup(Err(error), destroyed)
            }
        }
    }

    fn destroy_contraction_network(&self, network: OpaqueHandle) -> Result<(), NativeError> {
        let functions = self.tensor_functions()?;
        // SAFETY: its owner destroys each network exactly once, after the
        // optimizer infos made from it.
        let status = unsafe { (functions.destroy_network)(network.as_ptr()) };
        self.check_cutensornet("cutensornetDestroyNetwork", status)
    }

    fn create_optimizer_info(
        &self,
        handle: OpaqueHandle,
        network: OpaqueHandle,
    ) -> Result<OpaqueHandle, NativeError> {
        let functions = self.tensor_functions()?;
        let mut info = std::ptr::null_mut();
        // SAFETY: the handle and the network are live, and `info` is a
        // writable out-pointer.
        let status = unsafe {
            (functions.create_optimizer_info)(handle.as_ptr(), network.as_ptr(), &raw mut info)
        };
        self.check_cutensornet("cutensornetCreateContractionOptimizerInfo", status)?;
        created(
            info,
            "cutensornetCreateContractionOptimizerInfo",
            "contraction optimizer info",
        )
    }

    fn destroy_optimizer_info(&self, info: OpaqueHandle) -> Result<(), NativeError> {
        let functions = self.tensor_functions()?;
        // SAFETY: its owner destroys each info exactly once, before its
        // network.
        let status = unsafe { (functions.destroy_optimizer_info)(info.as_ptr()) };
        self.check_cutensornet("cutensornetDestroyContractionOptimizerInfo", status)
    }

    fn optimize_contraction(
        &self,
        handle: OpaqueHandle,
        network: OpaqueHandle,
        workspace_limit: u64,
        info: OpaqueHandle,
    ) -> Result<(), NativeError> {
        let functions = self.tensor_functions()?;
        let mut config = std::ptr::null_mut();
        // SAFETY: the handle is live and `config` is a writable out-pointer.
        let status =
            unsafe { (functions.create_optimizer_config)(handle.as_ptr(), &raw mut config) };
        self.check_cutensornet("cutensornetCreateContractionOptimizerConfig", status)?;
        let config = created(
            config,
            "cutensornetCreateContractionOptimizerConfig",
            "contraction optimizer configuration",
        )?;
        // Slicing is off: a contraction is one `cutensornetNetworkContract`
        // per call. Slicing trades memory for extra calls (one per slice),
        // no case needs that trade for a pairwise contraction, and no GPU
        // run has exercised a sliced path. Should a path still come back
        // sliced, `contract` passes a null slice group, which contracts
        // every slice (`cutensornetNetworkContract`'s doc of `sliceGroup`,
        // `generated/v2_13.rs`: "If set to null, all slices will be
        // contracted"), so the result stays whole and correct.
        let disable_slicing: i32 = 1;
        // SAFETY: the handle and the config are live, and the value is the
        // attribute's declared `int32_t`.
        let status = unsafe {
            (functions.optimizer_config_set_attribute)(
                handle.as_ptr(),
                config.as_ptr(),
                v2_13::cutensornetContractionOptimizerConfigAttributes_t_CUTENSORNET_CONTRACTION_OPTIMIZER_CONFIG_SLICER_DISABLE_SLICING,
                (&raw const disable_slicing).cast(),
                size_of_val(&disable_slicing),
            )
        };
        let optimized = self
            .check_cutensornet("cutensornetContractionOptimizerConfigSetAttribute", status)
            .and_then(|()| {
                // SAFETY: the handle, the network, the config and the info
                // are live.
                let status = unsafe {
                    (functions.optimize)(
                        handle.as_ptr(),
                        network.as_ptr(),
                        config.as_ptr(),
                        workspace_limit,
                        info.as_ptr(),
                    )
                };
                self.check_cutensornet("cutensornetContractionOptimize", status)
            });
        // SAFETY: the config was created above, and is destroyed once.
        let status = unsafe { (functions.destroy_optimizer_config)(config.as_ptr()) };
        let destroyed =
            self.check_cutensornet("cutensornetDestroyContractionOptimizerConfig", status);
        combine_execution_and_cleanup(optimized, destroyed)
    }

    fn contraction_workspace_sizes(
        &self,
        handle: OpaqueHandle,
        network: OpaqueHandle,
        info: OpaqueHandle,
        workspace: OpaqueHandle,
    ) -> Result<(), NativeError> {
        let functions = self.tensor_functions()?;
        // SAFETY: the handle, the network, its optimized info and the
        // workspace descriptor are live.
        let status = unsafe {
            (functions.contraction_sizes)(
                handle.as_ptr(),
                network.as_ptr(),
                info.as_ptr(),
                workspace.as_ptr(),
            )
        };
        self.check_cutensornet("cutensornetWorkspaceComputeContractionSizes", status)
    }

    fn prepare_contraction(
        &self,
        handle: OpaqueHandle,
        network: OpaqueHandle,
        workspace: OpaqueHandle,
    ) -> Result<(), NativeError> {
        let functions = self.tensor_functions()?;
        // SAFETY: the handle, the network, optimized, and the workspace
        // descriptor, sized for its path, are live.
        let status = unsafe {
            (functions.network_prepare_contraction)(
                handle.as_ptr(),
                network.as_ptr(),
                workspace.as_ptr(),
            )
        };
        self.check_cutensornet("cutensornetNetworkPrepareContraction", status)
    }

    fn contract(
        &self,
        handle: OpaqueHandle,
        network: ContractionNetwork,
        inputs: [OpaqueHandle; 2],
        output: OpaqueHandle,
        workspace: OpaqueHandle,
        stream: Stream,
    ) -> Result<(), NativeError> {
        let functions = self.tensor_functions()?;
        for (input_id, input) in network.input_ids.into_iter().zip(inputs) {
            // SAFETY: the handle and the network are live, `input_id` is
            // the id cuTensorNet gave this input, and the buffer holds its
            // extents (null strides: first mode fastest).
            let status = unsafe {
                (functions.network_set_input_tensor_memory)(
                    handle.as_ptr(),
                    network.descriptor.as_ptr(),
                    input_id,
                    input.as_ptr().cast_const(),
                    std::ptr::null(),
                )
            };
            self.check_cutensornet("cutensornetNetworkSetInputTensorMemory", status)?;
        }
        // SAFETY: the handle and the network are live, and the buffer holds
        // the output's extents (null strides: first mode fastest).
        let status = unsafe {
            (functions.network_set_output_tensor_memory)(
                handle.as_ptr(),
                network.descriptor.as_ptr(),
                output.as_ptr(),
                std::ptr::null(),
            )
        };
        self.check_cutensornet("cutensornetNetworkSetOutputTensorMemory", status)?;
        // SAFETY: the handle, the network, prepared, the workspace and the
        // stream are live; the workspace was sized for the network's path
        // and lent the memory it asked for. The buffers' owners keep them
        // until the stream is synchronized. A null slice group contracts
        // every slice, should the path have several despite the disabled
        // slicing (`optimize_contraction`); 0 overwrites the output.
        let status = unsafe {
            (functions.network_contract)(
                handle.as_ptr(),
                network.descriptor.as_ptr(),
                0,
                workspace.as_ptr(),
                std::ptr::null_mut(),
                stream.as_ptr().cast(),
            )
        };
        self.check_cutensornet("cutensornetNetworkContract", status)
    }
}

impl LoadedLibraries {
    /// Appends `inputs` to the empty `network` and sets its output, of the
    /// labels `output`; the ids cuTensorNet gave the inputs.
    fn describe_network(
        &self,
        functions: &CuTensorNetTensorFunctions,
        handle: OpaqueHandle,
        network: OpaqueHandle,
        inputs: [TensorModes<'_>; 2],
        input_mode_counts: [i32; 2],
        output: (i32, &[i32]),
    ) -> Result<[i64; 2], NativeError> {
        let mut input_ids = [0; 2];
        for ((tensor, mode_count), input_id) in inputs
            .into_iter()
            .zip(input_mode_counts)
            .zip(&mut input_ids)
        {
            // SAFETY: the handle and the network are live; `extents` and
            // `modes` hold `mode_count` entries each, which cuTensorNet
            // copies; null qualifiers are the defaults (no conjugation);
            // `input_id` is a writable out-pointer.
            let status = unsafe {
                (functions.network_append_tensor)(
                    handle.as_ptr(),
                    network.as_ptr(),
                    mode_count,
                    tensor.extents.as_ptr(),
                    tensor.modes.as_ptr(),
                    std::ptr::null(),
                    v2_13::cudaDataType_t_CUDA_C_64F,
                    input_id,
                )
            };
            self.check_cutensornet("cutensornetNetworkAppendTensor", status)?;
        }
        let (output_mode_count, output_modes) = output;
        // SAFETY: the handle and the network are live, and `output_modes`
        // holds `output_mode_count` labels, which cuTensorNet copies.
        let status = unsafe {
            (functions.network_set_output_tensor)(
                handle.as_ptr(),
                network.as_ptr(),
                output_mode_count,
                output_modes.as_ptr(),
                v2_13::cudaDataType_t_CUDA_C_64F,
            )
        };
        self.check_cutensornet("cutensornetNetworkSetOutputTensor", status)?;
        Ok(input_ids)
    }

    /// cuTensorNet's tensor API, or every function of it the library lacks.
    fn tensor_functions(&self) -> Result<&CuTensorNetTensorFunctions, NativeError> {
        self.cutensornet
            .functions
            .tensor
            .as_ref()
            .map_err(|faults| NativeError::missing_functions("cuTensorNet", faults))
    }
}

/// The mode count of `tensor`, which has one extent and one label per mode.
fn mode_count(tensor: TensorModes<'_>) -> Result<i32, NativeError> {
    if tensor.extents.len() != tensor.modes.len() {
        return Err(NativeError::InvalidNativeCall {
            reason: format!(
                "a tensor has {} extents and {} mode labels",
                tensor.extents.len(),
                tensor.modes.len()
            ),
        });
    }
    i32::try_from(tensor.modes.len()).map_err(|_| NativeError::ResourceSizeOverflow {
        resource: "tensor mode count",
    })
}

/// The object a successful `operation` created, unless it returned null.
fn created(
    object: *mut c_void,
    operation: &'static str,
    resource: &'static str,
) -> Result<OpaqueHandle, NativeError> {
    NonNull::new(object).ok_or(NativeError::MissingNativeResource {
        operation,
        resource,
    })
}
