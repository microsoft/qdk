// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_data_structures::target::{Profile, TargetCapabilityFlags};

use crate::rir::{
    Block, BlockId, Callable, CallableId, CallableType, Instruction, Literal, Operand, Prim,
    Program, Ty, Variable, VariableId,
};

#[must_use]
pub fn x_decl() -> Callable {
    Callable {
        name: "__quantum__qis__x__body".to_string(),
        input_type: vec![Ty::Prim(Prim::Qubit)],
        input_vars: Vec::new(),
        output_type: None,
        body: None,
        call_type: CallableType::Regular,
    }
}

#[must_use]
pub fn z_decl() -> Callable {
    Callable {
        name: "__quantum__qis__z__body".to_string(),
        input_type: vec![Ty::Prim(Prim::Qubit)],
        input_vars: Vec::new(),
        output_type: None,
        body: None,
        call_type: CallableType::Regular,
    }
}

#[must_use]
pub fn h_decl() -> Callable {
    Callable {
        name: "__quantum__qis__h__body".to_string(),
        input_type: vec![Ty::Prim(Prim::Qubit)],
        input_vars: Vec::new(),
        output_type: None,
        body: None,
        call_type: CallableType::Regular,
    }
}

#[must_use]
pub fn cx_decl() -> Callable {
    Callable {
        name: "__quantum__qis__cx__body".to_string(),
        input_type: vec![Ty::Prim(Prim::Qubit), Ty::Prim(Prim::Qubit)],
        input_vars: Vec::new(),
        output_type: None,
        body: None,
        call_type: CallableType::Regular,
    }
}

#[must_use]
pub fn rx_decl() -> Callable {
    Callable {
        name: "__quantum__qis__rx__body".to_string(),
        input_type: vec![Ty::Prim(Prim::Double), Ty::Prim(Prim::Qubit)],
        input_vars: Vec::new(),
        output_type: None,
        body: None,
        call_type: CallableType::Regular,
    }
}

#[must_use]
pub fn m_decl() -> Callable {
    Callable {
        name: "__quantum__qis__m__body".to_string(),
        input_type: vec![Ty::Prim(Prim::Qubit), Ty::Prim(Prim::Result)],
        input_vars: Vec::new(),
        output_type: None,
        body: None,
        call_type: CallableType::Measurement,
    }
}

#[must_use]
pub fn mresetz_decl() -> Callable {
    Callable {
        name: "__quantum__qis__mresetz__body".to_string(),
        input_type: vec![Ty::Prim(Prim::Qubit), Ty::Prim(Prim::Result)],
        input_vars: Vec::new(),
        output_type: None,
        body: None,
        call_type: CallableType::Measurement,
    }
}

#[must_use]
pub fn reset_decl() -> Callable {
    Callable {
        name: "__quantum__qis__reset__body".to_string(),
        input_type: vec![Ty::Prim(Prim::Qubit)],
        input_vars: Vec::new(),
        output_type: None,
        body: None,
        call_type: CallableType::Reset,
    }
}

#[must_use]
pub fn read_result_decl() -> Callable {
    Callable {
        name: "__quantum__rt__read_result".to_string(),
        input_type: vec![Ty::Prim(Prim::Result)],
        input_vars: Vec::new(),
        output_type: Some(Ty::Prim(Prim::Boolean)),
        body: None,
        call_type: CallableType::Readout,
    }
}

#[must_use]
pub fn write_result_decl() -> Callable {
    Callable {
        name: "__quantum__rt__write_result".to_string(),
        input_type: vec![Ty::Prim(Prim::Boolean), Ty::Prim(Prim::Result)],
        input_vars: Vec::new(),
        output_type: None,
        body: None,
        call_type: CallableType::Regular,
    }
}

#[must_use]
pub fn initialize_decl() -> Callable {
    Callable {
        name: "__quantum__rt__initialize".to_string(),
        input_type: vec![Ty::Prim(Prim::Pointer)],
        input_vars: Vec::new(),
        output_type: None,
        body: None,
        call_type: CallableType::Regular,
    }
}

#[must_use]
pub fn result_record_decl() -> Callable {
    Callable {
        name: "__quantum__rt__result_record_output".to_string(),
        input_type: vec![Ty::Prim(Prim::Result), Ty::Prim(Prim::Pointer)],
        input_vars: Vec::new(),
        output_type: None,
        body: None,
        call_type: CallableType::OutputRecording,
    }
}

#[must_use]
pub fn double_record_decl() -> Callable {
    Callable {
        name: "__quantum__rt__double_record_output".to_string(),
        input_type: vec![Ty::Prim(Prim::Double), Ty::Prim(Prim::Pointer)],
        input_vars: Vec::new(),
        output_type: None,
        body: None,
        call_type: CallableType::OutputRecording,
    }
}

