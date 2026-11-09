// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Opening the two libraries, looking up their functions and checking their
//! versions.
//!
//! This is where discovery's work is done ([`discover`]): it opens the CUDA
//! Runtime, then cuTensorNet, looks up every function of each library's
//! table by name (the tables are in `symbols`), probes the versions and
//! checks them. Looking a function up does not call it. Each step returns
//! the first fault as an [`AvailabilityError`], and a failed discovery
//! closes the libraries it opened.
//!
//! [`LoadedLibraries`] holds both tables and both libraries, so that no
//! function pointer outlives its library. The MPS engine, added in a later
//! change, calls cuTensorNet and the CUDA Runtime through it.

use crate::{
    Availability, AvailabilityError, AvailabilityReport, bindings::cudart_12,
    validate_override_path, version::check_versions,
};
use libloading::{Library, os::unix as unix_library};
use std::{
    ffi::{CStr, OsString},
    path::{Path, PathBuf},
    sync::Arc,
};

pub(super) mod symbols;

use symbols::{CuTensorNetFunctions, CudaFunctions};

/// One of the two libraries discovery loads: the name its errors give it,
/// the variable that overrides its path, and the paths tried otherwise.
///
/// Each library has one, so that no step can read one library's variable,
/// or try its paths, for the other.
pub(super) struct NativeLibrary {
    pub(super) name: &'static str,
    variable: &'static str,
    defaults: &'static [&'static str],
}

pub(super) static CUTENSORNET: NativeLibrary = NativeLibrary {
    name: "cuTensorNet",
    variable: "QDK_CUTENSORNET_LIBRARY",
    defaults: &[
        "/usr/lib/x86_64-linux-gnu/libcuquantum/12/libcutensornet.so.2",
        "libcutensornet.so.2",
    ],
};

pub(super) static CUDA_RUNTIME: NativeLibrary = NativeLibrary {
    name: "CUDA Runtime",
    variable: "QDK_CUDART_LIBRARY",
    defaults: &[
        "/usr/local/cuda-12.9/targets/x86_64-linux/lib/libcudart.so.12",
        "libcudart.so.12",
    ],
};

/// The paths to try for one library, in order.
pub(super) struct Candidates {
    pub(super) library: &'static NativeLibrary,
    pub(super) paths: Vec<PathBuf>,
}

impl NativeLibrary {
    /// The override that `read_variable` gives for this library's variable,
    /// alone, or else the default paths.
    pub(super) fn candidates(
        &'static self,
        read_variable: impl Fn(&'static str) -> Option<OsString>,
    ) -> Result<Candidates, AvailabilityError> {
        let paths = match validate_override_path(self.variable, read_variable(self.variable))? {
            Some(path) => vec![path],
            None => self.defaults.iter().map(PathBuf::from).collect(),
        };
        Ok(Candidates {
            library: self,
            paths,
        })
    }
}

/// One opened library and the path it was opened from. Dropping it closes
/// the library.
pub(super) struct LoadedLibrary {
    path: PathBuf,
    library: Library,
}

/// The two loaded libraries and their function tables.
///
/// The libraries are closed only when this is dropped, cuTensorNet first,
/// as it runs on the CUDA Runtime; so no function pointer of the tables
/// outlives its library. It holds no device, stream, handle or state:
/// whoever calls a function that creates one owns it.
#[expect(
    dead_code,
    reason = "the libraries are held to keep the tables valid; the MPS engine, added later, reads the tables"
)]
pub(crate) struct LoadedLibraries {
    cutensornet: LoadedLibrary,
    cudart: LoadedLibrary,
    cuda_functions: CudaFunctions,
    cutensornet_functions: CuTensorNetFunctions,
}

