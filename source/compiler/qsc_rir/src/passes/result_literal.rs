// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_eval::PackageSpan;

use crate::rir::{Instruction, Literal, Operand, Program};

/// Transforms result literals in the program.
/// Since result literals are not supported in QIR, this function attempts to handle them as best as possible.
/// A result literal of `Zero` (or false) will be replaced with an additional result id that is never measured into,
/// which defaults to returning false or 0 if read or recorded.
/// A result literal of `One` cannot be handled and is left un-transformed so that later checks on the program can
/// reject it as incompatible.
pub fn transform_result_literals(program: &mut Program) {
    let result_zero_id = program.num_results;
    let mut replaced_zero = false;

    for block in program.blocks.values_mut() {
        for instr in &mut block.0 {
            // Result literals are only expected in context of Store, Call, or Return instructions
            match instr {
                Instruction::Store(operand, _) | Instruction::Return(Some(operand))
                    if matches!(operand, Operand::Literal(Literal::ResultLit(false, _))) =>
                {
                    *operand = Operand::Literal(Literal::Result(result_zero_id));
                    replaced_zero = true;
                }

                Instruction::Call(_, operands, _, _) => {
                    for operand in operands.iter_mut() {
                        if matches!(operand, Operand::Literal(Literal::ResultLit(false, _))) {
                            *operand = Operand::Literal(Literal::Result(result_zero_id));
                            replaced_zero = true;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    if replaced_zero {
        program.num_results += 1;
    }
}

#[must_use]
pub fn has_result_one_literal(program: &Program) -> Option<PackageSpan> {
    for block in program.blocks.values() {
        for instr in &block.0 {
            // Result literals are only expected in context of Store, Call, or Return instructions
            match instr {
                Instruction::Store(Operand::Literal(Literal::ResultLit(_, span)), _)
                | Instruction::Return(Some(Operand::Literal(Literal::ResultLit(_, span)))) => {
                    return Some(*span);
                }

                Instruction::Call(_, operands, _, _) => {
                    for operand in operands {
                        if let Operand::Literal(Literal::ResultLit(_, span)) = operand {
                            return Some(*span);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    None
}
