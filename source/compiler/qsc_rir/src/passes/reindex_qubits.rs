// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use std::ops::Sub;

use qsc_data_structures::measurement::OneQubitMeasurement;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
    builder,
    debug::InstructionDbgMetadata,
    rir::{Block, CallableId, CallableType, Instruction, Literal, Operand, Program},
};

#[derive(Clone, Copy)]
enum CallableToAdd {
    M,
    CX,
    H,
    S,
    SAdj,
}

/// Reindexes qubits after they have been measured or reset. This ensures there is no qubit reuse in
/// the program. As part of the pass, reset callables are removed and mresetz calls are replaced with
/// mz calls.
/// Note that this pass has several assumptions:
/// 1. Only one callable has a body, which is the entry point callable.
/// 2. The entry point callable is a single block.
/// 3. No dynamic qubits are used.
///
/// The pass will panic if the input program violates any of these assumptions.
pub fn reindex_qubits(program: &mut Program) {
    validate_assumptions(program);

    // Get measurement callable IDs if they exist in the program
    let mut measurement_ids = MeasurementIds {
        mz: find_callable(program, "__quantum__qis__m__body"),
        mx: find_callable(program, "__quantum__qis__mx__body"),
        my: find_callable(program, "__quantum__qis__my__body"),
        mresetz: find_callable(program, "__quantum__qis__mresetz__body"),
    };
    if measurement_ids.mresetz.is_some() && measurement_ids.mz.is_none() {
        measurement_ids.mz = Some(add_callable(program, CallableToAdd::M));
    }

    let mut pass = ReindexQubitPass::new(
        measurement_ids,
        // For this calculation, qubit IDs can never be lower than zero but a program may not use
        // any qubits. Since `highest_used_id` is only needed for remapping pass and a program without any
        // qubits won't do any remapping, it's safe to treat this as 1 and let `highest_used_id` default
        // to zero.
        program.num_qubits.max(1).sub(1),
    );
    let Some((block_id, mut block)) = program.blocks.drain().next() else {
        panic!("program should have at least one block");
    };

    // Reverse walk program instructions to check for qubit reuse and measurement bases. This avoids
    // nested loops over instructions later when reindexing.
    pass.scan_for_qubit_reuse(program, &block);

    // Add any callables required for reindexing based on the measurement bases.
    pass.find_add_required_callables(program);

    // Perform the reindexing on the single entry block.
    pass.reindex_qubits_in_block(program, &mut block);
    program.blocks.insert(block_id, block);
    program.num_qubits = pass.highest_used_id + 1;

    // All reset function calls should be removed, so remove them from the callables.
    program.callables.retain(|id, callable| {
        callable.call_type != CallableType::Reset && Some(id) != pass.measurement_ids.mresetz
    });
}

#[derive(Default)]
struct GateIds {
    cx: Option<CallableId>,
    h: Option<CallableId>,
    s: Option<CallableId>,
    sadj: Option<CallableId>,
}

struct MeasurementIds {
    mz: Option<CallableId>,
    mx: Option<CallableId>,
    my: Option<CallableId>,
    mresetz: Option<CallableId>,
}

impl MeasurementIds {
    fn basis(&self, call_id: CallableId) -> Option<OneQubitMeasurement> {
        if self.mx == Some(call_id) {
            Some(OneQubitMeasurement::X)
        } else if self.my == Some(call_id) {
            Some(OneQubitMeasurement::Y)
        } else if self.mz == Some(call_id) {
            Some(OneQubitMeasurement::Z)
        } else {
            None
        }
    }

    fn is_measurement(&self, call_id: CallableId) -> bool {
        self.mx == Some(call_id) || self.my == Some(call_id) || self.mz == Some(call_id)
    }
}

struct ReindexQubitPass {
    measurement_ids: MeasurementIds,
    gate_ids: GateIds,
    instrs_to_transfer: FxHashMap<usize, OneQubitMeasurement>,
    bases_to_transfer: FxHashSet<OneQubitMeasurement>,
    highest_used_id: u32,
}

impl ReindexQubitPass {
    fn new(measurement_ids: MeasurementIds, highest_used_id: u32) -> Self {
        Self {
            measurement_ids,
            gate_ids: GateIds::default(),
            instrs_to_transfer: FxHashMap::default(),
            bases_to_transfer: FxHashSet::default(),
            highest_used_id,
        }
    }

    fn scan_for_qubit_reuse(&mut self, program: &Program, block: &Block) {
        let mut bases_needed = FxHashSet::default();
        let mut instrs_to_reindex = FxHashMap::default();
        let mut qubits_seen: FxHashSet<u32> = FxHashSet::default();

        let instrs = &block.0;
        for (i, instr) in instrs.iter().enumerate().rev() {
            if let Instruction::Call(call_id, args, _, _) = instr {
                let callable = program.get_callable(*call_id);
                let qubits = args.iter().filter_map(|arg| match arg {
                    Operand::Literal(Literal::Qubit(id)) => Some(*id),
                    _ => None,
                });

                if let Some(basis) = self.measurement_ids.basis(*call_id)
                    && qubits.clone().any(|qubit| qubits_seen.contains(&qubit))
                {
                    bases_needed.insert(basis);
                    instrs_to_reindex.insert(i, basis);
                }

                if callable.call_type == CallableType::Reset {
                    for qubit in qubits {
                        qubits_seen.remove(&qubit);
                    }
                } else {
                    for qubit in qubits {
                        qubits_seen.insert(qubit);
                    }
                }
            }
        }
        self.instrs_to_transfer = instrs_to_reindex;
        self.bases_to_transfer = bases_needed;
    }

