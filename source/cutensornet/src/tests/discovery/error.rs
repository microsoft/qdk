// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What the user reads when discovery refuses an installation.

use crate::{AvailabilityError, AvailabilityReport, LibraryFault, LibraryReport};
use std::path::PathBuf;

/// A user whose CUDA Runtime lacks two functions reads both, each after the
/// library and the path it was opened from; a library with no fault is
/// named neither by its file nor by its name. The faults are on the CUDA
/// Runtime, whose name the message's lead ("cuTensorNet acceleration…")
/// does not hold, so each fault must name it; when only cuTensorNet is at
/// fault, the healthy CUDA Runtime's name is absent.
#[test]
fn the_message_names_every_fault_with_its_library() {
    let cuda_path = "/opt/cuda/libcudart.so.12";
    let report = AvailabilityReport {
        libraries: vec![
            LibraryReport {
                library: "CUDA Runtime",
                path: Some(PathBuf::from(cuda_path)),
                versions: Vec::new(),
                faults: vec![
                    LibraryFault::MissingRequiredSymbol {
                        symbol: "cudaMalloc",
                    },
                    LibraryFault::MissingRequiredSymbol {
                        symbol: "cudaStreamCreateWithFlags",
                    },
                ],
                other_copies: Vec::new(),
                from_override: false,
            },
            LibraryReport {
                library: "cuTensorNet",
                path: Some(PathBuf::from("/opt/cutensornet/libcutensornet.so.2")),
                versions: Vec::new(),
                faults: Vec::new(),
                other_copies: Vec::new(),
                from_override: false,
            },
        ],
    };
    let message = AvailabilityError::Unusable(report).to_string();
    for symbol in ["cudaMalloc", "cudaStreamCreateWithFlags"] {
        assert_eq!(
            message.matches(symbol).count(),
            1,
            "{symbol} once: {message}"
        );
    }
    for (part, what) in [("CUDA Runtime", "the library"), (cuda_path, "its path")] {
        assert_eq!(
            message.matches(part).count(),
            2,
            "{what} with each fault: {message}"
        );
    }
    let at = |part| message.find(part).expect("the part is in the message");
    assert!(
        at("CUDA Runtime") < at(cuda_path) && at(cuda_path) < at("cudaMalloc"),
        "the library, then its path, then the fault: {message}"
    );
    assert!(
        !message.contains("libcutensornet"),
        "the library with no fault is not named: {message}"
    );
    assert!(
        !message.contains("Some("),
        "the path is shown, not Rust's value around it: {message}"
    );
    let only_cutensornet_at_fault = AvailabilityReport {
        libraries: vec![
            LibraryReport {
                library: "CUDA Runtime",
                path: Some(PathBuf::from(cuda_path)),
                versions: Vec::new(),
                faults: Vec::new(),
                other_copies: Vec::new(),
                from_override: false,
            },
            LibraryReport {
                library: "cuTensorNet",
                path: Some(PathBuf::from("/opt/cutensornet/libcutensornet.so.2")),
                versions: Vec::new(),
                faults: vec![LibraryFault::MissingRequiredSymbol {
                    symbol: "cutensornetCreate",
                }],
                other_copies: Vec::new(),
                from_override: false,
            },
        ],
    };
    let message = AvailabilityError::Unusable(only_cutensornet_at_fault).to_string();
    assert!(
        message.contains("cutensornetCreate"),
        "the fault is named: {message}"
    );
    for part in ["CUDA Runtime", "libcudart"] {
        assert!(
            !message.contains(part),
            "the library with no fault is not named ({part}): {message}"
        );
    }
}

/// A library that was not opened is named without a path. The fixture's
/// attempted name is bare (a real search may list absolute paths), so a `/`
/// in this message could only come from a path given to the library itself.
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
            other_copies: Vec::new(),
            from_override: false,
        }],
    };
    let message = AvailabilityError::Unusable(report).to_string();
    for part in ["CUDA Runtime", "not found", "libcudart.so.12"] {
        assert!(message.contains(part), "{part} is named: {message}");
    }
    assert!(
        !message.contains('/') && !message.contains("None"),
        "no path, nor Rust's word for its absence, is given for a library not opened: {message}"
    );
}

/// A user on an unsupported host reads which platform it is and which one
/// is supported. Discovery's test that it names this host runs only off
/// Linux x86-64; this one runs everywhere.
#[test]
fn an_unsupported_platform_is_named_beside_the_supported_one() {
    let message = AvailabilityError::UnsupportedPlatform {
        platform: "macos aarch64".to_owned(),
    }
    .to_string();
    for part in ["macos aarch64", "Linux x86-64"] {
        assert!(message.contains(part), "{part} is named: {message}");
    }
}
