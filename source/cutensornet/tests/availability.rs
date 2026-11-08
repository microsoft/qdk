// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Discovery on a host with the audited libraries installed.
//!
//! It needs the CUDA Runtime 12.9 and cuTensorNet 2.13, so it is ignored in
//! CI; run it with `--ignored` on such a host. Discovery runs no GPU work;
//! the diagnosis runs one gate on device 0.

use qdk_cutensornet::{PackagedLibraries, diagnose, discover};

#[test]
#[ignore = "requires the audited CUDA Runtime 12.9 and cuTensorNet 2.13 libraries"]
fn the_audited_libraries_are_discovered() {
    // Load both libraries, look up their functions and check their versions.
    let availability = discover().expect("the audited libraries are installed");

    // What was loaded, from where; `availability` keeps it loaded.
    let report = availability.report();
    assert_eq!(report.cutensornet_version, 21_300);
    assert_eq!(report.cutensornet_cuda_runtime_version, 12_090);
    assert_eq!(report.cuda_runtime_version, 12_090);
    println!("{report:#?}");
}

#[test]
#[ignore = "requires the audited libraries and a CUDA device"]
fn the_diagnosis_of_a_cuda_host_is_ready() {
    let diagnosis = diagnose(&PackagedLibraries::default());
    println!("{diagnosis}");
    assert!(diagnosis.is_ready(), "{diagnosis}");
}
