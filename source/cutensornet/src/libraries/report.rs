// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What opening and checking one library found: where it was loaded from,
//! the versions it reports, every fault found in it, the other copies
//! visible beside it, and whether it is the copy its override names.
//!
//! Each library is opened and checked in `libraries`, which fills its
//! report; discovery collects the reports, one per library, in an
//! [`AvailabilityReport`](crate::AvailabilityReport).
//!
//! Each library at fault lists every fault its checks could find, so that
//! the user fixes their installation in one pass: the variable, path,
//! function or version at fault. Only a fault that leaves nothing more to
//! check about its library (an override that is not a file, a library not
//! found, a path the process cannot access, or a library refused by the
//! system loader) stops the checks of that library.

use std::path::PathBuf;
use thiserror::Error;

/// What [`discover`](crate::discover) found of one library.
///
/// A library is usable when it has no fault. A library with a fault is
/// reported with what was found of it all the same: its path once it was
/// opened, and the versions read.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LibraryReport {
    /// The library's name, for instance `cuTensorNet`.
    pub library: &'static str,
    /// The path it was loaded from; `None` when it was not loaded.
    pub path: Option<PathBuf>,
    /// The versions it reports, in the order they are read.
    pub versions: Vec<Version>,
    /// Every fault found, in the order found; empty when it is usable.
    pub faults: Vec<LibraryFault>,
    /// Other copies of the library the setup makes visible, not loaded: a
    /// packaged copy beside an override or a system install, say. Two
    /// copies may be of different versions, and a dependency of the one
    /// loaded may be found beside the other, so the setup check warns of
    /// them. Empty when it was not loaded.
    pub other_copies: Vec<PathBuf>,
    /// Whether the copy tried is the one its override variable names,
    /// loaded or not, so that the setup check, when that copy is at fault,
    /// advises changing or unsetting the variable: an override is the only
    /// path tried, so no install displaces it. `false` when the override is
    /// unset or invalid.
    pub from_override: bool,
}

impl LibraryReport {
    /// The version of `component`, if it was read.
    #[must_use]
    pub fn version(&self, component: &str) -> Option<i128> {
        self.versions
            .iter()
            .find(|version| version.component == component)
            .map(|version| version.value)
    }
}

/// One version a library reports, as the library encodes it.
///
/// Each library's module says which versions it reads: today the CUDA
/// Runtime reports `CUDA Runtime` and `CUDA driver`, and cuTensorNet reports
/// `cuTensorNet` and `cuTensorNet's CUDA Runtime`, the CUDA Runtime it was
/// built against (README, "Versions").
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Version {
    /// What the version is of.
    pub component: &'static str,
    /// The version, as reported: `12090` is CUDA 12.9 and `21300` is
    /// cuTensorNet 2.13.0.
    pub value: i128,
}

/// One thing wrong with one library, in its [`LibraryReport`].
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum LibraryFault {
    /// The library's override variable is not an absolute path to a file
    /// (a symlink to a file is followed), or the path cannot be read.
    #[error("invalid {variable} override {path:?}: {reason}")]
    InvalidOverride {
        variable: &'static str,
        path: PathBuf,
        reason: &'static str,
    },

    /// No candidate path named a file of the library: an absolute path
    /// does not exist, and for a bare file name the system loader reported
    /// no file of that name (glibc reports so too for a file its search
    /// skipped because it could not use it).
    #[error("not found; attempted {attempted:?}")]
    NotFound { attempted: Vec<PathBuf> },

    /// A candidate path the process cannot access to see whether a file is
    /// there: for instance, a directory on it that the process may not
    /// search. A file may be there, so the later candidates are not tried.
    /// `message` is the system's error.
    #[error("cannot access {path:?}: {message}")]
    InaccessiblePath { path: PathBuf, message: String },

    /// A file of the library was found but the system loader refused it:
    /// for instance, it is not a library for this host, or a library it
    /// needs is missing.
    #[error("failed to load {path:?}: {message}")]
    LoadFailed { path: PathBuf, message: String },

    /// The loaded library lacks a required function.
    #[error("missing required symbol {symbol}")]
    MissingRequiredSymbol { symbol: &'static str },

    /// A version is not the audited one. `component` names the version as
    /// in [`Version`].
    #[error("{component} version {found} is unsupported; supported: {supported}")]
    UnsupportedVersion {
        component: &'static str,
        found: i128,
        supported: i128,
    },

    /// A call that reads a version failed, with the library's status and
    /// message (today only the CUDA Runtime's calls can fail).
    #[error("{component} version probe failed with status {status}: {message}")]
    VersionProbeFailed {
        component: &'static str,
        status: u32,
        message: String,
    },
}
