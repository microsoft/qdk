// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Development only (not for review): [`DeviceTensors`], the tensor calls
//! on the GPU: device buffers, `cutensornetGateSplit` with GESVD (or
//! another SVD, [`DeviceTensors::set_dev_svd_algo`]) and the `REDUCED`
//! gate-split algorithm (`policy` below), and the QR moves (`TensorQR` and
//! the contractions of `contraction.rs`), on the libraries an
//! [`Availability`](crate::Availability) loaded.
//!
//! Per call it creates the five tensor descriptors (`GateSplit` overwrites
//! U's and V's when the cutoff reduces the bond), reuses one SVD config per
//! SVD, cutoff and partition, one SVD info, and one workspace whose memory
//! only grows. U and V are allocated at the descriptors' extent; after a
//! reduction cuTensorNet packs them at the reduced extent, the layout the
//! engine reads (its documentation; checked on the device by
//! [`self_check`](super::self_check)).

mod contraction;

use super::{
    CudaMpsError, GateSplitCall, Partition, QrMoveCall, QrMoveOutcome, SplitOutcome, TensorCalls,
};
use crate::dev_gate_split::{
    BenchCudaFunctions, BenchTensorFunctions, Descriptor, DeviceBuffer, SvdConfig, SvdInfo,
    TensorSvdAlgo, check_cuda, check_tensor, cuda_failure,
};
use crate::libraries::LoadedLibraries;
use crate::libraries::cuda_runtime::cudart_12;
use crate::libraries::cutensornet::v2_13;
use crate::libraries::loader::FunctionTable;
use contraction::{ContractionFunctions, Operand, Output, Plans};
use num_complex::Complex64;
use std::cell::{Cell, RefCell};
use std::ffi::{c_uint, c_void};
use std::rc::Rc;
use std::sync::Arc;

/// The choices of the device's `GateSplit`.
mod policy {
    use super::{TensorSvdAlgo, v2_13};

    /// GESVD: cuTensorNet's default and the most accurate of its SVDs.
    /// EXPERIMENTAL: GESVDJ or GESVDP by `DeviceTensors::set_dev_svd_algo`,
    /// to measure them in whole runs (the gate-split bench measured GESVDP
    /// 0.39-0.88 of GESVD's time at bonds 8-512 on the A100, its singular
    /// values and discarded weight the CPU's, on random full-rank pairs).
    pub(super) const SVD_ALGO: TensorSvdAlgo = TensorSvdAlgo::Gesvd;

    /// `REDUCED`: QR of each input, then the SVD of the small middle
    /// tensor; the bench measured it faster than `DIRECT` at every bond.
    pub(super) const GATE_SPLIT_ALGO: v2_13::cutensornetGateSplitAlgo_t =
        v2_13::cutensornetGateSplitAlgo_t_CUTENSORNET_GATE_SPLIT_ALGO_REDUCED;
}

/// The tensor calls on device 0.
pub struct DeviceTensors {
    configs: RefCell<Vec<(ConfigKey, SvdConfig)>>,
    svd_algo: Cell<TensorSvdAlgo>,
    info: SvdInfo,
    workspace: RefCell<GrowingWorkspace>,
    values: RefCell<Option<DeviceBuffer>>,
    gate: DeviceBuffer,
    contraction: ContractionFunctions,
    plans: RefCell<Plans>,
    // Last: the handle and the stream outlive every object above.
    context: Rc<DeviceContext>,
}

/// The SVD, the cutoff's bits and the partition an SVD config holds.
type ConfigKey = (TensorSvdAlgo, Option<u64>, Partition);

/// The handle, the stream, and the functions, shared with every buffer so
/// that the libraries stay loaded until the last one is freed.
struct DeviceContext {
    libraries: Arc<LoadedLibraries>,
    tensor: BenchTensorFunctions,
    cuda: BenchCudaFunctions,
    stream: cudart_12::cudaStream_t,
    handle: v2_13::cutensornetHandle_t,
}

impl Drop for DeviceContext {
    fn drop(&mut self) {
        // SAFETY: the handle and the stream were created in `open` and are
        // destroyed once, after every object made with them.
        unsafe {
            let _ = (self.libraries.cutensornet.functions.destroy)(self.handle);
            let _ = (self.libraries.cuda_runtime.functions.stream_destroy)(self.stream);
        }
    }
}

