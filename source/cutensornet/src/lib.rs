//! Experimental runtime discovery for optional cuTensorNet acceleration.

#[allow(
    clippy::mod_module_files,
    reason = "the approved versioned FFI layout groups generated bindings under bindings/mod.rs"
)]
mod bindings;
mod error;
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod library;
mod simulation;
#[cfg(any(test, all(target_os = "linux", target_arch = "x86_64")))]
mod version;

pub use error::AvailabilityError;

use std::{fmt, path::PathBuf};

#[cfg(test)]
const CUTENSORNET_REQUIRED_SYMBOLS: &[&str] = &[
    "cutensornetGetVersion",
    "cutensornetGetCudartVersion",
    "cutensornetGetErrorString",
    "cutensornetCreate",
    "cutensornetDestroy",
    "cutensornetCreateState",
    "cutensornetDestroyState",
    "cutensornetStateApplyTensorOperator",
    "cutensornetStateFinalizeMPS",
    "cutensornetStateCaptureMPS",
    "cutensornetStateConfigure",
    "cutensornetCreateWorkspaceDescriptor",
    "cutensornetDestroyWorkspaceDescriptor",
    "cutensornetStatePrepare",
    "cutensornetWorkspaceGetMemorySize",
    "cutensornetWorkspaceSetMemory",
    "cutensornetStateCompute",
    "cutensornetCreateNetworkOperator",
    "cutensornetNetworkOperatorAppendProduct",
    "cutensornetDestroyNetworkOperator",
    "cutensornetCreateExpectation",
    "cutensornetExpectationConfigure",
    "cutensornetExpectationPrepare",
    "cutensornetExpectationCompute",
    "cutensornetDestroyExpectation",
    "cutensornetCreateSampler",
    "cutensornetSamplerConfigure",
    "cutensornetSamplerPrepare",
    "cutensornetSamplerSample",
    "cutensornetDestroySampler",
];
#[cfg(test)]
const CUDART_REQUIRED_SYMBOLS: &[&str] = &[
    "cudaRuntimeGetVersion",
    "cudaDriverGetVersion",
    "cudaGetDeviceCount",
    "cudaSetDevice",
    "cudaGetErrorString",
    "cudaMemGetInfo",
    "cudaMalloc",
    "cudaFree",
    "cudaMemcpy",
    "cudaStreamCreateWithFlags",
    "cudaStreamSynchronize",
    "cudaStreamDestroy",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AvailabilityReport {
    pub cutensornet_library: PathBuf,
    pub cuda_runtime_library: PathBuf,
    pub cutensornet_version: usize,
    pub cutensornet_cuda_runtime_version: usize,
    pub cuda_runtime_version: i32,
    pub cuda_driver_version: i32,
}

pub struct Availability {
    report: AvailabilityReport,
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    _libraries: std::sync::Arc<library::NativeApi>,
}

impl Availability {
    #[must_use]
    pub fn report(&self) -> &AvailabilityReport {
        &self.report
    }
}

impl fmt::Debug for Availability {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Availability")
            .field("report", &self.report)
            .finish_non_exhaustive()
    }
}

/// Discovers and validates the optional CUDA Runtime and cuTensorNet libraries.
///
/// This function performs no device selection, allocation, handle creation, or
/// GPU work.
pub fn discover() -> Result<Availability, AvailabilityError> {
    discover_with_overrides(
        std::env::var_os("QDK_CUTENSORNET_LIBRARY"),
        std::env::var_os("QDK_CUDART_LIBRARY"),
    )
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn discover_with_overrides(
    cutensornet_override: Option<std::ffi::OsString>,
    cudart_override: Option<std::ffi::OsString>,
) -> Result<Availability, AvailabilityError> {
    library::discover(cutensornet_override, cudart_override)
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
fn discover_with_overrides(
    _cutensornet_override: Option<std::ffi::OsString>,
    _cudart_override: Option<std::ffi::OsString>,
) -> Result<Availability, AvailabilityError> {
    Err(AvailabilityError::UnsupportedPlatform {
        platform: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
    })
}

#[cfg(any(test, all(target_os = "linux", target_arch = "x86_64")))]
fn validate_override_path(
    variable: &'static str,
    value: Option<std::ffi::OsString>,
) -> Result<Option<PathBuf>, AvailabilityError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(AvailabilityError::InvalidOverride {
            variable,
            path,
            reason: "path must be absolute",
        });
    }
    let metadata =
        std::fs::symlink_metadata(&path).map_err(|_| AvailabilityError::InvalidOverride {
            variable,
            path: path.clone(),
            reason: "path does not exist",
        })?;
    if !metadata.file_type().is_file() && !metadata.file_type().is_symlink() {
        return Err(AvailabilityError::InvalidOverride {
            variable,
            path,
            reason: "path must name a regular file or symlink",
        });
    }
    Ok(Some(path))
}

#[cfg(test)]
mod tests {
    use super::{
        Availability, AvailabilityError, CUDART_REQUIRED_SYMBOLS, CUTENSORNET_REQUIRED_SYMBOLS,
        validate_override_path,
    };
    use std::{ffi::OsString, path::Path};

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn availability_is_send_and_sync() {
        assert_send_sync::<Availability>();
    }

    #[test]
    fn rejects_relative_override() {
        assert!(matches!(
            validate_override_path("TEST_LIBRARY", Some(OsString::from("library.so"))),
            Err(AvailabilityError::InvalidOverride {
                variable: "TEST_LIBRARY",
                reason: "path must be absolute",
                ..
            })
        ));
    }

    #[test]
    fn rejects_missing_absolute_override_without_fallback() {
        let path = format!(
            "/qdk-cutensornet-test-missing-{}-{}",
            std::process::id(),
            line!()
        );
        assert!(matches!(
            validate_override_path("TEST_LIBRARY", Some(OsString::from(&path))),
            Err(AvailabilityError::InvalidOverride {
                variable: "TEST_LIBRARY",
                path: found,
                reason: "path does not exist",
            }) if found.as_path() == Path::new(&path)
        ));
    }

    #[test]
    fn rejects_directory_override_before_loading() {
        let path = std::env::temp_dir();
        assert!(matches!(
            validate_override_path("TEST_LIBRARY", Some(path.clone().into_os_string())),
            Err(AvailabilityError::InvalidOverride {
                variable: "TEST_LIBRARY",
                path: found,
                reason: "path must name a regular file or symlink",
            }) if found == path
        ));
    }

    #[test]
    fn symbol_inventories_match_the_frozen_surface() {
        assert_eq!(CUTENSORNET_REQUIRED_SYMBOLS.len(), 30);
        assert_eq!(CUDART_REQUIRED_SYMBOLS.len(), 12);
        assert!(!CUTENSORNET_REQUIRED_SYMBOLS.contains(&"cutensornetGetLastError"));
    }

    #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
    #[test]
    fn unsupported_target_does_not_attempt_discovery() {
        assert!(matches!(
            super::discover_with_overrides(None, None),
            Err(AvailabilityError::UnsupportedPlatform { .. })
        ));
    }
}
