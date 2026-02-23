// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_data_structures::index_map::IndexMap;

use crate::{
    rir::{BlockId, InstructionKind, Operand, Program, VariableId},
    utils::get_variable_assignments,
};

#[cfg(test)]
mod tests;

/// Verifies that the program is in Single Static Assignment (SSA) form.
/// This check ensures that:
/// - Each variable is assigned exactly once.
/// - Each variable is used after it is assigned.
/// - Each variable is used in a block that is dominated by the block in which it is assigned.
/// - Each phi node references only its predecessors.
pub fn check_ssa_form(
    program: &Program,
    preds: &IndexMap<BlockId, Vec<BlockId>>,
    doms: &IndexMap<BlockId, BlockId>,
) {
    check_phi_nodes(program, preds);
    let variable_assignments = get_variable_assignments(program);
    let variable_uses = get_variable_uses(program);

    for (var_id, uses) in variable_uses.iter() {
        let Some((def_block_id, def_idx)) = variable_assignments.get(var_id) else {
            panic!("{var_id:?} is used but not assigned");
        };
        for (use_block_id, use_idx) in uses {
            if use_block_id == def_block_id {
                assert!(
                    use_idx > def_idx,
                    "{var_id:?} is used before it is assigned in {use_block_id:?}, instruction {use_idx:?}"
                );
            } else {
                let mut dominator = doms
                    .get(*use_block_id)
                    .expect("all blocks should have dominator");
                while dominator != def_block_id {
                    let new_dom = doms
                        .get(*dominator)
                        .expect("all blocks should have dominator");
                    assert!(
                        new_dom != dominator || new_dom == def_block_id,
                        "Definition of {var_id:?} in {def_block_id:?} does not dominate use in {use_block_id:?}, instruction {use_idx:?}"
                    );
                    dominator = new_dom;
                }
            }
        }
    }
}

fn check_phi_nodes(program: &Program, preds: &IndexMap<BlockId, Vec<BlockId>>) {
    for (block_id, block) in program.blocks.iter() {
        let Some(block_preds) = preds.get(block_id) else {
            // Block with no predecessors cannot have phi nodes.
            assert!(
                block
                    .0
                    .iter()
                    .all(|instr| !matches!(instr.kind, InstructionKind::Phi(..))),
                "{block_id:?} has phi nodes but no predecessors"
            );
            continue;
        };
        for instr in &block.0 {
            if let InstructionKind::Phi(args, res) = &instr.kind {
                assert!(
                    block_preds.len() == args.len(),
                    "Phi node in {block_id:?} has {} arguments but {} predecessors",
                    args.len(),
                    block_preds.len()
                );
                for (val, pred_block_id) in args {
                    assert!(
                        preds
                            .get(block_id)
                            .expect("block with phi should have predecessors")
                            .contains(pred_block_id),
                        "Phi node in {block_id:?} references a non-predecessor {pred_block_id:?}"
                    );
                    if let Operand::Variable(var) = val {
                        assert!(
                            var.variable_id.0 != res.variable_id.0,
                            "Phi node in {block_id:?} assigns to {:?} to itself",
                            res.variable_id
                        );
                    }
                }
            }
        }
    }
}

