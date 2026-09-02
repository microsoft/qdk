use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AvailabilityError {
    #[error(
        "cuTensorNet acceleration is unsupported on {platform}; supported target: Linux x86-64"
    )]
    UnsupportedPlatform { platform: String },

    #[error("invalid {variable} override {path:?}: {reason}")]
    InvalidOverride {
        variable: &'static str,
        path: PathBuf,
        reason: &'static str,
    },

    #[error("{library} was not found; attempted {attempted:?}")]
    LibraryNotFound {
        library: &'static str,
        attempted: Vec<PathBuf>,
    },

    #[error("failed to load {library} from {path:?}: {message}")]
    LoadFailed {
        library: &'static str,
        path: PathBuf,
        message: String,
    },

    #[error("{library} at {path:?} is missing required symbol {symbol}")]
    MissingRequiredSymbol {
        library: &'static str,
        path: PathBuf,
        symbol: &'static str,
    },

    #[error("{component} version {found} is unsupported; supported: {supported}")]
    UnsupportedVersion {
        component: &'static str,
        found: u64,
        supported: &'static str,
    },

    #[error("{component} version probe failed with status {status}: {message}")]
    VersionProbeFailed {
        component: &'static str,
        status: u32,
        message: String,
    },
}

#[cfg(any(test, all(target_os = "linux", target_arch = "x86_64")))]
#[allow(
    dead_code,
    reason = "Phase 2 validates status mapping before Phase 3 resource calls consume it"
)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NativeStatus {
    Success,
    NotInitialized,
    AllocationFailed,
    InvalidValue,
    ArchitectureMismatch,
    MappingError,
    ExecutionFailed,
    InternalError,
    NotSupported,
    LicenseError,
    CublasError,
    CudaError,
    InsufficientWorkspace,
    InsufficientDriver,
    IoError,
    CutensorVersionMismatch,
    NoDeviceAllocator,
    AllHyperSamplesFailed,
    CusolverError,
    DeviceAllocatorError,
    DistributedFailure,
    Interrupted,
    Unknown(u32),
}

#[cfg(any(test, all(target_os = "linux", target_arch = "x86_64")))]
impl From<u32> for NativeStatus {
    fn from(value: u32) -> Self {
        match value {
            0 => Self::Success,
            1 => Self::NotInitialized,
            3 => Self::AllocationFailed,
            7 => Self::InvalidValue,
            8 => Self::ArchitectureMismatch,
            11 => Self::MappingError,
            13 => Self::ExecutionFailed,
            14 => Self::InternalError,
            15 => Self::NotSupported,
            16 => Self::LicenseError,
            17 => Self::CublasError,
            18 => Self::CudaError,
            19 => Self::InsufficientWorkspace,
            20 => Self::InsufficientDriver,
            21 => Self::IoError,
            22 => Self::CutensorVersionMismatch,
            23 => Self::NoDeviceAllocator,
            24 => Self::AllHyperSamplesFailed,
            25 => Self::CusolverError,
            26 => Self::DeviceAllocatorError,
            27 => Self::DistributedFailure,
            28 => Self::Interrupted,
            value => Self::Unknown(value),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AvailabilityError, NativeStatus};
    use std::path::PathBuf;

    #[test]
    fn native_status_preserves_unknown_values() {
        assert_eq!(NativeStatus::from(7), NativeStatus::InvalidValue);
        assert_eq!(NativeStatus::from(2), NativeStatus::Unknown(2));
        assert_eq!(
            NativeStatus::from(u32::MAX),
            NativeStatus::Unknown(u32::MAX)
        );
    }

    #[test]
    fn load_failure_message_preserves_actionable_context() {
        let error = AvailabilityError::LoadFailed {
            library: "cuTensorNet",
            path: PathBuf::from("/opt/nvidia/libcutensornet.so.2"),
            message: "libcusolver.so.11: cannot open shared object file".to_string(),
        };

        assert_eq!(
            error.to_string(),
            "failed to load cuTensorNet from \"/opt/nvidia/libcutensornet.so.2\": \
             libcusolver.so.11: cannot open shared object file"
        );
    }
}
