// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::rir::{
    Callable, Instruction, Literal, Operand, Prim, Program, Ty, Variable, VariableId,
};
use rustc_hash::FxHashSet;

#[cfg(test)]
mod tests;

pub fn check_types(program: &Program) {
    // Any variable that is defined more than once violates the required LLVM Single Static Assignment (SSA)
    // form, so we track the variable IDs defined by instructions to ensure they are unique.
    // LLVM only requires unique variables per-function, but since RIR uses global variable IDs,
    // we enforce uniqueness across the entire program.
    let mut defined_var_ids = FxHashSet::default();
    for (_, block) in program.blocks.iter() {
        for instr in &block.0 {
            check_instr_types(program, instr, &mut defined_var_ids);
        }
    }
}

fn check_instr_types(
    program: &Program,
    instr: &Instruction,
    defined_var_ids: &mut FxHashSet<VariableId>,
) {
    match instr {
        Instruction::Call(id, args, var, _) => {
            check_call_types(program.get_callable(*id), args, *var);
            if let Some(var) = var {
                assert!(defined_var_ids.insert(var.variable_id));
            }
        }

        Instruction::Branch(var, _, _, _) => assert_eq!(var.ty, Ty::Prim(Prim::Boolean)),

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
        | Instruction::Frem(opr1, opr2, var)
        | Instruction::LogicalAnd(opr1, opr2, var)
        | Instruction::LogicalOr(opr1, opr2, var)
        | Instruction::BitwiseAnd(opr1, opr2, var)
        | Instruction::BitwiseOr(opr1, opr2, var)
        | Instruction::BitwiseXor(opr1, opr2, var) => {
            assert_eq!(opr1.get_type(), opr2.get_type());
            assert_eq!(opr1.get_type(), var.ty);
            assert!(defined_var_ids.insert(var.variable_id));
        }

        Instruction::Fcmp(_, opr1, opr2, var) | Instruction::Icmp(_, opr1, opr2, var) => {
            assert_eq!(opr1.get_type(), opr2.get_type());
            assert_eq!(Ty::Prim(Prim::Boolean), var.ty);
            assert!(defined_var_ids.insert(var.variable_id));
        }

        Instruction::Store(opr, var)
        | Instruction::LogicalNot(opr, var)
        | Instruction::BitwiseNot(opr, var) => {
            assert_eq!(opr.get_type(), var.ty);
            if !matches!(instr, Instruction::Store(..)) {
                assert!(defined_var_ids.insert(var.variable_id));
            }
        }

        Instruction::Phi(args, var) => {
            for (opr, _) in args {
                assert_eq!(opr.get_type(), var.ty);
            }
            assert!(defined_var_ids.insert(var.variable_id));
        }

        Instruction::Index(array, index, var) => {
            match array {
                Operand::Literal(Literal::Array(global_idx)) => {
                    let Some(array_literal) = program.array_literals.get(*global_idx) else {
                        panic!("array literal index {global_idx} is out of bounds");
                    };
                    assert_eq!(Ty::Prim(array_literal.ty), var.ty);
                }
                Operand::Variable(Variable {
                    ty: Ty::Array(_, elem_ty),
                    ..
                }) => {
                    assert_eq!(Ty::Prim(*elem_ty), var.ty);
                }
                _ => panic!(
                    "expected array operand to be either a global array literal or an array variable"
                ),
            }
            assert_eq!(index.get_type(), Ty::Prim(Prim::Integer));
            assert!(defined_var_ids.insert(var.variable_id));
        }

        Instruction::StoreArray(operands, var) => {
            let Ty::Array(size, elem_ty) = &var.ty else {
                panic!("expected variable to be of array type");
            };
            assert_eq!(
                operands.len(),
                *size,
                "expected number of operands to match array size"
            );
            for opr in operands {
                assert_eq!(
                    opr.get_type(),
                    Ty::Prim(*elem_ty),
                    "expected operand type to match array element type"
                );
            }
        }

        Instruction::Convert(_, var) | Instruction::Alloca(var) | Instruction::Load(_, var) => {
            assert!(defined_var_ids.insert(var.variable_id));
        }

        Instruction::Jump(_) | Instruction::Return(..) => {}
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
