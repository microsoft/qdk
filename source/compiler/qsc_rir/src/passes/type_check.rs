// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::rir::{Callable, InstructionKind, Operand, Program, Ty, Variable};

#[cfg(test)]
mod tests;

pub fn check_types(program: &Program) {
    for (_, block) in program.blocks.iter() {
        for instr in &block.0 {
            check_instr_types(program, &instr.kind);
        }
    }
}

fn check_instr_types(program: &Program, instr: &InstructionKind) {
    match instr {
        InstructionKind::Call(id, args, var) => {
            check_call_types(program.get_callable(*id), args, *var);
        }

        InstructionKind::Branch(var, _, _) => assert_eq!(var.ty, Ty::Boolean),

        InstructionKind::Add(opr1, opr2, var)
        | InstructionKind::Sub(opr1, opr2, var)
        | InstructionKind::Mul(opr1, opr2, var)
        | InstructionKind::Sdiv(opr1, opr2, var)
        | InstructionKind::Srem(opr1, opr2, var)
        | InstructionKind::Shl(opr1, opr2, var)
        | InstructionKind::Ashr(opr1, opr2, var)
        | InstructionKind::Fadd(opr1, opr2, var)
        | InstructionKind::Fsub(opr1, opr2, var)
        | InstructionKind::Fmul(opr1, opr2, var)
        | InstructionKind::Fdiv(opr1, opr2, var)
        | InstructionKind::LogicalAnd(opr1, opr2, var)
        | InstructionKind::LogicalOr(opr1, opr2, var)
        | InstructionKind::BitwiseAnd(opr1, opr2, var)
        | InstructionKind::BitwiseOr(opr1, opr2, var)
        | InstructionKind::BitwiseXor(opr1, opr2, var) => {
            assert_eq!(opr1.get_type(), opr2.get_type());
            assert_eq!(opr1.get_type(), var.ty);
        }

        InstructionKind::Fcmp(_, opr1, opr2, var) | InstructionKind::Icmp(_, opr1, opr2, var) => {
            assert_eq!(opr1.get_type(), opr2.get_type());
            assert_eq!(Ty::Boolean, var.ty);
        }

        InstructionKind::Store(opr, var)
        | InstructionKind::LogicalNot(opr, var)
        | InstructionKind::BitwiseNot(opr, var) => {
            assert_eq!(opr.get_type(), var.ty);
        }

        InstructionKind::Phi(args, var) => {
            for (opr, _) in args {
                assert_eq!(opr.get_type(), var.ty);
            }
        }

        InstructionKind::Jump(_) | InstructionKind::Return => {}
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
