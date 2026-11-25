// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Discovery and the GPU setup check on a host with the audited libraries
//! installed.
//!
//! They need the CUDA Runtime 12.9 and cuTensorNet 2.13, and the setup
//! check a CUDA device too. So they are ignored in CI; run them with
//! `--ignored` on such a host. Discovery runs no GPU work. The setup check
//! makes one trial run on the engine's device.

use qdk_cutensornet::{GpuSetup, PackagedLibraries, discover};

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

/// A host with the audited libraries and a CUDA device is ready for a GPU
/// run.
#[test]
#[ignore = "requires the audited libraries and a CUDA device"]
fn the_gpu_setup_of_a_cuda_host_is_ready() {
    let setup = GpuSetup::check(&PackagedLibraries::default());
    println!("{setup}");
    assert!(setup.is_ready(), "{setup}");
}
