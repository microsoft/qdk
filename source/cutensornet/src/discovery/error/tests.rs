// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What the user reads when discovery refuses an installation.

use crate::{AvailabilityError, AvailabilityReport, LibraryFault, LibraryReport};
use std::path::PathBuf;

const CUDA_PATH: &str = "/opt/cuda/libcudart.so.12";
const CUTENSORNET_PATH: &str = "/opt/cutensornet/libcutensornet.so.2";

/// The message of an installation whose two libraries were opened, with
/// the given faults.
fn message_of(cuda_faults: Vec<LibraryFault>, cutensornet_faults: Vec<LibraryFault>) -> String {
    let report = AvailabilityReport {
        libraries: vec![
            LibraryReport {
                name: "CUDA Runtime",
                path: Some(PathBuf::from(CUDA_PATH)),
                versions: Vec::new(),
                faults: cuda_faults,
            },
            LibraryReport {
                name: "cuTensorNet",
                path: Some(PathBuf::from(CUTENSORNET_PATH)),
                versions: Vec::new(),
                faults: cutensornet_faults,
            },
        ],
    };
    AvailabilityError::Unusable(report).to_string()
}

/// The fault of a library that lacks `symbol`.
fn missing(symbol: &'static str) -> LibraryFault {
    LibraryFault::MissingRequiredSymbol { symbol }
}

/// A user whose CUDA Runtime lacks two functions reads both, each after the
/// library and the path it was opened from. The message's lead
/// ("cuTensorNet acceleration…") does not hold the CUDA Runtime's name, so
/// each fault must name it.
#[test]
fn each_fault_follows_its_library_and_path() {
    let message = message_of(
        vec![missing("cudaMalloc"), missing("cudaStreamCreateWithFlags")],
        Vec::new(),
    );
    for symbol in ["cudaMalloc", "cudaStreamCreateWithFlags"] {
        assert_eq!(
            message.matches(symbol).count(),
            1,
            "{symbol} once: {message}"
        );
    }
    let quoted_path = format!("`{CUDA_PATH}`");
    for (part, what) in [
        ("CUDA Runtime", "the library"),
        (&*quoted_path, "its path, as the faults show one"),
    ] {
        assert_eq!(
            message.matches(part).count(),
            2,
            "{what} with each fault: {message}"
        );
    }
    let at = |part| message.find(part).expect("the part is in the message");
    assert!(
        at("CUDA Runtime") < at(CUDA_PATH) && at(CUDA_PATH) < at("cudaMalloc"),
        "the library, then its path, then the fault: {message}"
    );
    assert!(
        !message.contains("Some("),
        "the path is shown, not Rust's value around it: {message}"
    );
}

/// A library with no fault is named neither by its file nor by its name, so
/// the user looks only at the library to fix, whichever it is.
/// (cuTensorNet's name is in the message's lead, so only its file is checked
/// when it is the healthy one.)
#[test]
fn a_library_without_a_fault_is_not_named() {
    let only_cuda_at_fault = message_of(vec![missing("cudaMalloc")], Vec::new());
    assert!(
        !only_cuda_at_fault.contains("libcutensornet"),
        "the healthy cuTensorNet is not named: {only_cuda_at_fault}"
    );
    let only_cutensornet_at_fault = message_of(Vec::new(), vec![missing("cutensornetCreate")]);
    assert!(
        only_cutensornet_at_fault.contains("cutensornetCreate"),
        "the fault is named: {only_cutensornet_at_fault}"
    );
    for part in ["CUDA Runtime", "libcudart"] {
        assert!(
            !only_cutensornet_at_fault.contains(part),
            "the healthy CUDA Runtime is not named ({part}): {only_cutensornet_at_fault}"
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
            name: "CUDA Runtime",
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
