// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Qodec's example qodecs, and this suite's own, read and planned as
//! Qodec's runtime runs them.

use std::path::PathBuf;

use qodec_runtime_recipes::{FrameDecoding, QodecModel, QodecPlan};

/// The qodec at `path`: under `fixtures/`, one of this suite's own, in
/// `tests/chains/fixtures`, for a shape no example has; otherwise one of
/// Qodec's examples, under `source/qodec/examples`.
pub fn model(path: &str) -> QodecModel {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let file = match path.strip_prefix("fixtures/") {
        Some(fixture) => crate_dir.join("tests/chains/fixtures").join(fixture),
        None => crate_dir.join("../qodec/examples").join(path),
    };
    let qodec = qodec::Qodec::load(file).unwrap_or_else(|error| panic!("{path} loads: {error}"));
    qodec_runtime_reader::read(&qodec).unwrap_or_else(|error| panic!("{path} reads: {error}"))
}

/// `model`'s layers, assembled by the reference design.
pub fn plan(model: &QodecModel) -> QodecPlan {
    QodecPlan::new(model, &FrameDecoding).expect("the reference design assembles the example")
}
