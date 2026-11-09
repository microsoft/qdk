// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Why a GPU run cannot use the libraries it needs.
//!
//! A caller that asked for a GPU run reports the error to the user or falls
//! back to the CPU. Off Linux x86-64 the error says so. Otherwise it holds
//! discovery's report on every library, in which each library at fault lists
//! every fault discovery could find in it (`libraries::report`).

use crate::AvailabilityReport;
use thiserror::Error;

/// Why a GPU run cannot use the libraries it needs: the platform is
/// unsupported, or [`discover`](crate::discover) found a library at fault.
#[derive(Debug, Error)]
pub enum AvailabilityError {
    /// The target is not Linux x86-64.
    #[error(
        "cuTensorNet acceleration is unsupported on {platform}; supported target: Linux x86-64"
    )]
    UnsupportedPlatform { platform: String },

    /// A library is at fault. The report has one entry per library, in the
    /// order they are opened; each library at fault lists its faults.
    #[error("cuTensorNet acceleration is unavailable: {}", faults_message(.0))]
    Unusable(AvailabilityReport),
}

/// Each fault of `report`, after its library's name and, when it was
/// opened, its path.
#[expect(
    clippy::unnecessary_debug_formatting,
    reason = "paths are quoted, as in the faults' own messages"
)]
fn faults_message(report: &AvailabilityReport) -> String {
    let mut lines = Vec::new();
    for library in &report.libraries {
        let place = match &library.path {
            Some(path) => format!("{} at {path:?}", library.library),
            None => library.library.to_string(),
        };
        lines.extend(
            library
                .faults
                .iter()
                .map(|fault| format!("{place}: {fault}")),
        );
    }
    lines.join("; ")
}
