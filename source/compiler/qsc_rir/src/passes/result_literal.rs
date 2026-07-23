// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
use crate::{
    builder,
    rir::{CallableId, Instruction, Literal, Operand, Program},
};

/// Transforms result literals in the program.
/// Since result literals are not supported in QIR, this function attempts to handle them as best as possible.
/// A result literal of `Zero` (or false) will be replaced with an additional result id that is never measured into,
/// which defaults to returning false or 0 if read or recorded.
/// A result literal of `One` cannot be handled and is left un-transformed so that later checks on the program can
/// reject it as incompatible.
pub fn transform_result_literals(program: &mut Program) {
    let result_zero_id = program.num_results;
    let result_one_id = program.num_results + 1;
    let mut replaced_zero = false;
    let mut replaced_one = false;

    for block in program.blocks.values_mut() {
        for instr in &mut block.0 {
            // Result literals are only expected in context of Store, Call, or Return instructions
            match instr {
                Instruction::Store(operand, _) | Instruction::Return(Some(operand)) => {
                    let Operand::Literal(Literal::ResultLit(val)) = *operand else {
                        continue;
                    };
                    let id = if val {
                        replaced_one = true;
                        result_one_id
                    } else {
                        replaced_zero = true;
                        result_zero_id
                    };
                    *operand = Operand::Literal(Literal::Result(id));
                }

                Instruction::Call(_, operands, _, _) => {
                    for operand in operands.iter_mut() {
                        if let Operand::Literal(Literal::ResultLit(val)) = *operand {
                            let id = if val {
                                replaced_one = true;
                                result_one_id
                            } else {
                                replaced_zero = true;
                                result_zero_id
                            };
                            *operand = Operand::Literal(Literal::Result(id));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // Also check the static arrays in global section for the presence of result literals, and replace them with the new result ids.
    for array in &mut program.array_literals {
        for elem in &mut array.contents {
            if let Literal::ResultLit(val) = elem {
                let id = if *val {
                    replaced_one = true;
                    result_one_id
                } else {
                    replaced_zero = true;
                    result_zero_id
                };
                *elem = Literal::Result(id);
            }
        }
    }

    if replaced_zero || replaced_one {
        let write_id = add_write_result(program);
        let entry_block_id = program
            .callables
            .get(program.entry)
            .expect("entry point should exist")
            .body
            .expect("entry point should have a body");
        let entry_block = program
            .blocks
            .get_mut(entry_block_id)
            .expect("entry block should exist");
        let mut instructions = Vec::new();
        if replaced_zero {
            instructions.push(Instruction::Call(
                write_id,
                vec![
                    Operand::Literal(Literal::Bool(false)),
                    Operand::Literal(Literal::Result(result_zero_id)),
                ],
                None,
                None,
            ));
        }
        if replaced_one {
            instructions.push(Instruction::Call(
                write_id,
                vec![
                    Operand::Literal(Literal::Bool(true)),
                    Operand::Literal(Literal::Result(result_one_id)),
                ],
                None,
                None,
            ));
        }
        instructions.append(&mut entry_block.0);
        entry_block.0 = instructions;
    }

    if replaced_one {
        program.num_results += 2;
    } else if replaced_zero {
        program.num_results += 1;
    }
}

fn add_write_result(program: &mut Program) -> CallableId {
    let write_id = CallableId(
        program
            .callables
            .iter()
            .map(|(id, _)| id.0)
            .max()
            .expect("should be at least one callable")
            + 1,
    );
    program
        .callables
        .insert(write_id, builder::write_result_decl());
    write_id
}
