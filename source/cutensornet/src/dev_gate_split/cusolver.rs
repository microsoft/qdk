// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Development only (not for review): cuSOLVER's batched SVDs, the
//! alternative to one `cutensornetGateSplit` per bond when many bonds are
//! updated at once: one launch for `K` matrices.
//!
//! The engine does not use cuSOLVER and the crate has no bindings for it,
//! so its few functions are declared here by hand, from cuSOLVER 11's
//! `cusolverDn.h`, and resolved in the library [`Cusolver::open`] is given.

use super::{BenchDevice, DeviceBuffer};
use crate::libraries::cuda_runtime::cudart_12;
use crate::libraries::loader::{OpenLibraryFile, SymbolResolver, load_library};
use num_complex::Complex64;
use std::ffi::{c_int, c_void};
use std::path::PathBuf;

/// cuSOLVER's dense API on the bench's stream.
pub struct Cusolver {
    functions: CusolverFunctions,
    handle: Handle,
    // Dropped after the handle is destroyed (`Drop` runs first).
    _library: OpenLibraryFile,
}

/// A batched SVD of cuSOLVER's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SvdMethod {
    /// `cusolverDnZgesvdjBatched`: Jacobi, every singular value and both
    /// vector sets, matrices at most 32 × 32. It overwrites its input, so
    /// each run first restores it from a copy on the device.
    Gesvdj,
    /// `cusolverDnZgesvdaStridedBatched`: the largest `rank` singular
    /// values and their vectors, of tall or square matrices.
    Gesvda,
}

/// The matrices of a batched SVD.
#[derive(Clone, Copy, Debug)]
pub struct BatchShape {
    /// Rows of each matrix.
    pub rows: usize,
    /// Columns of each matrix.
    pub columns: usize,
    /// Singular values kept by [`SvdMethod::Gesvda`].
    pub rank: usize,
    /// How many matrices one launch factors.
    pub batch: usize,
}

impl Cusolver {
    /// Opens cuSOLVER from `path` and creates a handle on `device`'s
    /// stream.
    ///
    /// # Errors
    ///
    /// When the library cannot be opened, lacks a function, or a call
    /// fails.
    pub fn open(path: PathBuf, device: &BenchDevice) -> Result<Self, String> {
        let library = load_library(vec![path]).map_err(|fault| format!("{fault:?}"))?;
        let functions = CusolverFunctions::resolve(&library)?;
        let mut handle = std::ptr::null_mut();
        // SAFETY: `handle` is a valid out-pointer.
        check_solver("cusolverDnCreate", unsafe {
            (functions.create)(&raw mut handle)
        })?;
        let solver = Self {
            functions,
            handle,
            _library: library,
        };
        // SAFETY: the handle was just created; the stream is live.
        check_solver("cusolverDnSetStream", unsafe {
            (solver.functions.set_stream)(solver.handle, device.stream)
        })?;
        Ok(solver)
    }

    /// A prepared batched SVD of the `shape.batch` column-major
    /// `rows × columns` matrices of `matrices`, one after another, keeping
    /// `shape.rank` singular values (all of them for [`SvdMethod::Gesvdj`]).
    /// Runs once.
    ///
    /// # Errors
    ///
    /// When a call fails.
    pub fn batched_svd<'d>(
        &'d self,
        device: &'d BenchDevice,
        method: SvdMethod,
        shape: BatchShape,
        matrices: &[Complex64],
    ) -> Result<BatchedSvd<'d>, String> {
        let BatchShape {
            rows,
            columns,
            rank,
            batch,
        } = shape;
        assert_eq!(
            matrices.len(),
            rows * columns * batch,
            "one matrix per batch entry"
        );
        let (kept, u_columns, v_columns) = match method {
            SvdMethod::Gesvdj => (rows.min(columns), rows, columns),
            SvdMethod::Gesvda => (rank, rank, rank),
        };
        let libraries = &device.libraries;
        let mut svd = BatchedSvd {
            solver: self,
            device,
            method,
            shape,
            kept,
            original: device.upload(matrices)?,
            input: device.upload(matrices)?,
            s: DeviceBuffer::new(libraries, kept * batch * 8)?,
            u: DeviceBuffer::new(libraries, rows * u_columns * batch * 16)?,
            v: DeviceBuffer::new(libraries, columns * v_columns * batch * 16)?,
            info: DeviceBuffer::new(libraries, batch * 4)?,
            work: DeviceBuffer::new(libraries, 16)?,
            work_elements: 0,
            jacobi: std::ptr::null_mut(),
            residuals: vec![0.0; batch],
        };
        svd.prepare()?;
        svd.run()?;
        Ok(svd)
    }
}

