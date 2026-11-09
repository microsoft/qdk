// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! How the loader finds and opens a library, looks up its functions and
//! probes its version, and what it reports when one of these fails.
//!
//! No NVIDIA library is opened: a resolver hands out a stand-in pointer for
//! every name but the one a test withholds, and the libraries are files
//! that are not shared objects, or paths and names that do not exist.

use super::unique_temp_path;
use crate::AvailabilityError;
use crate::library::{
    CUDA_RUNTIME, CUTENSORNET, Candidates, SymbolResolver, load_library, no_file_of_that_name,
    probe_cuda_version,
    symbols::{CuTensorNetFunctions, CudaFunctions},
};
use std::{
    ffi::{OsString, c_char},
    fs,
    mem::{size_of, transmute_copy},
    path::{Path, PathBuf},
};

/// The CUDA Runtime table's rows, as `(field, symbol)`: every function of
/// `cudart_12.rs`, each required.
const CUDA_RUNTIME_ROWS: &[(&str, &str)] = &[
    ("runtime_get_version", "cudaRuntimeGetVersion"),
    ("driver_get_version", "cudaDriverGetVersion"),
    ("get_device_count", "cudaGetDeviceCount"),
    ("set_device", "cudaSetDevice"),
    ("device_get_attribute", "cudaDeviceGetAttribute"),
    ("get_error_string", "cudaGetErrorString"),
    ("mem_get_info", "cudaMemGetInfo"),
    ("malloc", "cudaMalloc"),
    ("free", "cudaFree"),
    ("memcpy", "cudaMemcpy"),
    ("stream_create_with_flags", "cudaStreamCreateWithFlags"),
    ("stream_synchronize", "cudaStreamSynchronize"),
    ("stream_destroy", "cudaStreamDestroy"),
];

const MANIFEST: &str = include_str!("../../scripts/cutensornet-symbols.txt");

/// The manifest's rows, as `(field, symbol, required)`.
fn manifest() -> Vec<(&'static str, &'static str, bool)> {
    MANIFEST
        .lines()
        .filter(|line| !line.trim_start().starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let columns: Vec<&str> = line.split_whitespace().collect();
            assert_eq!(columns.len(), 3, "a manifest row has three columns: {line}");
            let required = match columns[2] {
                "required" => true,
                "optional" => false,
                other => panic!("a row is required or optional, not {other}"),
            };
            (columns[1], columns[0], required)
        })
        .collect()
}

fn sorted<T: Ord>(mut rows: Vec<T>) -> Vec<T> {
    rows.sort_unstable();
    rows
}

unsafe extern "C" fn stand_in_function() {}

/// Resolves every name to a stand-in pointer, except the one withheld.
struct StandInResolver {
    missing: Option<&'static str>,
}

impl StandInResolver {
    fn without(symbol: &'static str) -> Self {
        Self {
            missing: Some(symbol),
        }
    }
}

impl SymbolResolver for StandInResolver {
    unsafe fn resolve<T: Copy>(&self, symbol: &'static str) -> Option<T> {
        if self.missing == Some(symbol) {
            return None;
        }
        let pointer = stand_in_function as *const ();
        assert_eq!(size_of::<T>(), size_of::<*const ()>());
        // SAFETY: the tests never call the pointer; every table field is a
        // pointer-sized function pointer.
        Some(unsafe { transmute_copy(&pointer) })
    }
}

/// Removes a file the test created, even when the test fails.
struct RemoveFile(PathBuf);

impl Drop for RemoveFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn not_a_shared_object(label: &str) -> (PathBuf, RemoveFile) {
    let path = unique_temp_path(label);
    fs::write(&path, b"not an ELF shared object").expect("the temporary directory is writable");
    (path.clone(), RemoveFile(path))
}

/// A file name that no system loader finds.
fn absent_bare_name() -> PathBuf {
    PathBuf::from(format!(
        "libqdk-cutensornet-test-absent-{}.so.0",
        std::process::id()
    ))
}

/// The manifest is the list of cuTensorNet functions the engine may call:
/// the table has a field for each of them, looked up by its symbol, and no
/// other.
#[test]
fn the_cutensornet_table_has_the_manifests_rows() {
    assert_eq!(
        sorted(CuTensorNetFunctions::ROWS.to_vec()),
        sorted(manifest())
    );
}

/// As for cuTensorNet, the CUDA Runtime table looks each field up by its
/// own symbol: two functions of one signature cannot be swapped.
#[test]
fn the_cuda_runtime_table_looks_each_field_up_by_its_symbol() {
    let expected = CUDA_RUNTIME_ROWS
        .iter()
        .map(|&(field, symbol)| (field, symbol, true))
        .collect();
    assert_eq!(sorted(CudaFunctions::ROWS.to_vec()), sorted(expected));
}

/// An installation whose cuTensorNet lacks a required function is refused,
/// naming the function and the library's path.
#[test]
fn a_missing_required_cutensornet_function_is_reported() {
    for (_, missing, _) in manifest().into_iter().filter(|row| row.2) {
        let error = CuTensorNetFunctions::resolve(
            &StandInResolver::without(missing),
            Path::new("/fixture/libcutensornet.so.2"),
        )
        .err()
        .expect("a required function is missing");
        assert!(matches!(
            error,
            AvailabilityError::MissingRequiredSymbol { library: "cuTensorNet", path, symbol }
                if path == Path::new("/fixture/libcutensornet.so.2") && symbol == missing
        ));
    }
}

