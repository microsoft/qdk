// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! How a caller finds a library's report.

use crate::{AvailabilityReport, LibraryReport};

/// The report of a library that was not opened, so it has no path and no
/// versions.
fn not_opened(library: &'static str) -> LibraryReport {
    LibraryReport {
        library,
        path: None,
        versions: Vec::new(),
        faults: Vec::new(),
    }
}

/// A caller showing what was installed looks cuTensorNet's report up by
/// name, whichever its place in the report; a library discovery does not
/// check has none.
#[test]
fn a_library_is_found_by_its_name() {
    let report = AvailabilityReport {
        libraries: vec![not_opened("CUDA Runtime"), not_opened("cuTensorNet")],
    };
    assert_eq!(
        report.library("cuTensorNet").map(|found| found.library),
        Some("cuTensorNet")
    );
    assert_eq!(
        report.library("CUDA Runtime").map(|found| found.library),
        Some("CUDA Runtime")
    );
    assert_eq!(report.library("cuBLAS"), None);
}
