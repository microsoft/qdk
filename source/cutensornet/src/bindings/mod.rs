#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[allow(dead_code)]
pub(crate) mod cudart_12;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[allow(
    clippy::doc_markdown,
    dead_code,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    reason = "bindgen preserves NVIDIA C documentation and identifiers verbatim"
)]
pub(crate) mod v2_13;

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod abi {
    use super::{cudart_12, v2_13};
    use std::mem::{align_of, offset_of, size_of};

    #[repr(C, align(16))]
    struct Complex64Abi {
        re: f64,
        im: f64,
    }

    const _: () = assert!(size_of::<Complex64Abi>() == 16);
    const _: () = assert!(align_of::<Complex64Abi>() == 16);
    const _: () = assert!(offset_of!(Complex64Abi, re) == 0);
    const _: () = assert!(offset_of!(Complex64Abi, im) == 8);
    const _: () = assert!(size_of::<v2_13::cuDoubleComplex>() == 16);
    const _: () = assert!(align_of::<v2_13::cuDoubleComplex>() == 16);
    const _: () = assert!(offset_of!(v2_13::cuDoubleComplex, x) == 0);
    const _: () = assert!(offset_of!(v2_13::cuDoubleComplex, y) == 8);

    const _: () = assert!(size_of::<v2_13::cutensornetStatus_t>() == 4);
    const _: () = assert!(align_of::<v2_13::cutensornetStatus_t>() == 4);
    const _: () = assert!(size_of::<v2_13::cutensornetStatePurity_t>() == 4);
    const _: () = assert!(align_of::<v2_13::cutensornetStatePurity_t>() == 4);
    const _: () = assert!(size_of::<v2_13::cutensornetBoundaryCondition_t>() == 4);
    const _: () = assert!(align_of::<v2_13::cutensornetBoundaryCondition_t>() == 4);
    const _: () = assert!(size_of::<v2_13::cutensornetStateAttributes_t>() == 4);
    const _: () = assert!(align_of::<v2_13::cutensornetStateAttributes_t>() == 4);
    const _: () = assert!(size_of::<v2_13::cutensornetExpectationAttributes_t>() == 4);
    const _: () = assert!(align_of::<v2_13::cutensornetExpectationAttributes_t>() == 4);
    const _: () = assert!(size_of::<v2_13::cutensornetWorksizePref_t>() == 4);
    const _: () = assert!(align_of::<v2_13::cutensornetWorksizePref_t>() == 4);
    const _: () = assert!(size_of::<v2_13::cutensornetMemspace_t>() == 4);
    const _: () = assert!(align_of::<v2_13::cutensornetMemspace_t>() == 4);
    const _: () = assert!(size_of::<v2_13::cutensornetWorkspaceKind_t>() == 4);
    const _: () = assert!(align_of::<v2_13::cutensornetWorkspaceKind_t>() == 4);
    const _: () = assert!(size_of::<v2_13::cudaDataType_t>() == 4);
    const _: () = assert!(align_of::<v2_13::cudaDataType_t>() == 4);
    const _: () = assert!(size_of::<cudart_12::CudaError>() == 4);
    const _: () = assert!(align_of::<cudart_12::CudaError>() == 4);
    const _: () = assert!(size_of::<cudart_12::CudaMemcpyKind>() == 4);
    const _: () = assert!(align_of::<cudart_12::CudaMemcpyKind>() == 4);

    const _: () = assert!(size_of::<v2_13::cutensornetHandle_t>() == 8);
    const _: () = assert!(align_of::<v2_13::cutensornetHandle_t>() == 8);
    const _: () = assert!(size_of::<v2_13::cutensornetState_t>() == 8);
    const _: () = assert!(align_of::<v2_13::cutensornetState_t>() == 8);
    const _: () = assert!(size_of::<v2_13::cutensornetNetworkOperator_t>() == 8);
    const _: () = assert!(align_of::<v2_13::cutensornetNetworkOperator_t>() == 8);
    const _: () = assert!(size_of::<v2_13::cutensornetStateExpectation_t>() == 8);
    const _: () = assert!(align_of::<v2_13::cutensornetStateExpectation_t>() == 8);
    const _: () = assert!(size_of::<v2_13::cutensornetWorkspaceDescriptor_t>() == 8);
    const _: () = assert!(align_of::<v2_13::cutensornetWorkspaceDescriptor_t>() == 8);
    const _: () = assert!(size_of::<v2_13::cudaStream_t>() == 8);
    const _: () = assert!(align_of::<v2_13::cudaStream_t>() == 8);

    const _: () = assert!(v2_13::cudaDataType_t_CUDA_C_64F == 5);
    const _: () = assert!(
        v2_13::cutensornetExpectationAttributes_t_CUTENSORNET_EXPECTATION_CONFIG_NUM_HYPER_SAMPLES
            == 1
    );
    const _: () = assert!(cudart_12::CUDA_MEMCPY_HOST_TO_DEVICE == 1);
    const _: () = assert!(cudart_12::CUDA_MEMCPY_DEVICE_TO_HOST == 2);
}