/// As for cuTensorNet, for each required function of the CUDA Runtime.
#[test]
fn a_missing_required_cuda_function_is_reported() {
    for &(_, missing) in CUDA_RUNTIME_ROWS {
        let error = CudaFunctions::resolve(
            &StandInResolver::without(missing),
            Path::new("/fixture/libcudart.so.12"),
        )
        .err()
        .expect("a required function is missing");
        assert!(matches!(
            error,
            AvailabilityError::MissingRequiredSymbol { library: "CUDA Runtime", path, symbol }
                if path == Path::new("/fixture/libcudart.so.12") && symbol == missing
        ));
    }
}

/// `cutensornetGetLastError` only adds detail to native error messages, so a
/// library without it is still used.
#[test]
fn a_missing_optional_function_is_absent_from_the_table() {
    let functions = CuTensorNetFunctions::resolve(
        &StandInResolver::without("cutensornetGetLastError"),
        Path::new("/fixture/libcutensornet.so.2"),
    )
    .expect("an optional function is not required");
    assert!(functions.get_last_error.is_none());
}

/// A user who sets `QDK_CUDART_LIBRARY` changes where the CUDA Runtime is
/// loaded from, and only that: cuTensorNet is still looked for at its
/// defaults.
#[test]
fn each_library_is_overridden_by_its_own_variable() {
    let (path, _remove) = not_a_shared_object("override-candidate");
    let read_variable =
        |variable: &'static str| (variable == "QDK_CUDART_LIBRARY").then(|| OsString::from(&path));
    let cudart = CUDA_RUNTIME
        .candidates(read_variable)
        .expect("the override names a file");
    let cutensornet = CUTENSORNET
        .candidates(read_variable)
        .expect("no cuTensorNet override is set");
    assert_eq!(cudart.paths, std::slice::from_ref(&path));
    assert!(!cutensornet.paths.contains(&path));
}

/// Without an override, each library is looked for at the paths the README
/// lists, in order.
#[test]
fn without_an_override_the_default_paths_are_tried() {
    let cudart = CUDA_RUNTIME
        .candidates(|_| None)
        .expect("no override is set");
    let cutensornet = CUTENSORNET
        .candidates(|_| None)
        .expect("no override is set");
    assert_eq!(
        cudart.paths,
        [
            "/usr/local/cuda-12.9/targets/x86_64-linux/lib/libcudart.so.12",
            "libcudart.so.12"
        ]
        .map(PathBuf::from)
    );
    assert_eq!(
        cutensornet.paths,
        [
            "/usr/lib/x86_64-linux-gnu/libcuquantum/12/libcutensornet.so.2",
            "libcutensornet.so.2"
        ]
        .map(PathBuf::from)
    );
}

/// A file that the system loader refuses (here, not a shared object; on a
/// real host, also a library whose own dependencies are missing) is
/// reported with its path and the loader's message.
#[test]
fn a_library_the_system_loader_refuses_is_a_load_failure() {
    let (path, _remove) = not_a_shared_object("invalid-library");
    let error = load_library(Candidates {
        library: &CUDA_RUNTIME,
        paths: vec![path.clone()],
    })
    .err()
    .expect("the file is not a shared object");
    assert!(matches!(
        error,
        AvailabilityError::LoadFailed { library: "CUDA Runtime", path: found, message }
            if found == path && !message.is_empty()
    ));
}

/// A default path that does not exist on this host is skipped for the next
/// one, and the next one's failure is reported.
#[test]
fn a_default_path_that_does_not_exist_is_skipped() {
    let (path, _remove) = not_a_shared_object("second-default");
    let error = load_library(Candidates {
        library: &CUDA_RUNTIME,
        paths: vec![unique_temp_path("absent-default"), path.clone()],
    })
    .err()
    .expect("the second default is not a shared object");
    assert!(matches!(
        error,
        AvailabilityError::LoadFailed { path: found, .. } if found == path
    ));
}

/// A default file name the system loader does not find on its search path
/// is skipped for the next default, and the next one's failure is reported.
#[test]
fn a_file_name_the_system_loader_does_not_find_is_skipped() {
    let (path, _remove) = not_a_shared_object("after-bare-name");
    let error = load_library(Candidates {
        library: &CUDA_RUNTIME,
        paths: vec![absent_bare_name(), path.clone()],
    })
    .err()
    .expect("the second default is not a shared object");
    assert!(matches!(
        error,
        AvailabilityError::LoadFailed { path: found, .. } if found == path
    ));
}

/// The error when the library is not installed: every path tried, in order,
/// file names included.
#[test]
fn a_library_found_nowhere_reports_every_path_tried() {
    let paths = vec![unique_temp_path("absent-library"), absent_bare_name()];
    let error = load_library(Candidates {
        library: &CUTENSORNET,
        paths: paths.clone(),
    })
    .err()
    .expect("no candidate exists");
    assert!(matches!(
        error,
        AvailabilityError::LibraryNotFound { library: "cuTensorNet", attempted }
            if attempted == paths
    ));
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

static CUDA_ERROR_MESSAGE: &[u8] = b"simulated CUDA version failure\0";

unsafe extern "C" fn failing_version_probe(_version: *mut i32) -> u32 {
    42
}

unsafe extern "C" fn cuda_error_string(_status: u32) -> *const c_char {
    CUDA_ERROR_MESSAGE.as_ptr().cast()
}

/// A CUDA version call that fails is reported with CUDA's status and its
/// text.
#[test]
fn a_failed_version_probe_keeps_cudas_status_and_message() {
    let error = probe_cuda_version("CUDA Runtime", failing_version_probe, cuda_error_string)
        .expect_err("the probe returns status 42");
    assert!(matches!(
        error,
        AvailabilityError::VersionProbeFailed {
            component: "CUDA Runtime",
            status: 42,
            message,
        } if message == "simulated CUDA version failure"
    ));
}
