// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Development only (not for review): the device calls of the
//! `gate_split_bench` example, which measures what one MPS update costs
//! when it is made of cuTensorNet's tensor calls (`GateSplit`, with each
//! of its SVD and gate-split algorithms, and `TensorQR`) or of cuSOLVER's
//! batched SVDs, rather than of the state API the engine uses. The example
//! checks each call's answer on the host, then times it.
//!
//! The bench opens nothing of its own on cuTensorNet and the CUDA Runtime:
//! it resolves the extra functions it needs (`BenchTensorFunctions`,
//! `BenchCudaFunctions`) in the libraries an [`Availability`] loaded and
//! checked, so discovery and the engine's tables are unchanged. cuSOLVER,
//! which the engine does not use, is opened by path ([`cusolver`]).

mod cusolver;

pub use cusolver::{BatchShape, BatchedSvd, Cusolver, SvdMethod};

use crate::Availability;
use crate::libraries::LoadedLibraries;
use crate::libraries::cuda_runtime::cudart_12;
use crate::libraries::cutensornet::v2_13;
use crate::libraries::loader::{FunctionTable, function_table};
use num_complex::Complex64;
use std::ffi::{c_int, c_uint, c_void};
use std::sync::Arc;
use std::time::Instant;

/// The device the bench runs on: device 0, one stream and one cuTensorNet
/// handle, on the libraries an [`Availability`] loaded.
pub struct BenchDevice {
    libraries: Arc<LoadedLibraries>,
    tensor: BenchTensorFunctions,
    cuda: BenchCudaFunctions,
    stream: cudart_12::cudaStream_t,
    handle: v2_13::cutensornetHandle_t,
    launch_target: DeviceBuffer,
}

impl BenchDevice {
    /// Opens device 0, its stream and a cuTensorNet handle.
    ///
    /// # Errors
    ///
    /// When a function the bench needs is missing, or a call fails.
    pub fn open(availability: &Availability) -> Result<Self, String> {
        let libraries = Arc::clone(availability.libraries());
        let tensor = BenchTensorFunctions::resolve(&libraries.cutensornet.library)
            .map_err(|missing| format!("cuTensorNet lacks {missing:?}"))?;
        let cuda = BenchCudaFunctions::resolve(&libraries.cuda_runtime.library)
            .map_err(|missing| format!("the CUDA Runtime lacks {missing:?}"))?;
        libraries
            .cuda_runtime
            .set_device(0)
            .map_err(|failure| cuda_failure(&failure))?;
        let mut stream = std::ptr::null_mut();
        // SAFETY: `stream` is a valid out-pointer for the new stream.
        let status = unsafe {
            (libraries.cuda_runtime.functions.stream_create_with_flags)(
                &raw mut stream,
                cudart_12::cudaStreamNonBlocking,
            )
        };
        check_cuda("cudaStreamCreateWithFlags", status)?;
        let mut handle = std::ptr::null_mut();
        // SAFETY: `handle` is a valid out-pointer for the new handle.
        let status = unsafe { (libraries.cutensornet.functions.create)(&raw mut handle) };
        check_tensor("cutensornetCreate", status)?;
        let launch_target = DeviceBuffer::new(&libraries, 256)?;
        Ok(Self {
            libraries,
            tensor,
            cuda,
            stream,
            handle,
            launch_target,
        })
    }

    /// Waits for the (idle) stream: the floor of every synchronisation.
    ///
    /// # Errors
    ///
    /// When the synchronisation fails.
    pub fn synchronize(&self) -> Result<(), String> {
        // SAFETY: the stream is live until `self` drops.
        let status =
            unsafe { (self.libraries.cuda_runtime.functions.stream_synchronize)(self.stream) };
        check_cuda("cudaStreamSynchronize", status)
    }

    /// The smallest launch the crate can make without a kernel of its own:
    /// a 4-byte `cudaMemsetAsync` on the stream, then waiting for it.
    ///
    /// # Errors
    ///
    /// When the memset or the synchronisation fails.
    pub fn empty_launch(&self) -> Result<(), String> {
        // SAFETY: `launch_target` holds 256 device bytes; the stream is live.
        let status =
            unsafe { (self.cuda.memset_async)(self.launch_target.pointer, 0, 4, self.stream) };
        check_cuda("cudaMemsetAsync", status)?;
        self.synchronize()
    }

    /// A device buffer of `values.len()` elements holding `values`.
    fn upload<T: Copy>(&self, values: &[T]) -> Result<DeviceBuffer, String> {
        let buffer = DeviceBuffer::new(&self.libraries, size_of_val(values).max(16))?;
        self.copy(
            buffer.pointer,
            values.as_ptr().cast(),
            size_of_val(values),
            cudart_12::cudaMemcpyKind_cudaMemcpyHostToDevice,
        )?;
        self.synchronize()?;
        Ok(buffer)
    }

