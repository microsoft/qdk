// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Helpers the crate's unit tests share. Each module's tests are in a
//! `tests.rs` beside it.
//!
//! None needs a GPU or NVIDIA's libraries. The loader is driven through
//! files that are not libraries, paths that do not exist, and a resolver
//! that hands out stand-in functions (`stand_ins`). The loader compiles for
//! its tests on any Unix host, so they run off Linux x86-64 too.
//!
//! Some tests need a library open, to check what discovery reports of it or
//! concludes from it. They open the C library instead, on Linux with glibc
//! only, where it is always present. They, and the loader's tests, rely on
//! the host's system loader.

#[cfg(unix)]
pub(crate) mod stand_ins;

#[cfg(unix)]
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// A path under the system's temporary directory that this call alone uses,
/// so that tests running in parallel, or one fixture built twice, never
/// share a file.
pub(crate) fn unique_temp_path(label: &str) -> PathBuf {
    static CALLS: AtomicU64 = AtomicU64::new(0);
    let call = CALLS.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "qdk-cutensornet-test-{label}-{}-{call}",
        std::process::id()
    ))
}

/// Removes a file the test created, even when the test fails.
#[cfg(unix)]
pub(crate) struct RemoveFile(pub(crate) PathBuf);

#[cfg(unix)]
impl Drop for RemoveFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

/// A file that exists and is not a shared object, removed when the guard
/// is dropped.
#[cfg(unix)]
pub(crate) fn not_a_shared_object(label: &str) -> (PathBuf, RemoveFile) {
    let path = unique_temp_path(label);
    fs::write(&path, b"not an ELF shared object").expect("the temporary directory is writable");
    (path.clone(), RemoveFile(path))
}
