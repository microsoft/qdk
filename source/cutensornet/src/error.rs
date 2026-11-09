// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Why discovery found no usable cuTensorNet.
//!
//! A caller that asked for a GPU run reports the error to the user or falls
//! back to the CPU; each variant carries what the user needs to fix their
//! installation: the variable, path, library, function or version at fault.

use std::path::PathBuf;
use thiserror::Error;

/// Why [`discover`](crate::discover) found no usable cuTensorNet.
#[derive(Debug, Error)]
pub enum AvailabilityError {
    /// The target is not Linux x86-64.
    #[error(
        "cuTensorNet acceleration is unsupported on {platform}; supported target: Linux x86-64"
    )]
    UnsupportedPlatform { platform: String },

    /// An override variable is not an absolute path to a file (a symlink
    /// to a file is followed).
    #[error("invalid {variable} override {path:?}: {reason}")]
    InvalidOverride {
        variable: &'static str,
        path: PathBuf,
        reason: &'static str,
    },

    /// No file of a library was found at any of its candidate paths.
    #[error("{library} was not found; attempted {attempted:?}")]
    LibraryNotFound {
        library: &'static str,
        attempted: Vec<PathBuf>,
    },

    /// A library was found but the system loader refused it: for instance,
    /// it is not a library for this host, or a library it needs is missing.
    #[error("failed to load {library} from {path:?}: {message}")]
    LoadFailed {
        library: &'static str,
        path: PathBuf,
        message: String,
    },

    /// A loaded library lacks a required symbol.
    #[error("{library} at {path:?} is missing required symbol {symbol}")]
    MissingRequiredSymbol {
        library: &'static str,
        path: PathBuf,
        symbol: &'static str,
    },

    /// A version is not the audited one. `component` is `cuTensorNet`,
    /// `CUDA Runtime`, or `cuTensorNet's CUDA Runtime` (the CUDA Runtime
    /// version cuTensorNet was built against).
    #[error("{component} version {found} is unsupported; supported: {supported}")]
    UnsupportedVersion {
        component: &'static str,
        found: i128,
        supported: i128,
    },

    /// A CUDA version call returned an error.
    #[error("{component} version probe failed with status {status}: {message}")]
    VersionProbeFailed {
        component: &'static str,
        status: u32,
        message: String,
    },
}