#[must_use]
pub fn int_record_decl() -> Callable {
    Callable {
        name: "__quantum__rt__int_record_output".to_string(),
        input_type: vec![Ty::Prim(Prim::Integer), Ty::Prim(Prim::Pointer)],
        input_vars: Vec::new(),
        output_type: None,
        body: None,
        call_type: CallableType::OutputRecording,
    }
}

#[must_use]
pub fn bool_record_decl() -> Callable {
    Callable {
        name: "__quantum__rt__bool_record_output".to_string(),
        input_type: vec![Ty::Prim(Prim::Boolean), Ty::Prim(Prim::Pointer)],
        input_vars: Vec::new(),
        output_type: None,
        body: None,
        call_type: CallableType::OutputRecording,
    }
}

#[must_use]
pub fn array_record_decl() -> Callable {
    Callable {
        name: "__quantum__rt__array_record_output".to_string(),
        input_type: vec![Ty::Prim(Prim::Integer), Ty::Prim(Prim::Pointer)],
        input_vars: Vec::new(),
        output_type: None,
        body: None,
        call_type: CallableType::OutputRecording,
    }
}

#[must_use]
pub fn tuple_record_decl() -> Callable {
    Callable {
        name: "__quantum__rt__tuple_record_output".to_string(),
        input_type: vec![Ty::Prim(Prim::Integer), Ty::Prim(Prim::Pointer)],
        input_vars: Vec::new(),
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
            input_vars: Vec::new(),
            output_type: Some(Ty::Prim(Prim::Integer)),
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
            input_vars: vec![],
            output_type: Some(Ty::Prim(Prim::Integer)),
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
                None,
            ),
            Instruction::Call(
                CallableId(1),
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Qubit(1)),
                ],
                None,
                None,
            ),
            Instruction::Call(
                CallableId(2),
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(0)),
                ],
                None,
                None,
            ),
            Instruction::Call(
                CallableId(2),
                vec![
                    Operand::Literal(Literal::Qubit(1)),
                    Operand::Literal(Literal::Result(1)),
                ],
                None,
                None,
            ),
            Instruction::Call(
                CallableId(3),
                vec![
                    Operand::Literal(Literal::Integer(2)),
                    Operand::Literal(Literal::Tag(0, 3)),
                ],
                None,
                None,
            ),
            Instruction::Call(
                CallableId(4),
                vec![
                    Operand::Literal(Literal::Result(0)),
                    Operand::Literal(Literal::Tag(1, 5)),
                ],
                None,
                None,
            ),
            Instruction::Call(
                CallableId(4),
                vec![
                    Operand::Literal(Literal::Result(1)),
                    Operand::Literal(Literal::Tag(2, 5)),
                ],
                None,
                None,
            ),
            Instruction::Return(Some(Operand::Literal(Literal::Integer(0)))),
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
            input_vars: vec![],
            output_type: Some(Ty::Prim(Prim::Integer)),
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
                None,
            ),
            Instruction::Call(
                CallableId(0),
                vec![Operand::Literal(Literal::Qubit(2))],
                None,
                None,
            ),
            Instruction::Call(
                CallableId(3),
                vec![
                    Operand::Literal(Literal::Qubit(2)),
                    Operand::Literal(Literal::Qubit(1)),
                ],
                None,
                None,
            ),
            Instruction::Call(
                CallableId(3),
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Qubit(2)),
                ],
                None,
                None,
            ),
            Instruction::Call(
                CallableId(0),
                vec![Operand::Literal(Literal::Qubit(0))],
                None,
                None,
            ),
            Instruction::Call(
                CallableId(4),
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(0)),
                ],
                None,
                None,
            ),
            Instruction::Call(
                CallableId(5),
                vec![Operand::Literal(Literal::Result(0))],
                Some(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                None,
            ),
            Instruction::Branch(
                Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                },
                BlockId(1),
                BlockId(2),
                None,
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
                None,
            ),
            Instruction::Call(
                CallableId(5),
                vec![Operand::Literal(Literal::Result(1))],
                Some(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                None,
            ),
            Instruction::Branch(
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                },
                BlockId(3),
                BlockId(4),
                None,
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
                None,
            ),
            Instruction::Call(
                CallableId(6),
                vec![
                    Operand::Literal(Literal::Result(2)),
                    Operand::Literal(Literal::Tag(0, 3)),
                ],
                None,
                None,
            ),
            Instruction::Return(Some(Operand::Literal(Literal::Integer(0)))),
        ]),
    );
    program.num_qubits = 3;
    program.num_results = 3;
    program
}

