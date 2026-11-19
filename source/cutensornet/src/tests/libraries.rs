// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What every library shares: how its paths are chosen (the override
//! variable, checked before anything is loaded, or else the library's
//! packaged file, then its defaults), how an open library is checked and
//! reported with the other copies beside it, and that the setup check has
//! advice for each. Each
//! library's own rules (its table, probes and versions) are tested in its
//! module's file.

#[cfg(unix)]
mod cuda_runtime;
#[cfg(unix)]
mod cutensornet;
#[cfg(unix)]
mod function_table;
#[cfg(unix)]
mod loader;
mod report;

#[cfg(all(target_os = "linux", target_env = "gnu"))]
use super::stand_ins::ALWAYS_PRESENT_LIBRARY;
#[cfg(unix)]
use super::stand_ins::{StandInResolver, cuda_version, cutensornet_version};
use super::unique_temp_path;
#[cfg(unix)]
use super::{RemoveFile, not_a_shared_object, packaged_file, packaged_path};
use crate::{LibraryFault, libraries::validate_override_path};
#[cfg(unix)]
use crate::{
    LibraryReport, PackagedLibraries, Version,
    libraries::{
        InstalledLibraries, LoadedLibraries, NativeLibrary, Opening, candidates,
        cuda_runtime::CudaRuntime, cutensornet::CuTensorNet, inspect, open, other_copies,
    },
};
#[cfg(all(target_os = "linux", target_env = "gnu"))]
use crate::{
    libraries::ProbedVersions,
    libraries::function_table::{FunctionTable, SymbolResolver, resolve_required},
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
        from_override: false,
    }
}

/// The reason `validate_override_path` gives for refusing `value` as
/// `QDK_CUDART_LIBRARY`, once the fault is checked to be an invalid
/// override whose message names the variable and the path.
fn refusal_reason(value: impl Into<OsString>) -> &'static str {
    let value = value.into();
    let refused = validate_override_path("QDK_CUDART_LIBRARY", Some(value.clone()));
    let Err(
        fault @ LibraryFault::InvalidOverride {
            variable: "QDK_CUDART_LIBRARY",
            path,
            reason,
        },
    ) = &refused
    else {
        panic!("{value:?} is refused as an invalid override: {refused:?}");
    };
    assert_eq!(path.as_os_str(), value, "the fault keeps the path as set");
    let message = fault.to_string();
    for named in ["QDK_CUDART_LIBRARY", &*value.to_string_lossy()] {
        assert!(message.contains(named), "{named} is named: {message}");
    }
    reason
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

/// A user who sets the variable wrong is refused, and the reason says which
/// mistake to fix: a relative path (`libcudart.so.12` would let the system
/// loader pick any copy on its search path), a mistyped path, a folder, a
/// link left behind by an uninstalled library, a path through a file, or a
/// path that exists but cannot be read.
#[test]
fn an_invalid_override_is_refused_with_why() {
    #[cfg_attr(not(unix), expect(unused_mut, reason = "only Unix adds rows"))]
    let mut rows = vec![
        (
            "a relative path",
            PathBuf::from("libcudart.so.12"),
            "absolute",
        ),
        (
            "a mistyped path",
            unique_temp_path("missing-override"),
            "does not exist",
        ),
        ("a folder", std::env::temp_dir(), "file"),
    ];
    #[cfg(unix)]
    let _remove = {
        let dangling = unique_temp_path("dangling-link");
        std::os::unix::fs::symlink(unique_temp_path("removed-target"), &dangling)
            .expect("the temporary directory is writable");
        rows.push(("a dangling link", dangling.clone(), "does not exist"));
        let (file, remove_file) = not_a_shared_object("override-parent-file");
        rows.push((
            "a path through a file",
            file.join("libcudart.so.12"),
            "does not exist",
        ));
        let (looping, locked_dir, guard) = unreadable_paths();
        rows.push(("a symlink loop", looping, "cannot be read"));
        // Root reads a directory whatever its mode, so the row runs only
        // where the mode applies.
        if std::fs::read_dir(&locked_dir).is_err() {
            rows.push((
                "a file in a directory the user cannot search",
                locked_dir.join("libcudart.so.12"),
                "permission denied",
            ));
        }
        (RemoveFile(dangling), remove_file, guard)
    };
    for (case, path, expected) in rows {
        let reason = refusal_reason(path);
        assert!(reason.contains(expected), "{case}: {reason}");
    }
}