impl Drop for Cusolver {
    fn drop(&mut self) {
        // SAFETY: created in `open`, destroyed once.
        let _ = unsafe { (self.functions.destroy)(self.handle) };
    }
}

/// A prepared batched SVD, so that [`BatchedSvd::run`] times the launch.
pub struct BatchedSvd<'d> {
    solver: &'d Cusolver,
    device: &'d BenchDevice,
    method: SvdMethod,
    shape: BatchShape,
    kept: usize,
    original: DeviceBuffer,
    input: DeviceBuffer,
    s: DeviceBuffer,
    u: DeviceBuffer,
    v: DeviceBuffer,
    info: DeviceBuffer,
    work: DeviceBuffer,
    work_elements: c_int,
    jacobi: *mut c_void,
    residuals: Vec<f64>,
}

impl BatchedSvd<'_> {
    /// Restores the input when the method overwrites it (GESVDJ), then
    /// launches the batched SVD and waits for it.
    ///
    /// # Errors
    ///
    /// When a call fails.
    pub fn run(&mut self) -> Result<(), String> {
        if self.method == SvdMethod::Gesvdj {
            self.restore_input()?;
        }
        let functions = &self.solver.functions;
        let (m, n, rank, batch) = self.dimensions();
        match self.method {
            SvdMethod::Gesvdj => {
                // SAFETY: the buffers and the workspace are sized by
                // `prepare` for these dimensions.
                check_solver("cusolverDnZgesvdjBatched", unsafe {
                    (functions.zgesvdj_batched)(
                        self.solver.handle,
                        EIG_MODE_VECTOR,
                        m,
                        n,
                        self.input.pointer,
                        m,
                        self.s.pointer.cast(),
                        self.u.pointer,
                        m,
                        self.v.pointer,
                        n,
                        self.work.pointer,
                        self.work_elements,
                        self.info.pointer.cast(),
                        self.jacobi,
                        batch,
                    )
                })?;
            }
            SvdMethod::Gesvda => {
                let (stride_a, stride_s, stride_u, stride_v) = self.strides();
                // SAFETY: as above; `residuals` holds one entry per matrix.
                check_solver("cusolverDnZgesvdaStridedBatched", unsafe {
                    (functions.zgesvda_strided_batched)(
                        self.solver.handle,
                        EIG_MODE_VECTOR,
                        rank,
                        m,
                        n,
                        self.input.pointer,
                        m,
                        stride_a,
                        self.s.pointer.cast(),
                        stride_s,
                        self.u.pointer,
                        m,
                        stride_u,
                        self.v.pointer,
                        n,
                        stride_v,
                        self.work.pointer,
                        self.work_elements,
                        self.info.pointer.cast(),
                        self.residuals.as_mut_ptr(),
                        batch,
                    )
                })?;
            }
        }
        self.device.synchronize()
    }

    /// Copies the pristine input back over the one GESVDJ overwrites, and
    /// waits: the part of [`BatchedSvd::run`] that is not the SVD.
    ///
    /// # Errors
    ///
    /// When the copy fails.
    pub fn restore_input(&self) -> Result<(), String> {
        self.device.copy(
            self.input.pointer,
            self.original.pointer,
            self.original.bytes,
            cudart_12::cudaMemcpyKind_cudaMemcpyDeviceToDevice,
        )?;
        self.device.synchronize()
    }

    /// The last run's kept singular values, per matrix, largest first, and
    /// cuSOLVER's `info` per matrix (0 when it converged).
    ///
    /// # Errors
    ///
    /// When a copy fails.
    pub fn outcome(&self) -> Result<(Vec<Vec<f64>>, Vec<i32>), String> {
        let values: Vec<f64> = self
            .device
            .download(&self.s, self.kept * self.shape.batch)?;
        let info = self.device.download(&self.info, self.shape.batch)?;
        Ok((
            values.chunks(self.kept).map(<[f64]>::to_vec).collect(),
            info,
        ))
    }

    fn prepare(&mut self) -> Result<(), String> {
        let functions = &self.solver.functions;
        let (m, n, rank, batch) = self.dimensions();
        let mut work_elements = 0;
        match self.method {
            SvdMethod::Gesvdj => {
                // SAFETY: `jacobi` is a valid out-pointer.
                check_solver("cusolverDnCreateGesvdjInfo", unsafe {
                    (functions.create_gesvdj_info)(&raw mut self.jacobi)
                })?;
                // SAFETY: the buffers are sized for these dimensions.
                check_solver("cusolverDnZgesvdjBatched_bufferSize", unsafe {
                    (functions.zgesvdj_batched_buffer_size)(
                        self.solver.handle,
                        EIG_MODE_VECTOR,
                        m,
                        n,
                        self.input.pointer,
                        m,
                        self.s.pointer.cast(),
                        self.u.pointer,
                        m,
                        self.v.pointer,
                        n,
                        &raw mut work_elements,
                        self.jacobi,
                        batch,
                    )
                })?;
            }
            SvdMethod::Gesvda => {
                let (stride_a, stride_s, stride_u, stride_v) = self.strides();
                // SAFETY: the buffers are sized for these dimensions.
                check_solver("cusolverDnZgesvdaStridedBatched_bufferSize", unsafe {
                    (functions.zgesvda_strided_batched_buffer_size)(
                        self.solver.handle,
                        EIG_MODE_VECTOR,
                        rank,
                        m,
                        n,
                        self.input.pointer,
                        m,
                        stride_a,
                        self.s.pointer.cast(),
                        stride_s,
                        self.u.pointer,
                        m,
                        stride_u,
                        self.v.pointer,
                        n,
                        stride_v,
                        &raw mut work_elements,
                        batch,
                    )
                })?;
            }
        }
        self.work_elements = work_elements;
        let bytes = usize::try_from(work_elements).unwrap_or(0).max(1) * 16;
        self.work = DeviceBuffer::new(&self.device.libraries, bytes)?;
        Ok(())
    }

    fn dimensions(&self) -> (c_int, c_int, c_int, c_int) {
        let int = |value: usize| c_int::try_from(value).expect("a dimension fits in an int");
        (
            int(self.shape.rows),
            int(self.shape.columns),
            int(self.shape.rank),
            int(self.shape.batch),
        )
    }

    fn strides(&self) -> (i64, i64, i64, i64) {
        let long = |value: usize| i64::try_from(value).expect("a stride fits in i64");
        let BatchShape {
            rows,
            columns,
            rank,
            ..
        } = self.shape;
        (
            long(rows * columns),
            long(rank),
            long(rows * rank),
            long(columns * rank),
        )
    }
}

