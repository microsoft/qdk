// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! How the loader finds and opens a library, and what it reports when it
//! cannot.
//!
//! No NVIDIA library is opened: the libraries are files that are not shared
//! objects, or paths and names with no file the process can see.

use crate::tests::{not_a_shared_object, unique_temp_path};
use crate::{
    LibraryFault,
    libraries::loader::{load_library, no_file_of_that_name},
};
use std::path::Path;
#[cfg(all(target_os = "linux", target_env = "gnu"))]
use std::path::PathBuf;

/// A file name that no system loader finds.
///
/// The tests that use it run on Linux with glibc only: a file name is
/// skipped when glibc's message reports no file of that name
/// (`no_file_of_that_name`), and other loaders word it otherwise (macOS's
/// `dlopen` lists every path it tried).
#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn absent_bare_name() -> PathBuf {
    PathBuf::from(format!(
        "libqdk-cutensornet-test-absent-{}.so.0",
        std::process::id()
    ))
}

/// A file that the system loader refuses (here, not a shared object; on a
/// real host, also a library whose own dependencies are missing) is
/// reported with its path and the loader's message.
#[test]
fn a_library_the_system_loader_refuses_is_a_load_failure() {
    let (path, _remove) = not_a_shared_object("invalid-library");
    let fault = load_library(vec![path.clone()])
        .err()
        .expect("the file is not a shared object");
    assert!(matches!(
        fault,
        LibraryFault::LoadFailed { path: found, message }
            if found == path && !message.is_empty()
    ));
}

/// A default path with no file the process can see on this host (none
/// there, a symlink loop, or a file in a directory it cannot search) is
/// skipped for the next one, and the next one's failure is reported.
#[test]
fn a_default_path_whose_metadata_cannot_be_read_is_skipped() {
    let (path, _remove) = not_a_shared_object("second-default");
    let (looping, locked_dir, _remove_unreadable) = super::unreadable_paths();
    let mut unseen = vec![
        ("no file", unique_temp_path("absent-default")),
        ("a symlink loop", looping),
    ];
    // Root reads a directory whatever its mode, so the row runs only where
    // the mode applies.
    if std::fs::read_dir(&locked_dir).is_err() {
        unseen.push((
            "a file in a directory the process cannot search",
            locked_dir.join("libcudart.so.12"),
        ));
    }
    for (case, default) in unseen {
        let fault = load_library(vec![default, path.clone()])
            .err()
            .expect("the second default is not a shared object");
        assert!(
            matches!(&fault, LibraryFault::LoadFailed { path: found, .. } if *found == path),
            "{case}: {fault:?}"
        );
    }
}

/// A default file name the system loader does not find on its search path
/// is skipped for the next default, and the next one's failure is reported.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[test]
fn a_file_name_the_system_loader_does_not_find_is_skipped() {
    let (path, _remove) = not_a_shared_object("after-bare-name");
    let fault = load_library(vec![absent_bare_name(), path.clone()])
        .err()
        .expect("the second default is not a shared object");
    assert!(matches!(
        fault,
        LibraryFault::LoadFailed { path: found, .. } if found == path
    ));
}

/// The error when the library is not installed: every path tried, in order,
/// file names included.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[test]
fn a_library_found_nowhere_reports_every_path_tried() {
    let paths = vec![unique_temp_path("absent-library"), absent_bare_name()];
    let fault = load_library(paths.clone())
        .err()
        .expect("no candidate exists");
    assert_eq!(fault, LibraryFault::NotFound { attempted: paths });
}

/// The system loader names the file it could not open: a file name is
/// skipped only when no file of that name was found, not when the file was
/// found but one of its dependencies was not, or the file was refused.
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
/// machine's library) and then reports no such file, as when there is none:
/// discovery reads it so too, and tries the next candidate.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[test]
fn a_file_glibc_cannot_use_reads_as_not_found() {
    let (path, _remove) = another_machines_library("another-machine");
    let Err(LibraryFault::LoadFailed { message, .. }) = load_library(vec![path.clone()]) else {
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
fn another_machines_library(label: &str) -> (PathBuf, crate::tests::RemoveFile) {
    let mut header = [0_u8; 64];
    header[..7].copy_from_slice(&[0x7f, b'E', b'L', b'F', 2, 1, 1]);
    header[16] = 3;
    header[18] = 2;
    header[20] = 1;
    let path = unique_temp_path(label);
    std::fs::write(&path, header).expect("the temporary directory is writable");
    (path.clone(), crate::tests::RemoveFile(path))
}
