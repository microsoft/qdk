// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
    builder,
    rir::{
        BlockId, CallableId, Instruction, Literal, Operand, Prim, Program, Ty, Variable, VariableId,
    },
    utils::{get_all_block_successors, get_variable_assignments},
};

use super::result_literal::add_write_result;

/// Copies stored Result bits into destination-owned registers before stores can
/// be pruned. Measurement registers are reused by runtime loops, so retaining
/// their addresses does not retain the measured values.
///
/// For example, `saved = Store Result(0)` becomes:
/// ```text
/// bit = Call read_result(Result(0))
/// Call write_result(bit, Result(saved_register))
/// saved = Store Result(saved_register)
/// ```
/// Every store to `saved` reuses that register. Array stores read all source bits
/// before writing any destination element, including for overlapping copies.
/// Cyclic callable bodies and bodies with shared blocks keep their existing
/// storage: these static registers cannot provide per-recursive-frame lifetime.
pub fn copy_results(program: &mut Program) {
    let bodies = owned_nonrecursive_bodies(program);
    let mut copies = Copies::new(program);
    for blocks in bodies {
        let mut destinations: FxHashMap<VariableId, Vec<Operand>> = FxHashMap::default();
        for block_id in blocks {
            let instructions = std::mem::take(&mut program.get_block_mut(block_id).0);
            let mut output = Vec::with_capacity(instructions.len());
            for instruction in instructions {
                let (sources, destination) = match instruction {
                    Instruction::Store(source, var) if var.ty == Ty::Prim(Prim::Result) => {
                        (vec![source], var)
                    }
                    Instruction::StoreArray(sources, var) if matches!(var.ty, Ty::Array(size, Prim::Result) if size > 0) => {
                        (sources, var)
                    }
                    _ => {
                        output.push(instruction);
                        continue;
                    }
                };
                let registers = destinations
                    .entry(destination.variable_id)
                    .or_insert_with(|| {
                        sources
                            .iter()
                            .map(|_| {
                                let id = copies.next_result;
                                copies.next_result = id.checked_add(1).expect("result ID overflow");
                                Operand::Literal(Literal::Result(id))
                            })
                            .collect()
                    });
                let bits = sources
                    .into_iter()
                    .map(|source| copies.read(program, source, &mut output))
                    .collect::<Vec<_>>();
                let write_id = *copies
                    .write_id
                    .get_or_insert_with(|| add_write_result(program));
                for (bit, register) in bits.into_iter().zip(registers.iter().copied()) {
                    output.push(Instruction::Call(write_id, vec![bit, register], None, None));
                }
                output.push(match destination.ty {
                    Ty::Prim(Prim::Result) => Instruction::Store(registers[0], destination),
                    _ => Instruction::StoreArray(registers.clone(), destination),
                });
            }
            program.get_block_mut(block_id).0 = output;
        }
    }
    program.num_results = copies.next_result;
}

fn owned_nonrecursive_bodies(program: &Program) -> Vec<Vec<BlockId>> {
    let mut bodies = Vec::new();
    let mut owners: FxHashMap<BlockId, usize> = FxHashMap::default();
    let mut calls: FxHashMap<CallableId, FxHashSet<CallableId>> = FxHashMap::default();
    for (id, callable) in program.callables.iter() {
        let Some(entry) = callable.body else {
            continue;
        };
        let mut blocks = get_all_block_successors(entry, program);
        blocks.push(entry);
        blocks.sort_unstable();
        blocks.dedup();
        let callees = calls.entry(id).or_default();
        for &block_id in &blocks {
            *owners.entry(block_id).or_default() += 1;
            for instruction in &program.get_block(block_id).0 {
                if let Instruction::Call(callee, ..) = instruction {
                    callees.insert(*callee);
                }
            }
        }
        bodies.push((id, blocks));
    }
    bodies
        .into_iter()
        .filter(|(id, blocks)| {
            blocks.iter().all(|block| owners[block] == 1) && !is_recursive(*id, &calls)
        })
        .map(|(_, blocks)| blocks)
        .collect()
}

fn is_recursive(id: CallableId, calls: &FxHashMap<CallableId, FxHashSet<CallableId>>) -> bool {
    let mut visited = FxHashSet::default();
    let mut pending = vec![id];
    while let Some(current) = pending.pop() {
        if !visited.insert(current) {
            continue;
        }
        if let Some(callees) = calls.get(&current) {
            if callees.contains(&id) {
                return true;
            }
            pending.extend(callees);
        }
    }
    false
}