    /// The first `count` elements of `buffer`, once the stream's work is
    /// done.
    fn download<T: Copy + Default>(
        &self,
        buffer: &DeviceBuffer,
        count: usize,
    ) -> Result<Vec<T>, String> {
        let mut values = vec![T::default(); count];
        assert!(
            size_of_val(values.as_slice()) <= buffer.bytes,
            "the buffer holds the values"
        );
        self.copy(
            values.as_mut_ptr().cast(),
            buffer.pointer,
            size_of_val(values.as_slice()),
            cudart_12::cudaMemcpyKind_cudaMemcpyDeviceToHost,
        )?;
        self.synchronize()?;
        Ok(values)
    }

    /// An asynchronous copy on the stream.
    fn copy(
        &self,
        destination: *mut c_void,
        source: *const c_void,
        bytes: usize,
        kind: cudart_12::cudaMemcpyKind,
    ) -> Result<(), String> {
        // SAFETY: the callers pass buffers of at least `bytes` bytes on the
        // sides `kind` names; the stream is live.
        let status =
            unsafe { (self.cuda.memcpy_async)(destination, source, bytes, kind, self.stream) };
        check_cuda("cudaMemcpyAsync", status)
    }

    /// A tensor descriptor of complex doubles, its first mode fastest.
    fn descriptor(&self, extents: &[i64], modes: &[u8]) -> Result<Descriptor, String> {
        let labels: Vec<i32> = modes.iter().map(|&mode| i32::from(mode)).collect();
        let mut descriptor = std::ptr::null_mut();
        // SAFETY: `extents` and `labels` have one entry per mode; null
        // strides mean the first mode is fastest; the handle is live.
        let status = unsafe {
            (self.tensor.create_descriptor)(
                self.handle,
                i32::try_from(extents.len()).expect("a tensor has few modes"),
                extents.as_ptr(),
                std::ptr::null(),
                labels.as_ptr(),
                v2_13::cudaDataType_t_CUDA_C_64F,
                &raw mut descriptor,
            )
        };
        check_tensor("cutensornetCreateTensorDescriptor", status)?;
        Ok(Descriptor {
            descriptor,
            destroy: self.tensor.destroy_descriptor,
        })
    }

    /// A workspace descriptor whose sizes `compute_sizes` sets, with the
    /// recommended memory of each space and kind attached.
    fn workspace(
        &self,
        compute_sizes: impl FnOnce(v2_13::cutensornetWorkspaceDescriptor_t) -> Status,
        operation: &'static str,
    ) -> Result<Workspace, String> {
        let functions = &self.libraries.cutensornet.functions;
        let mut descriptor = std::ptr::null_mut();
        // SAFETY: `descriptor` is a valid out-pointer; the handle is live.
        let status = unsafe { (functions.create_workspace)(self.handle, &raw mut descriptor) };
        check_tensor("cutensornetCreateWorkspaceDescriptor", status)?;
        let mut workspace = Workspace {
            descriptor,
            destroy: functions.destroy_workspace,
            device: Vec::new(),
            host: Vec::new(),
        };
        check_tensor(operation, compute_sizes(descriptor))?;
        for memspace in [
            v2_13::cutensornetMemspace_t_CUTENSORNET_MEMSPACE_DEVICE,
            v2_13::cutensornetMemspace_t_CUTENSORNET_MEMSPACE_HOST,
        ] {
            for kind in [
                v2_13::cutensornetWorkspaceKind_t_CUTENSORNET_WORKSPACE_SCRATCH,
                v2_13::cutensornetWorkspaceKind_t_CUTENSORNET_WORKSPACE_CACHE,
            ] {
                let mut size = 0_i64;
                // SAFETY: the descriptor is live and `size` is writable.
                let status = unsafe {
                    (functions.workspace_get_memory_size)(
                        self.handle,
                        descriptor,
                        v2_13::cutensornetWorksizePref_t_CUTENSORNET_WORKSIZE_PREF_RECOMMENDED,
                        memspace,
                        kind,
                        &raw mut size,
                    )
                };
                check_tensor("cutensornetWorkspaceGetMemorySize", status)?;
                let bytes = usize::try_from(size).unwrap_or(0);
                if bytes == 0 {
                    continue;
                }
                let pointer =
                    if memspace == v2_13::cutensornetMemspace_t_CUTENSORNET_MEMSPACE_DEVICE {
                        let buffer = DeviceBuffer::new(&self.libraries, bytes)?;
                        let pointer = buffer.pointer;
                        workspace.device.push(buffer);
                        pointer
                    } else {
                        let mut buffer = vec![0_u8; bytes + 256];
                        let offset = buffer.as_ptr().align_offset(256);
                        let pointer = buffer[offset..].as_mut_ptr().cast();
                        workspace.host.push(buffer);
                        pointer
                    };
                // SAFETY: `pointer` holds `size` bytes in `memspace` and
                // lives as long as the workspace.
                let status = unsafe {
                    (functions.workspace_set_memory)(
                        self.handle,
                        descriptor,
                        memspace,
                        kind,
                        pointer,
                        size,
                    )
                };
                check_tensor("cutensornetWorkspaceSetMemory", status)?;
            }
        }
        Ok(workspace)
    }

