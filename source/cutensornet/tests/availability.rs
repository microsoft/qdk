// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Discovery on a host with the audited libraries installed.
//!
//! It needs the CUDA Runtime 12.9 and cuTensorNet 2.13, so it is ignored in
//! CI; run it with `--ignored` on such a host. It runs no GPU work.

use qdk_cutensornet::discover;

#[test]
#[ignore = "requires the audited CUDA Runtime 12.9 and cuTensorNet 2.13 libraries"]
fn the_audited_libraries_are_discovered() {
    // Load both libraries, look up their functions and check their versions.
    let availability = discover().expect("the audited libraries are installed");

    // What was loaded, from where; `availability` keeps it loaded.
    let report = availability.report();
    let cutensornet = report.library("cuTensorNet").expect("cuTensorNet is used");
    assert_eq!(cutensornet.version("cuTensorNet"), Some(21_300));
    assert_eq!(
        cutensornet.version("cuTensorNet's CUDA Runtime"),
        Some(12_090)
    );
    let cuda_runtime = report
        .library("CUDA Runtime")
        .expect("the CUDA Runtime is used");
    assert_eq!(cuda_runtime.version("CUDA Runtime"), Some(12_090));
    println!("{report:#?}");
}