impl Drop for BatchedSvd<'_> {
    fn drop(&mut self) {
        if !self.jacobi.is_null() {
            // SAFETY: created in `prepare`, destroyed once.
            let _ = unsafe { (self.solver.functions.destroy_gesvdj_info)(self.jacobi) };
        }
    }
}

/// `CUSOLVER_EIG_MODE_VECTOR`: compute the singular vectors too.
const EIG_MODE_VECTOR: c_int = 1;

/// `cusolverStatus_t`, 0 on success.
type SolverStatus = c_int;

fn check_solver(operation: &str, status: SolverStatus) -> Result<(), String> {
    if status == 0 {
        Ok(())
    } else {
        Err(format!("{operation} returned cuSOLVER status {status}"))
    }
}

type Handle = *mut c_void;
type CreateFn = unsafe extern "C" fn(*mut Handle) -> SolverStatus;
type DestroyFn = unsafe extern "C" fn(Handle) -> SolverStatus;
type SetStreamFn = unsafe extern "C" fn(Handle, cudart_12::cudaStream_t) -> SolverStatus;
type CreateGesvdjInfoFn = unsafe extern "C" fn(*mut *mut c_void) -> SolverStatus;
type DestroyGesvdjInfoFn = unsafe extern "C" fn(*mut c_void) -> SolverStatus;
/// `(handle, jobz, m, n, A, lda, S, U, ldu, V, ldv, lwork, params, batch)`.
type ZgesvdjBatchedBufferSizeFn = unsafe extern "C" fn(
    Handle,
    c_int,
    c_int,
    c_int,
    *const c_void,
    c_int,
    *const f64,
    *const c_void,
    c_int,
    *const c_void,
    c_int,
    *mut c_int,
    *mut c_void,
    c_int,
) -> SolverStatus;
/// `(handle, jobz, m, n, A, lda, S, U, ldu, V, ldv, work, lwork, info,
/// params, batch)`.
type ZgesvdjBatchedFn = unsafe extern "C" fn(
    Handle,
    c_int,
    c_int,
    c_int,
    *mut c_void,
    c_int,
    *mut f64,
    *mut c_void,
    c_int,
    *mut c_void,
    c_int,
    *mut c_void,
    c_int,
    *mut c_int,
    *mut c_void,
    c_int,
) -> SolverStatus;
/// `(handle, jobz, rank, m, n, A, lda, strideA, S, strideS, U, ldu,
/// strideU, V, ldv, strideV, lwork, batch)`.
type ZgesvdaStridedBatchedBufferSizeFn = unsafe extern "C" fn(
    Handle,
    c_int,
    c_int,
    c_int,
    c_int,
    *const c_void,
    c_int,
    i64,
    *const f64,
    i64,
    *const c_void,
    c_int,
    i64,
    *const c_void,
    c_int,
    i64,
    *mut c_int,
    c_int,
) -> SolverStatus;
/// `(handle, jobz, rank, m, n, A, lda, strideA, S, strideS, U, ldu,
/// strideU, V, ldv, strideV, work, lwork, info, h_R_nrmF, batch)`.
type ZgesvdaStridedBatchedFn = unsafe extern "C" fn(
    Handle,
    c_int,
    c_int,
    c_int,
    c_int,
    *const c_void,
    c_int,
    i64,
    *mut f64,
    i64,
    *mut c_void,
    c_int,
    i64,
    *mut c_void,
    c_int,
    i64,
    *mut c_void,
    c_int,
    *mut c_int,
    *mut f64,
    c_int,
) -> SolverStatus;