/// A tensor in device memory.
pub struct DeviceArray {
    buffer: DeviceBuffer,
    // Keeps the libraries loaded while the buffer lives.
    _context: Rc<DeviceContext>,
}

/// A workspace descriptor whose memory, per space and kind, only grows.
struct GrowingWorkspace {
    descriptor: v2_13::cutensornetWorkspaceDescriptor_t,
    destroy: crate::libraries::cutensornet::workspace::DestroyWorkspaceFn,
    device: [Option<DeviceBuffer>; 2],
    host: [Vec<u8>; 2],
}

impl Drop for GrowingWorkspace {
    fn drop(&mut self) {
        // SAFETY: created by `cutensornetCreateWorkspaceDescriptor`,
        // destroyed once, before its memory (fields drop after `drop`).
        let _ = unsafe { (self.destroy)(self.descriptor) };
    }
}

fn failed(message: String) -> CudaMpsError {
    CudaMpsError::TensorCallFailed(message)
}

impl DeviceTensors {
    /// Opens device 0, a stream and a cuTensorNet handle on the libraries
    /// `availability` loaded.
    ///
    /// # Errors
    ///
    /// When a function is missing or a call fails.
    pub fn open(availability: &crate::Availability) -> Result<Self, CudaMpsError> {
        let libraries = Arc::clone(availability.libraries());
        let tensor = BenchTensorFunctions::resolve(&libraries.cutensornet.library)
            .map_err(|missing| failed(format!("cuTensorNet lacks {missing:?}")))?;
        let contraction = ContractionFunctions::resolve(&libraries.cutensornet.library)
            .map_err(|missing| failed(format!("cuTensorNet lacks {missing:?}")))?;
        let cuda = BenchCudaFunctions::resolve(&libraries.cuda_runtime.library)
            .map_err(|missing| failed(format!("the CUDA Runtime lacks {missing:?}")))?;
        libraries
            .cuda_runtime
            .set_device(0)
            .map_err(|failure| failed(cuda_failure(&failure)))?;
        let mut stream = std::ptr::null_mut();
        // SAFETY: `stream` is a valid out-pointer for the new stream.
        let status = unsafe {
            (libraries.cuda_runtime.functions.stream_create_with_flags)(
                &raw mut stream,
                cudart_12::cudaStreamNonBlocking,
            )
        };
        check_cuda("cudaStreamCreateWithFlags", status).map_err(failed)?;
        let mut handle = std::ptr::null_mut();
        // SAFETY: `handle` is a valid out-pointer for the new handle.
        let status = unsafe { (libraries.cutensornet.functions.create)(&raw mut handle) };
        if let Err(message) = check_tensor("cutensornetCreate", status) {
            // SAFETY: the stream was just created and is destroyed once.
            let _ = unsafe { (libraries.cuda_runtime.functions.stream_destroy)(stream) };
            return Err(failed(message));
        }
        let context = Rc::new(DeviceContext {
            libraries,
            tensor,
            cuda,
            stream,
            handle,
        });
        let mut info = std::ptr::null_mut();
        // SAFETY: `info` is a valid out-pointer; the handle is live.
        let status = unsafe { (context.tensor.create_svd_info)(context.handle, &raw mut info) };
        check_tensor("cutensornetCreateTensorSVDInfo", status).map_err(failed)?;
        let info = SvdInfo {
            info,
            destroy: context.tensor.destroy_svd_info,
        };
        let functions = &context.libraries.cutensornet.functions;
        let mut descriptor = std::ptr::null_mut();
        // SAFETY: `descriptor` is a valid out-pointer; the handle is live.
        let status = unsafe { (functions.create_workspace)(context.handle, &raw mut descriptor) };
        check_tensor("cutensornetCreateWorkspaceDescriptor", status).map_err(failed)?;
        let workspace = GrowingWorkspace {
            descriptor,
            destroy: functions.destroy_workspace,
            device: [None, None],
            host: [Vec::new(), Vec::new()],
        };
        let gate = DeviceBuffer::new(&context.libraries, 256).map_err(failed)?;
        Ok(Self {
            configs: RefCell::new(Vec::new()),
            svd_algo: Cell::new(policy::SVD_ALGO),
            info,
            workspace: RefCell::new(workspace),
            values: RefCell::new(None),
            gate,
            contraction,
            plans: RefCell::new(Plans::new()),
            context,
        })
    }

