// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_data_structures::target::TargetCapabilityFlags;

use crate::rir::{
    Block, BlockId, Callable, CallableId, CallableType, Instruction, Literal, Operand, Program, Ty,
    Variable, VariableId,
};

#[must_use]
pub fn x_decl() -> Callable {
    Callable {
        name: "__quantum__qis__x__body".to_string(),
        input_type: vec![Ty::Qubit],
        output_type: None,
        body: None,
        call_type: CallableType::Regular,
    }
}

#[must_use]
pub fn z_decl() -> Callable {
    Callable {
        name: "__quantum__qis__z__body".to_string(),
        input_type: vec![Ty::Qubit],
        output_type: None,
        body: None,
        call_type: CallableType::Regular,
    }
}

#[must_use]
pub fn h_decl() -> Callable {
    Callable {
        name: "__quantum__qis__h__body".to_string(),
        input_type: vec![Ty::Qubit],
        output_type: None,
        body: None,
        call_type: CallableType::Regular,
    }
}

#[must_use]
pub fn cx_decl() -> Callable {
    Callable {
        name: "__quantum__qis__cx__body".to_string(),
        input_type: vec![Ty::Qubit, Ty::Qubit],
        output_type: None,
        body: None,
        call_type: CallableType::Regular,
    }
}

#[must_use]
pub fn rx_decl() -> Callable {
    Callable {
        name: "__quantum__qis__rx__body".to_string(),
        input_type: vec![Ty::Double, Ty::Qubit],
        output_type: None,
        body: None,
        call_type: CallableType::Regular,
    }
}

#[must_use]
pub fn m_decl() -> Callable {
    Callable {
        name: "__quantum__qis__m__body".to_string(),
        input_type: vec![Ty::Qubit, Ty::Result],
        output_type: None,
        body: None,
        call_type: CallableType::Measurement,
    }
}

#[must_use]
pub fn mresetz_decl() -> Callable {
    Callable {
        name: "__quantum__qis__mresetz__body".to_string(),
        input_type: vec![Ty::Qubit, Ty::Result],
        output_type: None,
        body: None,
        call_type: CallableType::Measurement,
    }
}

#[must_use]
pub fn reset_decl() -> Callable {
    Callable {
        name: "__quantum__qis__reset__body".to_string(),
        input_type: vec![Ty::Qubit],
        output_type: None,
        body: None,
        call_type: CallableType::Reset,
    }
}

#[must_use]
pub fn read_result_decl() -> Callable {
    Callable {
        name: "__quantum__rt__read_result".to_string(),
        input_type: vec![Ty::Result],
        output_type: Some(Ty::Boolean),
        body: None,
        call_type: CallableType::Readout,
    }
}

#[must_use]
pub fn initialize_decl() -> Callable {
    Callable {
        name: "__quantum__rt__initialize".to_string(),
        input_type: vec![Ty::Pointer],
        output_type: None,
        body: None,
        call_type: CallableType::Regular,
    }
}

#[must_use]
pub fn result_record_decl() -> Callable {
    Callable {
        name: "__quantum__rt__result_record_output".to_string(),
        input_type: vec![Ty::Result, Ty::Pointer],
        output_type: None,
        body: None,
        call_type: CallableType::OutputRecording,
    }
}

#[must_use]
pub fn double_record_decl() -> Callable {
    Callable {
        name: "__quantum__rt__double_record_output".to_string(),
        input_type: vec![Ty::Double, Ty::Pointer],
        output_type: None,
        body: None,
        call_type: CallableType::OutputRecording,
    }
}

#[must_use]
pub fn int_record_decl() -> Callable {
    Callable {
        name: "__quantum__rt__int_record_output".to_string(),
        input_type: vec![Ty::Integer, Ty::Pointer],
        output_type: None,
        body: None,
        call_type: CallableType::OutputRecording,
    }
}

#[must_use]
pub fn bool_record_decl() -> Callable {
    Callable {
        name: "__quantum__rt__bool_record_output".to_string(),
        input_type: vec![Ty::Boolean, Ty::Pointer],
        output_type: None,
        body: None,
        call_type: CallableType::OutputRecording,
    }
}

#[must_use]
pub fn array_record_decl() -> Callable {
    Callable {
        name: "__quantum__rt__array_record_output".to_string(),
        input_type: vec![Ty::Integer, Ty::Pointer],
        output_type: None,
        body: None,
        call_type: CallableType::OutputRecording,
    }
}

#[must_use]
pub fn tuple_record_decl() -> Callable {
    Callable {
        name: "__quantum__rt__tuple_record_output".to_string(),
        input_type: vec![Ty::Integer, Ty::Pointer],
        output_type: None,
        body: None,
        call_type: CallableType::OutputRecording,
    }
}

/// Creates a new program with a single, entry callable that has block 0 as its body.
#[must_use]
pub fn new_program() -> Program {
    let mut program = Program::new();
    program.entry = CallableId(0);
    program.callables.insert(
        CallableId(0),
        Callable {
            name: "main".to_string(),
            input_type: Vec::new(),
            output_type: Some(Ty::Integer),
            body: Some(BlockId(0)),
            call_type: CallableType::Regular,
        },
    );
    program
}

