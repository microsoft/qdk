// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use rustc_hash::FxHashMap;

use crate::{
    Block, Error, Trace, TraceTransform,
    instruction_ids::{READ_FROM_MEMORY, WRITE_TO_MEMORY},
    trace::Operation,
};

#[cfg(test)]
mod tests;

#[derive(Default)]
pub struct Unmemory;

impl TraceTransform for Unmemory {
    fn transform(&self, trace: &Trace) -> Result<Trace, Error> {
        let compute = trace.compute_qubits();
        let total = trace.total_qubits();
        let mut transformed = trace.clone_empty(Some(total));
        // Remove memory qubits — all qubits become compute qubits.
        transformed.memory_qubits = None;

        let mut state = TransformState::new(compute, total);

        process_block(&mut state, &trace.block, transformed.root_block_mut())?;

        // Set compute_qubits to cover all logical IDs that were assigned
        // (including any lazily-assigned IDs for temporary compute slots).
        transformed.set_compute_qubits(state.next_logical_id);

        Ok(transformed)
    }
}

#[derive(Clone)]
struct TransformState {
    /// Compute slot index → logical qubit ID currently residing there.
    slot_to_logical: FxHashMap<u64, u64>,
    /// Next logical qubit ID to assign for lazy placement.
    next_logical_id: u64,
}

impl TransformState {
    fn new(compute: u64, total: u64) -> Self {
        let mut slot_to_logical = FxHashMap::default();
        // Compute slots 0..compute initially hold their own index as
        // the logical qubit ID.
        for i in 0..compute {
            slot_to_logical.insert(i, i);
        }
        Self {
            slot_to_logical,
            next_logical_id: total,
        }
    }

    /// Look up the logical qubit residing in a compute slot. If the slot
    /// has not been explicitly mapped via `READ_FROM_MEMORY` or initial
    /// population, return the slot index itself (identity mapping).
    fn logical_qubit_in_slot(&mut self, slot: u64) -> u64 {
        if let Some(&lq) = self.slot_to_logical.get(&slot) {
            lq
        } else {
            // Slot was never explicitly loaded — use the slot index as the
            // logical qubit ID (identity) and record it.
            self.slot_to_logical.insert(slot, slot);
            if slot >= self.next_logical_id {
                self.next_logical_id = slot + 1;
            }
            slot
        }
    }

    /// Record that `logical` now occupies `slot`, and update
    /// `next_logical_id` if necessary.
    fn place_logical_in_slot(&mut self, logical: u64, slot: u64) {
        self.slot_to_logical.insert(slot, logical);
        if logical >= self.next_logical_id {
            self.next_logical_id = logical + 1;
        }
    }
}

#[allow(clippy::cast_possible_truncation)]
fn process_block(
    state: &mut TransformState,
    input: &Block,
    output: &mut Block,
) -> Result<(), Error> {
    for op in &input.operations {
        match op {
            Operation::GateOperation(gate) => {
                if gate.id == READ_FROM_MEMORY {
                    // READ_FROM_MEMORY(logical_qubit, compute_slot)
                    // The logical qubit moves from memory into the compute
                    // slot.
                    let logical = gate.qubits[0];
                    let slot = gate.qubits[1];
                    state.place_logical_in_slot(logical, slot);
                } else if gate.id == WRITE_TO_MEMORY {
                    // WRITE_TO_MEMORY(compute_slot, logical_qubit)
                    // The logical qubit moves from the compute slot into
                    // memory; the compute slot becomes empty.
                    let slot = gate.qubits[0];
                    state.slot_to_logical.remove(&slot);
                } else {
                    // Regular gate: remap compute-slot IDs to logical qubit
                    // IDs.
                    let remapped: Vec<u64> = gate
                        .qubits
                        .iter()
                        .map(|&slot| state.logical_qubit_in_slot(slot))
                        .collect();
                    output.add_operation(gate.id, remapped, gate.params.clone());
                }
            }

            Operation::BlockOperation(inner) => {
                let out_inner = output.add_block(inner.repetitions);

                if inner.repetitions > 1 {
                    // Repeated blocks maintain a consistent mapping across
                    // iterations.  Save the entry state and restore it after
                    // processing so the parent context is unaffected.
                    let entry = state.clone();
                    process_block(state, inner, out_inner)?;
                    let next_id = state.next_logical_id;
                    *state = entry;
                    state.next_logical_id = next_id;
                } else {
                    process_block(state, inner, out_inner)?;
                }
            }
        }
    }

    Ok(())
}
