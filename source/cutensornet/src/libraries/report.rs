// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What opening and checking one library found: where it was loaded from,
//! the versions it reports, every fault found in it, the other copies
//! visible beside it, and where the copy tried was found.
//!
//! `libraries` opens and checks each library and fills its report.
//! Discovery collects the reports, one per library, in an
//! [`AvailabilityReport`](crate::AvailabilityReport).
//!
//! Each library at fault lists every fault its checks could find: the
//! variable, path, function or version at fault. So the user fixes their
//! installation in one pass. Only a fault before the library is open (an
//! invalid override, or a library not found, at a path the process cannot
//! access, or refused by the system loader) stops the checks of that
//! library.

use std::path::PathBuf;
use thiserror::Error;

/// What [`discover`](crate::discover) or
/// [`discover_with`](crate::discover_with) found of one library.
///
/// A library is usable when it has no fault. A library with a fault is
/// reported with what was found of it all the same: its path once it was
/// opened, and the versions read.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LibraryReport {
    /// The library's name, for instance `cuTensorNet`.
    pub name: &'static str,
    /// The path it was loaded from; `None` when it was not loaded.
    pub path: Option<PathBuf>,
    /// The versions it reports, in the order they are read.
    pub versions: Vec<Version>,
    /// Every fault found, in the order found; empty when it is usable.
    pub faults: Vec<LibraryFault>,
    /// Other copies of the library the setup makes visible, not loaded, for
    /// instance a packaged copy beside an override or a system install.
    /// They matter because two copies can differ in version, or in the
    /// dependencies found beside them, without the user knowing which one
    /// a process loads. Empty when it was not loaded.
    pub other_copies: Vec<PathBuf>,
    /// Where the copy tried (the one loaded, or the one the system loader
    /// refused) was found. `None` when no copy was tried: the override is
    /// invalid, or no candidate named a file the process could access.
    pub source: Option<LibrarySource>,
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

/// Where discovery found a library's copy, among the paths it tries in
/// order (docs/loading.md, "The steps of discovery").
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LibrarySource {
    /// The path its override variable names. While the variable is set, no
    /// other path is tried.
    Override,
    /// Its file among the [`PackagedLibraries`](crate::PackagedLibraries).
    Packaged,
    /// One of its default absolute paths.
    DefaultPath,
    /// Its file name, searched for by the system loader.
    SystemSearch,
}

/// One version a library reports, as the library encodes it.
///
/// Each library's module says which versions it reads (README,
/// "Versions").
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
    #[error("invalid {variable} override `{}`: {reason}", .path.display())]
    InvalidOverride {
        variable: &'static str,
        path: PathBuf,
        reason: &'static str,
    },

    /// No candidate path named a file of the library. For an absolute path,
    /// nothing exists there (or part of it is a file rather than a
    /// directory). For a bare file name, the system loader reported no file
    /// of that name (docs/loading.md, "The steps of discovery").
    #[error("not found; attempted {}", quoted_paths(.attempted))]
    NotFound { attempted: Vec<PathBuf> },

    /// A candidate path the process cannot access to see whether a file is
    /// there, for instance through a directory the process lacks the
    /// permission to search, or a symlink loop. Something is there, or
    /// perhaps is, so the later candidates are not tried. `message` is the
    /// system's error.
    #[error("cannot access `{}`: {message}", .path.display())]
    InaccessiblePath { path: PathBuf, message: String },

    /// A file of the library was found but the system loader refused it.
    /// For instance, it is not a library for this host, or a library it
    /// needs is missing.
    #[error("failed to load `{}`: {message}", .path.display())]
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
    /// message. Today only the CUDA Runtime's calls can fail.
    #[error("{component} version probe failed with status {status}: {message}")]
    VersionProbeFailed {
        component: &'static str,
        status: u32,
        message: String,
    },
}

/// The paths of a [`LibraryFault::NotFound`], each in backticks.
fn quoted_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| format!("`{}`", path.display()))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests;
