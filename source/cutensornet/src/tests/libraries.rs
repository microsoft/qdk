// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What every library shares: how its paths are chosen (the override
//! variable, checked before anything is loaded, or else the library's
//! defaults), and how an open library is checked and reported. Each
//! library's own rules (its table, probes and versions) are tested in its
//! module's file.

#[cfg(unix)]
mod cuda_runtime;
#[cfg(unix)]
mod cutensornet;
#[cfg(unix)]
mod loader;
mod report;

#[cfg(unix)]
use super::not_a_shared_object;
#[cfg(all(target_os = "linux", target_env = "gnu"))]
use super::stand_ins::ALWAYS_PRESENT_LIBRARY;
#[cfg(unix)]
use super::stand_ins::{StandInResolver, cutensornet_version};
use super::unique_temp_path;
use crate::{LibraryFault, libraries::validate_override_path};
#[cfg(unix)]
use crate::{
    LibraryReport, Version,
    libraries::{
        NativeLibrary, candidates, cuda_runtime::CudaRuntime, cutensornet::CuTensorNet, inspect,
    },
};
#[cfg(all(target_os = "linux", target_env = "gnu"))]
use crate::{
    libraries::loader::{FunctionTable, SymbolResolver, resolve_required},
    libraries::{ProbedVersions, open},
};
use std::ffi::OsString;
#[cfg(unix)]
use std::path::PathBuf;

