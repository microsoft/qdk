// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! How a caller finds a version in a library's report.

use crate::{LibraryReport, Version};

/// A caller reads which CUDA Runtime cuTensorNet was built against by its
/// component's name, whichever its place; a version not read is absent.
#[test]
fn a_version_is_found_by_its_component() {
    let cutensornet = LibraryReport {
        library: "cuTensorNet",
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
        other_copies: Vec::new(),
        loaded_from_override: false,
    };
    assert_eq!(
        cutensornet.version("cuTensorNet's CUDA Runtime"),
        Some(12_090)
    );
    assert_eq!(cutensornet.version("cuTensorNet"), Some(21_300));
    assert_eq!(cutensornet.version("CUDA driver"), None);
}
