// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The crate's unit tests, one file per module with run-time behaviour.
//!
//! None needs a GPU or NVIDIA's libraries: the loader is driven through
//! files that are not libraries, paths that do not exist and a resolver that
//! hands out stand-in functions (`stand_ins`); the engine runs its real code
//! against fake libraries, cuTensorNet and the CUDA Runtime simulated on the
//! CPU (`src/engine/native_calls/fake_libraries.rs`), so its tests run on
//! every host.
//! The loader compiles for its tests on any Unix host, so they run off
//! Linux x86-64 too.
//!
//! The tests that need a library open, to check what discovery reports of it
//! or concludes from it, open the C library instead, on Linux with glibc
//! only, where it is always present. They, and the loader's tests, rely on
//! the host's system loader.

mod devices;
mod diagnosis;
mod discovery;
mod engine;
mod libraries;
#[cfg(unix)]
mod stand_ins;

#[cfg(unix)]
use std::fs;
use std::path::PathBuf;

/// A path under the system's temporary directory that this test process
/// alone uses.
fn unique_temp_path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "qdk-cutensornet-test-{label}-{}",
        std::process::id()
    ))
}

/// Removes a file the test created, even when the test fails.
#[cfg(unix)]
struct RemoveFile(PathBuf);

#[cfg(unix)]
impl Drop for RemoveFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// A file that exists and is not a shared object, removed when the guard
/// is dropped.
#[cfg(unix)]
fn not_a_shared_object(label: &str) -> (PathBuf, RemoveFile) {
    let path = unique_temp_path(label);
    fs::write(&path, b"not an ELF shared object").expect("the temporary directory is writable");
    (path.clone(), RemoveFile(path))
}
