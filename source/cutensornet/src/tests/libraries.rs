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

#[cfg(all(target_os = "linux", target_env = "gnu"))]
use super::stand_ins::ALWAYS_PRESENT_LIBRARY;
#[cfg(unix)]
use super::stand_ins::{StandInResolver, cutensornet_version};
use super::unique_temp_path;
#[cfg(unix)]
use super::{RemoveFile, not_a_shared_object};
use crate::{LibraryFault, libraries::validate_override_path};
#[cfg(unix)]
use crate::{
    LibraryReport, PackagedLibraries, Version,
    libraries::{
        NativeLibrary, candidates, cuda_runtime::CudaRuntime, cutensornet::CuTensorNet, inspect,
        other_copies,
    },
};
#[cfg(all(target_os = "linux", target_env = "gnu"))]
use crate::{
    libraries::loader::{FunctionTable, SymbolResolver, resolve_required},
    libraries::{ProbedVersions, open},
};
use std::ffi::OsString;
#[cfg(unix)]
use std::path::{Path, PathBuf};

/// An empty report of `L`, as discovery starts it once `L` is open.
#[cfg(unix)]
fn opened<L: NativeLibrary>() -> LibraryReport {
    LibraryReport {
        library: L::NAME,
        path: Some(PathBuf::from("/fixture/library.so")),
        versions: Vec::new(),
        faults: Vec::new(),
        other_copies: Vec::new(),
        loaded_from_override: false,
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
    let packaged = PackagedLibraries::default();
    let cudart =
        candidates::<CudaRuntime>(read_variable, &packaged).expect("the override names a file");
    let cutensornet = candidates::<CuTensorNet>(read_variable, &packaged)
        .expect("no cuTensorNet override is set");
    assert_eq!(cudart, std::slice::from_ref(&path));
    assert!(!cutensornet.contains(&path));
}

/// Without an override, each library is looked for at the paths the README
/// lists, in order.
#[cfg(unix)]
#[test]
fn without_an_override_the_default_paths_are_tried() {
    let packaged = PackagedLibraries::default();
    let cudart = candidates::<CudaRuntime>(|_| None, &packaged).expect("no override is set");
    let cutensornet = candidates::<CuTensorNet>(|_| None, &packaged).expect("no override is set");
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

/// A library a Python package installed is tried before the default paths:
/// a user who installed NVIDIA's wheels needs no `LD_LIBRARY_PATH`.
#[cfg(unix)]
#[test]
fn a_packaged_library_is_tried_before_the_default_paths() {
    let packaged = PackagedLibraries {
        cutensornet: Some(PathBuf::from("/env/cuquantum/lib/libcutensornet.so.2")),
        cuda_runtime: Some(PathBuf::from(
            "/env/nvidia/cuda_runtime/lib/libcudart.so.12",
        )),
    };
    let cudart = candidates::<CudaRuntime>(|_| None, &packaged).expect("no override is set");
    let cutensornet = candidates::<CuTensorNet>(|_| None, &packaged).expect("no override is set");
    assert_eq!(
        cudart,
        [
            "/env/nvidia/cuda_runtime/lib/libcudart.so.12",
            "/usr/local/cuda-12.9/targets/x86_64-linux/lib/libcudart.so.12",
            "libcudart.so.12"
        ]
        .map(PathBuf::from)
    );
    assert_eq!(
        cutensornet,
        [
            "/env/cuquantum/lib/libcutensornet.so.2",
            "/usr/lib/x86_64-linux-gnu/libcuquantum/12/libcutensornet.so.2",
            "libcutensornet.so.2"
        ]
        .map(PathBuf::from)
    );
}

/// A set override is still the only path tried when a package installed
/// the library: the user's setting decides.
#[cfg(unix)]
#[test]
fn an_override_is_tried_alone_even_beside_a_packaged_library() {
    let (path, _remove) = not_a_shared_object("override-beside-packaged");
    let read_variable =
        |variable: &'static str| (variable == "QDK_CUDART_LIBRARY").then(|| OsString::from(&path));
    let packaged = PackagedLibraries {
        cutensornet: None,
        cuda_runtime: Some(PathBuf::from(
            "/env/nvidia/cuda_runtime/lib/libcudart.so.12",
        )),
    };
    let cudart =
        candidates::<CudaRuntime>(read_variable, &packaged).expect("the override names a file");
    assert_eq!(cudart, std::slice::from_ref(&path));
}

/// The other copies of `L` reported beside `loaded`, among the test's own
/// files (a host's system copies, if any, left out).
#[cfg(unix)]
fn other_test_copies<L: NativeLibrary>(
    loaded: &Path,
    read_variable: impl Fn(&'static str) -> Option<OsString>,
    packaged: &PackagedLibraries,
) -> Vec<PathBuf> {
    let temporary = unique_temp_path("");
    let directory = temporary
        .parent()
        .expect("a temporary file has a directory");
    other_copies::<L>(loaded, read_variable, packaged)
        .into_iter()
        .filter(|path| path.starts_with(directory))
        .collect()
}

/// A user who overrides the CUDA Runtime while a wheel's copy is installed
/// is told of the wheel's copy.
#[cfg(unix)]
#[test]
fn a_wheels_copy_beside_an_override_is_another_copy() {
    let (loaded, _remove_loaded) = not_a_shared_object("loaded-copy");
    let (wheel, _remove_wheel) = not_a_shared_object("wheel-copy");
    let read_variable = |variable: &'static str| {
        (variable == "QDK_CUDART_LIBRARY").then(|| OsString::from(&loaded))
    };
    let packaged = PackagedLibraries {
        cutensornet: None,
        cuda_runtime: Some(wheel.clone()),
    };
    assert_eq!(
        other_test_copies::<CudaRuntime>(&loaded, read_variable, &packaged),
        vec![wheel.clone()]
    );
}

/// A user with no override whose CUDA Runtime was loaded from the wheel
/// has no other copy: the loaded one is not reported as another.
#[cfg(unix)]
#[test]
fn the_loaded_copy_is_no_other_copy() {
    let (wheel, _remove_wheel) = not_a_shared_object("loaded-wheel-copy");
    let packaged = PackagedLibraries {
        cutensornet: None,
        cuda_runtime: Some(wheel.clone()),
    };
    assert!(other_test_copies::<CudaRuntime>(&wheel, |_| None, &packaged).is_empty());
}

/// cuTensorNet's other copies are looked for among its own paths only: the
/// CUDA Runtime's wheel is no copy of it.
#[cfg(unix)]
#[test]
fn another_librarys_wheel_is_no_other_copy() {
    let (loaded, _remove_loaded) = not_a_shared_object("loaded-cutensornet");
    let (wheel, _remove_wheel) = not_a_shared_object("cuda-runtime-wheel");
    let packaged = PackagedLibraries {
        cutensornet: None,
        cuda_runtime: Some(wheel),
    };
    assert!(other_test_copies::<CuTensorNet>(&loaded, |_| None, &packaged).is_empty());
}

/// A bare file name, such as the default `libcudart.so.12`, is searched for
/// by the system loader, not in the working directory, so a file of that
/// name there is no other copy. The defaults are fixed, so the test names a
/// bare file through the override instead (which discovery refuses before
/// loading anything): tests run in the crate's directory, which holds a
/// `Cargo.toml`.
#[cfg(unix)]
#[test]
fn a_bare_file_name_is_not_looked_for_in_the_working_directory() {
    assert!(
        Path::new("Cargo.toml").is_file(),
        "tests run in the crate's directory"
    );
    let (loaded, _remove_loaded) = not_a_shared_object("loaded-beside-bare-name");
    let read_variable = |variable: &'static str| {
        (variable == "QDK_CUDART_LIBRARY").then(|| OsString::from("Cargo.toml"))
    };
    let copies = other_copies::<CudaRuntime>(&loaded, read_variable, &PackagedLibraries::default());
    assert!(!copies.contains(&PathBuf::from("Cargo.toml")));
}

/// Two links in one installation, `libcudart.so.12` naming the file loaded:
/// the link is no other copy.
#[cfg(unix)]
#[test]
fn a_link_to_the_loaded_file_is_no_other_copy() {
    let (loaded, _remove_loaded) = not_a_shared_object("loaded-linked");
    let link_to_loaded = unique_temp_path("link-to-loaded");
    std::os::unix::fs::symlink(&loaded, &link_to_loaded).expect("a link is made");
    let _remove_link_to_loaded = RemoveFile(link_to_loaded.clone());
    let packaged = PackagedLibraries {
        cutensornet: None,
        cuda_runtime: Some(link_to_loaded),
    };
    assert!(other_test_copies::<CudaRuntime>(&loaded, |_| None, &packaged).is_empty());
}

/// A library loaded through a link: the file the link names is no other
/// copy.
#[cfg(unix)]
#[test]
fn the_file_a_loaded_link_names_is_no_other_copy() {
    let (file, _remove_file) = not_a_shared_object("linked-file");
    let loaded_link = unique_temp_path("loaded-link");
    std::os::unix::fs::symlink(&file, &loaded_link).expect("a link is made");
    let _remove_loaded_link = RemoveFile(loaded_link.clone());
    let packaged = PackagedLibraries {
        cutensornet: None,
        cuda_runtime: Some(file),
    };
    assert!(other_test_copies::<CudaRuntime>(&loaded_link, |_| None, &packaged).is_empty());
}

/// One other file named twice, by the override and by a link to it, is
/// reported once, under the first name.
#[cfg(unix)]
#[test]
fn one_file_named_twice_is_reported_once() {
    let (loaded, _remove_loaded) = not_a_shared_object("loaded-beside-twice-named");
    let (other, _remove_other) = not_a_shared_object("other-named-twice");
    let link_to_other = unique_temp_path("link-to-other");
    std::os::unix::fs::symlink(&other, &link_to_other).expect("a link is made");
    let _remove_link_to_other = RemoveFile(link_to_other.clone());
    let read_variable =
        |variable: &'static str| (variable == "QDK_CUDART_LIBRARY").then(|| OsString::from(&other));
    let packaged = PackagedLibraries {
        cutensornet: None,
        cuda_runtime: Some(link_to_other),
    };
    assert_eq!(
        other_test_copies::<CudaRuntime>(&loaded, read_variable, &packaged),
        vec![other.clone()]
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
    const INSTALL: &'static str = "the C library";
    type Functions = AbsentFunctionTable;

    fn packaged(_packaged: &PackagedLibraries) -> Option<&Path> {
        None
    }

    fn probe_versions<R: SymbolResolver>(_resolver: &R) -> ProbedVersions {
        ProbedVersions::default()
    }

    fn describe(_report: &LibraryReport) -> Vec<String> {
        Vec::new()
    }
}

/// A library that was opened is reported with the path it was opened from,
/// whatever its faults, so the user sees which copy is at fault.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[test]
fn an_opened_library_at_fault_is_reported_with_its_path() {
    let (report, bound) = open::<OpenedWithoutItsFunction>(|_| None, &PackagedLibraries::default());
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

/// The C library again, with a second default path, `/bin/sh`, an
/// absolute file every Unix has, standing in for another copy of it.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
struct OpenedBesideAnotherFile;

#[cfg(all(target_os = "linux", target_env = "gnu"))]
impl NativeLibrary for OpenedBesideAnotherFile {
    const NAME: &'static str = "C library";
    const VARIABLE: &'static str = "QDK_CUTENSORNET_TEST_UNSET_VARIABLE";
    const DEFAULTS: &'static [&'static str] = &[ALWAYS_PRESENT_LIBRARY, "/bin/sh"];
    const INSTALL: &'static str = "the C library";
    type Functions = AbsentFunctionTable;

    fn packaged(_packaged: &PackagedLibraries) -> Option<&Path> {
        None
    }

    fn probe_versions<R: SymbolResolver>(_resolver: &R) -> ProbedVersions {
        ProbedVersions::default()
    }

    fn describe(_report: &LibraryReport) -> Vec<String> {
        Vec::new()
    }
}

/// A library that was opened reports the other copies its paths make
/// visible beside it, whatever its faults, so the setup check can warn of
/// them.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[test]
fn an_opened_library_reports_the_other_copies_beside_it() {
    let (report, _) = open::<OpenedBesideAnotherFile>(|_| None, &PackagedLibraries::default());
    assert_eq!(
        report.path.as_deref(),
        Some(Path::new(ALWAYS_PRESENT_LIBRARY)),
        "the first default path opens"
    );
    assert_eq!(report.other_copies, [PathBuf::from("/bin/sh")]);
}

/// The absolute path of the C library on this host, at one of the paths
/// distributions install it at.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn absolute_c_library() -> PathBuf {
    let arch = std::env::consts::ARCH;
    [
        format!("/lib/{arch}-linux-gnu/libc.so.6"),
        format!("/usr/lib/{arch}-linux-gnu/libc.so.6"),
        "/lib64/libc.so.6".to_string(),
        "/usr/lib64/libc.so.6".to_string(),
        "/usr/lib/libc.so.6".to_string(),
    ]
    .into_iter()
    .map(PathBuf::from)
    .find(|path| path.is_file())
    .expect("the C library is at one of the usual paths")
}

/// A library loaded from the path its override names is reported so, for
/// the setup check's warning to say to unset the variable; one loaded from
/// its defaults, or not loaded because its override is invalid, is not.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[test]
fn only_a_library_loaded_from_its_override_is_reported_so() {
    let c_library = absolute_c_library();
    let read_variable = |variable: &'static str| {
        (variable == OpenedBesideAnotherFile::VARIABLE).then(|| OsString::from(&c_library))
    };
    let (from_override, _) =
        open::<OpenedBesideAnotherFile>(read_variable, &PackagedLibraries::default());
    assert_eq!(
        from_override.path.as_deref(),
        Some(c_library.as_path()),
        "the override is loaded"
    );
    assert!(from_override.loaded_from_override);
    let (from_defaults, _) =
        open::<OpenedBesideAnotherFile>(|_| None, &PackagedLibraries::default());
    assert!(from_defaults.path.is_some(), "a default path is loaded");
    assert!(!from_defaults.loaded_from_override);
    let relative = |_: &'static str| Some(OsString::from("libc.so.6"));
    let (not_loaded, _) = open::<OpenedBesideAnotherFile>(relative, &PackagedLibraries::default());
    assert!(not_loaded.path.is_none(), "a relative override is refused");
    assert!(!not_loaded.loaded_from_override);
}
