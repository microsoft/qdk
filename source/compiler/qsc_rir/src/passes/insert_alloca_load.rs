// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_data_structures::index_map::IndexMap;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
    rir::{
        AdvancedInstr, BlockId, CallableId, Instruction, Operand, Program, Variable, VariableId,
    },
    utils::{get_block_successors, get_variable_assignments},
};

pub fn insert_alloca_load_instrs(program: &mut Program) {
    // Get the next available variable ID for use in newly generated loads.
    let mut next_var_id = get_variable_assignments(program)
        .iter()
        .next_back()
        .map(|(var_id, _)| var_id.successor())
        .unwrap_or_default();

    for callable_id in program.all_callable_ids() {
        process_callable(program, callable_id, &mut next_var_id);
    }
}

fn process_callable(program: &mut Program, callable_id: CallableId, next_var_id: &mut VariableId) {
    let callable = program.get_callable(callable_id);

    let Some(entry_block_id) = callable.body else {
        return;
    };

    let mut vars_to_alloca = IndexMap::default();
    let mut visited_blocks = FxHashSet::default();
    let mut blocks_to_visit = vec![entry_block_id];
    while let Some(block_id) = blocks_to_visit.pop() {
        if visited_blocks.contains(&block_id) {
            continue;
        }
        visited_blocks.insert(block_id);
        add_alloca_load_to_block(program, block_id, &mut vars_to_alloca, next_var_id);
        for successor_id in get_block_successors(program.get_block(block_id)) {
            if !visited_blocks.contains(&successor_id) {
                blocks_to_visit.push(successor_id);
            }
        }
    }

    let mut alloca_instrs = Vec::new();
    for (_, variable) in vars_to_alloca.iter() {
        alloca_instrs.push(AdvancedInstr::Alloca(*variable).into());
    }
    let entry_block = program.get_block_mut(entry_block_id);
    let new_instrs = alloca_instrs
        .into_iter()
        .chain(entry_block.0.drain(..))
        .collect();
    entry_block.0 = new_instrs;
}

fn add_alloca_load_to_block(
    program: &mut Program,
    block_id: BlockId,
    vars_to_alloca: &mut IndexMap<VariableId, Variable>,
    next_var_id: &mut VariableId,
) {
    let block = program.get_block_mut(block_id);
    let instrs = block.0.drain(..).collect::<Vec<_>>();
    let mut var_map = FxHashMap::default();
    for mut instr in instrs {
        match &mut instr {
            // Track that this is a value that needs to be allocated, and clear any previous loaded variables.
            Instruction::Store(operand, var) => {
                vars_to_alloca.insert(var.variable_id, *var);
                let new_operand = map_or_load_operand(
                    operand,
                    &mut var_map,
                    &mut block.0,
                    next_var_id,
                    should_load_operand(operand, vars_to_alloca),
                );
                block.0.push(Instruction::Store(new_operand, *var));
                *next_var_id = next_var_id.successor();
                continue;
            }

            // Replace any arguments with the new values of stored variables.
            Instruction::Call(_, args, _) => {
                *args = args
                    .iter()
                    .map(|arg| match arg {
                        Operand::Variable(var) => map_or_load_variable_to_operand(
                            *var,
                            &mut var_map,
                            &mut block.0,
                            next_var_id,
                            vars_to_alloca.contains_key(var.variable_id),
                        ),
                        Operand::Literal(_) => *arg,
                    })
                    .collect();
            }

            // Replace the branch condition with the new value of the variable.
            Instruction::Branch(var, _, _) => {
                *var = map_or_load_variable(
                    *var,
                    &mut var_map,
                    &mut block.0,
                    next_var_id,
                    vars_to_alloca.contains_key(var.variable_id),
                );
            }

            // Two variable instructions, replace left and right operands with new values.
            Instruction::Add(lhs, rhs, _)
            | Instruction::Sub(lhs, rhs, _)
            | Instruction::Mul(lhs, rhs, _)
            | Instruction::Sdiv(lhs, rhs, _)
            | Instruction::Srem(lhs, rhs, _)
            | Instruction::Shl(lhs, rhs, _)
            | Instruction::Ashr(lhs, rhs, _)
            | Instruction::Fadd(lhs, rhs, _)
            | Instruction::Fsub(lhs, rhs, _)
            | Instruction::Fmul(lhs, rhs, _)
            | Instruction::Fdiv(lhs, rhs, _)
            | Instruction::Fcmp(_, lhs, rhs, _)
            | Instruction::Icmp(_, lhs, rhs, _)
            | Instruction::LogicalAnd(lhs, rhs, _)
            | Instruction::LogicalOr(lhs, rhs, _)
            | Instruction::BitwiseAnd(lhs, rhs, _)
            | Instruction::BitwiseOr(lhs, rhs, _)
            | Instruction::BitwiseXor(lhs, rhs, _) => {
                *lhs = map_or_load_operand(
                    lhs,
                    &mut var_map,
                    &mut block.0,
                    next_var_id,
                    should_load_operand(lhs, vars_to_alloca),
                );
                *rhs = map_or_load_operand(
                    rhs,
                    &mut var_map,
                    &mut block.0,
                    next_var_id,
                    should_load_operand(rhs, vars_to_alloca),
                );
            }

            // Single variable instructions, replace operand with new value.
            Instruction::BitwiseNot(operand, _)
            | Instruction::LogicalNot(operand, _)
            | Instruction::Convert(operand, _) => {
                *operand = map_or_load_operand(
                    operand,
                    &mut var_map,
                    &mut block.0,
                    next_var_id,
                    should_load_operand(operand, vars_to_alloca),
                );
            }

            // Phi nodes are handled separately in the SSA transformation, but need to be passed through
            // like the unconditional terminators.
            Instruction::Phi(..) | Instruction::Jump(..) | Instruction::Return => {}

            Instruction::Advanced(AdvancedInstr::Alloca(..)) => {
                panic!("alloca not expected in alloca insertion")
            }
            Instruction::Advanced(AdvancedInstr::Load(..)) => {
                panic!("load not expected in alloca insertion")
            }
        }
        block.0.push(instr);
    }
}