    /// EXPERIMENTAL, development only (not for review): the SVD of every
    /// later `GateSplit`, GESVD (`policy::SVD_ALGO`), GESVDJ or GESVDP.
    /// GESVDP (a polar decomposition, then an eigensolver) shifts the
    /// singular values of a rank-deficient pair by a perturbation it
    /// reports, read after every split into
    /// [`SplitOutcome::svd_perturbation`], so a run shows how far it may
    /// be off. (`CuTensorNetEngine`'s failure of H₂ with GESVDP, status 14
    /// at its 39th measurement, was in the state API's own splits, not in
    /// `GateSplit`.)
    ///
    /// # Errors
    ///
    /// For GESVDR: it takes a fixed extent only, so it cannot apply the
    /// discarded-weight cutoff.
    #[doc(hidden)]
    pub fn set_dev_svd_algo(&self, algo: TensorSvdAlgo) -> Result<(), CudaMpsError> {
        match algo {
            TensorSvdAlgo::Gesvd | TensorSvdAlgo::Gesvdj | TensorSvdAlgo::Gesvdp => {
                self.svd_algo.set(algo);
                Ok(())
            }
            TensorSvdAlgo::Gesvdr => Err(failed(format!(
                "the engine refuses the SVD {}: it cannot apply the discarded-weight cutoff",
                algo.name()
            ))),
        }
    }

    fn synchronize(&self) -> Result<(), CudaMpsError> {
        // SAFETY: the stream is live until the context drops.
        let status = unsafe {
            (self
                .context
                .libraries
                .cuda_runtime
                .functions
                .stream_synchronize)(self.context.stream)
        };
        check_cuda("cudaStreamSynchronize", status).map_err(failed)
    }

    /// An asynchronous copy on the stream.
    fn copy(
        &self,
        destination: *mut c_void,
        source: *const c_void,
        bytes: usize,
        kind: cudart_12::cudaMemcpyKind,
    ) -> Result<(), CudaMpsError> {
        // SAFETY: the callers pass buffers of at least `bytes` bytes on the
        // sides `kind` names; the stream is live.
        let status = unsafe {
            (self.context.cuda.memcpy_async)(destination, source, bytes, kind, self.context.stream)
        };
        check_cuda("cudaMemcpyAsync", status).map_err(failed)
    }

    fn allocate(&self, bytes: usize) -> Result<DeviceArray, CudaMpsError> {
        Ok(DeviceArray {
            buffer: DeviceBuffer::new(&self.context.libraries, bytes.max(16)).map_err(failed)?,
            _context: Rc::clone(&self.context),
        })
    }

    /// A tensor descriptor of complex doubles, its first mode fastest.
    fn descriptor(&self, extents: &[usize], modes: &[u8]) -> Result<Descriptor, CudaMpsError> {
        let extents: Vec<i64> = extents
            .iter()
            .map(|&extent| i64::try_from(extent).expect("an extent fits in i64"))
            .collect();
        let labels: Vec<i32> = modes.iter().map(|&mode| i32::from(mode)).collect();
        let mut descriptor = std::ptr::null_mut();
        // SAFETY: `extents` and `labels` have one entry per mode; null
        // strides mean the first mode is fastest; the handle is live.
        let status = unsafe {
            (self.context.tensor.create_descriptor)(
                self.context.handle,
                i32::try_from(extents.len()).expect("a tensor has few modes"),
                extents.as_ptr(),
                std::ptr::null(),
                labels.as_ptr(),
                v2_13::cudaDataType_t_CUDA_C_64F,
                &raw mut descriptor,
            )
        };
        check_tensor("cutensornetCreateTensorDescriptor", status).map_err(failed)?;
        Ok(Descriptor {
            descriptor,
            destroy: self.context.tensor.destroy_descriptor,
        })
    }