    fn find_add_required_callables(&mut self, program: &mut Program) {
        if !self.bases_to_transfer.is_empty() {
            self.gate_ids.cx = Some(
                find_callable(program, "__quantum__qis__cx__body")
                    .unwrap_or_else(|| add_callable(program, CallableToAdd::CX)),
            );
        }
        if self.bases_to_transfer.contains(&OneQubitMeasurement::X)
            || self.bases_to_transfer.contains(&OneQubitMeasurement::Y)
        {
            self.gate_ids.h = Some(
                find_callable(program, "__quantum__qis__h__body")
                    .unwrap_or_else(|| add_callable(program, CallableToAdd::H)),
            );
        }
        if self.bases_to_transfer.contains(&OneQubitMeasurement::Y) {
            self.gate_ids.s = Some(
                find_callable(program, "__quantum__qis__s__body")
                    .unwrap_or_else(|| add_callable(program, CallableToAdd::S)),
            );
            self.gate_ids.sadj = Some(
                find_callable(program, "__quantum__qis__s__adj")
                    .unwrap_or_else(|| add_callable(program, CallableToAdd::SAdj)),
            );
        }
    }

    fn reindex_qubits_in_block(&mut self, program: &Program, block: &mut Block) {
        let mut map = FxHashMap::default();
        let mut used_qubits: FxHashSet<u32> = FxHashSet::default();
        let mut next_qubit_id = self.highest_used_id + 1;
        let instrs = std::mem::take(&mut block.0);
        for (i, instr) in instrs.iter().enumerate() {
            // Assume qubits only appear in void call instructions.
            match instr {
                Instruction::Call(call_id, args, _, _)
                    if program.get_callable(*call_id).call_type == CallableType::Reset =>
                {
                    // Generate any new qubit ids and skip adding the instruction.
                    for arg in args {
                        if let Operand::Literal(Literal::Qubit(qubit_id)) = arg
                            && used_qubits.contains(qubit_id)
                        {
                            map.insert(*qubit_id, next_qubit_id);
                            next_qubit_id += 1;
                        }
                    }
                }
                Instruction::Call(call_id, args, None, metadata) => {
                    let mut ids_used = Vec::new();

                    // Map the qubit args, if any, and copy over the instruction.
                    let new_args = args
                        .iter()
                        .map(|arg| match arg {
                            Operand::Literal(Literal::Qubit(qubit_id)) => {
                                ids_used.push(*qubit_id);
                                match map.get(qubit_id) {
                                    Some(mapped_id) => {
                                        // If the qubit has already been mapped, use the mapped id.
                                        self.highest_used_id = self.highest_used_id.max(*mapped_id);
                                        Operand::Literal(Literal::Qubit(*mapped_id))
                                    }
                                    None => *arg,
                                }
                            }
                            _ => *arg,
                        })
                        .collect::<Vec<_>>();
                    used_qubits.extend(ids_used.iter());

                    if self.measurement_ids.is_measurement(*call_id) {
                        if let Some(&basis) = self.instrs_to_transfer.get(&i) {
                            // Since the call was to m* and the qubit is reused later in the block,
                            // the new qubit replacing this one must mirror the old qubit's state.
                            // Achieve this by entangling the qubits using relevant basis transformations
                            // and a CNOT gate before the m* call.
                            let old_qubit_id = match new_args.first() {
                                Some(Operand::Literal(Literal::Qubit(id))) => *id,
                                _ => panic!("Expected the first argument to be a qubit literal"),
                            };
                            self.emit_state_transfer_instrs(
                                block,
                                basis,
                                old_qubit_id,
                                next_qubit_id,
                                metadata.clone(),
                            );
                            self.highest_used_id = self.highest_used_id.max(next_qubit_id);
                        } else {
                            // The call was to m* and the qubit is not reused later in the block, so
                            // there is no need to remap it at all as this is the last operation. Skip
                            // the rest of the logic.
                            block.0.push(Instruction::Call(
                                *call_id,
                                new_args,
                                None,
                                metadata.clone(),
                            ));
                            continue;
                        }
                    }

                    // If the call was to mresetz, replace with mz.
                    let call_id = if Some(*call_id) == self.measurement_ids.mresetz {
                        self.measurement_ids
                            .mz
                            .expect("MZ must be set if MResetZ is present")
                    } else {
                        *call_id
                    };

                    block
                        .0
                        .push(Instruction::Call(call_id, new_args, None, metadata.clone()));
                    if program.get_callable(call_id).call_type == CallableType::Measurement {
                        // Generate any new qubit ids after a measurement.
                        for arg in args {
                            if let Operand::Literal(Literal::Qubit(qubit_id)) = arg {
                                map.insert(*qubit_id, next_qubit_id);
                                next_qubit_id += 1;
                            }
                        }
                    }
                }
                _ => {
                    // Copy over the instruction.
                    block.0.push(instr.clone());
                }
            }
        }
    }

