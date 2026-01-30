// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::trace::TraceTransform;
use crate::{Error, Trace, instruction_ids};

#[derive(Default)]
pub struct LatticeSurgery;

impl LatticeSurgery {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl TraceTransform for LatticeSurgery {
    fn transform(&self, trace: &Trace) -> Result<Trace, Error> {
        let mut transformed = trace.clone_empty(None);

        let block = transformed.add_block(trace.depth());
        block.add_operation(
            instruction_ids::LATTICE_SURGERY,
            (0..trace.compute_qubits()).collect(),
            vec![],
        );

        Ok(transformed)
    }
}
