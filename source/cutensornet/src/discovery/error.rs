// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Why a GPU run cannot use the libraries it needs.
//!
//! A caller that asked for a GPU run reports the error to the user or falls
//! back to the CPU. Off Linux x86-64 the error says so. Otherwise it holds
//! discovery's report, which lists every fault of every library.

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
    /// order they are opened. Each library at fault lists its faults. The
    /// message shows each fault after its library and, once opened, its
    /// path, so each can be read alone.
    #[error("cuTensorNet acceleration is unavailable: {}", faults_message(.0))]
    Unusable(AvailabilityReport),
}

/// The message of [`AvailabilityError::Unusable`].
fn faults_message(report: &AvailabilityReport) -> String {
    report
        .faults()
        .map(|(library, fault)| match &library.path {
            Some(path) => format!("{} at `{}`: {fault}", library.name, path.display()),
            None => format!("{}: {fault}", library.name),
        })
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests;