    /// `cutensornetGateSplit` of `gate` applied to the bond between `left`
    /// and `right`, each `bond × 2 × bond`, the first mode fastest, with
    /// the SVD algorithm `svd`, the gate-split algorithm `split` and the
    /// truncation `truncation` (see [`Truncation`]). `gate` is
    /// `2 × 2 × 2 × 2`, its output modes first. Runs once, so that the
    /// outcome can be read.
    ///
    /// # Errors
    ///
    /// When a call fails, the library's refusal of a combination included.
    pub fn gate_split(
        &self,
        bond: usize,
        svd: TensorSvdAlgo,
        split: GateSplitAlgo,
        truncation: Truncation,
        problem: GateSplitInputs<'_>,
    ) -> Result<GateSplit<'_>, String> {
        let b = i64::try_from(bond).expect("a bond fits in i64");
        let shared = match truncation {
            Truncation::DiscardedWeight(_) => b,
            Truncation::FixedExtent(extent) => {
                assert!(
                    (1..=bond).contains(&extent),
                    "a fixed extent lies in 1..=bond"
                );
                i64::try_from(extent).expect("an extent fits in i64")
            }
        };
        let started = Instant::now();
        let descriptors = [
            self.descriptor(&[b, 2, b], b"ipj")?,
            self.descriptor(&[b, 2, b], b"jqk")?,
            self.descriptor(&[2, 2, 2, 2], b"PQpq")?,
            self.descriptor(&[b, 2, shared], b"iPs")?,
            self.descriptor(&[shared, 2, b], b"sQk")?,
        ];
        let config = self.svd_config(svd, truncation)?;
        let mut info = std::ptr::null_mut();
        // SAFETY: `info` is a valid out-pointer; the handle is live.
        let status = unsafe { (self.tensor.create_svd_info)(self.handle, &raw mut info) };
        check_tensor("cutensornetCreateTensorSVDInfo", status)?;
        let info = SvdInfo {
            info,
            destroy: self.tensor.destroy_svd_info,
        };
        let [left_mode, right_mode, gate_mode, u_mode, v_mode] = &descriptors;
        let workspace = self.workspace(
            |workspace| {
                // SAFETY: every descriptor and the config are live.
                unsafe {
                    (self.tensor.workspace_gate_split_sizes)(
                        self.handle,
                        left_mode.descriptor,
                        right_mode.descriptor,
                        gate_mode.descriptor,
                        u_mode.descriptor,
                        v_mode.descriptor,
                        split.raw(),
                        config.config,
                        v2_13::cutensornetComputeType_t_CUTENSORNET_COMPUTE_64F,
                        workspace,
                    )
                }
            },
            "cutensornetWorkspaceComputeGateSplitSizes",
        )?;
        let setup_seconds = started.elapsed().as_secs_f64();
        let mut prepared = GateSplit {
            device: self,
            bond,
            svd,
            split,
            left: self.upload(problem.left)?,
            right: self.upload(problem.right)?,
            gate: self.upload(problem.gate)?,
            u: DeviceBuffer::new(&self.libraries, 2 * bond * bond * 16)?,
            s: DeviceBuffer::new(&self.libraries, bond * 8)?,
            v: DeviceBuffer::new(&self.libraries, 2 * bond * bond * 16)?,
            descriptors,
            config,
            info,
            workspace,
            setup_seconds,
        };
        prepared.run()?;
        Ok(prepared)
    }

    /// An SVD config with the algorithm `svd`, the library's default
    /// parameters for it (`CONFIG_ALGO_PARAMS` is not set), the
    /// discarded-weight cutoff when `truncation` has one, and no
    /// normalisation of the singular values.
    fn svd_config(&self, svd: TensorSvdAlgo, truncation: Truncation) -> Result<SvdConfig, String> {
        let mut config = std::ptr::null_mut();
        // SAFETY: `config` is a valid out-pointer; the handle is live.
        let status = unsafe { (self.tensor.create_svd_config)(self.handle, &raw mut config) };
        check_tensor("cutensornetCreateTensorSVDConfig", status)?;
        let config = SvdConfig {
            config,
            destroy: self.tensor.destroy_svd_config,
        };
        let algo = svd.raw();
        // SAFETY: the attribute is a `cutensornetTensorSVDAlgo_t`, passed
        // with its size.
        let status = unsafe {
            (self.tensor.svd_config_set_attribute)(
                self.handle,
                config.config,
                v2_13::cutensornetTensorSVDConfigAttributes_t_CUTENSORNET_TENSOR_SVD_CONFIG_ALGO,
                (&raw const algo).cast(),
                size_of::<v2_13::cutensornetTensorSVDAlgo_t>(),
            )
        };
        check_tensor("cutensornetTensorSVDConfigSetAttribute(ALGO)", status)?;
        if let Truncation::DiscardedWeight(cutoff) = truncation {
            // SAFETY: the attribute is a `double`, passed with its size.
            let status = unsafe {
                (self.tensor.svd_config_set_attribute)(
                    self.handle,
                    config.config,
                    v2_13::cutensornetTensorSVDConfigAttributes_t_CUTENSORNET_TENSOR_SVD_CONFIG_DISCARDED_WEIGHT_CUTOFF,
                    (&raw const cutoff).cast(),
                    size_of::<f64>(),
                )
            };
            check_tensor(
                "cutensornetTensorSVDConfigSetAttribute(DISCARDED_WEIGHT_CUTOFF)",
                status,
            )?;
        }
        Ok(config)
    }

    /// `cutensornetTensorQR` of `tensor`, `bond × 2 × bond`, the first mode
    /// fastest, into `Q` (`bond × 2 × bond`) and `R` (`bond × bond`): the
    /// move of an orthogonality centre by one site. Runs once.
    ///
    /// # Errors
    ///
    /// When a call fails.
    pub fn tensor_qr(&self, bond: usize, tensor: &[Complex64]) -> Result<TensorQr<'_>, String> {
        let b = i64::try_from(bond).expect("a bond fits in i64");
        let descriptors = [
            self.descriptor(&[b, 2, b], b"ipj")?,
            self.descriptor(&[b, 2, b], b"ips")?,
            self.descriptor(&[b, b], b"sj")?,
        ];
        let [input, q, r] = &descriptors;
        let workspace = self.workspace(
            |workspace| {
                // SAFETY: every descriptor is live.
                unsafe {
                    (self.tensor.workspace_qr_sizes)(
                        self.handle,
                        input.descriptor,
                        q.descriptor,
                        r.descriptor,
                        workspace,
                    )
                }
            },
            "cutensornetWorkspaceComputeQRSizes",
        )?;
        let mut qr = TensorQr {
            device: self,
            bond,
            input: self.upload(tensor)?,
            q: DeviceBuffer::new(&self.libraries, 2 * bond * bond * 16)?,
            r: DeviceBuffer::new(&self.libraries, bond * bond * 16)?,
            descriptors,
            workspace,
        };
        qr.run()?;
        Ok(qr)
    }
}

