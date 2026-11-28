// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The rules every library shares, shown on cuTensorNet and the CUDA
//! Runtime. They are how its paths are chosen (the override variable,
//! checked before anything is loaded, or else the library's packaged file,
//! then its defaults), and how an open library is checked and reported
//! with the other copies beside it. Each library's own rules (its table,
//! probes and versions) are tested in its module's file.

#[cfg(all(target_os = "linux", target_env = "gnu"))]
use crate::test_utils::stand_ins::ALWAYS_PRESENT_LIBRARY;
#[cfg(unix)]
use crate::test_utils::stand_ins::{StandInResolver, cuda_version, cutensornet_version};
use crate::test_utils::unique_temp_path;
#[cfg(unix)]
use crate::test_utils::{RemoveFile, not_a_shared_object, packaged_file, packaged_path};
use crate::{LibraryFault, libraries::validate_override_path};
#[cfg(unix)]
use crate::{
    LibraryReport, PackagedLibraries, Version,
    libraries::{
        NativeLibrary, candidates, cuda_runtime::CudaRuntime, cutensornet::CuTensorNet, inspect,
        other_copies, override_of,
    },
};
#[cfg(all(target_os = "linux", target_env = "gnu"))]
use crate::{
    LibrarySource,
    libraries::function_table::{FunctionTable, SymbolResolver, resolve_required},
    libraries::{ProbedVersions, open},
};
use std::ffi::OsString;
#[cfg(unix)]
use std::path::Path;
use std::path::PathBuf;

/// An empty report of `L`, as discovery starts it once `L` is open.
#[cfg(unix)]
fn opened<L: NativeLibrary>() -> LibraryReport {
    LibraryReport {
        name: L::NAME,
        path: Some(PathBuf::from("/fixture/library.so")),
        versions: Vec::new(),
        faults: Vec::new(),
        other_copies: Vec::new(),
        source: None,
    }
}

/// The reason `validate_override_path` gives for refusing `value` as
/// `QDK_CUDART_LIBRARY`, once the fault is checked to be an invalid
/// override whose message names the variable and the path as written (on
/// Windows, with its single backslashes).
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
    for named in ["QDK_CUDART_LIBRARY", &*path.display().to_string()] {
        assert!(message.contains(named), "{named} is named: {message}");
    }
    reason
}

/// A user sets the variable to the library's path in their installation,
/// or to a link to it: installations name a library by a link to its
/// versioned file (`libcudart.so.12` is a link). The path is kept as set.
#[test]
fn an_absolute_path_to_a_file_or_a_link_to_one_is_the_override() {
    let file = unique_temp_path("override-file");
    std::fs::write(&file, b"").expect("the temporary directory is writable");
    #[cfg_attr(not(unix), expect(unused_mut, reason = "only Unix adds rows"))]
    let mut overrides = vec![file.clone()];
    #[cfg(unix)]
    let link = {
        let link = unique_temp_path("link-to-file");
        std::os::unix::fs::symlink(&file, &link).expect("the temporary directory is writable");
        overrides.push(link.clone());
        link
    };
    let found: Vec<_> = overrides
        .iter()
        .map(|path| validate_override_path("QDK_CUDART_LIBRARY", Some(path.clone().into())))
        .collect();
    #[cfg(unix)]
    std::fs::remove_file(&link).expect("the test created the link");
    std::fs::remove_file(&file).expect("the test created the file");
    for (path, found) in overrides.into_iter().zip(found) {
        assert_eq!(found.expect("the path names a file"), Some(path));
    }
}

/// A user who sets the variable wrong is refused, and the reason alone says
/// which mistake to fix: a relative path (`libcudart.so.12` would let the
/// system loader pick any copy on its search path), a mistyped path, a
/// folder, a link left behind by an uninstalled library, a path through a
/// file, or a path that exists but cannot be read. Each kind of mistake has
/// its own reason. The mistakes that leave no file there share one.
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
    let mut reasons = Vec::new();
    for (case, path, expected) in rows {
        let reason = refusal_reason(path);
        assert!(reason.contains(expected), "{case}: {reason}");
        reasons.push((expected, reason));
    }
    for (i, (kind, reason)) in reasons.iter().enumerate() {
        for (other_kind, other_reason) in &reasons[..i] {
            assert!(
                kind == other_kind || reason != other_reason,
                "{reason:?} is given for two kinds of mistake: {reasons:?}"
            );
        }
    }
}

