// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The rules every library shares, shown on cuTensorNet and the CUDA
//! Runtime: how its paths are chosen (the override variable, checked before
//! anything is loaded, or else the library's defaults), and how an open
//! library is checked and reported. Each library's own rules (its table,
//! probes and versions) are tested in its module's file.

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
use super::{RemoveFile, not_a_shared_object};
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
    libraries::function_table::{FunctionTable, SymbolResolver, resolve_required},
    libraries::{ProbedVersions, open},
};
use std::ffi::OsString;
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
/// link left behind by an uninstalled library, or a path that exists but
/// cannot be read.
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
        let (looping, locked_dir, guard) = unreadable_overrides();
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
        (RemoveFile(dangling), guard)
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
        let (looping, _, guard) = unreadable_overrides();
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
fn unreadable_overrides() -> (PathBuf, PathBuf, RemoveUnreadable) {
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
