// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::trace::TraceTransform;
use crate::{Error, Trace, instruction_ids};

pub struct LatticeSurgery {
    slow_down_factor: f64,
}

impl Default for LatticeSurgery {
    fn default() -> Self {
        Self {
            slow_down_factor: 1.0,
        }
    }
}

impl LatticeSurgery {
    #[must_use]
    pub fn new(slow_down_factor: f64) -> Self {
        Self { slow_down_factor }
    }
}

impl TraceTransform for LatticeSurgery {
    fn transform(&self, trace: &Trace) -> Result<Trace, Error> {
        let mut transformed = trace.clone_empty(None);

        let block = transformed.add_block((trace.depth() * self.slow_down_factor).ceil());
        block.add_operation(
            instruction_ids::LATTICE_SURGERY,
            (0..trace.compute_qubits()).collect(),
            vec![],
        );

        Ok(transformed)
    }
}