/// Two links naming each other, and a directory with no permissions,
/// removed (the directory's permissions restored first) when the guard is
/// dropped.
#[cfg(unix)]
pub(super) fn unreadable_paths() -> (PathBuf, PathBuf, RemoveUnreadable) {
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
pub(super) struct RemoveUnreadable {
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

/// A user who sets `QDK_CUDART_LIBRARY` changes where the CUDA Runtime is
/// loaded from, and only that: cuTensorNet is still looked for at its
/// defaults.
#[cfg(unix)]
#[test]
fn each_library_is_overridden_by_its_own_variable() {
    let (path, _remove) = not_a_shared_object("override-candidate");
    let read_variable =
        |variable: &'static str| (variable == "QDK_CUDART_LIBRARY").then(|| OsString::from(&path));
    assert_eq!(
        override_of::<CudaRuntime>(read_variable),
        Ok(Some(path.clone())),
        "the CUDA Runtime reads its override"
    );
    assert_eq!(
        override_of::<CuTensorNet>(read_variable),
        Ok(None),
        "cuTensorNet does not"
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

/// A packaged file is tried before the default paths. So a user who
/// installed NVIDIA's Python wheels needs no `LD_LIBRARY_PATH`. A packager's
/// list holds every file it installed, of every library. Each library tries
/// only the file of its own name: the CUDA Runtime's file is not tried as
/// cuTensorNet, and a file of another version of cuTensorNet is not tried
/// at all. Of two files of one name, only the first listed is tried.
#[cfg(unix)]
#[test]
fn a_packaged_file_is_tried_before_the_defaults_only_for_the_library_it_is_named() {
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

/// A path that names the loaded file, through a link or not, or a file of
/// another library, is no other copy. A user with no override is told of
/// none when the CUDA Runtime was loaded from the wheel, when the wheel
/// holds a link to the file loaded, or when it was loaded through a link to
/// the wheel's file. cuTensorNet's other copies are looked for among its
/// own paths only, so the CUDA Runtime's wheel is none. Each row is a scene
/// and the other copies reported in it.
#[cfg(unix)]
#[test]
fn a_path_to_the_loaded_file_or_another_library_is_no_other_copy() {
    let (wheel, _remove_wheel) = packaged_file::<CudaRuntime>("loaded-wheel-copy");
    let (cutensornet, _remove_cutensornet) = not_a_shared_object("loaded-cutensornet");
    let (loaded, _remove_loaded) = not_a_shared_object("loaded-linked");
    let (link_to_loaded, _remove_link) = packaged_path::<CudaRuntime>("link-to-loaded");
    std::os::unix::fs::symlink(&loaded, &link_to_loaded).expect("a link is made");
    let loaded_link = unique_temp_path("loaded-link");
    std::os::unix::fs::symlink(&wheel, &loaded_link).expect("a link is made");
    let _remove_loaded_link = RemoveFile(loaded_link.clone());
    for (scene, copies) in [
        (
            "the wheel's copy loaded",
            other_test_copies::<CudaRuntime>(&wheel, None, &packaged(&[&wheel])),
        ),
        (
            "a link in the wheel to the file loaded",
            other_test_copies::<CudaRuntime>(&loaded, None, &packaged(&[&link_to_loaded])),
        ),
        (
            "the wheel's file loaded through a link",
            other_test_copies::<CudaRuntime>(&loaded_link, None, &packaged(&[&wheel])),
        ),
        (
            "cuTensorNet beside the CUDA Runtime's wheel",
            other_test_copies::<CuTensorNet>(&cutensornet, None, &packaged(&[&wheel])),
        ),
    ] {
        assert_eq!(copies, Vec::<PathBuf>::new(), "{scene}");
    }
}

/// A bare file name, such as the default `libcudart.so.12`, is searched for
/// by the system loader, not in the working directory. So a file of that
/// name there is no other copy. The defaults are fixed, so the test names a
/// bare file through the override instead (which discovery refuses before
/// loading anything). Tests run in the crate's directory, which holds a
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

/// A library at fault is not bound, and every fault is reported, so the
/// user learns the cause beside the symptom. A cuTensorNet 2.14 that lacks
/// functions the crate binds reports both the missing functions and its
/// version, the cause. One with every function but that version is refused
/// all the same.
#[cfg(unix)]
#[test]
fn a_library_at_fault_is_not_bound_and_reports_every_fault() {
    let cutensornet_2_14 = || {
        StandInResolver::audited().with(
            "cutensornetGetVersion",
            cutensornet_version::<21_400> as *const (),
        )
    };
    let missing = |symbol| LibraryFault::MissingRequiredSymbol { symbol };
    let unsupported = LibraryFault::UnsupportedVersion {
        component: "cuTensorNet",
        found: 21_400,
        supported: 21_300,
    };
    for (resolver, faults) in [
        (
            cutensornet_2_14()
                .without("cutensornetGateSplit")
                .without("cutensornetTensorQR"),
            vec![
                missing("cutensornetGateSplit"),
                missing("cutensornetTensorQR"),
                unsupported.clone(),
            ],
        ),
        (cutensornet_2_14(), vec![unsupported]),
    ] {
        let mut report = opened::<CuTensorNet>();
        let functions = inspect::<CuTensorNet, _>(&resolver, &mut report);
        assert!(functions.is_none(), "the library is at fault: {faults:?}");
        assert_eq!(report.faults, faults);
        assert_eq!(report.version("cuTensorNet"), Some(21_400));
    }
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
/// read, and the missing function is its fault. Every other version is
/// still read and checked, so a wrong one is reported beside it. That holds
/// also when the CUDA Runtime lacks `cudaGetErrorString`, which only words
/// a failed read.
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

/// A library whose one default path is an absolute file, the shell, which
/// the system loader refuses, as it is no library.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
struct RefusedAtItsDefaultPath;

#[cfg(all(target_os = "linux", target_env = "gnu"))]
impl NativeLibrary for RefusedAtItsDefaultPath {
    const NAME: &'static str = "shell";
    const VARIABLE: &'static str = "QDK_CUTENSORNET_TEST_UNSET_VARIABLE";
    const FILE_NAME: &'static str = "sh";
    const DEFAULTS: &'static [&'static str] = &["/bin/sh"];
    type Functions = AbsentFunctionTable;

    fn probe_versions<R: SymbolResolver>(_resolver: &R) -> ProbedVersions {
        ProbedVersions::default()
    }
}

/// Each library reports where the copy tried, loaded or refused by the
/// system loader, was found: the path its override names, even if the
/// variable changes during the check (another thread unsets it), its
/// packaged file, a default absolute path, or a file name the system
/// loader searched for. It reports no source when no copy was tried.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[test]
fn each_library_reports_where_the_copy_tried_was_found() {
    let no_package = PackagedLibraries::default();
    let c_library = absolute_c_library();
    let names_c_library = |variable: &'static str| {
        (variable == OpenedBesideAnotherFile::VARIABLE).then(|| OsString::from(&c_library))
    };
    let (not_a_library, _remove) = packaged_file::<OpenedBesideAnotherFile>("source-of-copy");
    let names_not_a_library = |_: &'static str| Some(OsString::from(&not_a_library));
    let reads = std::cell::Cell::new(0);
    let unset_after_one_read = |_: &'static str| {
        reads.set(reads.get() + 1);
        (reads.get() == 1).then(|| OsString::from(&not_a_library))
    };
    let relative = |_: &'static str| Some(OsString::from("libc.so.6"));
    let packaged = PackagedLibraries {
        files: vec![not_a_library.clone()],
        install_command: None,
    };
    let open_c = |read_variable: &dyn Fn(&'static str) -> Option<OsString>,
                  packaged: &PackagedLibraries| {
        open::<OpenedBesideAnotherFile>(read_variable, packaged).0
    };
    for (case, report, tried, source) in [
        (
            "an override loaded",
            open_c(&names_c_library, &no_package),
            Some(c_library.clone()),
            Some(LibrarySource::Override),
        ),
        (
            "an override refused",
            open_c(&names_not_a_library, &no_package),
            Some(not_a_library.clone()),
            Some(LibrarySource::Override),
        ),
        (
            "an override unset during the check",
            open_c(&unset_after_one_read, &no_package),
            Some(not_a_library.clone()),
            Some(LibrarySource::Override),
        ),
        (
            "a packaged file refused",
            open_c(&|_| None, &packaged),
            Some(not_a_library.clone()),
            Some(LibrarySource::Packaged),
        ),
        (
            "a default absolute path refused",
            open::<RefusedAtItsDefaultPath>(|_| None, &no_package).0,
            Some(PathBuf::from("/bin/sh")),
            Some(LibrarySource::DefaultPath),
        ),
        (
            "a file name the system loader found",
            open_c(&|_| None, &no_package),
            Some(PathBuf::from(ALWAYS_PRESENT_LIBRARY)),
            Some(LibrarySource::SystemSearch),
        ),
        (
            "an invalid override",
            open_c(&relative, &no_package),
            None,
            None,
        ),
    ] {
        let tried_path = report.path.clone().or_else(|| {
            report.faults.iter().find_map(|fault| match fault {
                LibraryFault::LoadFailed { path, .. } => Some(path.clone()),
                _ => None,
            })
        });
        assert_eq!(tried_path, tried, "{case}: the copy tried: {report:?}");
        assert_eq!(report.source, source, "{case}: {report:?}");
    }
}
