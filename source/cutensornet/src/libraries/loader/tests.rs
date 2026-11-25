// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! How the loader finds and opens a library, and what it reports when it
//! cannot.
//!
//! No NVIDIA library is opened: the libraries are files that are not shared
//! objects, or paths and names with no file the process can see.

use crate::test_utils::{not_a_shared_object, unique_temp_path};
use crate::{
    LibraryFault,
    libraries::loader::{no_file_of_that_name, open_library_file},
};
use std::path::Path;
#[cfg(all(target_os = "linux", target_env = "gnu"))]
use std::path::PathBuf;

/// A file name that no system loader finds.
///
/// The tests that use it run on Linux with glibc only. A file name is
/// skipped when glibc's message reports no file of that name
/// (`no_file_of_that_name`). Other loaders word it otherwise (macOS's
/// `dlopen` lists every path it tried).
#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn absent_bare_name() -> PathBuf {
    PathBuf::from(format!(
        "libqdk-cutensornet-test-absent-{}.so.0",
        std::process::id()
    ))
}

/// A candidate the system loader refuses is reported with its path and the
/// loader's message, and no later candidate is tried. The cases are a file
/// that is not a shared object (on a real host, also a library whose own
/// dependencies are missing), and a directory, which is something other
/// than no file.
#[test]
fn a_library_the_system_loader_refuses_is_a_load_failure() {
    let (file, _remove) = not_a_shared_object("invalid-library");
    let directory = unique_temp_path("directory-library");
    std::fs::create_dir(&directory).expect("the temporary directory is writable");
    let (later, _remove_later) = not_a_shared_object("after-refused");
    let faults = [&file, &directory].map(|refused| {
        (
            refused,
            open_library_file(vec![refused.clone(), later.clone()]).err(),
        )
    });
    std::fs::remove_dir(&directory).expect("the test created the directory");
    for (refused, fault) in faults {
        assert!(
            matches!(
                &fault,
                Some(LibraryFault::LoadFailed { path, message })
                    if path == refused && !message.is_empty()
            ),
            "{refused:?}: {fault:?}"
        );
    }
}

/// A default path where there is no file on this host is skipped for the
/// next one, and the next one's failure is reported. The cases are a path
/// that does not exist, one that passes through a regular file as if it
/// were a directory, and, with glibc, a file name the system loader does
/// not find on its search path.
#[test]
fn a_default_path_that_does_not_exist_is_skipped() {
    let (path, _remove) = not_a_shared_object("second-default");
    let (file, _remove_file) = not_a_shared_object("file-as-directory");
    for absent in [
        unique_temp_path("absent-default"),
        file.join("libcudart.so.12"),
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        absent_bare_name(),
    ] {
        let fault = open_library_file(vec![absent.clone(), path.clone()])
            .err()
            .expect("the second default is not a shared object");
        assert!(
            matches!(&fault, LibraryFault::LoadFailed { path: found, .. } if *found == path),
            "{absent:?}: {fault:?}"
        );
    }
}

/// A candidate the process cannot access is reported as a path it cannot
/// access. It is not skipped for a later candidate the user did not mean to
/// load. The cases are a symlink loop, and a path in a directory the
/// process lacks the permission to search, where a file is perhaps there.
#[test]
fn a_path_the_process_cannot_access_is_reported() {
    let (later, _remove) = not_a_shared_object("after-inaccessible");
    let (looping, locked_dir, _remove_unreadable) = crate::libraries::tests::unreadable_paths();
    let mut inaccessible = vec![("a symlink loop", looping)];
    // Root reads a directory whatever its mode, so the row runs only where
    // the mode applies.
    if std::fs::read_dir(&locked_dir).is_err() {
        inaccessible.push((
            "a file in a directory the process cannot search",
            locked_dir.join("libcudart.so.12"),
        ));
    }
    for (case, candidate) in inaccessible {
        let fault = open_library_file(vec![candidate.clone(), later.clone()]).err();
        assert!(
            matches!(
                &fault,
                Some(LibraryFault::InaccessiblePath { path, .. }) if *path == candidate
            ),
            "{case}: {fault:?}"
        );
    }
}

/// The error when the library is not installed: every path tried, in order,
/// file names included.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[test]
fn a_library_found_nowhere_reports_every_path_tried() {
    let paths = vec![unique_temp_path("absent-library"), absent_bare_name()];
    let fault = open_library_file(paths.clone())
        .err()
        .expect("no candidate exists");
    assert_eq!(fault, LibraryFault::NotFound { attempted: paths });
}

/// The system loader names the file it could not open. A file name is
/// skipped only when no file of that name was found. It is not skipped when
/// the file was found but one of its dependencies was not, or the file was
/// refused.
#[test]
fn only_a_file_name_not_found_is_skipped() {
    let name = Path::new("libcutensornet.so.2");
    assert!(no_file_of_that_name(
        name,
        "libcutensornet.so.2: cannot open shared object file: No such file or directory"
    ));
    assert!(!no_file_of_that_name(
        name,
        "libcublasLt.so.12: cannot open shared object file: No such file or directory"
    ));
    assert!(!no_file_of_that_name(
        name,
        "libcutensornet.so.2: wrong ELF class: ELFCLASS32"
    ));
}

/// glibc's search skips a file of the name it cannot use (here, another
/// machine's library), then reports no such file, as when there is none.
/// Discovery reads it so too, and tries the next candidate.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[test]
fn a_file_glibc_cannot_use_reads_as_not_found() {
    let (path, _remove) = another_machines_library("another-machine");
    let Err(LibraryFault::LoadFailed { message, .. }) = open_library_file(vec![path.clone()])
    else {
        panic!("precondition: glibc refuses another machine's library at its path");
    };
    assert!(
        no_file_of_that_name(&path, &message),
        "glibc's message for a file it could not use: {message}"
    );
}

/// A 64-bit ELF shared object for a machine no test runs on (SPARC), its
/// header only, removed when the guard is dropped.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn another_machines_library(label: &str) -> (PathBuf, crate::test_utils::RemoveFile) {
    let mut header = [0_u8; 64];
    header[..7].copy_from_slice(&[0x7f, b'E', b'L', b'F', 2, 1, 1]);
    header[16] = 3;
    header[18] = 2;
    header[20] = 1;
    let path = unique_temp_path(label);
    std::fs::write(&path, header).expect("the temporary directory is writable");
    (path.clone(), crate::test_utils::RemoveFile(path))
}