    /// The SVD config for the current SVD, `cutoff` and `partition`, made
    /// on first use.
    fn config(
        &self,
        cutoff: Option<f64>,
        partition: Partition,
    ) -> Result<v2_13::cutensornetTensorSVDConfig_t, CudaMpsError> {
        let algo = self.svd_algo.get();
        let key = (algo, cutoff.map(f64::to_bits), partition);
        if let Some((_, config)) = self.configs.borrow().iter().find(|(k, _)| *k == key) {
            return Ok(config.config);
        }
        let mut config = std::ptr::null_mut();
        // SAFETY: `config` is a valid out-pointer; the handle is live.
        let status = unsafe {
            (self.context.tensor.create_svd_config)(self.context.handle, &raw mut config)
        };
        check_tensor("cutensornetCreateTensorSVDConfig", status).map_err(failed)?;
        let config = SvdConfig {
            config,
            destroy: self.context.tensor.destroy_svd_config,
        };
        let algo = algo.raw();
        self.set_config(&config, attribute::ALGO, &algo, "ALGO")?;
        let normalization: v2_13::cutensornetTensorSVDNormalization_t =
            v2_13::cutensornetTensorSVDNormalization_t_CUTENSORNET_TENSOR_SVD_NORMALIZATION_L2;
        self.set_config(
            &config,
            attribute::S_NORMALIZATION,
            &normalization,
            "S_NORMALIZATION",
        )?;
        let side: v2_13::cutensornetTensorSVDPartition_t = match partition {
            Partition::Us => {
                v2_13::cutensornetTensorSVDPartition_t_CUTENSORNET_TENSOR_SVD_PARTITION_US
            }
            Partition::Sv => {
                v2_13::cutensornetTensorSVDPartition_t_CUTENSORNET_TENSOR_SVD_PARTITION_SV
            }
        };
        self.set_config(&config, attribute::S_PARTITION, &side, "S_PARTITION")?;
        if let Some(cutoff) = cutoff {
            self.set_config(
                &config,
                attribute::DISCARDED_WEIGHT_CUTOFF,
                &cutoff,
                "DISCARDED_WEIGHT_CUTOFF",
            )?;
        }
        let raw = config.config;
        self.configs.borrow_mut().push((key, config));
        Ok(raw)
    }

    fn set_config<V>(
        &self,
        config: &SvdConfig,
        attribute: v2_13::cutensornetTensorSVDConfigAttributes_t,
        value: &V,
        name: &str,
    ) -> Result<(), CudaMpsError> {
        // SAFETY: `value` is the attribute's type, passed with its size.
        let status = unsafe {
            (self.context.tensor.svd_config_set_attribute)(
                self.context.handle,
                config.config,
                attribute,
                std::ptr::from_ref(value).cast(),
                size_of::<V>(),
            )
        };
        check_tensor(
            &format!("cutensornetTensorSVDConfigSetAttribute({name})"),
            status,
        )
        .map_err(failed)
    }

    /// Sizes the workspace for one call (`GateSplit`, `TensorQR` or a
    /// contraction) by `compute_sizes`, the cuTensorNet function `sizes_call`
    /// names, and attaches enough memory.
    fn prepare_workspace(
        &self,
        sizes_call: &str,
        compute_sizes: impl FnOnce(
            v2_13::cutensornetWorkspaceDescriptor_t,
        ) -> crate::dev_gate_split::Status,
    ) -> Result<v2_13::cutensornetWorkspaceDescriptor_t, CudaMpsError> {
        let functions = &self.context.libraries.cutensornet.functions;
        let mut workspace = self.workspace.borrow_mut();
        let descriptor = workspace.descriptor;
        check_tensor(sizes_call, compute_sizes(descriptor)).map_err(failed)?;
        let kinds = [
            v2_13::cutensornetWorkspaceKind_t_CUTENSORNET_WORKSPACE_SCRATCH,
            v2_13::cutensornetWorkspaceKind_t_CUTENSORNET_WORKSPACE_CACHE,
        ];
        for memspace in [
            v2_13::cutensornetMemspace_t_CUTENSORNET_MEMSPACE_DEVICE,
            v2_13::cutensornetMemspace_t_CUTENSORNET_MEMSPACE_HOST,
        ] {
            for (slot, kind) in kinds.into_iter().enumerate() {
                let mut size = 0_i64;
                // SAFETY: the descriptor is live and `size` is writable.
                let status = unsafe {
                    (functions.workspace_get_memory_size)(
                        self.context.handle,
                        descriptor,
                        v2_13::cutensornetWorksizePref_t_CUTENSORNET_WORKSIZE_PREF_RECOMMENDED,
                        memspace,
                        kind,
                        &raw mut size,
                    )
                };
                check_tensor("cutensornetWorkspaceGetMemorySize", status).map_err(failed)?;
                let bytes = usize::try_from(size).unwrap_or(0);
                // A size of 0 with a pointer is an invalid workspace
                // (`cutensornetWorkspaceSetMemory`: status 7 on the A100),
                // so a kind this call does not need is left as it is: unset,
                // or still pointing at the live buffer an earlier call set,
                // since a held buffer is replaced only just before it is set
                // again below.
                if bytes == 0 {
                    continue;
                }
                let pointer =
                    if memspace == v2_13::cutensornetMemspace_t_CUTENSORNET_MEMSPACE_DEVICE {
                        let held = &mut workspace.device[slot];
                        if held.as_ref().is_none_or(|buffer| buffer.bytes < bytes) {
                            *held = None;
                            *held = Some(
                                DeviceBuffer::new(&self.context.libraries, bytes.max(256))
                                    .map_err(failed)?,
                            );
                        }
                        held.as_ref().expect("a device workspace").pointer
                    } else {
                        let held = &mut workspace.host[slot];
                        if held.len() < bytes + 256 {
                            *held = vec![0_u8; bytes + 256];
                        }
                        let offset = held.as_ptr().align_offset(256);
                        held[offset..].as_mut_ptr().cast()
                    };
                // SAFETY: `pointer` holds at least `size` bytes in
                // `memspace` and lives as long as the workspace.
                let status = unsafe {
                    (functions.workspace_set_memory)(
                        self.context.handle,
                        descriptor,
                        memspace,
                        kind,
                        pointer,
                        size,
                    )
                };
                check_tensor("cutensornetWorkspaceSetMemory", status).map_err(failed)?;
            }
        }
        Ok(descriptor)
    }