/// Builds a program with two bodied callables: an entry callable that calls a
/// second `Regular` callable. Both bodies are single-block and return a value;
/// the second body reads its `input_vars` parameter. The second body's block id
/// (0) is lower than the entry body's block id (2), exercising a non-contiguous
/// block arena where a callable's body does not start at block 0.
#[must_use]
pub fn two_body_program() -> Program {
    let mut program = Program::default();
    program.config.capabilities = Profile::AdaptiveRIF.into();

    // Entry callable. Its body lives in block 2, which is higher than the helper
    // body's block id, so the arena is not in callable order.
    program.callables.insert(
        CallableId(0),
        Callable {
            name: "main".to_string(),
            input_type: Vec::new(),
            input_vars: Vec::new(),
            output_type: Some(Ty::Prim(Prim::Integer)),
            body: Some(BlockId(2)),
            call_type: CallableType::Regular,
        },
    );
    // A second bodied callable that takes an integer parameter.
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "helper".to_string(),
            input_type: vec![Ty::Prim(Prim::Integer)],
            input_vars: vec![VariableId(0)],
            output_type: Some(Ty::Prim(Prim::Integer)),
            body: Some(BlockId(0)),
            call_type: CallableType::Regular,
        },
    );

    // Helper body: reads its parameter and returns a derived value.
    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Add(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Integer),
                }),
                Operand::Literal(Literal::Integer(1)),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Integer),
                },
            ),
            Instruction::Return(Some(Operand::Variable(Variable {
                variable_id: VariableId(1),
                ty: Ty::Prim(Prim::Integer),
            }))),
        ]),
    );
    // Entry body: calls the helper and returns its result.
    program.blocks.insert(
        BlockId(2),
        Block(vec![
            Instruction::Call(
                CallableId(1),
                vec![Operand::Literal(Literal::Integer(7))],
                Some(Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Prim(Prim::Integer),
                }),
                None,
            ),
            Instruction::Return(Some(Operand::Variable(Variable {
                variable_id: VariableId(2),
                ty: Ty::Prim(Prim::Integer),
            }))),
        ]),
    );

    program.entry = CallableId(0);
    program
}

/// Builds a two-body program whose second body contains a forward branch
/// (a diamond that reconverges on a value return). The branch condition is the
/// body's boolean `input_vars` parameter.
#[must_use]
pub fn two_body_program_with_branch() -> Program {
    let mut program = Program::default();
    program.config.capabilities = Profile::AdaptiveRIF.into();

    // Entry callable; its body (block 3) is higher than the helper body blocks.
    program.callables.insert(
        CallableId(0),
        Callable {
            name: "main".to_string(),
            input_type: Vec::new(),
            input_vars: Vec::new(),
            output_type: Some(Ty::Prim(Prim::Integer)),
            body: Some(BlockId(3)),
            call_type: CallableType::Regular,
        },
    );
    // Second bodied callable that branches on a boolean parameter.
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "helper".to_string(),
            input_type: vec![Ty::Prim(Prim::Boolean)],
            input_vars: vec![VariableId(0)],
            output_type: Some(Ty::Prim(Prim::Integer)),
            body: Some(BlockId(0)),
            call_type: CallableType::Regular,
        },
    );

    // Helper header: forward branch to one of two return blocks.
    program.blocks.insert(
        BlockId(0),
        Block(vec![Instruction::Branch(
            Variable {
                variable_id: VariableId(0),
                ty: Ty::Prim(Prim::Boolean),
            },
            BlockId(1),
            BlockId(2),
            None,
        )]),
    );
    program.blocks.insert(
        BlockId(1),
        Block(vec![Instruction::Return(Some(Operand::Literal(
            Literal::Integer(1),
        )))]),
    );
    program.blocks.insert(
        BlockId(2),
        Block(vec![Instruction::Return(Some(Operand::Literal(
            Literal::Integer(0),
        )))]),
    );
    // Entry body: calls the helper and returns its result.
    program.blocks.insert(
        BlockId(3),
        Block(vec![
            Instruction::Call(
                CallableId(1),
                vec![Operand::Literal(Literal::Bool(true))],
                Some(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Integer),
                }),
                None,
            ),
            Instruction::Return(Some(Operand::Variable(Variable {
                variable_id: VariableId(1),
                ty: Ty::Prim(Prim::Integer),
            }))),
        ]),
    );

    program.entry = CallableId(0);
    program
}