/// A user reads from the reason alone which of their mistakes to fix: a
/// relative path, a missing one, one that cannot be read, or a folder.
#[test]
fn each_kind_of_invalid_override_has_its_own_reason() {
    #[cfg_attr(not(unix), expect(unused_mut, reason = "only Unix adds rows"))]
    let mut reasons = vec![
        refusal_reason("libcudart.so.12"),
        refusal_reason(unique_temp_path("missing-override")),
        refusal_reason(std::env::temp_dir()),
    ];
    #[cfg(unix)]
    let _remove = {
        let (looping, _, guard) = unreadable_paths();
        reasons.push(refusal_reason(looping));
        guard
    };
    for (i, reason) in reasons.iter().enumerate() {
        assert!(
            !reasons[..i].contains(reason),
            "{reason:?} is given for two kinds: {reasons:?}"
        );
    }
}

/// Two links naming each other, and a directory with no permissions,
/// removed (the directory's permissions restored first) when the guard is
/// dropped.
#[cfg(unix)]
fn unreadable_paths() -> (PathBuf, PathBuf, RemoveUnreadable) {
    use std::os::unix::fs::PermissionsExt;
    let (first, second) = (unique_temp_path("loop-a"), unique_temp_path("loop-b"));
    std::os::unix::fs::symlink(&second, &first).expect("the temporary directory is writable");
    std::os::unix::fs::symlink(&first, &second).expect("the temporary directory is writable");
    let locked_dir = unique_temp_path("locked-dir");
    std::fs::create_dir(&locked_dir).expect("the temporary directory is writable");
    std::fs::set_permissions(&locked_dir, std::fs::Permissions::from_mode(0o000))
        .expect("the test created the directory");
    let guard = RemoveUnreadable {
        links: [first.clone(), second],
        locked_dir: locked_dir.clone(),
    };
    (first, locked_dir, guard)
}

#[cfg(unix)]
struct RemoveUnreadable {
    links: [PathBuf; 2],
    locked_dir: PathBuf,
}

#[cfg(unix)]
impl Drop for RemoveUnreadable {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        for link in &self.links {
            let _ = std::fs::remove_file(link);
        }
        let _ = std::fs::set_permissions(&self.locked_dir, std::fs::Permissions::from_mode(0o700));
        let _ = std::fs::remove_dir(&self.locked_dir);
    }
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
    let (cudart, _) = open::<CudaRuntime>(read_variable, &packaged);
    let (cutensornet, _) = open::<CuTensorNet>(read_variable, &packaged);
    assert!(
        matches!(&cudart.faults[..], [LibraryFault::LoadFailed { path: tried, .. }] if *tried == path),
        "the CUDA Runtime tries its override: {cudart:?}"
    );
    assert!(
        !cutensornet.from_override && !format!("{cutensornet:?}").contains("override-candidate"),
        "cuTensorNet does not: {cutensornet:?}"
    );
}