    fn info_i64(&self, attribute: c_uint) -> Result<i64, CudaMpsError> {
        let mut value = 0_i64;
        self.read_info(attribute, (&raw mut value).cast(), size_of::<i64>())?;
        Ok(value)
    }

    fn read_info(
        &self,
        attribute: c_uint,
        value: *mut c_void,
        bytes: usize,
    ) -> Result<(), CudaMpsError> {
        // SAFETY: `value` holds `bytes` bytes, the attribute's size.
        let status = unsafe {
            (self.context.tensor.svd_info_get_attribute)(
                self.context.handle,
                self.info.info,
                attribute,
                value,
                bytes,
            )
        };
        check_tensor("cutensornetTensorSVDInfoGetAttribute", status).map_err(failed)
    }

    /// The buffer for the singular values, at least `bytes` long.
    fn values_buffer(&self, bytes: usize) -> Result<*mut c_void, CudaMpsError> {
        let mut values = self.values.borrow_mut();
        if values.as_ref().is_none_or(|buffer| buffer.bytes < bytes) {
            *values = None;
            *values =
                Some(DeviceBuffer::new(&self.context.libraries, bytes.max(256)).map_err(failed)?);
        }
        Ok(values.as_ref().expect("a values buffer").pointer)
    }
}

/// The SVD config's attributes.
mod attribute {
    use super::v2_13;

    pub(super) const ALGO: v2_13::cutensornetTensorSVDConfigAttributes_t =
        v2_13::cutensornetTensorSVDConfigAttributes_t_CUTENSORNET_TENSOR_SVD_CONFIG_ALGO;
    pub(super) const S_NORMALIZATION: v2_13::cutensornetTensorSVDConfigAttributes_t =
        v2_13::cutensornetTensorSVDConfigAttributes_t_CUTENSORNET_TENSOR_SVD_CONFIG_S_NORMALIZATION;
    pub(super) const S_PARTITION: v2_13::cutensornetTensorSVDConfigAttributes_t =
        v2_13::cutensornetTensorSVDConfigAttributes_t_CUTENSORNET_TENSOR_SVD_CONFIG_S_PARTITION;
    pub(super) const DISCARDED_WEIGHT_CUTOFF: v2_13::cutensornetTensorSVDConfigAttributes_t =
        v2_13::cutensornetTensorSVDConfigAttributes_t_CUTENSORNET_TENSOR_SVD_CONFIG_DISCARDED_WEIGHT_CUTOFF;
}

impl TensorCalls for DeviceTensors {
    type Buffer = DeviceArray;

