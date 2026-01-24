// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::rir::{Callable, Instruction, Operand, Program, Ty, Variable};

#[cfg(test)]
mod tests;

pub fn check_types(program: &Program) {
    for (_, block) in program.blocks.iter() {
        for instr in &block.0 {
            check_instr_types(program, &instr.instruction);
        }
    }
}

fn check_instr_types(program: &Program, instr: &Instruction) {
    match instr {
        Instruction::Call(id, args, var) => check_call_types(program.get_callable(*id), args, *var),

        Instruction::Branch(var, _, _) => assert_eq!(var.ty, Ty::Boolean),

        Instruction::Add(opr1, opr2, var)
        | Instruction::Sub(opr1, opr2, var)
        | Instruction::Mul(opr1, opr2, var)
        | Instruction::Sdiv(opr1, opr2, var)
        | Instruction::Srem(opr1, opr2, var)
        | Instruction::Shl(opr1, opr2, var)
        | Instruction::Ashr(opr1, opr2, var)
        | Instruction::Fadd(opr1, opr2, var)
        | Instruction::Fsub(opr1, opr2, var)
        | Instruction::Fmul(opr1, opr2, var)
        | Instruction::Fdiv(opr1, opr2, var)
        | Instruction::LogicalAnd(opr1, opr2, var)
        | Instruction::LogicalOr(opr1, opr2, var)
        | Instruction::BitwiseAnd(opr1, opr2, var)
        | Instruction::BitwiseOr(opr1, opr2, var)
        | Instruction::BitwiseXor(opr1, opr2, var) => {
            assert_eq!(opr1.get_type(), opr2.get_type());
            assert_eq!(opr1.get_type(), var.ty);
        }

        Instruction::Fcmp(_, opr1, opr2, var) | Instruction::Icmp(_, opr1, opr2, var) => {
            assert_eq!(opr1.get_type(), opr2.get_type());
            assert_eq!(Ty::Boolean, var.ty);
        }

        Instruction::Store(opr, var)
        | Instruction::LogicalNot(opr, var)
        | Instruction::BitwiseNot(opr, var) => {
            assert_eq!(opr.get_type(), var.ty);
        }

        Instruction::Phi(args, var) => {
            for (opr, _) in args {
                assert_eq!(opr.get_type(), var.ty);
            }
        }

        Instruction::Jump(_) | Instruction::Return => {}
    }
}

fn check_call_types(callable: &Callable, args: &[Operand], var: Option<Variable>) {
    assert_eq!(
        callable.input_type.len(),
        args.len(),
        "incorrect number of arguments"
    );
    for (arg, ty) in args.iter().zip(callable.input_type.iter()) {
        assert_eq!(arg.get_type(), *ty);
    }

    match (var, callable.output_type) {
        (Some(var), Some(ty)) => assert_eq!(ty, var.ty),
        (None, None) => {}
        _ => panic!("expected return type to be present in both the instruction and the callable"),
    }
}
