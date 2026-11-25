// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Opening a native library file.
//!
//! [`open_library_file`] opens a library from the first of its candidate
//! paths where something exists. A candidate is an absolute path, or a
//! bare file name the system loader searches for. It tells apart a library
//! not found, one at a path the process cannot access, and one the system
//! loader refused ([`LibraryFault`]'s `NotFound`, `InaccessiblePath` and
//! `LoadFailed`).

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
        // SAFETY: by `resolve`'s contract, `T` is the function's pointer
        // type, so `Option<T>` reads a zero address as `None`, and the
        // caller keeps `self` alive while it uses the result.
        let resolved = unsafe { self.library.get::<Option<T>>(symbol.as_bytes()) }.ok()?;
        *resolved
    }
}

/// Opens the first of `paths`, a library's candidates in order, where
/// something exists.
///
/// An absolute path where nothing exists is skipped, including one that
/// passes through a regular file as if it were a directory. Anything else
/// there, a directory for instance, is opened, and the loader's refusal is
/// the library's [`LibraryFault::LoadFailed`]. A path the process cannot
/// access is not skipped but is the library's
/// [`LibraryFault::InaccessiblePath`]. Behind a directory the process lacks
/// the permission to search, a file is perhaps there; a symlink loop is a
/// link, not nothing.
/// A bare file name is searched for by the system loader. It is skipped
/// when the loader reports no file of that name ([`no_file_of_that_name`]).
/// Any other failure is the library's.
pub(crate) fn open_library_file(paths: Vec<PathBuf>) -> Result<OpenLibraryFile, LibraryFault> {
    for path in &paths {
        if path.is_absolute() {
            match path.try_exists() {
                Ok(true) => {}
                Ok(false) => continue,
                // `try_exists` already reads `NotFound` as `Ok(false)`.
                Err(error) if error.kind() == std::io::ErrorKind::NotADirectory => continue,
                Err(error) => {
                    return Err(LibraryFault::InaccessiblePath {
                        path: path.clone(),
                        message: error.to_string(),
                    });
                }
            }
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
/// open. That is the candidate's when there is no such file. It is a
/// dependency's when the candidate was found but needs a library that is
/// missing. Only the first lets discovery try the next candidate.
///
/// glibc's search skips a file of that name it cannot use (another
/// machine's, or one it cannot read). It then reports no such file, as when
/// there is none. Telling the two apart would mean repeating glibc's search,
/// so both are read as no file.
///
/// Another loader's message (musl's, or macOS's when the tests run there)
/// is never read as "no such file". So the file name is reported as a load
/// failure rather than skipped.
fn no_file_of_that_name(candidate: &Path, message: &str) -> bool {
    candidate
        .to_str()
        .and_then(|name| message.strip_prefix(name))
        .and_then(|rest| rest.strip_prefix(": "))
        .is_some_and(|reason| reason.ends_with("No such file or directory"))
}

#[cfg(all(test, unix))]
mod tests;