fn should_load_operand(operand: &Operand, vars_to_alloca: &IndexMap<VariableId, Variable>) -> bool {
    match operand {
        Operand::Literal(_) => false,
        Operand::Variable(var) => vars_to_alloca.contains_key(var.variable_id),
    }
}

fn map_or_load_operand(
    operand: &Operand,
    var_map: &mut FxHashMap<VariableId, Operand>,
    instrs: &mut Vec<Instruction>,
    next_var_id: &mut VariableId,
    should_load: bool,
) -> Operand {
    match operand {
        Operand::Literal(_) => *operand,
        Operand::Variable(var) => {
            map_or_load_variable_to_operand(*var, var_map, instrs, next_var_id, should_load)
        }
    }
}

fn map_or_load_variable_to_operand(
    variable: Variable,
    var_map: &mut FxHashMap<VariableId, Operand>,
    instrs: &mut Vec<Instruction>,
    next_var_id: &mut VariableId,
    should_load: bool,
) -> Operand {
    if let Some(operand) = var_map.get(&variable.variable_id) {
        *operand
    } else if should_load {
        let new_var = Variable {
            variable_id: *next_var_id,
            ty: variable.ty,
        };
        instrs.push(AdvancedInstr::Load(variable, new_var).into());
        var_map.insert(variable.variable_id, Operand::Variable(new_var));
        *next_var_id = next_var_id.successor();
        Operand::Variable(new_var)
    } else {
        Operand::Variable(variable)
    }
}

fn map_or_load_variable(
    variable: Variable,
    var_map: &mut FxHashMap<VariableId, Operand>,
    instrs: &mut Vec<Instruction>,
    next_var_id: &mut VariableId,
    should_load: bool,
) -> Variable {
    match var_map.get(&variable.variable_id) {
        Some(Operand::Variable(var)) => *var,
        Some(Operand::Literal(_)) => panic!("literal not expected in variable mapping"),
        None => {
            if should_load {
                let new_var = Variable {
                    variable_id: *next_var_id,
                    ty: variable.ty,
                };
                instrs.push(AdvancedInstr::Load(variable, new_var).into());
                var_map.insert(variable.variable_id, Operand::Variable(new_var));
                *next_var_id = next_var_id.successor();
                new_var
            } else {
                variable
            }
        }
    }
}