    fn upload(&self, values: &[Complex64]) -> Result<DeviceArray, CudaMpsError> {
        let array = self.allocate(size_of_val(values))?;
        self.copy(
            array.buffer.pointer,
            values.as_ptr().cast(),
            size_of_val(values),
            cudart_12::cudaMemcpyKind_cudaMemcpyHostToDevice,
        )?;
        self.synchronize()?;
        Ok(array)
    }

    fn download(&self, buffer: &DeviceArray, count: usize) -> Result<Vec<Complex64>, CudaMpsError> {
        let mut values = vec![Complex64::new(0.0, 0.0); count];
        assert!(
            size_of_val(values.as_slice()) <= buffer.buffer.bytes,
            "the buffer holds the values"
        );
        self.copy(
            values.as_mut_ptr().cast(),
            buffer.buffer.pointer,
            size_of_val(values.as_slice()),
            cudart_12::cudaMemcpyKind_cudaMemcpyDeviceToHost,
        )?;
        self.synchronize()?;
        Ok(values)
    }

    fn duplicate(&self, buffer: &DeviceArray, count: usize) -> Result<DeviceArray, CudaMpsError> {
        let bytes = count * size_of::<Complex64>();
        let array = self.allocate(bytes)?;
        self.copy(
            array.buffer.pointer,
            buffer.buffer.pointer,
            bytes,
            cudart_12::cudaMemcpyKind_cudaMemcpyDeviceToDevice,
        )?;
        self.synchronize()?;
        Ok(array)
    }

