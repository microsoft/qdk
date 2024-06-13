// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! This module handles the logic that constitutes the Q# project system.
//! This includes locating a manifest file in the filesystem, loading and parsing
//! the manifest, and determining which files are members of the project.

mod error;
#[cfg(feature = "fs")]
mod fs;
mod js;
mod manifest;
mod project;

pub use error::Error;
#[cfg(feature = "fs")]
pub use fs::StdFs;
pub use js::{JSFileEntry, ProjectSystemCallbacks};
pub use manifest::{Dependency, Manifest, ManifestDescriptor, MANIFEST_FILE_NAME};
pub use project::FileSystemAsync;
pub use project::{
    DirEntry, EntryType, FileSystem, PackageCache, PackageGraphSources, PackageInfo, ProgramConfig,
    Project,
};
