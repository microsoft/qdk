// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Discovery and the setup check on a host with the audited libraries
//! installed.
//!
//! They need the CUDA Runtime 12.9 and cuTensorNet 2.13, and the setup
//! check a CUDA device too, so they are ignored in CI; run them with
//! `--ignored` on such a host. Discovery runs no GPU work; the setup check
//! runs one gate on device 0.

use qdk_cutensornet::{PackagedLibraries, diagnose, discover};

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

#[test]
#[ignore = "requires the audited libraries and a CUDA device"]
fn the_diagnosis_of_a_cuda_host_is_ready() {
    let diagnosis = diagnose(&PackagedLibraries::default());
    println!("{diagnosis}");
    assert!(diagnosis.is_ready(), "{diagnosis}");
}