    #[allow(
        clippy::too_many_lines,
        clippy::many_single_char_names,
        reason = "one GateSplit call, its extents named as in its modes"
    )]
    fn gate_split(
        &self,
        call: &GateSplitCall<'_, DeviceArray>,
    ) -> Result<SplitOutcome<DeviceArray>, CudaMpsError> {
        let (l, m, r) = (call.left_extent, call.bond, call.right_extent);
        let k = call.max_extent;
        let descriptors = [
            self.descriptor(&[l, 2, m], b"ipj")?,
            self.descriptor(&[m, 2, r], b"jqk")?,
            self.descriptor(&[2, 2, 2, 2], b"PQpq")?,
            self.descriptor(&[l, 2, k], b"iPs")?,
            self.descriptor(&[k, 2, r], b"sQk")?,
        ];
        let [a_mode, b_mode, g_mode, u_mode, v_mode] = &descriptors;
        // `G[P, Q, p, q]` at `P + 2Q + 4p + 8q`, the output modes first.
        let gate: [Complex64; 16] = std::array::from_fn(|at| {
            let (big_p, big_q, p, q) = (at & 1, (at >> 1) & 1, (at >> 2) & 1, at >> 3);
            call.gate[(2 * big_p + big_q) * 4 + 2 * p + q]
        });
        self.copy(
            self.gate.pointer,
            gate.as_ptr().cast(),
            size_of_val(&gate),
            cudart_12::cudaMemcpyKind_cudaMemcpyHostToDevice,
        )?;
        let config = self.config(call.cutoff, call.partition)?;
        let workspace =
            self.prepare_workspace("cutensornetWorkspaceComputeGateSplitSizes", |workspace| {
                // SAFETY: every descriptor and the config are live.
                unsafe {
                    (self.context.tensor.workspace_gate_split_sizes)(
                        self.context.handle,
                        a_mode.descriptor,
                        b_mode.descriptor,
                        g_mode.descriptor,
                        u_mode.descriptor,
                        v_mode.descriptor,
                        policy::GATE_SPLIT_ALGO,
                        config,
                        v2_13::cutensornetComputeType_t_CUTENSORNET_COMPUTE_64F,
                        workspace,
                    )
                }
            })?;
        let u = self.allocate(l * 2 * k * size_of::<Complex64>())?;
        let v = self.allocate(k * 2 * r * size_of::<Complex64>())?;
        let values = self.values_buffer(k * size_of::<f64>())?;
        // SAFETY: every descriptor, buffer, the config, the info and the
        // workspace are live and sized for these descriptors.
        let status = unsafe {
            (self.context.tensor.gate_split)(
                self.context.handle,
                a_mode.descriptor,
                call.left.buffer.pointer,
                b_mode.descriptor,
                call.right.buffer.pointer,
                g_mode.descriptor,
                self.gate.pointer,
                u_mode.descriptor,
                u.buffer.pointer,
                values,
                v_mode.descriptor,
                v.buffer.pointer,
                policy::GATE_SPLIT_ALGO,
                config,
                v2_13::cutensornetComputeType_t_CUTENSORNET_COMPUTE_64F,
                self.info.info,
                workspace,
                self.context.stream,
            )
        };
        check_tensor("cutensornetGateSplit", status).map_err(failed)?;
        self.synchronize()?;
        let reduced = self.info_i64(
            v2_13::cutensornetTensorSVDInfoAttributes_t_CUTENSORNET_TENSOR_SVD_INFO_REDUCED_EXTENT,
        )?;
        let full = self.info_i64(
            v2_13::cutensornetTensorSVDInfoAttributes_t_CUTENSORNET_TENSOR_SVD_INFO_FULL_EXTENT,
        )?;
        let mut discarded_weight = 0.0_f64;
        self.read_info(
            v2_13::cutensornetTensorSVDInfoAttributes_t_CUTENSORNET_TENSOR_SVD_INFO_DISCARDED_WEIGHT,
            (&raw mut discarded_weight).cast(),
            size_of::<f64>(),
        )?;
        let svd_perturbation = if self.svd_algo.get() == TensorSvdAlgo::Gesvdp {
            let mut status = v2_13::cutensornetGesvdpStatus_t { errSigma: 0.0 };
            self.read_info(
                v2_13::cutensornetTensorSVDInfoAttributes_t_CUTENSORNET_TENSOR_SVD_INFO_ALGO_STATUS,
                (&raw mut status).cast(),
                size_of::<v2_13::cutensornetGesvdpStatus_t>(),
            )?;
            status.errSigma
        } else {
            0.0
        };
        let reduced = usize::try_from(reduced)
            .ok()
            .filter(|&reduced| (1..=k).contains(&reduced))
            .ok_or_else(|| {
                failed(format!(
                    "GateSplit reported a reduced extent {reduced} for {k}"
                ))
            })?;
        let singular_values = if call.read_values {
            let mut host = vec![0.0_f64; reduced];
            self.copy(
                host.as_mut_ptr().cast(),
                values,
                size_of_val(host.as_slice()),
                cudart_12::cudaMemcpyKind_cudaMemcpyDeviceToHost,
            )?;
            self.synchronize()?;
            host
        } else {
            Vec::new()
        };
        Ok(SplitOutcome {
            u,
            v,
            reduced,
            full: usize::try_from(full).unwrap_or(k),
            discarded_weight,
            svd_perturbation,
            singular_values,
        })
    }

    /// One `TensorQR` and one contraction (`contraction.rs`), after one
    /// more contraction for the gate, each waited for.
    #[expect(
        clippy::many_single_char_names,
        reason = "the extents as the trait's docs name them: l, m, r, and the new bond k"
    )]
    fn qr_move(
        &self,
        call: &QrMoveCall<'_, DeviceArray>,
    ) -> Result<QrMoveOutcome<DeviceArray>, CudaMpsError> {
        let (l, m, r) = (call.left_extent, call.bond, call.right_extent);
        let bytes = |count: usize| count * size_of::<Complex64>();
        let outcome = match call.partition {
            Partition::Sv => {
                let k = m.min(2 * l);
                let gated = self.gated(call.left, [l, 2, m], call.gate.as_ref())?;
                let input = gated.as_ref().unwrap_or(call.left);
                let q = self.allocate(bytes(l * 2 * k))?;
                let factor = self.allocate(bytes(k * m))?;
                self.tensor_qr(
                    (input, &[l, 2, m], b"ipj"),
                    (&q, &[l, 2, k], b"ips"),
                    (&factor, &[k, m], b"sj"),
                )?;
                let right = self.allocate(bytes(k * 2 * r))?;
                self.contract(
                    &Operand {
                        data: factor.buffer.pointer,
                        extents: &[k, m],
                        modes: b"sj",
                    },
                    &Operand {
                        data: call.right.buffer.pointer,
                        extents: &[m, 2, r],
                        modes: b"jqk",
                    },
                    &Output {
                        data: right.buffer.pointer,
                        extents: &[k, 2, r],
                        modes: b"sqk",
                    },
                )?;
                QrMoveOutcome {
                    left: q,
                    right,
                    bond: k,
                }
            }
            Partition::Us => {
                let k = m.min(2 * r);
                let gated = self.gated(call.right, [m, 2, r], call.gate.as_ref())?;
                let input = gated.as_ref().unwrap_or(call.right);
                let q = self.allocate(bytes(k * 2 * r))?;
                let factor = self.allocate(bytes(m * k))?;
                self.tensor_qr(
                    (input, &[m, 2, r], b"jqk"),
                    (&q, &[k, 2, r], b"sqk"),
                    (&factor, &[m, k], b"js"),
                )?;
                let left = self.allocate(bytes(l * 2 * k))?;
                self.contract(
                    &Operand {
                        data: call.left.buffer.pointer,
                        extents: &[l, 2, m],
                        modes: b"ipj",
                    },
                    &Operand {
                        data: factor.buffer.pointer,
                        extents: &[m, k],
                        modes: b"js",
                    },
                    &Output {
                        data: left.buffer.pointer,
                        extents: &[l, 2, k],
                        modes: b"ips",
                    },
                )?;
                QrMoveOutcome {
                    left,
                    right: q,
                    bond: k,
                }
            }
        };
        Ok(outcome)
    }
}