#[must_use]
pub fn bell_program() -> Program {
    let mut program = Program::default();
    program.callables.insert(CallableId(0), h_decl());
    program.callables.insert(CallableId(1), cx_decl());
    program.callables.insert(CallableId(2), m_decl());
    program.callables.insert(CallableId(3), array_record_decl());
    program
        .callables
        .insert(CallableId(4), result_record_decl());
    program.callables.insert(
        CallableId(5),
        Callable {
            name: "main".to_string(),
            input_type: vec![],
            output_type: Some(Ty::Integer),
            body: Some(BlockId(0)),
            call_type: CallableType::Regular,
        },
    );
    program.tags = vec!["0_a".to_string(), "1_a0r".to_string(), "2_a1r".to_string()];
    program.entry = CallableId(5);
    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(
                CallableId(0),
                vec![Operand::Literal(Literal::Qubit(0))],
                None,
            ),
            Instruction::Call(
                CallableId(1),
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Qubit(1)),
                ],
                None,
            ),
            Instruction::Call(
                CallableId(2),
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(0)),
                ],
                None,
            ),
            Instruction::Call(
                CallableId(2),
                vec![
                    Operand::Literal(Literal::Qubit(1)),
                    Operand::Literal(Literal::Result(1)),
                ],
                None,
            ),
            Instruction::Call(
                CallableId(3),
                vec![
                    Operand::Literal(Literal::Integer(2)),
                    Operand::Literal(Literal::Tag(0, 3)),
                ],
                None,
            ),
            Instruction::Call(
                CallableId(4),
                vec![
                    Operand::Literal(Literal::Result(0)),
                    Operand::Literal(Literal::Tag(1, 5)),
                ],
                None,
            ),
            Instruction::Call(
                CallableId(4),
                vec![
                    Operand::Literal(Literal::Result(1)),
                    Operand::Literal(Literal::Tag(2, 5)),
                ],
                None,
            ),
            Instruction::Return,
        ]),
    );
    program.num_qubits = 2;
    program.num_results = 2;
    program
}

#[allow(clippy::too_many_lines)]
#[must_use]
pub fn teleport_program() -> Program {
    let mut program = Program::default();
    program.config.capabilities = TargetCapabilityFlags::Adaptive;
    program.callables.insert(CallableId(0), h_decl());
    program.callables.insert(CallableId(1), z_decl());
    program.callables.insert(CallableId(2), x_decl());
    program.callables.insert(CallableId(3), cx_decl());
    program.callables.insert(CallableId(4), mresetz_decl());
    program.callables.insert(CallableId(5), read_result_decl());
    program
        .callables
        .insert(CallableId(6), result_record_decl());
    program.callables.insert(
        CallableId(7),
        Callable {
            name: "main".to_string(),
            input_type: vec![],
            output_type: Some(Ty::Integer),
            body: Some(BlockId(0)),
            call_type: CallableType::Regular,
        },
    );
    program.tags = vec!["0_r".to_string()];
    program.entry = CallableId(7);
    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(
                CallableId(2),
                vec![Operand::Literal(Literal::Qubit(0))],
                None,
            ),
            Instruction::Call(
                CallableId(0),
                vec![Operand::Literal(Literal::Qubit(2))],
                None,
            ),
            Instruction::Call(
                CallableId(3),
                vec![
                    Operand::Literal(Literal::Qubit(2)),
                    Operand::Literal(Literal::Qubit(1)),
                ],
                None,
            ),
            Instruction::Call(
                CallableId(3),
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Qubit(2)),
                ],
                None,
            ),
            Instruction::Call(
                CallableId(0),
                vec![Operand::Literal(Literal::Qubit(0))],
                None,
            ),
            Instruction::Call(
                CallableId(4),
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(0)),
                ],
                None,
            ),
            Instruction::Call(
                CallableId(5),
                vec![Operand::Literal(Literal::Result(0))],
                Some(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                }),
            ),
            Instruction::Branch(
                Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                },
                BlockId(1),
                BlockId(2),
            ),
        ]),
    );
    program.blocks.insert(
        BlockId(1),
        Block(vec![
            Instruction::Call(
                CallableId(1),
                vec![Operand::Literal(Literal::Qubit(1))],
                None,
            ),
            Instruction::Jump(BlockId(2)),
        ]),
    );
    program.blocks.insert(
        BlockId(2),
        Block(vec![
            Instruction::Call(
                CallableId(4),
                vec![
                    Operand::Literal(Literal::Qubit(2)),
                    Operand::Literal(Literal::Result(1)),
                ],
                None,
            ),
            Instruction::Call(
                CallableId(5),
                vec![Operand::Literal(Literal::Result(1))],
                Some(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                }),
            ),
            Instruction::Branch(
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Boolean,
                },
                BlockId(3),
                BlockId(4),
            ),
        ]),
    );
    program.blocks.insert(
        BlockId(3),
        Block(vec![
            Instruction::Call(
                CallableId(2),
                vec![Operand::Literal(Literal::Qubit(1))],
                None,
            ),
            Instruction::Jump(BlockId(4)),
        ]),
    );
    program.blocks.insert(
        BlockId(4),
        Block(vec![
            Instruction::Call(
                CallableId(4),
                vec![
                    Operand::Literal(Literal::Qubit(1)),
                    Operand::Literal(Literal::Result(2)),
                ],
                None,
            ),
            Instruction::Call(
                CallableId(6),
                vec![
                    Operand::Literal(Literal::Result(2)),
                    Operand::Literal(Literal::Tag(0, 3)),
                ],
                None,
            ),
            Instruction::Return,
        ]),
    );
    program.num_qubits = 3;
    program.num_results = 3;
    program
}
