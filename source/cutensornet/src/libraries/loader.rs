// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Opening a native library file.
//!
//! [`load_library`] opens a library from the first of its candidate paths
//! (absolute paths, or bare file names the system loader searches for) that
//! names a file of it, and tells a library not found from one the system
//! loader refused ([`LibraryFault`]'s `NotFound` and `LoadFailed`).

use super::function_table::SymbolResolver;
use crate::LibraryFault;
use libloading::{Library, os::unix as unix_library};
use std::path::{Path, PathBuf};

/// A library file the system loader opened, and the path it was opened
/// from. Dropping it closes the library.
pub(crate) struct OpenLibraryFile {
    path: PathBuf,
    library: Library,
}

impl OpenLibraryFile {
    /// The path the library was opened from.
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl SymbolResolver for OpenLibraryFile {
    unsafe fn resolve<T: Copy>(&self, symbol: &'static str) -> Option<T> {
        // SAFETY: `T` is a function-pointer type (the caller's contract), so
        // `Option<T>` reads a zero address as `None`; `self.library`
        // outlives every copied function pointer.
        let resolved = unsafe { self.library.get::<Option<T>>(symbol.as_bytes()) }.ok()?;
        *resolved
    }
}

/// Opens the first of `paths`, a library's candidates in order, that exists.
///
/// An absolute path is skipped when its metadata cannot be read: there is
/// no file there, or a symlink loop or a directory the process cannot
/// search hides it. A bare file name is searched for by the system loader,
/// and skipped when the loader reports no file of that name
/// ([`no_file_of_that_name`]). Any other failure is the library's.
pub(crate) fn load_library(paths: Vec<PathBuf>) -> Result<OpenLibraryFile, LibraryFault> {
    for path in &paths {
        if path.is_absolute() && !path.exists() {
            continue;
        }

        // SAFETY: opening a library runs its initialisers (and, when it is
        // closed, its finalisers) as foreign code. The candidate is one of a
        // `NativeLibrary`'s paths, a fixed name or an absolute path the user
        // set, whose initialisers are trusted. The `Library` lives in the
        // returned `OpenLibraryFile`, which callers keep while a resolved
        // pointer is live (`SymbolResolver::resolve`'s contract).
        let result = unsafe {
            unix_library::Library::open(
                Some(path.as_os_str()),
                unix_library::RTLD_NOW | unix_library::RTLD_LOCAL,
            )
        };
        match result {
            Ok(opened) => {
                return Ok(OpenLibraryFile {
                    path: path.clone(),
                    library: opened.into(),
                });
            }
            Err(error) => {
                let message = error.to_string();
                if path.is_absolute() || !no_file_of_that_name(path, &message) {
                    return Err(LibraryFault::LoadFailed {
                        path: path.clone(),
                        message,
                    });
                }
            }
        }
    }
    Err(LibraryFault::NotFound { attempted: paths })
}

/// Whether the system loader's `message`, on failing to open the bare file
/// name `candidate`, reports that there is no file of that name.
///
/// glibc's loader starts its message with the name of the file it could not
/// open: the candidate's when there is no such file, a dependency's when the
/// candidate was found but needs a library that is missing. Only the first
/// lets discovery try the next candidate. glibc's search skips a file of
/// that name it cannot use (another machine's, or one it cannot read) and
/// then reports no such file, as when there is none; telling the two apart
/// would mean repeating glibc's search, so both are read as no file.
/// Another loader's message (musl's, or macOS's when the tests run there)
/// is never read as "no such file", so the file name is reported as a load
/// failure rather than skipped.
pub(crate) fn no_file_of_that_name(candidate: &Path, message: &str) -> bool {
    candidate
        .to_str()
        .and_then(|name| message.strip_prefix(name))
        .and_then(|rest| rest.strip_prefix(": "))
        .is_some_and(|reason| reason.ends_with("No such file or directory"))
}
