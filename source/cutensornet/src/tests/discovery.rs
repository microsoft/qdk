// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What [`discover`](crate::discover) promises its callers: where it
//! refuses to run, that it checks every library, that it succeeds only when
//! every library is usable, and that its result can be shared. How a caller
//! reads the report, and the error's message, are tested in their modules'
//! files.

mod error;
mod report;

use crate::{Availability, AvailabilityError};
#[cfg(unix)]
use crate::{LibraryFault, discovery::discover_from};
#[cfg(all(target_os = "linux", target_env = "gnu"))]
use crate::{
    LibraryReport,
    discovery::availability_of,
    libraries::loader::FunctionTable,
    libraries::{BoundLibrary, CheckLibrary, Checked, NativeLibrary},
    tests::stand_ins::{StandInResolver, always_present_library},
};
#[cfg(unix)]
use std::ffi::OsString;
#[cfg(all(target_os = "linux", target_env = "gnu"))]
use std::path::PathBuf;

fn assert_send_sync<T: Send + Sync>() {}

/// A caller discovers once and hands the result to the threads that run
/// shots.
#[test]
fn availability_can_be_shared_across_threads() {
    assert_send_sync::<Availability>();
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

/// A user whose CUDA Runtime override names a file that is not a library,
/// and whose cuTensorNet override is a relative path, learns of both at
/// once: each library is checked whatever the other's faults.
#[cfg(unix)]
#[test]
fn every_library_is_checked_and_reported() {
    let (cudart, _remove) = super::not_a_shared_object("discover-cudart");
    let read_variable = |variable: &str| match variable {
        "QDK_CUDART_LIBRARY" => Some(OsString::from(&cudart)),
        "QDK_CUTENSORNET_LIBRARY" => Some(OsString::from("libcutensornet.so.2")),
        _ => None,
    };
    let Err(AvailabilityError::Unusable(report)) = discover_from(read_variable) else {
        panic!("neither library can be used");
    };
    let libraries: Vec<_> = report.libraries.iter().map(|l| l.library).collect();
    assert_eq!(libraries, ["CUDA Runtime", "cuTensorNet"]);
    assert!(matches!(
        &report.libraries[0].faults[..],
        [LibraryFault::LoadFailed { path, .. }] if *path == cudart
    ));
    assert!(matches!(
        &report.libraries[1].faults[..],
        [LibraryFault::InvalidOverride {
            variable: "QDK_CUTENSORNET_LIBRARY",
            ..
        }]
    ));
}

/// A usable library: the always-present library, bound to a table of
/// stand-in functions.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn usable<L: NativeLibrary>() -> Checked<L> {
    let library = always_present_library();
    let report = LibraryReport {
        library: L::NAME,
        path: Some(library.path().to_path_buf()),
        versions: Vec::new(),
        faults: Vec::new(),
    };
    let functions = L::Functions::resolve(&StandInResolver::audited())
        .expect("the stand-ins resolve every name");
    (report, Some(BoundLibrary { functions, library }))
}

/// A library that was not found.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn not_found<L: NativeLibrary>() -> Checked<L> {
    let report = LibraryReport {
        library: L::NAME,
        path: None,
        versions: Vec::new(),
        faults: vec![LibraryFault::NotFound {
            attempted: vec![PathBuf::from("libabsent.so")],
        }],
    };
    (report, None)
}

/// Every library usable, except the one named `at_fault`, which is not
/// found.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
struct StandIns {
    at_fault: Option<&'static str>,
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
impl CheckLibrary for StandIns {
    fn check<L: NativeLibrary>(&self) -> Checked<L> {
        if self.at_fault == Some(L::NAME) {
            not_found()
        } else {
            usable()
        }
    }
}

/// With every library usable, discovery succeeds, and its report has each
/// library's, in the order they are opened.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[test]
fn discovery_succeeds_when_every_library_is_usable() {
    let availability =
        availability_of(StandIns { at_fault: None }).expect("every library is usable");
    let libraries = &availability.report().libraries;
    let names: Vec<_> = libraries.iter().map(|library| library.library).collect();
    assert_eq!(names, ["CUDA Runtime", "cuTensorNet"]);
    assert!(libraries.iter().all(|library| library.faults.is_empty()));
}

/// A user whose cuTensorNet is missing, or whose CUDA Runtime is, cannot
/// run on the GPU, and the report also shows the library that is fine.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[test]
fn one_library_at_fault_makes_discovery_fail_with_every_report() {
    for at_fault in ["CUDA Runtime", "cuTensorNet"] {
        let Err(AvailabilityError::Unusable(report)) = availability_of(StandIns {
            at_fault: Some(at_fault),
        }) else {
            panic!("{at_fault} is at fault");
        };
        let names: Vec<_> = report
            .libraries
            .iter()
            .map(|library| library.library)
            .collect();
        assert_eq!(names, ["CUDA Runtime", "cuTensorNet"]);
        for library in &report.libraries {
            assert_eq!(
                library.faults.is_empty(),
                library.library != at_fault,
                "{} while {at_fault} is at fault",
                library.library
            );
        }
    }
}
