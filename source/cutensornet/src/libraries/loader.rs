// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Opening a native library file.
//!
//! `libraries::open` opens each library the crate uses with
//! [`load_library`], which tries its candidate paths in order, skips those
//! where there is no file, and stops at the first other one: it opens it,
//! or reports why it cannot (the process cannot access the path, or the
//! system loader refuses the file). Then `libraries::open` fills the table
//! of the library's functions from the open file, which is a
//! `SymbolResolver` (`function_table`). Opening returns its fault as a
//! [`LibraryFault`].

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

/// Opens the first of `paths`, a library's candidates in order, where a
/// file may be.
///
/// An absolute path where there is no file is skipped, including one that
/// passes through a regular file as if it were a directory. One the process
/// cannot access (a directory on it the process may not search) is the
/// library's [`LibraryFault::InaccessiblePath`], as a file may be there. A
/// bare file name is searched for by the system loader, and skipped when the
/// loader reports no file of that name: glibc reports so too when its search
/// found only files of that name it could not use (another machine's, or
/// unreadable ones), as its own search skips them. Any other failure is the
/// library's.
pub(crate) fn load_library(paths: Vec<PathBuf>) -> Result<OpenLibraryFile, LibraryFault> {
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