pub(super) trait SymbolResolver {
    /// The function exported under `symbol`, or `None` when the library
    /// does not export it or exports it at address zero.
    ///
    /// # Safety
    /// `T` must be the function-pointer type of the function exported under
    /// `symbol`, and the library must outlive every use of the result.
    unsafe fn resolve<T: Copy>(&self, symbol: &'static str) -> Option<T>;
}

impl SymbolResolver for LoadedLibrary {
    unsafe fn resolve<T: Copy>(&self, symbol: &'static str) -> Option<T> {
        // SAFETY: `T` is a function-pointer type (the caller's contract), so
        // `Option<T>` reads a zero address as `None`; `self.library`
        // outlives every copied function pointer.
        let resolved = unsafe { self.library.get::<Option<T>>(symbol.as_bytes()) }.ok()?;
        *resolved
    }
}

pub(crate) fn discover(
    read_variable: impl Fn(&'static str) -> Option<OsString>,
) -> Result<Availability, AvailabilityError> {
    let cudart_candidates = CUDA_RUNTIME.candidates(&read_variable)?;
    let cutensornet_candidates = CUTENSORNET.candidates(&read_variable)?;

    let cudart = load_library(cudart_candidates)?;
    let cuda_functions = CudaFunctions::resolve(&cudart, &cudart.path)?;
    let cutensornet = load_library(cutensornet_candidates)?;
    let cutensornet_functions = CuTensorNetFunctions::resolve(&cutensornet, &cutensornet.path)?;

    let cuda_runtime_version = probe_cuda_version(
        CUDA_RUNTIME.name,
        cuda_functions.runtime_get_version,
        cuda_functions.get_error_string,
    )?;
    let cuda_driver_version = probe_cuda_version(
        "CUDA driver API",
        cuda_functions.driver_get_version,
        cuda_functions.get_error_string,
    )?;
    // SAFETY: the function pointer was resolved with its exact audited
    // signature and the owning library is alive.
    let cutensornet_version = unsafe { (cutensornet_functions.get_version)() };
    // SAFETY: same invariant as the preceding version call.
    let cutensornet_cuda_runtime_version = unsafe { (cutensornet_functions.get_cudart_version)() };

    let report = AvailabilityReport {
        cutensornet_library: cutensornet.path.clone(),
        cuda_runtime_library: cudart.path.clone(),
        cutensornet_version,
        cutensornet_cuda_runtime_version,
        cuda_runtime_version,
        cuda_driver_version,
    };
    check_versions(&report)?;
    Ok(Availability {
        report,
        libraries: Arc::new(LoadedLibraries {
            cutensornet,
            cudart,
            cuda_functions,
            cutensornet_functions,
        }),
    })
}

/// Opens the first candidate that exists.
///
/// An absolute path that does not exist is skipped. A bare file name is
/// searched for by the system loader, and skipped when the loader finds no
/// file of that name. Any other failure is the library's.
pub(super) fn load_library(candidates: Candidates) -> Result<LoadedLibrary, AvailabilityError> {
    let Candidates { library, paths } = candidates;
    for path in &paths {
        if path.is_absolute() && !path.exists() {
            continue;
        }

        // SAFETY: opening a library runs its initialisers (and, when it is
        // closed, its finalisers) as foreign code. The candidate is NVIDIA's
        // CUDA Runtime or cuTensorNet, by a fixed name or by an absolute path
        // the user set, and its initialisers are trusted as any CUDA
        // library's are. The `Library` is never closed while a resolved
        // pointer is live: it is kept in the `Arc<LoadedLibraries>` that the
        // pointers live in.
        let result = unsafe {
            unix_library::Library::open(
                Some(path.as_os_str()),
                unix_library::RTLD_NOW | unix_library::RTLD_LOCAL,
            )
        };
        match result {
            Ok(opened) => {
                return Ok(LoadedLibrary {
                    path: path.clone(),
                    library: opened.into(),
                });
            }
            Err(error) => {
                let message = error.to_string();
                if path.is_absolute() || !no_file_of_that_name(path, &message) {
                    return Err(AvailabilityError::LoadFailed {
                        library: library.name,
                        path: path.clone(),
                        message,
                    });
                }
            }
        }
    }
    Err(AvailabilityError::LibraryNotFound {
        library: library.name,
        attempted: paths,
    })
}

/// Whether the system loader's `message`, on failing to open the bare file
/// name `candidate`, says that it found no file of that name.
///
/// The loader starts its message with the name of the file it could not
/// open: the candidate's when there is no such file, a dependency's when the
/// candidate was found but needs a library that is missing. Only the first
/// lets discovery try the next candidate.
pub(super) fn no_file_of_that_name(candidate: &Path, message: &str) -> bool {
    candidate
        .to_str()
        .and_then(|name| message.strip_prefix(name))
        .and_then(|rest| rest.strip_prefix(": "))
        .is_some_and(|reason| reason.ends_with("No such file or directory"))
}

pub(super) fn probe_cuda_version(
    component: &'static str,
    probe: unsafe extern "C" fn(*mut i32) -> cudart_12::CudaError,
    get_error_string: cudart_12::CudaGetErrorStringFn,
) -> Result<i32, AvailabilityError> {
    let mut version = 0;
    // SAFETY: `version` is a valid writable `int` and both pointers were
    // resolved with their exact audited signatures from a retained library.
    let status = unsafe { probe(&raw mut version) };
    if status == 0 {
        return Ok(version);
    }
    // SAFETY: CUDA owns the returned null-terminated string; it is copied
    // immediately and never retained as a borrowed pointer.
    let message_ptr = unsafe { get_error_string(status) };
    let message = if message_ptr.is_null() {
        "CUDA returned a null error string".to_string()
    } else {
        // SAFETY: non-null CUDA error strings are documented as
        // null-terminated and library-owned.
        unsafe { CStr::from_ptr(message_ptr) }
            .to_string_lossy()
            .into_owned()
    };
    Err(AvailabilityError::VersionProbeFailed {
        component,
        status,
        message,
    })
}
