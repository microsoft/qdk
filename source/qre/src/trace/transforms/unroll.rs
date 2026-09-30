// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{Block, Error, Trace, TraceTransform, trace::Operation};

#[cfg(test)]
mod tests;

/// Expands every repeated block into its repetitions, so that later
/// transforms process the gates in execution order instead of approximating
/// repeated blocks.  The result is as large as the fully executed program.
#[derive(Default)]
pub struct Unroll;

impl TraceTransform for Unroll {
    fn transform(&self, trace: &Trace) -> Result<Trace, Error> {
        let mut transformed = trace.clone_empty(None);
        unroll_block(&trace.block, transformed.root_block_mut());
        Ok(transformed)
    }
}

fn unroll_block(input: &Block, output: &mut Block) {
    for _ in 0..input.repetitions {
        for op in &input.operations {
            match op {
                Operation::GateOperation(gate) => {
                    output.add_operation(gate.id, gate.qubits.clone(), gate.params.clone());
                }
                Operation::BlockOperation(inner) => unroll_block(inner, output),
            }
        }
    }
}
