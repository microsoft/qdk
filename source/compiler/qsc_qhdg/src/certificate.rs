// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The parts of a rewrite certificate that do not depend on how a graph is
//! shaped. The certificate itself lives with the graph it certifies.

use thiserror::Error;

pub const REWRITE_CERTIFICATE_VERSION: u32 = 1;

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum CertificateError {
    #[error("QHDG-REWRITE-CERTIFICATE-ENCODING-001: {0}")]
    Encoding(#[from] crate::SnapshotError),
    #[error("QHDG-REWRITE-CERTIFICATE-VERSION-001: unsupported rewrite certificate version")]
    Version,
    #[error("QHDG-REWRITE-CERTIFICATE-STALE-001: rewrite certificate input is stale")]
    Stale,
    #[error("QHDG-REWRITE-CERTIFICATE-MATCH-001: matched node is missing or stale")]
    Match,
    #[error("QHDG-REWRITE-CERTIFICATE-PROOF-001: rewrite proof is incomplete")]
    Proof,
}