impl Drop for BenchDevice {
    fn drop(&mut self) {
        // SAFETY: the handle and the stream were created in `open` and are
        // destroyed once; the statuses are not needed when dropping.
        unsafe {
            let _ = (self.libraries.cutensornet.functions.destroy)(self.handle);
            let _ = (self.libraries.cuda_runtime.functions.stream_destroy)(self.stream);
        }
    }
}

/// cuTensorNet's SVD algorithms (`cutensornetTensorSVDAlgo_t`), each a
/// cuSOLVER routine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TensorSvdAlgo {
    /// `cusolverDnGesvd`, cuTensorNet's default.
    Gesvd,
    /// `cusolverDnGesvdj`, Jacobi sweeps.
    Gesvdj,
    /// `cusolverDnXgesvdp`, through a polar decomposition.
    Gesvdp,
    /// `cusolverDnXgesvdr`, randomized: a fixed extent only, no
    /// discarded-weight cutoff.
    Gesvdr,
}

impl TensorSvdAlgo {
    /// Every algorithm, the default first.
    pub const ALL: [Self; 4] = [Self::Gesvd, Self::Gesvdj, Self::Gesvdp, Self::Gesvdr];

    /// The name the bench's flags and lines use (`gesvdj`).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Gesvd => "gesvd",
            Self::Gesvdj => "gesvdj",
            Self::Gesvdp => "gesvdp",
            Self::Gesvdr => "gesvdr",
        }
    }

    /// The algorithm named `name`, as [`Self::name`] writes it.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|algo| algo.name() == name)
    }

    fn raw(self) -> v2_13::cutensornetTensorSVDAlgo_t {
        match self {
            Self::Gesvd => v2_13::cutensornetTensorSVDAlgo_t_CUTENSORNET_TENSOR_SVD_ALGO_GESVD,
            Self::Gesvdj => v2_13::cutensornetTensorSVDAlgo_t_CUTENSORNET_TENSOR_SVD_ALGO_GESVDJ,
            Self::Gesvdp => v2_13::cutensornetTensorSVDAlgo_t_CUTENSORNET_TENSOR_SVD_ALGO_GESVDP,
            Self::Gesvdr => v2_13::cutensornetTensorSVDAlgo_t_CUTENSORNET_TENSOR_SVD_ALGO_GESVDR,
        }
    }
}