/// Without an override, each library is looked for at the paths the README
/// lists, in order.
#[cfg(unix)]
#[test]
fn without_an_override_the_default_paths_are_tried() {
    let packaged = PackagedLibraries::default();
    let cudart = candidates::<CudaRuntime>(None, &packaged);
    let cutensornet = candidates::<CuTensorNet>(None, &packaged);
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

/// A library a package manager installed is tried before the default
/// paths: a user who installed NVIDIA's Python wheels needs no
/// `LD_LIBRARY_PATH`.
#[cfg(unix)]
#[test]
fn a_packaged_library_is_tried_before_the_default_paths() {
    let packaged = PackagedLibraries {
        files: [
            "/env/cuquantum/lib/libcutensornet.so.2",
            "/env/nvidia/cuda_runtime/lib/libcudart.so.12",
        ]
        .map(PathBuf::from)
        .to_vec(),
        install_command: None,
    };
    let cudart = candidates::<CudaRuntime>(None, &packaged);
    let cutensornet = candidates::<CuTensorNet>(None, &packaged);
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

/// A packager's list holds every file it installed, of every library: each
/// library tries only the file of its own name, so the CUDA Runtime's file
/// is not tried as cuTensorNet, and a file of another version of
/// cuTensorNet is not tried at all; of two files of one name, only the
/// first listed is tried.
#[cfg(unix)]
#[test]
fn a_packaged_file_is_tried_only_for_the_library_it_is_named() {
    let packaged = PackagedLibraries {
        files: [
            "/env/nvidia/cuda_runtime/lib/libcudart.so.12",
            "/env/cuquantum/lib/libcutensornet.so.3",
            "/env/torch/lib/libcudart.so.12",
        ]
        .map(PathBuf::from)
        .to_vec(),
        install_command: None,
    };
    let cudart = candidates::<CudaRuntime>(None, &packaged);
    let cutensornet = candidates::<CuTensorNet>(None, &packaged);
    assert_eq!(
        cudart,
        std::iter::once("/env/nvidia/cuda_runtime/lib/libcudart.so.12")
            .chain(CudaRuntime::DEFAULTS.iter().copied())
            .map(PathBuf::from)
            .collect::<Vec<_>>(),
        "the first file of its name, then the default paths"
    );
    assert_eq!(
        cutensornet,
        CuTensorNet::DEFAULTS
            .iter()
            .map(PathBuf::from)
            .collect::<Vec<_>>(),
        "only the default paths are tried"
    );
}

/// A set override is still the only path tried when a package installed
/// the library: the user's setting decides.
#[cfg(unix)]
#[test]
fn an_override_is_tried_alone_even_beside_a_packaged_library() {
    let path = PathBuf::from("/opt/cuda/libcudart.so.12");
    let packaged = PackagedLibraries {
        files: vec![PathBuf::from(
            "/env/nvidia/cuda_runtime/lib/libcudart.so.12",
        )],
        install_command: None,
    };
    let cudart = candidates::<CudaRuntime>(Some(path.clone()), &packaged);
    assert_eq!(cudart, std::slice::from_ref(&path));
}

/// The packaged files `files`, with no install command.
#[cfg(unix)]
fn packaged(files: &[&Path]) -> PackagedLibraries {
    PackagedLibraries {
        files: files.iter().map(|file| file.to_path_buf()).collect(),
        install_command: None,
    }
}

/// The other copies of `L` reported beside `loaded`, among the test's own
/// files (a host's system copies, if any, left out).
#[cfg(unix)]
fn other_test_copies<L: NativeLibrary>(
    loaded: &Path,
    override_path: Option<&Path>,
    packaged: &PackagedLibraries,
) -> Vec<PathBuf> {
    let temporary = unique_temp_path("");
    let directory = temporary
        .parent()
        .expect("a temporary file has a directory");
    other_copies::<L>(loaded, override_path, packaged)
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
    let (wheel, _remove_wheel) = packaged_file::<CudaRuntime>("wheel-copy");
    assert_eq!(
        other_test_copies::<CudaRuntime>(&loaded, Some(&loaded), &packaged(&[&wheel])),
        vec![wheel.clone()]
    );
}

/// A user with no override whose CUDA Runtime was loaded from the wheel
/// has no other copy: the loaded one is not reported as another.
#[cfg(unix)]
#[test]
fn the_loaded_copy_is_no_other_copy() {
    let (wheel, _remove_wheel) = packaged_file::<CudaRuntime>("loaded-wheel-copy");
    assert_eq!(
        other_test_copies::<CudaRuntime>(&wheel, None, &packaged(&[&wheel])),
        Vec::<PathBuf>::new()
    );
}

/// cuTensorNet's other copies are looked for among its own paths only: the
/// CUDA Runtime's wheel is no copy of it.
#[cfg(unix)]
#[test]
fn another_librarys_wheel_is_no_other_copy() {
    let (loaded, _remove_loaded) = not_a_shared_object("loaded-cutensornet");
    let (wheel, _remove_wheel) = packaged_file::<CudaRuntime>("cuda-runtime-wheel");
    assert_eq!(
        other_test_copies::<CuTensorNet>(&loaded, None, &packaged(&[&wheel])),
        Vec::<PathBuf>::new()
    );
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
    let copies = other_copies::<CudaRuntime>(
        &loaded,
        Some(Path::new("Cargo.toml")),
        &PackagedLibraries::default(),
    );
    assert!(!copies.contains(&PathBuf::from("Cargo.toml")));
}

/// Two links in one installation, `libcudart.so.12` naming the file loaded:
/// the link is no other copy.
#[cfg(unix)]
#[test]
fn a_link_to_the_loaded_file_is_no_other_copy() {
    let (loaded, _remove_loaded) = not_a_shared_object("loaded-linked");
    let (link_to_loaded, _remove_link) = packaged_path::<CudaRuntime>("link-to-loaded");
    std::os::unix::fs::symlink(&loaded, &link_to_loaded).expect("a link is made");
    assert_eq!(
        other_test_copies::<CudaRuntime>(&loaded, None, &packaged(&[&link_to_loaded])),
        Vec::<PathBuf>::new()
    );
}

/// A library loaded through a link: the file the link names is no other
/// copy.
#[cfg(unix)]
#[test]
fn the_file_a_loaded_link_names_is_no_other_copy() {
    let (file, _remove_file) = packaged_file::<CudaRuntime>("linked-file");
    let loaded_link = unique_temp_path("loaded-link");
    std::os::unix::fs::symlink(&file, &loaded_link).expect("a link is made");
    let _remove_loaded_link = RemoveFile(loaded_link.clone());
    assert_eq!(
        other_test_copies::<CudaRuntime>(&loaded_link, None, &packaged(&[&file])),
        Vec::<PathBuf>::new()
    );
}

/// One other file named twice, by the override and by a link to it, is
/// reported once, under the first name.
#[cfg(unix)]
#[test]
fn one_file_named_twice_is_reported_once() {
    let (loaded, _remove_loaded) = not_a_shared_object("loaded-beside-twice-named");
    let (other, _remove_other) = not_a_shared_object("other-named-twice");
    let (link_to_other, _remove_link) = packaged_path::<CudaRuntime>("link-to-other");
    std::os::unix::fs::symlink(&other, &link_to_other).expect("a link is made");
    assert_eq!(
        other_test_copies::<CudaRuntime>(&loaded, Some(&other), &packaged(&[&link_to_other])),
        vec![other.clone()]
    );
}

/// cuTensorNet's state API (the state, its network operators and their
/// expectations), which the crate does not use: an installation without it
/// is not at fault.
#[cfg(unix)]
const STATE_API: &[&str] = &[
    "cutensornetCreateState",
    "cutensornetDestroyState",
    "cutensornetStateApplyTensorOperator",
    "cutensornetStateCaptureMPS",
    "cutensornetStateCompute",
    "cutensornetStateConfigure",
    "cutensornetStateFinalizeMPS",
    "cutensornetStateInitializeMPS",
    "cutensornetStatePrepare",
    "cutensornetCreateNetworkOperator",
    "cutensornetDestroyNetworkOperator",
    "cutensornetNetworkOperatorAppendProduct",
    "cutensornetCreateExpectation",
    "cutensornetDestroyExpectation",
    "cutensornetExpectationCompute",
    "cutensornetExpectationConfigure",
    "cutensornetExpectationPrepare",
];

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
        .without("cutensornetGateSplit")
        .without("cutensornetTensorQR")
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
                symbol: "cutensornetGateSplit"
            },
            LibraryFault::MissingRequiredSymbol {
                symbol: "cutensornetTensorQR"
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

/// Checks a resolver's functions as one library's, and returns its report.
#[cfg(unix)]
type Inspect = fn(&StandInResolver) -> LibraryReport;

/// The report of `L` once `resolver`'s functions are checked as `L`'s.
#[cfg(unix)]
fn inspected<L: NativeLibrary>(resolver: &StandInResolver) -> LibraryReport {
    let mut report = opened::<L>();
    let functions = inspect::<L, _>(resolver, &mut report);
    assert!(functions.is_none(), "a required function is missing");
    report
}

/// A library without one of its version functions has that version not
/// read, and the missing function is its fault; every other version is
/// still read and checked, so a wrong one is reported beside it, also when
/// the CUDA Runtime lacks `cudaGetErrorString`, which only words a failed
/// read.
#[cfg(unix)]
#[test]
fn a_missing_version_function_hides_only_its_own_version() {
    /// A library checked with some of its functions withheld or replaced,
    /// the faults it must report, a version it must leave unread, and one
    /// it must still read.
    struct Case {
        inspected: Inspect,
        resolver: StandInResolver,
        faults: Vec<LibraryFault>,
        unread: Option<&'static str>,
        read: &'static str,
    }
    let missing = |symbol| LibraryFault::MissingRequiredSymbol { symbol };
    let unsupported = |component, found, supported| LibraryFault::UnsupportedVersion {
        component,
        found,
        supported,
    };
    let cases = [
        Case {
            inspected: inspected::<CuTensorNet>,
            resolver: StandInResolver::audited().without("cutensornetGetVersion"),
            faults: vec![missing("cutensornetGetVersion")],
            unread: Some("cuTensorNet"),
            read: "cuTensorNet's CUDA Runtime",
        },
        Case {
            inspected: inspected::<CuTensorNet>,
            resolver: StandInResolver::audited()
                .without("cutensornetGetVersion")
                .with(
                    "cutensornetGetCudartVersion",
                    cutensornet_version::<12_080> as *const (),
                ),
            faults: vec![
                missing("cutensornetGetVersion"),
                unsupported("cuTensorNet's CUDA Runtime", 12_080, 12_090),
            ],
            unread: Some("cuTensorNet"),
            read: "cuTensorNet's CUDA Runtime",
        },
        Case {
            inspected: inspected::<CuTensorNet>,
            resolver: StandInResolver::audited()
                .without("cutensornetGetCudartVersion")
                .with(
                    "cutensornetGetVersion",
                    cutensornet_version::<21_400> as *const (),
                ),
            faults: vec![
                missing("cutensornetGetCudartVersion"),
                unsupported("cuTensorNet", 21_400, 21_300),
            ],
            unread: Some("cuTensorNet's CUDA Runtime"),
            read: "cuTensorNet",
        },
        Case {
            inspected: inspected::<CudaRuntime>,
            resolver: StandInResolver::audited().without("cudaRuntimeGetVersion"),
            faults: vec![missing("cudaRuntimeGetVersion")],
            unread: Some("CUDA Runtime"),
            read: "CUDA driver",
        },
        Case {
            inspected: inspected::<CudaRuntime>,
            resolver: StandInResolver::audited()
                .without("cudaDriverGetVersion")
                .with("cudaRuntimeGetVersion", cuda_version::<13_000> as *const ()),
            faults: vec![
                missing("cudaDriverGetVersion"),
                unsupported("CUDA Runtime", 13_000, 12_090),
            ],
            unread: Some("CUDA driver"),
            read: "CUDA Runtime",
        },
        Case {
            inspected: inspected::<CudaRuntime>,
            resolver: StandInResolver::audited()
                .without("cudaGetErrorString")
                .with("cudaRuntimeGetVersion", cuda_version::<13_000> as *const ()),
            faults: vec![
                missing("cudaGetErrorString"),
                unsupported("CUDA Runtime", 13_000, 12_090),
            ],
            unread: None,
            read: "CUDA driver",
        },
    ];
    for Case {
        inspected,
        resolver,
        faults,
        unread,
        read,
    } in cases
    {
        let report = inspected(&resolver);
        assert_eq!(report.faults, faults);
        if let Some(unread) = unread {
            assert_eq!(report.version(unread), None);
        }
        assert!(report.version(read).is_some(), "{read} is read");
    }
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
    const FILE_NAME: &'static str = "libc.so.6";
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
    const FILE_NAME: &'static str = "libc.so.6";
    const DEFAULTS: &'static [&'static str] = &[ALWAYS_PRESENT_LIBRARY, "/bin/sh"];
    type Functions = AbsentFunctionTable;

    fn probe_versions<R: SymbolResolver>(_resolver: &R) -> ProbedVersions {
        ProbedVersions::default()
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

/// The copy its override names is reported so, loaded or not, for the
/// setup check to advise changing or unsetting the variable when that copy
/// is at fault, even if the variable changes during the check (another
/// thread unsets it); a copy from its defaults, or none when its override
/// is invalid, is not.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[test]
fn only_the_copy_its_override_names_is_reported_so() {
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
    assert!(from_override.from_override);
    let (from_defaults, _) =
        open::<OpenedBesideAnotherFile>(|_| None, &PackagedLibraries::default());
    assert!(from_defaults.path.is_some(), "a default path is loaded");
    assert!(!from_defaults.from_override);
    let relative = |_: &'static str| Some(OsString::from("libc.so.6"));
    let (not_loaded, _) = open::<OpenedBesideAnotherFile>(relative, &PackagedLibraries::default());
    assert!(not_loaded.path.is_none(), "a relative override is refused");
    assert!(!not_loaded.from_override);
    let (not_a_library, _remove) = packaged_file::<OpenedBesideAnotherFile>("override-fails");
    let names_it = |_: &'static str| Some(OsString::from(&not_a_library));
    let (failed_to_load, _) =
        open::<OpenedBesideAnotherFile>(names_it, &PackagedLibraries::default());
    assert!(
        matches!(failed_to_load.faults[..], [LibraryFault::LoadFailed { .. }]),
        "a file that is not a library does not load: {:?}",
        failed_to_load.faults
    );
    assert!(failed_to_load.from_override);
    let reads = std::cell::Cell::new(0);
    let unset_after_one_read = |_: &'static str| {
        reads.set(reads.get() + 1);
        (reads.get() == 1).then(|| OsString::from(&not_a_library))
    };
    let (unset_later, _) =
        open::<OpenedBesideAnotherFile>(unset_after_one_read, &PackagedLibraries::default());
    assert!(
        matches!(&unset_later.faults[..], [LibraryFault::LoadFailed { path, .. }] if *path == not_a_library),
        "the copy the override named is tried: {unset_later:?}"
    );
    assert!(unset_later.from_override, "{unset_later:?}");
}

/// The setup check has advice for every library a GPU run opens, so none
/// is left out of what it shows and advises.
#[cfg(unix)]
#[test]
fn every_library_discovery_opens_has_setup_advice() {
    let mut guided: Vec<_> = LoadedLibraries::SETUP_GUIDES
        .iter()
        .map(|guide| guide.library)
        .collect();
    let packaged = PackagedLibraries::default();
    let mut opening = Opening::new(InstalledLibraries {
        read_variable: |_: &'static str| Some(OsString::from("relative.so")),
        packaged: &packaged,
    });
    assert!(
        LoadedLibraries::open(&mut opening).is_none(),
        "a relative override makes every library unusable"
    );
    let mut opened: Vec<_> = opening
        .into_reports()
        .iter()
        .map(|report| report.library)
        .collect();
    guided.sort_unstable();
    opened.sort_unstable();
    assert_eq!(guided, opened);
}
