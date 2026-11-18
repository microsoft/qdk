// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The crate's unit tests, one file per module with run-time behaviour.
//!
//! None needs a GPU or NVIDIA's libraries: the loader is driven through files
//! that are not libraries, paths that do not exist and a resolver that hands
//! out stand-in functions (`stand_ins`); the CUDA algebra runs its real code
//! against fake libraries, cuTensorNet and the CUDA Runtime simulated on the
//! CPU (`src/engine/native_calls/fake_libraries.rs`), so its tests run on every
//! host. The loader compiles for its tests on any Unix host, so they run off
//! Linux x86-64 too.
//!
//! The tests that need a library open, to check what discovery reports of it
//! or concludes from it, open the C library instead, on Linux with glibc
//! only, where it is always present. They, and the loader's tests, rely on
//! the host's system loader.

mod discovery;
mod engine;
mod gpu_setup;
mod libraries;
#[cfg(unix)]
mod stand_ins;

#[cfg(unix)]
use crate::libraries::NativeLibrary;
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

/// Where a package would install `L`'s file: `L::FILE_NAME` in a directory
/// of the test's own, made now and removed, with what the test puts at the
/// path, when the guard is dropped.
#[cfg(unix)]
fn packaged_path<L: NativeLibrary>(label: &str) -> (PathBuf, RemovePackage) {
    let directory = unique_temp_path(label);
    fs::create_dir(&directory).expect("the temporary directory is writable");
    let path = directory.join(L::FILE_NAME);
    (path.clone(), RemovePackage(path))
}

/// Removes a packaged file and its directory, even when the test fails.
#[cfg(unix)]
struct RemovePackage(PathBuf);

#[cfg(unix)]
impl Drop for RemovePackage {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
        if let Some(directory) = self.0.parent() {
            let _ = fs::remove_dir(directory);
        }
    }
}

/// `L`'s file, as a package installs it, with contents that are not a
/// shared object: see [`packaged_path`].
#[cfg(unix)]
fn packaged_file<L: NativeLibrary>(label: &str) -> (PathBuf, RemovePackage) {
    let (path, remove) = packaged_path::<L>(label);
    fs::write(&path, b"not an ELF shared object").expect("the directory is writable");
    (path, remove)
}