impl DeviceTensors {
    /// `site` (`[left, 2, right]`) with the one-site `gate` (row-major)
    /// applied, waited for; `None` when there is no gate.
    fn gated(
        &self,
        site: &DeviceArray,
        extents: [usize; 3],
        gate: Option<&[Complex64; 4]>,
    ) -> Result<Option<DeviceArray>, CudaMpsError> {
        let Some(gate) = gate else {
            return Ok(None);
        };
        // `G[P, p]` at `P + 2p`, the output mode first.
        let fortran: [Complex64; 4] = std::array::from_fn(|at| gate[2 * (at & 1) + (at >> 1)]);
        self.copy(
            self.gate.pointer,
            fortran.as_ptr().cast(),
            size_of_val(&fortran),
            cudart_12::cudaMemcpyKind_cudaMemcpyHostToDevice,
        )?;
        let out = self.allocate(extents.iter().product::<usize>() * size_of::<Complex64>())?;
        self.contract(
            &Operand {
                data: self.gate.pointer,
                extents: &[2, 2],
                modes: b"Pp",
            },
            &Operand {
                data: site.buffer.pointer,
                extents: &extents,
                modes: b"ipj",
            },
            &Output {
                data: out.buffer.pointer,
                extents: &extents,
                modes: b"iPj",
            },
        )?;
        Ok(Some(out))
    }

    /// `cutensornetTensorQR` of `input` into `q` and `r`, each given with
    /// its extents and modes (the shared mode in `q` and `r` only), waited
    /// for.
    fn tensor_qr(
        &self,
        input: (&DeviceArray, &[usize], &[u8]),
        q: (&DeviceArray, &[usize], &[u8]),
        r: (&DeviceArray, &[usize], &[u8]),
    ) -> Result<(), CudaMpsError> {
        let descriptors = [
            self.descriptor(input.1, input.2)?,
            self.descriptor(q.1, q.2)?,
            self.descriptor(r.1, r.2)?,
        ];
        let [input_mode, q_mode, r_mode] = &descriptors;
        let workspace =
            self.prepare_workspace("cutensornetWorkspaceComputeQRSizes", |workspace| {
                // SAFETY: the three descriptors are live.
                unsafe {
                    (self.context.tensor.workspace_qr_sizes)(
                        self.context.handle,
                        input_mode.descriptor,
                        q_mode.descriptor,
                        r_mode.descriptor,
                        workspace,
                    )
                }
            })?;
        // SAFETY: every descriptor, buffer and the workspace are live and
        // sized for these descriptors.
        let status = unsafe {
            (self.context.tensor.tensor_qr)(
                self.context.handle,
                input_mode.descriptor,
                input.0.buffer.pointer,
                q_mode.descriptor,
                q.0.buffer.pointer,
                r_mode.descriptor,
                r.0.buffer.pointer,
                workspace,
                self.context.stream,
            )
        };
        check_tensor("cutensornetTensorQR", status).map_err(failed)?;
        // The next call may resize the workspace this one uses, and the
        // descriptors drop here.
        self.synchronize()
    }
}
