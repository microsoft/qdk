// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What the user reads when discovery refuses an installation.

use crate::{AvailabilityError, AvailabilityReport, LibraryFault, LibraryReport};
use std::path::PathBuf;

/// A user whose cuTensorNet lacks two functions reads both, each after the
/// library and the path it was opened from; a library with no fault is not
/// named.
#[test]
fn the_message_names_every_fault_with_its_library() {
    let report = AvailabilityReport {
        libraries: vec![
            LibraryReport {
                library: "CUDA Runtime",
                path: Some(PathBuf::from("/opt/cuda/libcudart.so.12")),
                versions: Vec::new(),
                faults: Vec::new(),
            },
            LibraryReport {
                library: "cuTensorNet",
                path: Some(PathBuf::from("/opt/cutensornet/libcutensornet.so.2")),
                versions: Vec::new(),
                faults: vec![
                    LibraryFault::MissingRequiredSymbol {
                        symbol: "cutensornetGateSplit",
                    },
                    LibraryFault::MissingRequiredSymbol {
                        symbol: "cutensornetTensorQR",
                    },
                ],
            },
        ],
    };
    let message = AvailabilityError::Unusable(report).to_string();
    for symbol in ["cutensornetGateSplit", "cutensornetTensorQR"] {
        assert_eq!(
            message.matches(symbol).count(),
            1,
            "{symbol} once: {message}"
        );
    }
    assert_eq!(
        message
            .matches("/opt/cutensornet/libcutensornet.so.2")
            .count(),
        2,
        "the path with each fault: {message}"
    );
    assert!(
        !message.contains("CUDA Runtime") && !message.contains("libcudart"),
        "the library with no fault is not named: {message}"
    );
}

/// A library that was not opened is named without a path.
#[test]
fn a_library_not_opened_is_named_without_a_path() {
    let report = AvailabilityReport {
        libraries: vec![LibraryReport {
            library: "CUDA Runtime",
            path: None,
            versions: Vec::new(),
            faults: vec![LibraryFault::NotFound {
                attempted: vec![PathBuf::from("libcudart.so.12")],
            }],
        }],
    };
    let message = AvailabilityError::Unusable(report).to_string();
    for part in ["CUDA Runtime", "not found", "libcudart.so.12"] {
        assert!(message.contains(part), "{part} is named: {message}");
    }
    assert!(
        !message.contains("CUDA Runtime at"),
        "no path is given for a library not opened: {message}"
    );
}