    /// Emits basis-dependent measurement state-transfer gates, with the new qubit in |0⟩.
    /// In execution order:
    /// - Z: CNOT from the old qubit to the new qubit.
    /// - X: H on the new qubit, then CNOT from the new qubit to the old qubit.
    /// - Y: S† on the old qubit, the X-basis sequence, then S on both qubits.
    ///
    /// For X, the basis changes simplify using
    /// `(H_1 H_2){CNOT}_{1,2}(H_1 I_2) = {CNOT}_{2,1}(I_1 H_2)`,
    /// where 1 is the old qubit, 2 is the new qubit, and operators act right to left.
    ///
    /// # Panics
    /// Panics if the required gate IDs are not set.
    fn emit_state_transfer_instrs(
        &mut self,
        block: &mut Block,
        basis: OneQubitMeasurement,
        old_qubit_id: u32,
        new_qubit_id: u32,
        metadata: Option<Box<InstructionDbgMetadata>>,
    ) {
        // For Y, map the old qubit's basis to X before the X-basis transfer.
        if basis == OneQubitMeasurement::Y {
            block.0.push(Instruction::Call(
                self.gate_ids
                    .sadj
                    .expect("SAdj must be set if Y-basis reindexing is required"),
                vec![Operand::Literal(Literal::Qubit(old_qubit_id))],
                None,
                metadata.clone(),
            ));
        }
        if basis == OneQubitMeasurement::X || basis == OneQubitMeasurement::Y {
            block.0.push(Instruction::Call(
                self.gate_ids
                    .h
                    .expect("H must be set if X-basis reindexing is required"),
                vec![Operand::Literal(Literal::Qubit(new_qubit_id))],
                None,
                metadata.clone(),
            ));
            block.0.push(Instruction::Call(
                self.gate_ids
                    .cx
                    .expect("CX must be set if any reindexing is required"),
                vec![
                    Operand::Literal(Literal::Qubit(new_qubit_id)),
                    Operand::Literal(Literal::Qubit(old_qubit_id)),
                ],
                None,
                metadata.clone(),
            ));
        }
        if basis == OneQubitMeasurement::Y {
            block.0.push(Instruction::Call(
                self.gate_ids
                    .s
                    .expect("S must be set if Y-basis reindexing is required"),
                vec![Operand::Literal(Literal::Qubit(old_qubit_id))],
                None,
                metadata.clone(),
            ));
            block.0.push(Instruction::Call(
                self.gate_ids
                    .s
                    .expect("S must be set if Y-basis reindexing is required"),
                vec![Operand::Literal(Literal::Qubit(new_qubit_id))],
                None,
                metadata.clone(),
            ));
        }

        // Z-basis transfer uses a CNOT from the old qubit to the new qubit.
        if basis == OneQubitMeasurement::Z {
            block.0.push(Instruction::Call(
                self.gate_ids
                    .cx
                    .expect("CX must be set if any reindexing is required"),
                vec![
                    Operand::Literal(Literal::Qubit(old_qubit_id)),
                    Operand::Literal(Literal::Qubit(new_qubit_id)),
                ],
                None,
                metadata,
            ));
        }
    }
}

fn validate_assumptions(program: &Program) {
    // Ensure only one callable with a body exists.
    for (callable_id, callable) in program.callables.iter() {
        assert!(
            callable.body.is_none() || callable_id == program.entry,
            "Only the entry point callable should have a body"
        );
    }

    // Ensure the program is a single block, as optimized reindexing across multiple blocks is not supported.
    assert_eq!(
        program.blocks.iter().count(),
        1,
        "Reindexing qubits across multiple blocks is not supported"
    );
}

fn find_callable(program: &Program, name: &str) -> Option<CallableId> {
    for (callable_id, callable) in program.callables.iter() {
        if callable.name == name {
            return Some(callable_id);
        }
    }
    None
}

fn add_callable(program: &mut Program, name: CallableToAdd) -> CallableId {
    let op_id = CallableId(
        program
            .callables
            .iter()
            .map(|(id, _)| id.0)
            .max()
            .expect("should be at least one callable")
            + 1,
    );
    match name {
        CallableToAdd::M => program.callables.insert(op_id, builder::m_decl()),
        CallableToAdd::CX => program.callables.insert(op_id, builder::cx_decl()),
        CallableToAdd::H => program.callables.insert(op_id, builder::h_decl()),
        CallableToAdd::S => program.callables.insert(op_id, builder::s_decl()),
        CallableToAdd::SAdj => program.callables.insert(op_id, builder::sadj_decl()),
    }
    op_id
}
