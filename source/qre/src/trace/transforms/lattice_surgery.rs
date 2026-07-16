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
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss
    )]
    fn transform(&self, trace: &Trace) -> Result<Trace, Error> {
        let mut transformed = trace.clone_empty(None);

        let block =
            transformed.add_block((trace.depth() as f64 * self.slow_down_factor).ceil() as u64);
        block.add_operation(
            instruction_ids::LATTICE_SURGERY,
            (0..trace.compute_qubits()).collect(),
            vec![],
        );

        Ok(transformed)
    }
}