/// cuTensorNet's gate-split algorithms (`cutensornetGateSplitAlgo_t`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GateSplitAlgo {
    /// QR of each input first, then the SVD of the small middle tensor.
    Reduced,
    /// The contraction of the inputs and the gate, then its SVD.
    Direct,
}

impl GateSplitAlgo {
    /// Every algorithm.
    pub const ALL: [Self; 2] = [Self::Reduced, Self::Direct];

    /// The name the bench's flags and lines use (`reduced`).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Reduced => "reduced",
            Self::Direct => "direct",
        }
    }

    /// The algorithm named `name`, as [`Self::name`] writes it.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|algo| algo.name() == name)
    }

    fn raw(self) -> v2_13::cutensornetGateSplitAlgo_t {
        match self {
            Self::Reduced => v2_13::cutensornetGateSplitAlgo_t_CUTENSORNET_GATE_SPLIT_ALGO_REDUCED,
            Self::Direct => v2_13::cutensornetGateSplitAlgo_t_CUTENSORNET_GATE_SPLIT_ALGO_DIRECT,
        }
    }
}

/// How a gate split truncates the shared bond.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Truncation {
    /// At most `bond` values, fewer when the discarded weight stays within
    /// this cutoff (`CONFIG_DISCARDED_WEIGHT_CUTOFF`), as the engine does.
    DiscardedWeight(f64),
    /// Exactly this many values, set as the shared mode's extent in the
    /// output descriptors: the only truncation GESVDR accepts.
    FixedExtent(usize),
}

/// The tensors one gate split reads, on the host.
#[derive(Clone, Copy)]
pub struct GateSplitInputs<'a> {
    /// `bond × 2 × bond`, modes `(i, p, j)`, the first fastest.
    pub left: &'a [Complex64],
    /// `bond × 2 × bond`, modes `(j, q, k)`.
    pub right: &'a [Complex64],
    /// `2 × 2 × 2 × 2`, modes `(P, Q, p, q)`, its output modes first.
    pub gate: &'a [Complex64],
}

/// A prepared `cutensornetGateSplit`: its descriptors, config, workspace
/// and buffers, made once, so that [`GateSplit::run`] times the call alone.
pub struct GateSplit<'d> {
    device: &'d BenchDevice,
    bond: usize,
    svd: TensorSvdAlgo,
    split: GateSplitAlgo,
    left: DeviceBuffer,
    right: DeviceBuffer,
    gate: DeviceBuffer,
    u: DeviceBuffer,
    s: DeviceBuffer,
    v: DeviceBuffer,
    descriptors: [Descriptor; 5],
    config: SvdConfig,
    info: SvdInfo,
    workspace: Workspace,
    setup_seconds: f64,
}

/// What one `cutensornetGateSplit` returned.
pub struct GateSplitOutcome {
    /// The kept singular values, largest first.
    pub singular_values: Vec<f64>,
    /// The shared bond's extent before truncation.
    pub full_extent: i64,
    /// The shared bond's extent after truncation.
    pub reduced_extent: i64,
    /// The truncation's discarded weight, as cuTensorNet reports it; `None`
    /// for GESVDR, whose fixed-extent truncation does not compute it.
    pub discarded_weight: Option<f64>,
}