struct Copies {
    next_variable: u32,
    next_result: u32,
    read_id: Option<CallableId>,
    write_id: Option<CallableId>,
}

impl Copies {
    fn new(program: &Program) -> Self {
        let assignments = get_variable_assignments(program);
        let next_variable = assignments
            .iter()
            .map(|(id, _)| id.0)
            .chain(
                program
                    .callables
                    .values()
                    .flat_map(|callable| callable.input_vars.iter().map(|id| id.0)),
            )
            .max()
            .map_or(0, |id| id.checked_add(1).expect("variable ID overflow"));
        let mut next_result = program.num_results;
        for literal in program
            .blocks
            .values()
            .flat_map(|block| &block.0)
            .flat_map(operands)
            .filter_map(|operand| match operand {
                Operand::Literal(literal) => Some(literal),
                Operand::Variable(_) => None,
            })
            .chain(
                program
                    .array_literals
                    .iter()
                    .flat_map(|array| &array.contents),
            )
        {
            if let Literal::Result(id) = literal {
                next_result = next_result.max(id.checked_add(1).expect("result ID overflow"));
            }
        }
        Self {
            next_variable,
            next_result,
            read_id: None,
            write_id: None,
        }
    }

    fn read(
        &mut self,
        program: &mut Program,
        source: Operand,
        output: &mut Vec<Instruction>,
    ) -> Operand {
        if let Operand::Literal(Literal::ResultLit(bit)) = source {
            return Operand::Literal(Literal::Bool(bit));
        }
        let read_id = *self.read_id.get_or_insert_with(|| add_read_result(program));
        let variable = Variable::new_boolean(VariableId(self.next_variable));
        self.next_variable = self
            .next_variable
            .checked_add(1)
            .expect("variable ID overflow");
        output.push(Instruction::Call(
            read_id,
            vec![source],
            Some(variable),
            None,
        ));
        Operand::Variable(variable)
    }
}

fn add_read_result(program: &mut Program) -> CallableId {
    let declaration = builder::read_result_decl();
    if let Some((id, _)) = program.callables.iter().find(|(_, callable)| {
        callable.name == declaration.name
            && callable.body.is_none()
            && callable.input_type == declaration.input_type
            && callable.output_type == declaration.output_type
            && callable.call_type == declaration.call_type
    }) {
        return id;
    }
    let id = CallableId(
        program
            .callables
            .iter()
            .map(|(id, _)| id.0)
            .max()
            .expect("entry callable should exist")
            .checked_add(1)
            .expect("callable ID overflow"),
    );
    program.callables.insert(id, declaration);
    id
}

fn operands(instruction: &Instruction) -> Vec<&Operand> {
    match instruction {
        Instruction::Call(_, args, ..) | Instruction::StoreArray(args, _) => args.iter().collect(),
        Instruction::Store(source, _)
        | Instruction::Return(Some(source))
        | Instruction::LogicalNot(source, _)
        | Instruction::BitwiseNot(source, _)
        | Instruction::Convert(source, _) => vec![source],
        Instruction::Add(left, right, _)
        | Instruction::Sub(left, right, _)
        | Instruction::Mul(left, right, _)
        | Instruction::Sdiv(left, right, _)
        | Instruction::Srem(left, right, _)
        | Instruction::Shl(left, right, _)
        | Instruction::Ashr(left, right, _)
        | Instruction::Fadd(left, right, _)
        | Instruction::Fsub(left, right, _)
        | Instruction::Fmul(left, right, _)
        | Instruction::Fdiv(left, right, _)
        | Instruction::Frem(left, right, _)
        | Instruction::Fcmp(_, left, right, _)
        | Instruction::Icmp(_, left, right, _)
        | Instruction::LogicalAnd(left, right, _)
        | Instruction::LogicalOr(left, right, _)
        | Instruction::BitwiseAnd(left, right, _)
        | Instruction::BitwiseOr(left, right, _)
        | Instruction::BitwiseXor(left, right, _)
        | Instruction::Index(left, right, _) => vec![left, right],
        Instruction::Phi(args, _) => args.iter().map(|(operand, _)| operand).collect(),
        Instruction::Jump(_)
        | Instruction::Branch(..)
        | Instruction::Load(..)
        | Instruction::Alloca(_)
        | Instruction::Return(None) => vec![],
    }
}
