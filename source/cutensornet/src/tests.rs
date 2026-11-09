// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The crate's unit tests, one file per module with run-time behaviour.
//!
//! None needs a GPU or NVIDIA's libraries: the loader is driven through
//! files that are not libraries, paths that do not exist and a resolver that
//! hands out stand-in pointers. The loader compiles for its tests on any
//! Unix host, so they run off Linux x86-64 too.

mod discovery;
#[cfg(unix)]
mod library;
mod version;

use std::path::PathBuf;

/// A path under the system's temporary directory that this test process
/// alone uses.
fn unique_temp_path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "qdk-cutensornet-test-{label}-{}",
        std::process::id()
    ))
}
