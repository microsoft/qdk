// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! How a caller finds a library's report, and every fault found.

use crate::{AvailabilityReport, LibraryFault, LibraryReport};

/// The report of a library that was not opened, so it has no path and no
/// versions.
fn not_opened(name: &'static str) -> LibraryReport {
    LibraryReport {
        name,
        path: None,
        versions: Vec::new(),
        faults: Vec::new(),
    }
}

/// A caller showing what was installed looks cuTensorNet's report up by
/// name, whichever its place in the report. A library discovery does not
/// check has none.
#[test]
fn a_library_is_found_by_its_name() {
    let report = AvailabilityReport {
        libraries: vec![not_opened("CUDA Runtime"), not_opened("cuTensorNet")],
    };
    assert_eq!(
        report.library("cuTensorNet").map(|found| found.name),
        Some("cuTensorNet")
    );
    assert_eq!(
        report.library("CUDA Runtime").map(|found| found.name),
        Some("CUDA Runtime")
    );
    assert_eq!(report.library("cuBLAS"), None);
}

/// A caller telling the user what to fix reads every fault with the
/// library it was found in, library by library in discovery's order, and
/// skips a library without faults.
#[test]
fn every_fault_is_read_with_its_library_in_discovery_order() {
    let missing = |symbol| LibraryFault::MissingRequiredSymbol { symbol };
    let with_faults = |name, faults| LibraryReport {
        faults,
        ..not_opened(name)
    };
    let report = AvailabilityReport {
        libraries: vec![
            with_faults(
                "CUDA Runtime",
                vec![missing("cudaMalloc"), missing("cudaFree")],
            ),
            not_opened("cuBLAS"),
            with_faults("cuTensorNet", vec![missing("cutensornetCreate")]),
        ],
    };
    let faults: Vec<_> = report
        .faults()
        .map(|(library, fault)| (library.name, fault.clone()))
        .collect();
    assert_eq!(
        faults,
        [
            ("CUDA Runtime", missing("cudaMalloc")),
            ("CUDA Runtime", missing("cudaFree")),
            ("cuTensorNet", missing("cutensornetCreate")),
        ]
    );
}
