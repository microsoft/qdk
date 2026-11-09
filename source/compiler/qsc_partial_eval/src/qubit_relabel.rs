// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::{collections::BTreeMap, mem::take};

use qsc_rir::rir::{Instruction, Literal, Operand, Prim, Program, Ty, Variable};

use crate::management::ResourceManager;

/// Keeps logical qubit identities distinct from physical handles until a quantum call.
/// Copies and array indices retain labels, so aliases also observe subsequent relabels.
#[derive(Default)]
pub(super) struct RuntimeQubitRelabeling {
    bindings: BTreeMap<u32, Variable>,
}

impl RuntimeQubitRelabeling {
    pub fn swap(
        &mut self,
        left: usize,
        right: usize,
        resources: &mut ResourceManager,
    ) -> Vec<Instruction> {
        let left = self.binding(left, resources);
        let right = self.binding(right, resources);
        let temp = Variable {
            variable_id: resources.next_var(),
            ty: Ty::Prim(Prim::Qubit),
        };
        vec![
            Instruction::Store(Operand::Variable(left), temp),
            Instruction::Store(Operand::Variable(right), left),
            Instruction::Store(Operand::Variable(temp), right),
        ]
    }

    fn binding(&mut self, label: usize, resources: &mut ResourceManager) -> Variable {
        *self
            .bindings
            .entry(label.try_into().expect("qubit ID should fit into u32"))
            .or_insert_with(|| Variable {
                variable_id: resources.next_var(),
                ty: Ty::Prim(Prim::Qubit),
            })
    }

    pub fn lower(self, program: &mut Program, resources: &mut ResourceManager) {
        if self.bindings.is_empty() {
            return;
        }

        for (_, block) in program.blocks.iter_mut() {
            for mut instruction in take(&mut block.0) {
                if let Instruction::Call(_, args, _, _) = &mut instruction {
                    for arg in args {
                        match *arg {
                            Operand::Literal(Literal::Qubit(label)) => {
                                if let Some(binding) = self.bindings.get(&label) {
                                    *arg = Operand::Variable(*binding);
                                }
                            }
                            Operand::Variable(label) if label.ty == Ty::Prim(Prim::Qubit) => {
                                // Runtime-selected qubits still contain logical IDs. Resolve them
                                // at the call, not at the index or binding that created the alias.
                                let index = Variable::new_integer(resources.next_var());
                                block.0.push(Instruction::Convert(*arg, index));
                                let bindings = Variable {
                                    variable_id: resources.next_var(),
                                    ty: Ty::Array(program.num_qubits as usize, Prim::Qubit),
                                };
                                let operands = (0..program.num_qubits)
                                    .map(|id| {
                                        self.bindings.get(&id).map_or(
                                            Operand::Literal(Literal::Qubit(id)),
                                            |binding| Operand::Variable(*binding),
                                        )
                                    })
                                    .collect();
                                block.0.push(Instruction::StoreArray(operands, bindings));
                                let physical = Variable {
                                    variable_id: resources.next_var(),
                                    ty: Ty::Prim(Prim::Qubit),
                                };
                                block.0.push(Instruction::Index(
                                    Operand::Variable(bindings),
                                    Operand::Variable(index),
                                    physical,
                                ));
                                *arg = Operand::Variable(physical);
                            }
                            _ => {}
                        }
                    }
                }
                block.0.push(instruction);
            }
        }

        // These initializers dominate every branch and loop, including uses emitted before
        // the first relabel. Static allocation still determines required_num_qubits.
        let entry = program
            .get_callable(program.entry)
            .body
            .expect("entry body");
        let initializers = self
            .bindings
            .into_iter()
            .map(|(id, binding)| Instruction::Store(Operand::Literal(Literal::Qubit(id)), binding));
        program.get_block_mut(entry).0.splice(1..1, initializers);
    }
}