/// Builds a two-body program whose second body contains a loop (a backward
/// branch to a header block). A counter is seeded before the loop and updated
/// inside it via `Store`, so the SSA transform must place a loop-header phi.
#[allow(clippy::too_many_lines)]
#[must_use]
pub fn two_body_program_with_loop() -> Program {
    let mut program = Program::default();
    program.config.capabilities = Profile::AdaptiveRIF.into();

    // Entry callable; its body (block 4) is higher than the helper body blocks.
    program.callables.insert(
        CallableId(0),
        Callable {
            name: "main".to_string(),
            input_type: Vec::new(),
            input_vars: Vec::new(),
            output_type: Some(Ty::Prim(Prim::Integer)),
            body: Some(BlockId(4)),
            call_type: CallableType::Regular,
        },
    );
    // Second bodied callable with a loop controlled by a boolean parameter.
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "helper".to_string(),
            input_type: vec![Ty::Prim(Prim::Boolean)],
            input_vars: vec![VariableId(0)],
            output_type: Some(Ty::Prim(Prim::Integer)),
            body: Some(BlockId(0)),
            call_type: CallableType::Regular,
        },
    );

    // Preheader: seed the loop counter and jump to the header.
    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Store(
                Operand::Literal(Literal::Integer(0)),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Integer),
                },
            ),
            Instruction::Jump(BlockId(1)),
        ]),
    );
    // Header: branch on the parameter back into the body or out to the exit.
    program.blocks.insert(
        BlockId(1),
        Block(vec![Instruction::Branch(
            Variable {
                variable_id: VariableId(0),
                ty: Ty::Prim(Prim::Boolean),
            },
            BlockId(2),
            BlockId(3),
            None,
        )]),
    );
    // Loop body: increment the counter and branch backward to the header.
    program.blocks.insert(
        BlockId(2),
        Block(vec![
            Instruction::Add(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Integer),
                }),
                Operand::Literal(Literal::Integer(1)),
                Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Prim(Prim::Integer),
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Prim(Prim::Integer),
                }),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Integer),
                },
            ),
            Instruction::Jump(BlockId(1)),
        ]),
    );
    // Exit: return the final counter value.
    program.blocks.insert(
        BlockId(3),
        Block(vec![Instruction::Return(Some(Operand::Variable(
            Variable {
                variable_id: VariableId(1),
                ty: Ty::Prim(Prim::Integer),
            },
        )))]),
    );
    // Entry body: calls the helper and returns its result.
    program.blocks.insert(
        BlockId(4),
        Block(vec![
            Instruction::Call(
                CallableId(1),
                vec![Operand::Literal(Literal::Bool(true))],
                Some(Variable {
                    variable_id: VariableId(3),
                    ty: Ty::Prim(Prim::Integer),
                }),
                None,
            ),
            Instruction::Return(Some(Operand::Variable(Variable {
                variable_id: VariableId(3),
                ty: Ty::Prim(Prim::Integer),
            }))),
        ]),
    );

    program.entry = CallableId(0);
    program
}

/// Builds a two-body program whose second body stores into one of its
/// `input_vars` parameters. The parameter is therefore both seeded as the body's
/// entry definition and versioned by the store, exercising mutable-parameter
/// handling in the SSA passes.
#[must_use]
pub fn two_body_mutable_param_program() -> Program {
    let mut program = Program::default();
    program.config.capabilities = Profile::AdaptiveRIF.into();

    // Entry callable; its body (block 1) is higher than the helper body block.
    program.callables.insert(
        CallableId(0),
        Callable {
            name: "main".to_string(),
            input_type: Vec::new(),
            input_vars: Vec::new(),
            output_type: Some(Ty::Prim(Prim::Integer)),
            body: Some(BlockId(1)),
            call_type: CallableType::Regular,
        },
    );
    // Second bodied callable that mutates its integer parameter.
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "helper".to_string(),
            input_type: vec![Ty::Prim(Prim::Integer)],
            input_vars: vec![VariableId(0)],
            output_type: Some(Ty::Prim(Prim::Integer)),
            body: Some(BlockId(0)),
            call_type: CallableType::Regular,
        },
    );

    // Helper body: derive a value from the parameter, then store it back into the
    // parameter before returning it.
    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Add(
                Operand::Variable(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Integer),
                }),
                Operand::Literal(Literal::Integer(1)),
                Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Integer),
                },
            ),
            Instruction::Store(
                Operand::Variable(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Integer),
                }),
                Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Integer),
                },
            ),
            Instruction::Return(Some(Operand::Variable(Variable {
                variable_id: VariableId(0),
                ty: Ty::Prim(Prim::Integer),
            }))),
        ]),
    );
    // Entry body: calls the helper and returns its result.
    program.blocks.insert(
        BlockId(1),
        Block(vec![
            Instruction::Call(
                CallableId(1),
                vec![Operand::Literal(Literal::Integer(5))],
                Some(Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Prim(Prim::Integer),
                }),
                None,
            ),
            Instruction::Return(Some(Operand::Variable(Variable {
                variable_id: VariableId(2),
                ty: Ty::Prim(Prim::Integer),
            }))),
        ]),
    );

    program.entry = CallableId(0);
    program
}
