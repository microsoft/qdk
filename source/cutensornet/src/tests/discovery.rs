// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! How [`discover`](crate::discover) reads the override variables, and where
//! it refuses to run.

use super::unique_temp_path;
use crate::{Availability, AvailabilityError, validate_override_path};
use std::ffi::OsString;

fn assert_send_sync<T: Send + Sync>() {}

/// A caller discovers once and hands the result to the threads that run
/// shots.
#[test]
fn availability_can_be_shared_across_threads() {
    assert_send_sync::<Availability>();
}

/// An unset variable leaves the default search in place.
#[test]
fn an_unset_override_selects_no_path() {
    let path = validate_override_path("QDK_CUDART_LIBRARY", None)
        .expect("an unset variable is not an error");
    assert_eq!(path, None);
}

/// A user sets the variable to the library's path in their installation.
#[test]
fn an_absolute_path_to_a_file_is_the_override() {
    let file = unique_temp_path("override-file");
    std::fs::write(&file, b"").expect("the temporary directory is writable");
    let found = validate_override_path("QDK_CUDART_LIBRARY", Some(file.clone().into_os_string()));
    std::fs::remove_file(&file).expect("the test created the file");
    assert_eq!(
        found.expect("the path is absolute and names a file"),
        Some(file)
    );
}

/// `QDK_CUDART_LIBRARY=libcudart.so.12` would let the system loader pick any
/// copy on its search path.
#[test]
fn a_relative_override_is_refused() {
    assert!(matches!(
        validate_override_path(
            "QDK_CUDART_LIBRARY",
            Some(OsString::from("libcudart.so.12"))
        ),
        Err(AvailabilityError::InvalidOverride {
            variable: "QDK_CUDART_LIBRARY",
            reason: "path must be absolute",
            ..
        })
    ));
}

/// A mistyped path is reported with the variable that holds it.
#[test]
fn an_override_that_does_not_exist_is_refused() {
    let path = unique_temp_path("missing-override");
    assert!(matches!(
        validate_override_path("QDK_CUDART_LIBRARY", Some(path.clone().into_os_string())),
        Err(AvailabilityError::InvalidOverride {
            variable: "QDK_CUDART_LIBRARY",
            path: found,
            reason: "path does not exist",
        }) if found == path
    ));
}

/// The variable names the library's folder instead of the library.
#[test]
fn an_override_naming_a_directory_is_refused() {
    let path = std::env::temp_dir();
    assert!(matches!(
        validate_override_path("QDK_CUDART_LIBRARY", Some(path.clone().into_os_string())),
        Err(AvailabilityError::InvalidOverride {
            variable: "QDK_CUDART_LIBRARY",
            path: found,
            reason: "path must name a file",
        }) if found == path
    ));
}

/// Installations name a library by a link to its versioned file:
/// `libcudart.so.12` is a link.
#[cfg(unix)]
#[test]
fn an_override_naming_a_link_to_a_file_is_followed() {
    let file = unique_temp_path("link-target");
    let link = unique_temp_path("link-to-file");
    std::fs::write(&file, b"").expect("the temporary directory is writable");
    std::os::unix::fs::symlink(&file, &link).expect("the temporary directory is writable");
    let found = validate_override_path("QDK_CUDART_LIBRARY", Some(link.clone().into_os_string()));
    std::fs::remove_file(&link).expect("the test created the link");
    std::fs::remove_file(&file).expect("the test created the file");
    assert_eq!(found.expect("the link names a file"), Some(link));
}

/// A link left behind by an uninstalled library names nothing to load.
#[cfg(unix)]
#[test]
fn an_override_naming_a_dangling_link_is_refused() {
    let link = unique_temp_path("dangling-link");
    std::os::unix::fs::symlink(unique_temp_path("removed-target"), &link)
        .expect("the temporary directory is writable");
    let found = validate_override_path("QDK_CUDART_LIBRARY", Some(link.clone().into_os_string()));
    std::fs::remove_file(&link).expect("the test created the link");
    assert!(matches!(
        found,
        Err(AvailabilityError::InvalidOverride {
            variable: "QDK_CUDART_LIBRARY",
            path,
            reason: "path does not exist",
        }) if path == link
    ));
}

/// On macOS or Windows, a GPU run is refused at once, with the platform.
#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
#[test]
fn discovery_off_linux_x86_64_is_unsupported() {
    assert!(matches!(
        crate::discover(),
        Err(AvailabilityError::UnsupportedPlatform { .. })
    ));
}
