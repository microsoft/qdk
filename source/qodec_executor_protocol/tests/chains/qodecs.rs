// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Qodec's example qodecs, read and planned as Qodec's runtime runs them.

use std::path::PathBuf;

use qodec_runtime_recipes::{FrameDecoding, QodecModel, QodecPlan};

/// The example qodec at `path`, under `source/qodec/examples`.
pub fn model(path: &str) -> QodecModel {
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../qodec/examples")
        .join(path);
    let qodec = qodec::Qodec::load(file).unwrap_or_else(|error| panic!("{path} loads: {error}"));
    qodec_runtime_reader::read(&qodec).unwrap_or_else(|error| panic!("{path} reads: {error}"))
}

/// `model`'s layers, assembled by the reference design.
pub fn plan(model: &QodecModel) -> QodecPlan {
    QodecPlan::new(model, &FrameDecoding).expect("the reference design assembles the example")
}