impl GateSplit<'_> {
    /// Calls `cutensornetGateSplit`, waits for it, and reads the reduced
    /// extent, as an update that must know the new bond does.
    ///
    /// # Errors
    ///
    /// When a call fails.
    pub fn run(&mut self) -> Result<i64, String> {
        let device = self.device;
        let [left_mode, right_mode, gate_mode, u_mode, v_mode] = &self.descriptors;
        // SAFETY: every descriptor, buffer, the config, the info and the
        // workspace are live and sized for these descriptors.
        let status = unsafe {
            (device.tensor.gate_split)(
                device.handle,
                left_mode.descriptor,
                self.left.pointer,
                right_mode.descriptor,
                self.right.pointer,
                gate_mode.descriptor,
                self.gate.pointer,
                u_mode.descriptor,
                self.u.pointer,
                self.s.pointer,
                v_mode.descriptor,
                self.v.pointer,
                self.split.raw(),
                self.config.config,
                v2_13::cutensornetComputeType_t_CUTENSORNET_COMPUTE_64F,
                self.info.info,
                self.workspace.descriptor,
                device.stream,
            )
        };
        check_tensor("cutensornetGateSplit", status)?;
        device.synchronize()?;
        self.info_i64(
            v2_13::cutensornetTensorSVDInfoAttributes_t_CUTENSORNET_TENSOR_SVD_INFO_REDUCED_EXTENT,
        )
    }

    /// The last run's singular values, extents and discarded weight.
    ///
    /// # Errors
    ///
    /// When a call fails.
    pub fn outcome(&self) -> Result<GateSplitOutcome, String> {
        let reduced_extent = self.info_i64(
            v2_13::cutensornetTensorSVDInfoAttributes_t_CUTENSORNET_TENSOR_SVD_INFO_REDUCED_EXTENT,
        )?;
        let full_extent = self.info_i64(
            v2_13::cutensornetTensorSVDInfoAttributes_t_CUTENSORNET_TENSOR_SVD_INFO_FULL_EXTENT,
        )?;
        let discarded_weight = if self.svd == TensorSvdAlgo::Gesvdr {
            None
        } else {
            let mut weight = 0.0_f64;
            self.read_info(
                v2_13::cutensornetTensorSVDInfoAttributes_t_CUTENSORNET_TENSOR_SVD_INFO_DISCARDED_WEIGHT,
                (&raw mut weight).cast(),
                size_of::<f64>(),
            )?;
            Some(weight)
        };
        let kept = usize::try_from(reduced_extent).unwrap_or(0).min(self.bond);
        Ok(GateSplitOutcome {
            singular_values: self.device.download(&self.s, kept)?,
            full_extent,
            reduced_extent,
            discarded_weight,
        })
    }

    /// What the last run's SVD algorithm reports of its own accuracy
    /// (`INFO_ALGO_STATUS`), as `key=value` pairs: GESVDJ's residual and
    /// sweeps, GESVDP's perturbation; `None` for the algorithms that
    /// report nothing.
    ///
    /// # Errors
    ///
    /// When the read fails.
    pub fn algo_status(&self) -> Result<Option<String>, String> {
        let attribute =
            v2_13::cutensornetTensorSVDInfoAttributes_t_CUTENSORNET_TENSOR_SVD_INFO_ALGO_STATUS;
        match self.svd {
            TensorSvdAlgo::Gesvdj => {
                let mut status = v2_13::cutensornetGesvdjStatus_t {
                    residual: 0.0,
                    sweeps: 0,
                };
                self.read_info(
                    attribute,
                    (&raw mut status).cast(),
                    size_of::<v2_13::cutensornetGesvdjStatus_t>(),
                )?;
                Ok(Some(format!(
                    "gesvdj_residual={:.2e} gesvdj_sweeps={}",
                    status.residual, status.sweeps
                )))
            }
            TensorSvdAlgo::Gesvdp => {
                let mut status = v2_13::cutensornetGesvdpStatus_t { errSigma: 0.0 };
                self.read_info(
                    attribute,
                    (&raw mut status).cast(),
                    size_of::<v2_13::cutensornetGesvdpStatus_t>(),
                )?;
                Ok(Some(format!("gesvdp_err_sigma={:.2e}", status.errSigma)))
            }
            TensorSvdAlgo::Gesvd | TensorSvdAlgo::Gesvdr => Ok(None),
        }
    }

    /// The seconds spent creating the descriptors, the config, the info and
    /// the workspace's sizes, before the first run: what an update pays
    /// when its shapes are new.
    #[must_use]
    pub fn setup_seconds(&self) -> f64 {
        self.setup_seconds
    }

    fn info_i64(&self, attribute: c_uint) -> Result<i64, String> {
        let mut value = 0_i64;
        self.read_info(attribute, (&raw mut value).cast(), size_of::<i64>())?;
        Ok(value)
    }

    fn read_info(&self, attribute: c_uint, value: *mut c_void, bytes: usize) -> Result<(), String> {
        // SAFETY: `value` holds `bytes` bytes, the attribute's size.
        let status = unsafe {
            (self.device.tensor.svd_info_get_attribute)(
                self.device.handle,
                self.info.info,
                attribute,
                value,
                bytes,
            )
        };
        check_tensor("cutensornetTensorSVDInfoGetAttribute", status)
    }
}

/// A prepared `cutensornetTensorQR`, so that [`TensorQr::run`] times the
/// call alone.
pub struct TensorQr<'d> {
    device: &'d BenchDevice,
    bond: usize,
    input: DeviceBuffer,
    q: DeviceBuffer,
    r: DeviceBuffer,
    descriptors: [Descriptor; 3],
    workspace: Workspace,
}

impl TensorQr<'_> {
    /// Calls `cutensornetTensorQR` and waits for it.
    ///
    /// # Errors
    ///
    /// When a call fails.
    pub fn run(&mut self) -> Result<(), String> {
        let device = self.device;
        let [input, q, r] = &self.descriptors;
        // SAFETY: every descriptor, buffer and the workspace are live and
        // sized for these descriptors.
        let status = unsafe {
            (device.tensor.tensor_qr)(
                device.handle,
                input.descriptor,
                self.input.pointer,
                q.descriptor,
                self.q.pointer,
                r.descriptor,
                self.r.pointer,
                self.workspace.descriptor,
                device.stream,
            )
        };
        check_tensor("cutensornetTensorQR", status)?;
        device.synchronize()
    }

    /// The last run's `Q` (`bond × 2 × bond`) and `R` (`bond × bond`), the
    /// first mode fastest.
    ///
    /// # Errors
    ///
    /// When a copy fails.
    pub fn factors(&self) -> Result<(Vec<Complex64>, Vec<Complex64>), String> {
        Ok((
            self.device.download(&self.q, 2 * self.bond * self.bond)?,
            self.device.download(&self.r, self.bond * self.bond)?,
        ))
    }
}

