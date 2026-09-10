use crate::{
    Availability, AvailabilityError, AvailabilityReport, bindings::cudart_12,
    validate_override_path, version::POLICY,
};
use libloading::{Library, os::unix as unix_library};
use std::{
    ffi::{CStr, OsString},
    path::{Path, PathBuf},
    sync::Arc,
};

mod simulation;
mod symbols;

pub(crate) use simulation::Session;
use symbols::{CuTensorNetFunctions, resolve_cutensornet_functions};

const CUTENSORNET_NAME: &str = "cuTensorNet";
const CUDART_NAME: &str = "CUDA Runtime";
const CUTENSORNET_OVERRIDE: &str = "QDK_CUTENSORNET_LIBRARY";
const CUDART_OVERRIDE: &str = "QDK_CUDART_LIBRARY";

const CUTENSORNET_DEFAULTS: &[&str] = &[
    "/usr/lib/x86_64-linux-gnu/libcuquantum/12/libcutensornet.so.2",
    "libcutensornet.so.2",
];
const CUDART_DEFAULTS: &[&str] = &[
    "/usr/local/cuda-12.9/targets/x86_64-linux/lib/libcudart.so.12",
    "libcudart.so.12",
];

#[allow(dead_code)]
struct CudaFunctions {
    runtime_get_version: cudart_12::CudaRuntimeGetVersionFn,
    driver_get_version: cudart_12::CudaDriverGetVersionFn,
    get_device_count: cudart_12::CudaGetDeviceCountFn,
    set_device: cudart_12::CudaSetDeviceFn,
    get_error_string: cudart_12::CudaGetErrorStringFn,
    mem_get_info: cudart_12::CudaMemGetInfoFn,
    malloc: cudart_12::CudaMallocFn,
    free: cudart_12::CudaFreeFn,
    memcpy: cudart_12::CudaMemcpyFn,
    stream_create_with_flags: cudart_12::CudaStreamCreateWithFlagsFn,
    stream_synchronize: cudart_12::CudaStreamSynchronizeFn,
    stream_destroy: cudart_12::CudaStreamDestroyFn,
}

struct LoadedLibrary {
    path: PathBuf,
    library: Library,
}

// `LoadedLibrary` values are RAII guards: every early return during resolution
// or version probing unloads already-opened libraries before the error escapes.

#[allow(dead_code)]
pub(crate) struct NativeApi {
    cudart: LoadedLibrary,
    cutensornet: LoadedLibrary,
    cuda_functions: CudaFunctions,
    cutensornet_functions: CuTensorNetFunctions,
}