/// An empty report of `L`, as discovery starts it once `L` is open.
#[cfg(unix)]
fn opened<L: NativeLibrary>() -> LibraryReport {
    LibraryReport {
        library: L::NAME,
        path: Some(PathBuf::from("/fixture/library.so")),
        versions: Vec::new(),
        faults: Vec::new(),
    }
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
        Err(LibraryFault::InvalidOverride {
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
        Err(LibraryFault::InvalidOverride {
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
        Err(LibraryFault::InvalidOverride {
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
        Err(LibraryFault::InvalidOverride {
            variable: "QDK_CUDART_LIBRARY",
            path,
            reason: "path does not exist",
        }) if path == link
    ));
}

/// A user who sets `QDK_CUDART_LIBRARY` changes where the CUDA Runtime is
/// loaded from, and only that: cuTensorNet is still looked for at its
/// defaults.
#[cfg(unix)]
#[test]
fn each_library_is_overridden_by_its_own_variable() {
    let (path, _remove) = not_a_shared_object("override-candidate");
    let read_variable =
        |variable: &'static str| (variable == "QDK_CUDART_LIBRARY").then(|| OsString::from(&path));
    let cudart = candidates::<CudaRuntime>(read_variable).expect("the override names a file");
    let cutensornet =
        candidates::<CuTensorNet>(read_variable).expect("no cuTensorNet override is set");
    assert_eq!(cudart, std::slice::from_ref(&path));
    assert!(!cutensornet.contains(&path));
}

/// Without an override, each library is looked for at the paths the README
/// lists, in order.
#[cfg(unix)]
#[test]
fn without_an_override_the_default_paths_are_tried() {
    let cudart = candidates::<CudaRuntime>(|_| None).expect("no override is set");
    let cutensornet = candidates::<CuTensorNet>(|_| None).expect("no override is set");
    assert_eq!(
        cudart,
        [
            "/usr/local/cuda-12.9/targets/x86_64-linux/lib/libcudart.so.12",
            "libcudart.so.12"
        ]
        .map(PathBuf::from)
    );
    assert_eq!(
        cutensornet,
        [
            "/usr/lib/x86_64-linux-gnu/libcuquantum/12/libcutensornet.so.2",
            "libcutensornet.so.2"
        ]
        .map(PathBuf::from)
    );
}

/// A library with no fault is bound to its table, and its versions are
/// reported.
#[cfg(unix)]
#[test]
fn a_library_without_a_fault_is_bound_to_its_table() {
    let mut report = opened::<CuTensorNet>();
    let functions = inspect::<CuTensorNet, _>(&StandInResolver::audited(), &mut report);
    assert!(functions.is_some(), "the library has no fault");
    assert_eq!(report.faults, []);
    assert_eq!(
        report.versions,
        [
            Version {
                component: "cuTensorNet",
                value: 21_300
            },
            Version {
                component: "cuTensorNet's CUDA Runtime",
                value: 12_090
            },
        ]
    );
}

/// A user who installed cuTensorNet 2.14, which lacks functions the crate
/// binds, learns both that functions are missing and that its version is
/// not the audited one, which is the cause.
#[cfg(unix)]
#[test]
fn an_open_library_reports_its_missing_functions_and_its_versions_together() {
    let resolver = StandInResolver::audited()
        .without("cutensornetCreateState")
        .without("cutensornetStateCompute")
        .with(
            "cutensornetGetVersion",
            cutensornet_version::<21_400> as *const (),
        );
    let mut report = opened::<CuTensorNet>();
    let functions = inspect::<CuTensorNet, _>(&resolver, &mut report);
    assert!(functions.is_none(), "the library is at fault");
    assert_eq!(
        report.faults,
        [
            LibraryFault::MissingRequiredSymbol {
                symbol: "cutensornetCreateState"
            },
            LibraryFault::MissingRequiredSymbol {
                symbol: "cutensornetStateCompute"
            },
            LibraryFault::UnsupportedVersion {
                component: "cuTensorNet",
                found: 21_400,
                supported: 21_300,
            },
        ]
    );
    assert_eq!(report.version("cuTensorNet"), Some(21_400));
}

/// A library whose functions are all found but whose version is not the
/// audited one is not used.
#[cfg(unix)]
#[test]
fn a_library_with_only_a_version_fault_is_not_bound() {
    let resolver = StandInResolver::audited().with(
        "cutensornetGetVersion",
        cutensornet_version::<21_400> as *const (),
    );
    let mut report = opened::<CuTensorNet>();
    let functions = inspect::<CuTensorNet, _>(&resolver, &mut report);
    assert!(functions.is_none(), "the version is not the audited one");
    assert_eq!(report.faults.len(), 1);
}

/// A library without one of its version functions has that version not
/// read; the missing function is its fault.
#[cfg(unix)]
#[test]
fn a_missing_version_function_is_reported_and_its_version_not_read() {
    let resolver = StandInResolver::audited().without("cutensornetGetVersion");
    let mut report = opened::<CuTensorNet>();
    let functions = inspect::<CuTensorNet, _>(&resolver, &mut report);
    assert!(functions.is_none(), "a required function is missing");
    assert_eq!(
        report.faults,
        [LibraryFault::MissingRequiredSymbol {
            symbol: "cutensornetGetVersion"
        }]
    );
    assert_eq!(report.version("cuTensorNet"), None);
}

/// A library that the system loader opens but that lacks a function: the C
/// library, standing in for an NVIDIA library, and a function it does not
/// export.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
struct OpenedWithoutItsFunction;

#[cfg(all(target_os = "linux", target_env = "gnu"))]
struct AbsentFunctionTable;

#[cfg(all(target_os = "linux", target_env = "gnu"))]
impl FunctionTable for AbsentFunctionTable {
    fn resolve<R: SymbolResolver>(resolver: &R) -> Result<Self, Vec<LibraryFault>> {
        let mut missing = Vec::new();
        resolve_required::<R, unsafe extern "C" fn()>(
            resolver,
            &mut missing,
            "qdk_cutensornet_test_absent_function",
        );
        if missing.is_empty() {
            Ok(Self)
        } else {
            Err(missing)
        }
    }
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
impl NativeLibrary for OpenedWithoutItsFunction {
    const NAME: &'static str = "C library";
    const VARIABLE: &'static str = "QDK_CUTENSORNET_TEST_UNSET_VARIABLE";
    const DEFAULTS: &'static [&'static str] = &[ALWAYS_PRESENT_LIBRARY];
    type Functions = AbsentFunctionTable;

    fn probe_versions<R: SymbolResolver>(_resolver: &R) -> ProbedVersions {
        ProbedVersions::default()
    }
}

/// A library that was opened is reported with the path it was opened from,
/// whatever its faults, so the user sees which copy is at fault.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[test]
fn an_opened_library_at_fault_is_reported_with_its_path() {
    let (report, bound) = open::<OpenedWithoutItsFunction>(|_| None);
    assert!(bound.is_none(), "the library lacks a required function");
    assert_eq!(
        report.path.as_deref(),
        Some(std::path::Path::new(OpenedWithoutItsFunction::DEFAULTS[0]))
    );
    assert_eq!(
        report.faults,
        [LibraryFault::MissingRequiredSymbol {
            symbol: "qdk_cutensornet_test_absent_function"
        }]
    );
}