#[allow(clippy::too_many_lines)]
fn get_variable_uses(program: &Program) -> IndexMap<VariableId, Vec<(BlockId, usize)>> {
    let mut uses: IndexMap<VariableId, Vec<(BlockId, usize)>> = IndexMap::default();
    let mut add_use = |var_id, block_id, idx| {
        if let Some(entry) = uses.get_mut(var_id) {
            entry.push((block_id, idx));
        } else {
            uses.insert(var_id, vec![(block_id, idx)]);
        }
    };
    for (block_id, block) in program.blocks.iter() {
        for (idx, instr) in block.0.iter().enumerate() {
            match &instr.kind {
                // Single variable
                InstructionKind::Add(Operand::Variable(var), Operand::Literal(_), _)
                | InstructionKind::Add(Operand::Literal(_), Operand::Variable(var), _)
                | InstructionKind::Sub(Operand::Variable(var), Operand::Literal(_), _)
                | InstructionKind::Sub(Operand::Literal(_), Operand::Variable(var), _)
                | InstructionKind::Mul(Operand::Variable(var), Operand::Literal(_), _)
                | InstructionKind::Mul(Operand::Literal(_), Operand::Variable(var), _)
                | InstructionKind::Sdiv(Operand::Variable(var), Operand::Literal(_), _)
                | InstructionKind::Sdiv(Operand::Literal(_), Operand::Variable(var), _)
                | InstructionKind::Srem(Operand::Variable(var), Operand::Literal(_), _)
                | InstructionKind::Srem(Operand::Literal(_), Operand::Variable(var), _)
                | InstructionKind::Shl(Operand::Variable(var), Operand::Literal(_), _)
                | InstructionKind::Shl(Operand::Literal(_), Operand::Variable(var), _)
                | InstructionKind::Ashr(Operand::Variable(var), Operand::Literal(_), _)
                | InstructionKind::Ashr(Operand::Literal(_), Operand::Variable(var), _)
                | InstructionKind::Fadd(Operand::Variable(var), Operand::Literal(_), _)
                | InstructionKind::Fadd(Operand::Literal(_), Operand::Variable(var), _)
                | InstructionKind::Fsub(Operand::Variable(var), Operand::Literal(_), _)
                | InstructionKind::Fsub(Operand::Literal(_), Operand::Variable(var), _)
                | InstructionKind::Fmul(Operand::Variable(var), Operand::Literal(_), _)
                | InstructionKind::Fmul(Operand::Literal(_), Operand::Variable(var), _)
                | InstructionKind::Fdiv(Operand::Variable(var), Operand::Literal(_), _)
                | InstructionKind::Fdiv(Operand::Literal(_), Operand::Variable(var), _)
                | InstructionKind::Fcmp(_, Operand::Variable(var), Operand::Literal(_), _)
                | InstructionKind::Fcmp(_, Operand::Literal(_), Operand::Variable(var), _)
                | InstructionKind::Icmp(_, Operand::Variable(var), Operand::Literal(_), _)
                | InstructionKind::Icmp(_, Operand::Literal(_), Operand::Variable(var), _)
                | InstructionKind::LogicalNot(Operand::Variable(var), _)
                | InstructionKind::LogicalAnd(Operand::Variable(var), Operand::Literal(_), _)
                | InstructionKind::LogicalAnd(Operand::Literal(_), Operand::Variable(var), _)
                | InstructionKind::LogicalOr(Operand::Variable(var), Operand::Literal(_), _)
                | InstructionKind::LogicalOr(Operand::Literal(_), Operand::Variable(var), _)
                | InstructionKind::BitwiseNot(Operand::Variable(var), _)
                | InstructionKind::BitwiseAnd(Operand::Variable(var), Operand::Literal(_), _)
                | InstructionKind::BitwiseAnd(Operand::Literal(_), Operand::Variable(var), _)
                | InstructionKind::BitwiseOr(Operand::Variable(var), Operand::Literal(_), _)
                | InstructionKind::BitwiseOr(Operand::Literal(_), Operand::Variable(var), _)
                | InstructionKind::BitwiseXor(Operand::Variable(var), Operand::Literal(_), _)
                | InstructionKind::BitwiseXor(Operand::Literal(_), Operand::Variable(var), _)
                | InstructionKind::Branch(var, _, _) => {
                    add_use(var.variable_id, block_id, idx);
                }

                // Double variable
                InstructionKind::Add(Operand::Variable(var1), Operand::Variable(var2), _)
                | InstructionKind::Sub(Operand::Variable(var1), Operand::Variable(var2), _)
                | InstructionKind::Mul(Operand::Variable(var1), Operand::Variable(var2), _)
                | InstructionKind::Sdiv(Operand::Variable(var1), Operand::Variable(var2), _)
                | InstructionKind::Srem(Operand::Variable(var1), Operand::Variable(var2), _)
                | InstructionKind::Shl(Operand::Variable(var1), Operand::Variable(var2), _)
                | InstructionKind::Ashr(Operand::Variable(var1), Operand::Variable(var2), _)
                | InstructionKind::Fadd(Operand::Variable(var1), Operand::Variable(var2), _)
                | InstructionKind::Fsub(Operand::Variable(var1), Operand::Variable(var2), _)
                | InstructionKind::Fmul(Operand::Variable(var1), Operand::Variable(var2), _)
                | InstructionKind::Fdiv(Operand::Variable(var1), Operand::Variable(var2), _)
                | InstructionKind::Fcmp(_, Operand::Variable(var1), Operand::Variable(var2), _)
                | InstructionKind::Icmp(_, Operand::Variable(var1), Operand::Variable(var2), _)
                | InstructionKind::LogicalAnd(
                    Operand::Variable(var1),
                    Operand::Variable(var2),
                    _,
                )
                | InstructionKind::LogicalOr(Operand::Variable(var1), Operand::Variable(var2), _)
                | InstructionKind::BitwiseAnd(
                    Operand::Variable(var1),
                    Operand::Variable(var2),
                    _,
                )
                | InstructionKind::BitwiseOr(Operand::Variable(var1), Operand::Variable(var2), _)
                | InstructionKind::BitwiseXor(
                    Operand::Variable(var1),
                    Operand::Variable(var2),
                    _,
                ) => {
                    add_use(var1.variable_id, block_id, idx);
                    add_use(var2.variable_id, block_id, idx);
                }

                // Multiple variables
                InstructionKind::Call(_, vals, _) => {
                    for val in vals {
                        if let Operand::Variable(var) = val {
                            add_use(var.variable_id, block_id, idx);
                        }
                    }
                }
                InstructionKind::Phi(args, _) => {
                    for (val, pred_block_id) in args {
                        if let Operand::Variable(var) = val {
                            // As a special case for phi, treat the variable as used in the predecessor block
                            // rather than the phi block, at the max instruction index to avoid failing the
                            // dominance check within that block.
                            add_use(var.variable_id, *pred_block_id, usize::MAX);
                        }
                    }
                }

                // If an instruction has no variables, it should have been inlined by partial eval.
                InstructionKind::Add(Operand::Literal(_), Operand::Literal(_), _)
                | InstructionKind::Sub(Operand::Literal(_), Operand::Literal(_), _)
                | InstructionKind::Mul(Operand::Literal(_), Operand::Literal(_), _)
                | InstructionKind::Sdiv(Operand::Literal(_), Operand::Literal(_), _)
                | InstructionKind::Srem(Operand::Literal(_), Operand::Literal(_), _)
                | InstructionKind::Shl(Operand::Literal(_), Operand::Literal(_), _)
                | InstructionKind::Ashr(Operand::Literal(_), Operand::Literal(_), _)
                | InstructionKind::Fadd(Operand::Literal(_), Operand::Literal(_), _)
                | InstructionKind::Fsub(Operand::Literal(_), Operand::Literal(_), _)
                | InstructionKind::Fmul(Operand::Literal(_), Operand::Literal(_), _)
                | InstructionKind::Fdiv(Operand::Literal(_), Operand::Literal(_), _)
                | InstructionKind::Fcmp(_, Operand::Literal(_), Operand::Literal(_), _)
                | InstructionKind::Icmp(_, Operand::Literal(_), Operand::Literal(_), _)
                | InstructionKind::LogicalNot(Operand::Literal(_), _)
                | InstructionKind::LogicalAnd(Operand::Literal(_), Operand::Literal(_), _)
                | InstructionKind::LogicalOr(Operand::Literal(_), Operand::Literal(_), _)
                | InstructionKind::BitwiseNot(Operand::Literal(_), _)
                | InstructionKind::BitwiseAnd(Operand::Literal(_), Operand::Literal(_), _)
                | InstructionKind::BitwiseOr(Operand::Literal(_), Operand::Literal(_), _)
                | InstructionKind::BitwiseXor(Operand::Literal(_), Operand::Literal(_), _) => {
                    panic!("{block_id:?}, instruction {idx} has no variables: {instr}")
                }

                InstructionKind::Jump(..) | InstructionKind::Return => {}

                InstructionKind::Store(..) => {
                    panic!("Unexpected Store at {block_id:?}, instruction {idx}")
                }
            }
        }
    }
    uses
}