trait SymbolResolver {
    /// Resolves `symbol` as `T`.
    ///
    /// # Safety
    /// The caller must provide the exact function-pointer type exported under
    /// `symbol` and must keep the owning library alive while using the result.
    unsafe fn resolve<T: Copy>(&self, symbol: &'static [u8]) -> Result<T, String>;
}

impl SymbolResolver for LoadedLibrary {
    unsafe fn resolve<T: Copy>(&self, symbol: &'static [u8]) -> Result<T, String> {
        // SAFETY: callers supply the audited signature for each named symbol,
        // and `self.library` outlives every copied function pointer.
        let resolved =
            unsafe { self.library.get::<T>(symbol) }.map_err(|error| error.to_string())?;
        Ok(*resolved)
    }
}

struct Candidates {
    paths: Vec<PathBuf>,
    exclusive: bool,
}

pub(crate) fn discover(
    cutensornet_override: Option<OsString>,
    cudart_override: Option<OsString>,
) -> Result<Availability, AvailabilityError> {
    let cudart_candidates = candidates(CUDART_OVERRIDE, cudart_override, CUDART_DEFAULTS)?;
    let cutensornet_candidates = candidates(
        CUTENSORNET_OVERRIDE,
        cutensornet_override,
        CUTENSORNET_DEFAULTS,
    )?;

    let cudart = load_library(CUDART_NAME, cudart_candidates)?;
    let cuda_functions = resolve_cuda_functions(&cudart, &cudart.path)?;
    let cutensornet = load_library(CUTENSORNET_NAME, cutensornet_candidates)?;
    let cutensornet_functions = resolve_cutensornet_functions(&cutensornet, &cutensornet.path)?;

    let cuda_runtime_version = probe_cuda_version(
        "CUDA Runtime",
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

    POLICY.validate_cutensornet(cutensornet_version)?;
    POLICY.validate_cuda_runtime(cuda_runtime_version)?;
    let cutensornet_cuda_runtime_version_i32 = i32::try_from(cutensornet_cuda_runtime_version)
        .map_err(|_| AvailabilityError::UnsupportedVersion {
            component: "cuTensorNet CUDA Runtime ABI",
            found: cutensornet_cuda_runtime_version as u64,
            supported: "12090",
        })?;
    POLICY.validate_cuda_runtime(cutensornet_cuda_runtime_version_i32)?;

    let report = AvailabilityReport {
        cutensornet_library: cutensornet.path.clone(),
        cuda_runtime_library: cudart.path.clone(),
        cutensornet_version,
        cutensornet_cuda_runtime_version,
        cuda_runtime_version,
        cuda_driver_version,
    };
    Ok(Availability {
        report,
        libraries: Arc::new(NativeApi {
            cudart,
            cutensornet,
            cuda_functions,
            cutensornet_functions,
        }),
    })
}

fn candidates(
    variable: &'static str,
    override_value: Option<OsString>,
    defaults: &[&str],
) -> Result<Candidates, AvailabilityError> {
    if let Some(path) = validate_override_path(variable, override_value)? {
        Ok(Candidates {
            paths: vec![path],
            exclusive: true,
        })
    } else {
        Ok(Candidates {
            paths: defaults.iter().map(PathBuf::from).collect(),
            exclusive: false,
        })
    }
}

fn load_library(
    library_name: &'static str,
    candidates: Candidates,
) -> Result<LoadedLibrary, AvailabilityError> {
    let attempted = candidates.paths.clone();
    for path in candidates.paths {
        let absolute_candidate_exists = path.is_absolute() && path.exists();
        if path.is_absolute() && !absolute_candidate_exists {
            continue;
        }

        // SAFETY: loading is explicit and the resulting `Library` is retained
        // for longer than every resolved function pointer.
        let result = unsafe {
            unix_library::Library::open(
                Some(path.as_os_str()),
                unix_library::RTLD_NOW | unix_library::RTLD_LOCAL,
            )
        };
        match result {
            Ok(library) => {
                return Ok(LoadedLibrary {
                    path,
                    library: library.into(),
                });
            }
            Err(error) => {
                let message = error.to_string();
                if candidates.exclusive
                    || absolute_candidate_exists
                    || !message.contains("No such file or directory")
                {
                    return Err(AvailabilityError::LoadFailed {
                        library: library_name,
                        path,
                        message,
                    });
                }
            }
        }
    }
    Err(AvailabilityError::LibraryNotFound {
        library: library_name,
        attempted,
    })
}

fn resolve_required<R: SymbolResolver, T: Copy>(
    resolver: &R,
    library: &'static str,
    path: &Path,
    name: &'static str,
    symbol: &'static [u8],
) -> Result<T, AvailabilityError> {
    // SAFETY: each call site supplies the exact audited signature associated
    // with `name`; the resolver owner is retained by `NativeApi`.
    unsafe { resolver.resolve(symbol) }.map_err(|_| AvailabilityError::MissingRequiredSymbol {
        library,
        path: path.to_path_buf(),
        symbol: name,
    })
}

fn resolve_optional<R: SymbolResolver, T: Copy>(resolver: &R, symbol: &'static [u8]) -> Option<T> {
    // SAFETY: the call site supplies the audited optional-symbol signature;
    // resolution failure is intentionally represented as `None`.
    unsafe { resolver.resolve(symbol) }.ok()
}

fn resolve_cuda_functions<R: SymbolResolver>(
    resolver: &R,
    path: &Path,
) -> Result<CudaFunctions, AvailabilityError> {
    Ok(CudaFunctions {
        runtime_get_version: resolve_required(
            resolver,
            CUDART_NAME,
            path,
            "cudaRuntimeGetVersion",
            b"cudaRuntimeGetVersion\0",
        )?,
        driver_get_version: resolve_required(
            resolver,
            CUDART_NAME,
            path,
            "cudaDriverGetVersion",
            b"cudaDriverGetVersion\0",
        )?,
        get_device_count: resolve_required(
            resolver,
            CUDART_NAME,
            path,
            "cudaGetDeviceCount",
            b"cudaGetDeviceCount\0",
        )?,
        set_device: resolve_required(
            resolver,
            CUDART_NAME,
            path,
            "cudaSetDevice",
            b"cudaSetDevice\0",
        )?,
        get_error_string: resolve_required(
            resolver,
            CUDART_NAME,
            path,
            "cudaGetErrorString",
            b"cudaGetErrorString\0",
        )?,
        mem_get_info: resolve_required(
            resolver,
            CUDART_NAME,
            path,
            "cudaMemGetInfo",
            b"cudaMemGetInfo\0",
        )?,
        malloc: resolve_required(resolver, CUDART_NAME, path, "cudaMalloc", b"cudaMalloc\0")?,
        free: resolve_required(resolver, CUDART_NAME, path, "cudaFree", b"cudaFree\0")?,
        memcpy: resolve_required(resolver, CUDART_NAME, path, "cudaMemcpy", b"cudaMemcpy\0")?,
        stream_create_with_flags: resolve_required(
            resolver,
            CUDART_NAME,
            path,
            "cudaStreamCreateWithFlags",
            b"cudaStreamCreateWithFlags\0",
        )?,
        stream_synchronize: resolve_required(
            resolver,
            CUDART_NAME,
            path,
            "cudaStreamSynchronize",
            b"cudaStreamSynchronize\0",
        )?,
        stream_destroy: resolve_required(
            resolver,
            CUDART_NAME,
            path,
            "cudaStreamDestroy",
            b"cudaStreamDestroy\0",
        )?,
    })
}

fn probe_cuda_version(
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

#[cfg(test)]
mod tests {
    use super::{SymbolResolver, resolve_cuda_functions, resolve_cutensornet_functions};
    use crate::{AvailabilityError, CUDART_REQUIRED_SYMBOLS, cutensornet_required_symbols};
    use std::{
        ffi::c_char,
        fs,
        mem::{size_of, transmute_copy},
        path::{Path, PathBuf},
    };

    unsafe extern "C" fn fake_symbol() {}

    static CUDA_ERROR_MESSAGE: &[u8] = b"simulated CUDA version failure\0";

    unsafe extern "C" fn failing_version_probe(_version: *mut i32) -> u32 {
        42
    }

    unsafe extern "C" fn fake_cuda_error_string(_status: u32) -> *const c_char {
        CUDA_ERROR_MESSAGE.as_ptr().cast()
    }

    struct FakeResolver {
        missing: Option<String>,
    }

    struct RemoveFile(PathBuf);

    impl Drop for RemoveFile {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    fn unique_temp_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "qdk-cutensornet-{label}-{}-{}",
            std::process::id(),
            line!()
        ))
    }

    impl SymbolResolver for FakeResolver {
        unsafe fn resolve<T: Copy>(&self, symbol: &'static [u8]) -> Result<T, String> {
            let name = std::str::from_utf8(symbol)
                .map_err(|error| error.to_string())?
                .trim_end_matches('\0');
            if self.missing.as_deref() == Some(name) {
                return Err("deliberately missing".to_string());
            }
            let pointer = fake_symbol as *const ();
            assert_eq!(size_of::<T>(), size_of::<*const ()>());
            // SAFETY: tests never invoke the pointer. They only exercise
            // all-or-nothing table construction with pointer-sized function
            // types.
            Ok(unsafe { transmute_copy(&pointer) })
        }
    }

    #[test]
    fn every_missing_cutensornet_symbol_is_reported() {
        for missing in cutensornet_required_symbols() {
            let error = resolve_cutensornet_functions(
                &FakeResolver {
                    missing: Some(missing.clone()),
                },
                Path::new("/fixture/libcutensornet.so.2"),
            )
            .err()
            .expect("missing symbol should reject the table");
            assert!(matches!(
                error,
                AvailabilityError::MissingRequiredSymbol { path, symbol, .. }
                    if path == Path::new("/fixture/libcutensornet.so.2") && symbol == missing
            ));
        }
    }

    #[test]
    fn every_missing_cuda_symbol_is_reported() {
        for &missing in CUDART_REQUIRED_SYMBOLS {
            let error = resolve_cuda_functions(
                &FakeResolver {
                    missing: Some(missing.to_owned()),
                },
                Path::new("/fixture/libcudart.so.12"),
            )
            .err()
            .expect("missing symbol should reject the table");
            assert!(matches!(
                error,
                AvailabilityError::MissingRequiredSymbol { path, symbol, .. }
                    if path == Path::new("/fixture/libcudart.so.12") && symbol == missing
            ));
        }
    }

    #[test]
    fn optional_last_error_does_not_reject_table() {
        let functions = resolve_cutensornet_functions(
            &FakeResolver {
                missing: Some("cutensornetGetLastError".to_owned()),
            },
            Path::new("/fixture/libcutensornet.so.2"),
        )
        .expect("optional diagnostic should not reject table");
        assert!(functions.get_last_error.is_none());
    }

    #[test]
    fn invalid_shared_object_reports_load_failure() {
        let path = unique_temp_path("invalid-library");
        fs::write(&path, b"not an ELF shared object")
            .expect("temporary invalid library should be writable");
        let _remove_file = RemoveFile(path.clone());

        let error = super::load_library(
            "fixture library",
            super::Candidates {
                paths: vec![path.clone()],
                exclusive: true,
            },
        )
        .err()
        .expect("invalid shared object should fail to load");

        assert!(matches!(
            error,
            AvailabilityError::LoadFailed {
                library: "fixture library",
                path: found,
                message,
            } if found == path && !message.is_empty()
        ));
    }

    #[test]
    fn exhausted_candidates_report_every_attempted_path() {
        let paths = vec![
            unique_temp_path("missing-library-a"),
            unique_temp_path("missing-library-b"),
        ];

        let error = super::load_library(
            "fixture library",
            super::Candidates {
                paths: paths.clone(),
                exclusive: false,
            },
        )
        .err()
        .expect("missing candidates should not load");

        assert!(matches!(
            error,
            AvailabilityError::LibraryNotFound {
                library: "fixture library",
                attempted,
            } if attempted == paths
        ));
    }

    #[test]
    fn cuda_version_probe_failure_preserves_status_and_message() {
        let error = super::probe_cuda_version(
            "CUDA Runtime",
            failing_version_probe,
            fake_cuda_error_string,
        )
        .expect_err("failing CUDA probe should return an error");

        assert!(matches!(
            error,
            AvailabilityError::VersionProbeFailed {
                component: "CUDA Runtime",
                status: 42,
                message,
            } if message == "simulated CUDA version failure"
        ));
    }
}
