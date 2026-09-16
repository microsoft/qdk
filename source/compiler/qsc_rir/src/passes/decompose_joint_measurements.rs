// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    debug::InstructionDbgMetadata,
    passes::utils::{add_cx, add_h, add_m, add_s, add_s_adj, find_callable},
    rir::{Block, CallableId, Instruction, Operand, Program},
};

#[allow(clippy::similar_names, clippy::too_many_lines)]
pub(crate) fn decompose_joint_measurements(program: &mut Program) {
    let (mxx_id, myy_id, mzz_id, myz_id) = (
        find_callable(program, "__quantum__qis__mxx__body"),
        find_callable(program, "__quantum__qis__myy__body"),
        find_callable(program, "__quantum__qis__mzz__body"),
        find_callable(program, "__quantum__qis__myz__body"),
    );

    if mxx_id.is_none() && myy_id.is_none() && mzz_id.is_none() && myz_id.is_none() {
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
    let (mut used_s, s_id) = match find_callable(program, "__quantum__qis__s__body") {
        Some(id) => (true, id),
        None => (false, add_s(program)),
    };
    let (mut used_s_adj, s_adj_id) = match find_callable(program, "__quantum__qis__s__adj") {
        Some(id) => (true, id),
        None => (false, add_s_adj(program)),
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
            } else if Some(*call_id) == myy_id {
                // Decompose myy
                used_cx = true;
                used_s_adj = true;
                used_s = true;
                used_h = true;
                used_m = true;
                new_block.0.append(&mut generate_decomposed_myy(
                    cx_id,
                    s_id,
                    s_adj_id,
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
            } else if Some(*call_id) == myz_id {
                // Decompose myz
                used_cx = true;
                used_s_adj = true;
                used_s = true;
                used_h = true;
                used_m = true;
                new_block.0.append(&mut generate_decomposed_myz(
                    cx_id,
                    s_id,
                    s_adj_id,
                    h_id,
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
    if !used_h {
        program.callables.remove(h_id);
    }
    if !used_s {
        program.callables.remove(s_id);
    }
    if !used_s_adj {
        program.callables.remove(s_adj_id);
    }
    if !used_m {
        program.callables.remove(m_id);
    }
    if let Some(mxx_id) = mxx_id {
        program.callables.remove(mxx_id);
    }
    if let Some(myy_id) = myy_id {
        program.callables.remove(myy_id);
    }
    if let Some(mzz_id) = mzz_id {
        program.callables.remove(mzz_id);
    }
    if let Some(myz_id) = myz_id {
        program.callables.remove(myz_id);
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

// Decompose myy 0 1 to sdg 0, sdg 1, cx 0 1, h 0, m 0, h 0, cx 0 1, s 1, s 0
fn generate_decomposed_myy(
    cx_id: CallableId,
    s_id: CallableId,
    s_adj_id: CallableId,
    h_id: CallableId,
    m_id: CallableId,
    metadata: Option<Box<InstructionDbgMetadata>>,
    args: &[Operand],
) -> Vec<Instruction> {
    vec![
        Instruction::Call(s_adj_id, vec![args[0]], None, metadata.clone()),
        Instruction::Call(s_adj_id, vec![args[1]], None, metadata.clone()),
        Instruction::Call(cx_id, vec![args[0], args[1]], None, metadata.clone()),
        Instruction::Call(h_id, vec![args[0]], None, metadata.clone()),
        Instruction::Call(m_id, vec![args[0], args[2]], None, metadata.clone()),
        Instruction::Call(h_id, vec![args[0]], None, metadata.clone()),
        Instruction::Call(cx_id, vec![args[0], args[1]], None, metadata.clone()),
        Instruction::Call(s_id, vec![args[1]], None, metadata.clone()),
        Instruction::Call(s_id, vec![args[0]], None, metadata),
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

// Decompose myz 0 1 to sdg 0, h 0, cx 1 0, m 0, cx 1 0, h 0, s 0
fn generate_decomposed_myz(
    cx_id: CallableId,
    s_id: CallableId,
    s_adj_id: CallableId,
    h_id: CallableId,
    m_id: CallableId,
    metadata: Option<Box<InstructionDbgMetadata>>,
    args: &[Operand],
) -> Vec<Instruction> {
    vec![
        Instruction::Call(s_adj_id, vec![args[0]], None, metadata.clone()),
        Instruction::Call(h_id, vec![args[0]], None, metadata.clone()),
        Instruction::Call(cx_id, vec![args[1], args[0]], None, metadata.clone()),
        Instruction::Call(m_id, vec![args[0], args[2]], None, metadata.clone()),
        Instruction::Call(cx_id, vec![args[1], args[0]], None, metadata.clone()),
        Instruction::Call(h_id, vec![args[0]], None, metadata.clone()),
        Instruction::Call(s_id, vec![args[0]], None, metadata),
    ]
}