type Status = v2_13::cutensornetStatus_t;

/// Device memory, freed on drop.
struct DeviceBuffer {
    pointer: *mut c_void,
    bytes: usize,
    free: crate::libraries::cuda_runtime::FreeFn,
}

impl DeviceBuffer {
    fn new(libraries: &LoadedLibraries, bytes: usize) -> Result<Self, String> {
        let mut pointer = std::ptr::null_mut();
        // SAFETY: `pointer` is a valid out-pointer.
        let status = unsafe { (libraries.cuda_runtime.functions.malloc)(&raw mut pointer, bytes) };
        check_cuda("cudaMalloc", status)?;
        Ok(Self {
            pointer,
            bytes,
            free: libraries.cuda_runtime.functions.free,
        })
    }
}

impl Drop for DeviceBuffer {
    fn drop(&mut self) {
        // SAFETY: the pointer came from `cudaMalloc` and is freed once.
        let _ = unsafe { (self.free)(self.pointer) };
    }
}

struct Descriptor {
    descriptor: v2_13::cutensornetTensorDescriptor_t,
    destroy: DestroyDescriptorFn,
}

impl Drop for Descriptor {
    fn drop(&mut self) {
        // SAFETY: created by `cutensornetCreateTensorDescriptor`, destroyed
        // once.
        let _ = unsafe { (self.destroy)(self.descriptor) };
    }
}

struct SvdConfig {
    config: v2_13::cutensornetTensorSVDConfig_t,
    destroy: DestroySvdConfigFn,
}

impl Drop for SvdConfig {
    fn drop(&mut self) {
        // SAFETY: created by `cutensornetCreateTensorSVDConfig`, destroyed
        // once.
        let _ = unsafe { (self.destroy)(self.config) };
    }
}

struct SvdInfo {
    info: v2_13::cutensornetTensorSVDInfo_t,
    destroy: DestroySvdInfoFn,
}

impl Drop for SvdInfo {
    fn drop(&mut self) {
        // SAFETY: created by `cutensornetCreateTensorSVDInfo`, destroyed
        // once.
        let _ = unsafe { (self.destroy)(self.info) };
    }
}

/// A workspace descriptor and the memory attached to it.
struct Workspace {
    descriptor: v2_13::cutensornetWorkspaceDescriptor_t,
    destroy: crate::libraries::cutensornet::workspace::DestroyWorkspaceFn,
    // Held so that the attached memory lives as long as the descriptor.
    device: Vec<DeviceBuffer>,
    host: Vec<Vec<u8>>,
}

impl Drop for Workspace {
    fn drop(&mut self) {
        // SAFETY: created by `cutensornetCreateWorkspaceDescriptor`,
        // destroyed once, before its memory (fields drop after `drop`).
        let _ = unsafe { (self.destroy)(self.descriptor) };
    }
}

fn check_cuda(operation: &str, status: cudart_12::cudaError_t) -> Result<(), String> {
    if status == cudart_12::cudaError_cudaSuccess {
        Ok(())
    } else {
        Err(format!("{operation} returned CUDA status {status}"))
    }
}

fn cuda_failure(failure: &crate::libraries::cuda_runtime::CudaCallFailed) -> String {
    format!(
        "{} returned CUDA status {}: {}",
        failure.operation, failure.status, failure.message
    )
}

fn check_tensor(operation: &str, status: Status) -> Result<(), String> {
    if status == v2_13::cutensornetStatus_t_CUTENSORNET_STATUS_SUCCESS {
        Ok(())
    } else {
        Err(format!("{operation} returned cuTensorNet status {status}"))
    }
}

type CreateDescriptorFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    i32,
    *const i64,
    *const i64,
    *const i32,
    v2_13::cudaDataType_t,
    *mut v2_13::cutensornetTensorDescriptor_t,
) -> Status;
type DestroyDescriptorFn = unsafe extern "C" fn(v2_13::cutensornetTensorDescriptor_t) -> Status;
type CreateSvdConfigFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    *mut v2_13::cutensornetTensorSVDConfig_t,
) -> Status;
type DestroySvdConfigFn = unsafe extern "C" fn(v2_13::cutensornetTensorSVDConfig_t) -> Status;
type SvdConfigSetAttributeFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    v2_13::cutensornetTensorSVDConfig_t,
    v2_13::cutensornetTensorSVDConfigAttributes_t,
    *const c_void,
    usize,
) -> Status;
type CreateSvdInfoFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    *mut v2_13::cutensornetTensorSVDInfo_t,
) -> Status;
type DestroySvdInfoFn = unsafe extern "C" fn(v2_13::cutensornetTensorSVDInfo_t) -> Status;
type SvdInfoGetAttributeFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    v2_13::cutensornetTensorSVDInfo_t,
    v2_13::cutensornetTensorSVDInfoAttributes_t,
    *mut c_void,
    usize,
) -> Status;
type WorkspaceQrSizesFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    v2_13::cutensornetTensorDescriptor_t,
    v2_13::cutensornetTensorDescriptor_t,
    v2_13::cutensornetTensorDescriptor_t,
    v2_13::cutensornetWorkspaceDescriptor_t,
) -> Status;
type TensorQrFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    v2_13::cutensornetTensorDescriptor_t,
    *const c_void,
    v2_13::cutensornetTensorDescriptor_t,
    *mut c_void,
    v2_13::cutensornetTensorDescriptor_t,
    *mut c_void,
    v2_13::cutensornetWorkspaceDescriptor_t,
    cudart_12::cudaStream_t,
) -> Status;
type WorkspaceGateSplitSizesFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    v2_13::cutensornetTensorDescriptor_t,
    v2_13::cutensornetTensorDescriptor_t,
    v2_13::cutensornetTensorDescriptor_t,
    v2_13::cutensornetTensorDescriptor_t,
    v2_13::cutensornetTensorDescriptor_t,
    v2_13::cutensornetGateSplitAlgo_t,
    v2_13::cutensornetTensorSVDConfig_t,
    v2_13::cutensornetComputeType_t,
    v2_13::cutensornetWorkspaceDescriptor_t,
) -> Status;
type GateSplitFn = unsafe extern "C" fn(
    v2_13::cutensornetHandle_t,
    v2_13::cutensornetTensorDescriptor_t,
    *const c_void,
    v2_13::cutensornetTensorDescriptor_t,
    *const c_void,
    v2_13::cutensornetTensorDescriptor_t,
    *const c_void,
    v2_13::cutensornetTensorDescriptor_t,
    *mut c_void,
    *mut c_void,
    v2_13::cutensornetTensorDescriptor_t,
    *mut c_void,
    v2_13::cutensornetGateSplitAlgo_t,
    v2_13::cutensornetTensorSVDConfig_t,
    v2_13::cutensornetComputeType_t,
    v2_13::cutensornetTensorSVDInfo_t,
    v2_13::cutensornetWorkspaceDescriptor_t,
    cudart_12::cudaStream_t,
) -> Status;
type MemsetAsyncFn = unsafe extern "C" fn(
    *mut c_void,
    c_int,
    usize,
    cudart_12::cudaStream_t,
) -> cudart_12::cudaError_t;
type MemcpyAsyncFn = unsafe extern "C" fn(
    *mut c_void,
    *const c_void,
    usize,
    cudart_12::cudaMemcpyKind,
    cudart_12::cudaStream_t,
) -> cudart_12::cudaError_t;

function_table! {
    /// The cuTensorNet functions the bench calls beyond the engine's.
    struct BenchTensorFunctions, declared in v2_13 {
        required create_descriptor: CreateDescriptorFn = cutensornetCreateTensorDescriptor,
        required destroy_descriptor: DestroyDescriptorFn = cutensornetDestroyTensorDescriptor,
        required create_svd_config: CreateSvdConfigFn = cutensornetCreateTensorSVDConfig,
        required destroy_svd_config: DestroySvdConfigFn = cutensornetDestroyTensorSVDConfig,
        required svd_config_set_attribute: SvdConfigSetAttributeFn =
            cutensornetTensorSVDConfigSetAttribute,
        required create_svd_info: CreateSvdInfoFn = cutensornetCreateTensorSVDInfo,
        required destroy_svd_info: DestroySvdInfoFn = cutensornetDestroyTensorSVDInfo,
        required svd_info_get_attribute: SvdInfoGetAttributeFn = cutensornetTensorSVDInfoGetAttribute,
        required workspace_qr_sizes: WorkspaceQrSizesFn = cutensornetWorkspaceComputeQRSizes,
        required tensor_qr: TensorQrFn = cutensornetTensorQR,
        required workspace_gate_split_sizes: WorkspaceGateSplitSizesFn =
            cutensornetWorkspaceComputeGateSplitSizes,
        required gate_split: GateSplitFn = cutensornetGateSplit,
    }
}

function_table! {
    /// The CUDA Runtime functions the bench calls beyond the engine's.
    struct BenchCudaFunctions, declared in cudart_12 {
        required memset_async: MemsetAsyncFn = cudaMemsetAsync,
        required memcpy_async: MemcpyAsyncFn = cudaMemcpyAsync,
    }
}
