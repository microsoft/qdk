// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    debug::InstructionDbgMetadata,
    passes::utils::{add_cx, add_cz, add_h, add_m, find_callable},
    rir::{Block, CallableId, Instruction, Operand, Program},
};

#[allow(clippy::similar_names)]
pub(crate) fn decompose_joint_measurements(program: &mut Program) {
    let (mxx_id, mxz_id, mzz_id) = (
        find_callable(program, "__quantum__qis__mxx__body"),
        find_callable(program, "__quantum__qis__mxz__body"),
        find_callable(program, "__quantum__qis__mzz__body"),
    );

    if mxx_id.is_none() && mxz_id.is_none() && mzz_id.is_none() {
        // No use of joint measurements, so no decomposition is needed.
        return;
    }

    let (mut used_m, m_id) = match find_callable(program, "__quantum__qis__m__body") {
        Some(id) => (true, id),
        None => (false, add_m(program)),
    };
    let (mut used_cx, cx_id) = match find_callable(program, "__quantum__qis__cx__body") {
        Some(id) => (true, id),
        None => (false, add_cx(program)),
    };
    let (mut used_h, h_id) = match find_callable(program, "__quantum__qis__h__body") {
        Some(id) => (true, id),
        None => (false, add_h(program)),
    };
    let (mut used_cz, cz_id) = match find_callable(program, "__quantum__qis__cz__body") {
        Some(id) => (true, id),
        None => (false, add_cz(program)),
    };

    for block in program.blocks.values_mut() {
        let mut new_block = Block::default();
        for instr in block.0.drain(..) {
            let Instruction::Call(call_id, args, _, metadata) = &instr else {
                new_block.0.push(instr);
                continue;
            };
            if Some(*call_id) == mxx_id {
                used_cx = true;
                used_h = true;
                used_m = true;
                new_block.0.append(&mut generate_decomposed_mxx(
                    cx_id,
                    h_id,
                    m_id,
                    metadata.clone(),
                    args,
                ));
            } else if Some(*call_id) == mxz_id {
                // Decompose mxz
                used_cz = true;
                used_h = true;
                used_m = true;
                new_block.0.append(&mut generate_decomposed_mxz(
                    cz_id,
                    h_id,
                    m_id,
                    metadata.clone(),
                    args,
                ));
            } else if Some(*call_id) == mzz_id {
                // Decompose mzz
                used_cx = true;
                used_h = true;
                used_m = true;
                new_block.0.append(&mut generate_decomposed_mzz(
                    cx_id,
                    m_id,
                    metadata.clone(),
                    args,
                ));
            } else {
                new_block.0.push(instr);
            }
        }
        *block = new_block;
    }

    if !used_cx {
        program.callables.remove(cx_id);
    }
    if !used_cz {
        program.callables.remove(cz_id);
    }
    if !used_h {
        program.callables.remove(h_id);
    }
    if !used_m {
        program.callables.remove(m_id);
    }
    if let Some(mxx_id) = mxx_id {
        program.callables.remove(mxx_id);
    }
    if let Some(mxz_id) = mxz_id {
        program.callables.remove(mxz_id);
    }
    if let Some(mzz_id) = mzz_id {
        program.callables.remove(mzz_id);
    }
}

// Decompose mxx 0 1 to cx 0 1, h 0, m 0, h 0, cx 0 1
fn generate_decomposed_mxx(
    cx_id: CallableId,
    h_id: CallableId,
    m_id: CallableId,
    metadata: Option<Box<InstructionDbgMetadata>>,
    args: &[Operand],
) -> Vec<Instruction> {
    vec![
        Instruction::Call(cx_id, vec![args[0], args[1]], None, metadata.clone()),
        Instruction::Call(h_id, vec![args[0]], None, metadata.clone()),
        Instruction::Call(m_id, vec![args[0], args[2]], None, metadata.clone()),
        Instruction::Call(h_id, vec![args[0]], None, metadata.clone()),
        Instruction::Call(cx_id, vec![args[0], args[1]], None, metadata),
    ]
}

// Decompose mxz 0 1 to cz 0 1, h 0, m 0, h 0, cz 0 1
fn generate_decomposed_mxz(
    cz_id: CallableId,
    h_id: CallableId,
    m_id: CallableId,
    metadata: Option<Box<InstructionDbgMetadata>>,
    args: &[Operand],
) -> Vec<Instruction> {
    vec![
        Instruction::Call(cz_id, vec![args[0], args[1]], None, metadata.clone()),
        Instruction::Call(h_id, vec![args[0]], None, metadata.clone()),
        Instruction::Call(m_id, vec![args[0], args[2]], None, metadata.clone()),
        Instruction::Call(h_id, vec![args[0]], None, metadata.clone()),
        Instruction::Call(cz_id, vec![args[0], args[1]], None, metadata),
    ]
}

// Decompose mzz 0 1 to cx 1 0, m 0, cx 1 0
fn generate_decomposed_mzz(
    cx_id: CallableId,
    m_id: CallableId,
    metadata: Option<Box<InstructionDbgMetadata>>,
    args: &[Operand],
) -> Vec<Instruction> {
    vec![
        Instruction::Call(cx_id, vec![args[1], args[0]], None, metadata.clone()),
        Instruction::Call(m_id, vec![args[0], args[2]], None, metadata.clone()),
        Instruction::Call(cx_id, vec![args[1], args[0]], None, metadata),
    ]
}
