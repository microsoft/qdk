// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::rir::{
    BlockId, Callable, CallableId, CallableType, Instruction, Literal, Operand, Program, Ty,
    Variable, VariableId,
};

use super::check_instr_types;

#[test]
fn binop_instr_matching_types_passes_check() {
    let var = Variable {
        variable_id: VariableId(0),
        ty: Ty::Integer,
    };
    let opr1 = Operand::Variable(var.clone());
    let opr2 = Operand::Literal(Literal::Integer(0));

    check_instr_types(&Program::new(), &Instruction::Add(opr1, opr2, var));
}

#[test]
#[should_panic(expected = "assertion `left == right` failed")]
fn binop_instr_mismatching_types_fails_check() {
    let var = Variable {
        variable_id: VariableId(0),
        ty: Ty::Integer,
    };
    let opr1 = Operand::Variable(var.clone());
    let opr2 = Operand::Literal(Literal::Bool(false));

    check_instr_types(&Program::new(), &Instruction::Add(opr1, opr2, var));
}

#[test]
fn unop_instr_matching_types_passes_check() {
    let var = Variable {
        variable_id: VariableId(0),
        ty: Ty::Boolean,
    };
    let opr = Operand::Variable(var.clone());

    check_instr_types(&Program::new(), &Instruction::BitwiseNot(opr, var));
}

#[test]
#[should_panic(expected = "assertion `left == right` failed")]
fn unop_instr_mismatching_types_fails_check() {
    let var = Variable {
        variable_id: VariableId(0),
        ty: Ty::Integer,
    };
    let opr = Operand::Variable(var);

    check_instr_types(
        &Program::new(),
        &Instruction::BitwiseNot(
            opr,
            Variable {
                variable_id: VariableId(1),
                ty: Ty::Boolean,
            },
        ),
    );
}

#[test]
fn phi_instr_matching_types_passes_check() {
    let var = Variable {
        variable_id: VariableId(0),
        ty: Ty::Integer,
    };
    let opr = Operand::Variable(var.clone());

    check_instr_types(
        &Program::new(),
        &Instruction::Phi(vec![(opr.clone(), BlockId(0)), (opr, BlockId(1))], var),
    );
}

#[test]
#[should_panic(expected = "assertion `left == right` failed")]
fn phi_instr_mismatching_types_fails_check() {
    let var = Variable {
        variable_id: VariableId(0),
        ty: Ty::Integer,
    };
    let opr = Operand::Variable(var);

    check_instr_types(
        &Program::new(),
        &Instruction::Phi(
            vec![(opr.clone(), BlockId(0)), (opr, BlockId(1))],
            Variable {
                variable_id: VariableId(1),
                ty: Ty::Boolean,
            },
        ),
    );
}

#[test]
fn call_instr_matching_types_passes_check() {
    let var = Variable {
        variable_id: VariableId(0),
        ty: Ty::Integer,
    };
    let opr = Operand::Variable(var);

    let mut program = Program::new();
    program.callables.insert(
        CallableId(0),
        Callable {
            name: "foo".to_string(),
            input_type: vec![Ty::Integer],
            output_type: Some(Ty::Integer),
            call_type: CallableType::Regular,
            body: None,
        },
    );

    check_instr_types(
        &program,
        &Instruction::Call(
            CallableId(0),
            vec![opr],
            Some(Variable {
                variable_id: VariableId(1),
                ty: Ty::Integer,
            }),
        ),
    );
}

#[test]
#[should_panic(expected = "assertion `left == right` failed")]
fn call_instr_mismatching_output_types_fails_check() {
    let var = Variable {
        variable_id: VariableId(0),
        ty: Ty::Integer,
    };
    let opr = Operand::Variable(var);

    let mut program = Program::new();
    program.callables.insert(
        CallableId(0),
        Callable {
            name: "foo".to_string(),
            input_type: vec![Ty::Integer],
            output_type: Some(Ty::Integer),
            call_type: CallableType::Regular,
            body: None,
        },
    );

    check_instr_types(
        &program,
        &Instruction::Call(
            CallableId(0),
            vec![opr],
            Some(Variable {
                variable_id: VariableId(1),
                ty: Ty::Boolean,
            }),
        ),
    );
}

#[test]
#[should_panic(expected = "assertion `left == right` failed")]
fn call_instr_mismatching_input_types_fails_check() {
    let mut program = Program::new();
    program.callables.insert(
        CallableId(0),
        Callable {
            name: "foo".to_string(),
            input_type: vec![Ty::Integer],
            output_type: Some(Ty::Integer),
            call_type: CallableType::Regular,
            body: None,
        },
    );

    check_instr_types(
        &program,
        &Instruction::Call(
            CallableId(0),
            vec![Operand::Literal(Literal::Bool(true))],
            Some(Variable {
                variable_id: VariableId(0),
                ty: Ty::Integer,
            }),
        ),
    );
}

#[test]
#[should_panic(expected = "assertion `left == right` failed")]
fn call_instr_too_many_args_fails_check() {
    let var = Variable {
        variable_id: VariableId(0),
        ty: Ty::Integer,
    };
    let opr = Operand::Variable(var);

    let mut program = Program::new();
    program.callables.insert(
        CallableId(0),
        Callable {
            name: "foo".to_string(),
            input_type: vec![Ty::Integer],
            output_type: Some(Ty::Integer),
            call_type: CallableType::Regular,
            body: None,
        },
    );

    check_instr_types(
        &program,
        &Instruction::Call(
            CallableId(0),
            vec![opr.clone(), opr],
            Some(Variable {
                variable_id: VariableId(1),
                ty: Ty::Integer,
            }),
        ),
    );
}

#[test]
fn call_instr_no_return_type_no_output_var_passes_check() {
    let var = Variable {
        variable_id: VariableId(0),
        ty: Ty::Integer,
    };
    let opr = Operand::Variable(var);

    let mut program = Program::new();
    program.callables.insert(
        CallableId(0),
        Callable {
            name: "foo".to_string(),
            input_type: vec![Ty::Integer],
            output_type: None,
            call_type: CallableType::Regular,
            body: None,
        },
    );

    check_instr_types(&program, &Instruction::Call(CallableId(0), vec![opr], None));
}

#[test]
#[should_panic(
    expected = "expected return type to be present in both the instruction and the callable"
)]
fn call_instr_return_type_without_output_var_fails() {
    let var = Variable {
        variable_id: VariableId(0),
        ty: Ty::Integer,
    };
    let opr = Operand::Variable(var);

    let mut program = Program::new();
    program.callables.insert(
        CallableId(0),
        Callable {
            name: "foo".to_string(),
            input_type: vec![Ty::Integer],
            output_type: Some(Ty::Integer),
            call_type: CallableType::Regular,
            body: None,
        },
    );

    check_instr_types(&program, &Instruction::Call(CallableId(0), vec![opr], None));
}

#[test]
#[should_panic(
    expected = "expected return type to be present in both the instruction and the callable"
)]
fn call_instr_output_var_without_return_type_fails() {
    let var = Variable {
        variable_id: VariableId(0),
        ty: Ty::Integer,
    };
    let opr = Operand::Variable(var);

    let mut program = Program::new();
    program.callables.insert(
        CallableId(0),
        Callable {
            name: "foo".to_string(),
            input_type: vec![Ty::Integer],
            output_type: None,
            call_type: CallableType::Regular,
            body: None,
        },
    );

    check_instr_types(
        &program,
        &Instruction::Call(
            CallableId(0),
            vec![opr],
            Some(Variable {
                variable_id: VariableId(1),
                ty: Ty::Integer,
            }),
        ),
    );
}