/// cuSOLVER's functions, declared by hand (above).
struct CusolverFunctions {
    create: CreateFn,
    destroy: DestroyFn,
    set_stream: SetStreamFn,
    create_gesvdj_info: CreateGesvdjInfoFn,
    destroy_gesvdj_info: DestroyGesvdjInfoFn,
    zgesvdj_batched_buffer_size: ZgesvdjBatchedBufferSizeFn,
    zgesvdj_batched: ZgesvdjBatchedFn,
    zgesvda_strided_batched_buffer_size: ZgesvdaStridedBatchedBufferSizeFn,
    zgesvda_strided_batched: ZgesvdaStridedBatchedFn,
}

impl CusolverFunctions {
    fn resolve(library: &OpenLibraryFile) -> Result<Self, String> {
        fn get<T: Copy>(library: &OpenLibraryFile, symbol: &'static str) -> Result<T, String> {
            // SAFETY: each `T` is the hand-declared type of `symbol`; the
            // library outlives the table (`Cusolver` holds both).
            unsafe { library.resolve::<T>(symbol) }
                .ok_or_else(|| format!("cuSOLVER lacks {symbol}"))
        }
        Ok(Self {
            create: get(library, "cusolverDnCreate")?,
            destroy: get(library, "cusolverDnDestroy")?,
            set_stream: get(library, "cusolverDnSetStream")?,
            create_gesvdj_info: get(library, "cusolverDnCreateGesvdjInfo")?,
            destroy_gesvdj_info: get(library, "cusolverDnDestroyGesvdjInfo")?,
            zgesvdj_batched_buffer_size: get(library, "cusolverDnZgesvdjBatched_bufferSize")?,
            zgesvdj_batched: get(library, "cusolverDnZgesvdjBatched")?,
            zgesvda_strided_batched_buffer_size: get(
                library,
                "cusolverDnZgesvdaStridedBatched_bufferSize",
            )?,
            zgesvda_strided_batched: get(library, "cusolverDnZgesvdaStridedBatched")?,
        })
    }
}
