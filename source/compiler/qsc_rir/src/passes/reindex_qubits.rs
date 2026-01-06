// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use std::ops::Sub;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
    builder,
    rir::{Block, CallableId, CallableType, Instruction, Literal, Operand, Program},
};

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

    let (used_m, m_id) = match find_callable(program, "__quantum__qis__m__body") {
        Some(id) => (true, id),
        None => (false, add_m(program)),
    };
    let (used_cx, cx_id) = match find_callable(program, "__quantum__qis__cx__body") {
        Some(id) => (true, id),
        None => (false, add_cx(program)),
    };
    let mresetz_id = find_callable(program, "__quantum__qis__mresetz__body");
    let mut pass = ReindexQubitPass {
        used_m,
        m_id,
        used_cx,
        cx_id,
        mresetz_id,
        // For this calculation, qubit IDs can never be lower than zero but a program may not use
        // any qubits. Since `highest_used_id` is only needed for remapping pass and a program without any
        // qubits won't do any remapping, it's safe to treat this as 1 and let `highest_used_id` default
        // to zero.
        highest_used_id: program.num_qubits.max(1).sub(1),
    };

    // Perform the reindexing on the single entry block.
    let Some((block_id, mut block)) = program.blocks.drain().next() else {
        panic!("program should have at least one block");
    };
    pass.reindex_qubits_in_block(program, &mut block);
    program.blocks.insert(block_id, block);
    program.num_qubits = pass.highest_used_id + 1;

    // All reset function calls should be removed, so remove them from the callables.
    program
        .callables
        .retain(|id, callable| callable.call_type != CallableType::Reset && Some(id) != mresetz_id);

    // If mz or cx were added but not used, remove them.
    if !pass.used_m {
        program.callables.remove(m_id);
    }
    if !pass.used_cx {
        program.callables.remove(cx_id);
    }
}

struct ReindexQubitPass {
    used_m: bool,
    m_id: CallableId,
    used_cx: bool,
    cx_id: CallableId,
    mresetz_id: Option<CallableId>,
    highest_used_id: u32,
}

impl ReindexQubitPass {
    fn reindex_qubits_in_block(&mut self, program: &Program, block: &mut Block) {
        let mut map = FxHashMap::default();
        let mut used_qubits: FxHashSet<u32> = FxHashSet::default();
        let mut next_qubit_id = self.highest_used_id + 1;
        let instrs = std::mem::take(&mut block.0);
        for i in 0..instrs.len() {
            // Assume qubits only appear in void call instructions.
            let instr = &instrs[i];
            match instr {
                Instruction::Call(call_id, args, _)
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
                Instruction::Call(call_id, args, None) => {
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

                    if *call_id == self.m_id {
                        if qubit_used_in_instrs(
                            *ids_used
                                .first()
                                .expect("measurement call should have at least one argument"),
                            instrs.iter().skip(i + 1),
                        ) {
                            // Since the call was to mz and the qubit is reused later in the block,
                            // the new qubit replacing this one must be conditionally flipped.
                            // Achieve this by adding a CNOT gate before the mz call.
                            self.used_cx = true;
                            block.0.push(Instruction::Call(
                                self.cx_id,
                                vec![new_args[0], Operand::Literal(Literal::Qubit(next_qubit_id))],
                                None,
                            ));
                            self.highest_used_id = self.highest_used_id.max(next_qubit_id);
                        } else {
                            // The call was to mz and the qubit is not reused later in the block, so
                            // there is no need to remap it at all as this is the last operation. Skip
                            // the rest of the logic.
                            block.0.push(Instruction::Call(*call_id, new_args, None));
                            continue;
                        }
                    }

                    // If the call was to mresetz, replace with mz.
                    let call_id = if Some(*call_id) == self.mresetz_id {
                        self.used_m = true;
                        self.m_id
                    } else {
                        *call_id
                    };

                    block.0.push(Instruction::Call(call_id, new_args, None));

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
}

fn qubit_used_in_instrs<'a>(id: u32, instrs: impl Iterator<Item = &'a Instruction>) -> bool {
    for instr in instrs {
        if let Instruction::Call(_, args, _) = instr {
            for arg in args {
                if let Operand::Literal(Literal::Qubit(qubit_id)) = arg
                    && *qubit_id == id
                {
                    return true;
                }
            }
        }
    }
    false
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

fn add_m(program: &mut Program) -> CallableId {
    let m_id = CallableId(
        program
            .callables
            .iter()
            .map(|(id, _)| id.0)
            .max()
            .expect("should be at least one callable")
            + 1,
    );
    program.callables.insert(m_id, builder::m_decl());
    m_id
}

fn add_cx(program: &mut Program) -> CallableId {
    let cx_id = CallableId(
        program
            .callables
            .iter()
            .map(|(id, _)| id.0)
            .max()
            .expect("should be at least one callable")
            + 1,
    );
    program.callables.insert(cx_id, builder::cx_decl());
    cx_id
}
