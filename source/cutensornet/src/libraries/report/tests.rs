// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! How a caller finds a version in a library's report, and how a fault
//! names a path.

use crate::{LibraryFault, LibraryReport, Version};
use std::path::PathBuf;

/// A caller reads which CUDA Runtime cuTensorNet was built against by its
/// component's name, whichever its place; a version not read is absent.
#[test]
fn a_version_is_found_by_its_component() {
    let cutensornet = LibraryReport {
        name: "cuTensorNet",
        path: None,
        versions: vec![
            Version {
                component: "cuTensorNet",
                value: 21_300,
            },
            Version {
                component: "cuTensorNet's CUDA Runtime",
                value: 12_090,
            },
        ],
        faults: Vec::new(),
    };
    assert_eq!(
        cutensornet.version("cuTensorNet's CUDA Runtime"),
        Some(12_090)
    );
    assert_eq!(cutensornet.version("cuTensorNet"), Some(21_300));
    assert_eq!(cutensornet.version("CUDA driver"), None);
}

/// A Windows user reads the path of the library at fault as they would
/// type it, with single backslashes, in backticks as in the QDK's other
/// messages. Each fault that names a path shows it so.
#[test]
fn a_fault_names_its_path_as_written() {
    let path = PathBuf::from(r"C:\CUDA\bin\cudart64_12.dll");
    let faults = [
        LibraryFault::InvalidOverride {
            variable: "QDK_CUDART_LIBRARY",
            path: path.clone(),
            reason: "path does not exist",
        },
        LibraryFault::NotFound {
            attempted: vec![PathBuf::from("libcudart.so.12"), path.clone()],
        },
        LibraryFault::InaccessiblePath {
            path: path.clone(),
            message: "permission denied".to_owned(),
        },
        LibraryFault::LoadFailed {
            path,
            message: "wrong ELF class".to_owned(),
        },
    ];
    for fault in faults {
        let message = fault.to_string();
        assert!(
            message.contains(r"`C:\CUDA\bin\cudart64_12.dll`"),
            "the path as written, in backticks: {message}"
        );
    }
}
