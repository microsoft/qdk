// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::too_many_lines, clippy::needless_raw_string_hashes)]

use expect_test::expect;

use crate::{
    builder::{cx_decl, h_decl, m_decl, mresetz_decl, read_result_decl, reset_decl, x_decl},
    rir::{
        Block, BlockId, CallableId, CallableType, Instruction, Literal, Operand, Program, Ty,
        Variable, VariableId,
    },
};

use super::reindex_qubits;

#[test]
fn qubit_reindexed_after_reset_removes_reset() {
    const X: CallableId = CallableId(0);
    const RESET: CallableId = CallableId(1);
    let mut program = Program::new();
    program.num_qubits = 1;
    program.callables.insert(X, x_decl());
    program.callables.insert(RESET, reset_decl());
    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(X, vec![Operand::Literal(Literal::Qubit(0))], None),
            Instruction::Call(RESET, vec![Operand::Literal(Literal::Qubit(0))], None),
            Instruction::Call(X, vec![Operand::Literal(Literal::Qubit(0))], None),
            Instruction::Call(RESET, vec![Operand::Literal(Literal::Qubit(0))], None),
            Instruction::Return,
        ]),
    );

    // Before
    expect![[r#"
        Block:
            Call id(0), args( Qubit(0), )
            Call id(1), args( Qubit(0), )
            Call id(0), args( Qubit(0), )
            Call id(1), args( Qubit(0), )
            Return"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());

    // After
    reindex_qubits(&mut program);
    expect![[r#"
        Block:
            Call id(0), args( Qubit(0), )
            Call id(0), args( Qubit(1), )
            Return"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());
    assert_eq!(program.num_qubits, 2);

    // Reset callable should be removed.
    for callable in program.callables.values() {
        assert_ne!(callable.call_type, CallableType::Reset);
    }
}

#[test]
fn qubit_reindexed_after_mz() {
    const X: CallableId = CallableId(0);
    const M: CallableId = CallableId(1);
    let mut program = Program::new();
    program.num_qubits = 1;
    program.callables.insert(X, x_decl());
    program.callables.insert(M, m_decl());
    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(X, vec![Operand::Literal(Literal::Qubit(0))], None),
            Instruction::Call(
                M,
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(0)),
                ],
                None,
            ),
            Instruction::Call(X, vec![Operand::Literal(Literal::Qubit(0))], None),
            Instruction::Call(
                M,
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(1)),
                ],
                None,
            ),
            Instruction::Return,
        ]),
    );

    // Before
    expect![[r#"
        Block:
            Call id(0), args( Qubit(0), )
            Call id(1), args( Qubit(0), Result(0), )
            Call id(0), args( Qubit(0), )
            Call id(1), args( Qubit(0), Result(1), )
            Return"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());

    // After
    reindex_qubits(&mut program);
    expect![[r#"
        Block:
            Call id(0), args( Qubit(0), )
            Call id(2), args( Qubit(0), Qubit(1), )
            Call id(1), args( Qubit(0), Result(0), )
            Call id(0), args( Qubit(1), )
            Call id(1), args( Qubit(1), Result(1), )
            Return"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());
    assert_eq!(program.num_qubits, 2);
}

#[test]
fn qubit_reindexed_after_mresetz_and_changed_to_mz() {
    const X: CallableId = CallableId(0);
    const MRESETZ: CallableId = CallableId(1);
    let mut program = Program::new();
    program.num_qubits = 1;
    program.callables.insert(X, x_decl());
    program.callables.insert(MRESETZ, mresetz_decl());
    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(X, vec![Operand::Literal(Literal::Qubit(0))], None),
            Instruction::Call(
                MRESETZ,
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(0)),
                ],
                None,
            ),
            Instruction::Call(X, vec![Operand::Literal(Literal::Qubit(0))], None),
            Instruction::Call(
                MRESETZ,
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(1)),
                ],
                None,
            ),
            Instruction::Return,
        ]),
    );

    // Before
    expect![[r#"
        Block:
            Call id(0), args( Qubit(0), )
            Call id(1), args( Qubit(0), Result(0), )
            Call id(0), args( Qubit(0), )
            Call id(1), args( Qubit(0), Result(1), )
            Return"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());

    // After
    reindex_qubits(&mut program);
    expect![[r#"
        Block:
            Call id(0), args( Qubit(0), )
            Call id(2), args( Qubit(0), Result(0), )
            Call id(0), args( Qubit(1), )
            Call id(2), args( Qubit(1), Result(1), )
            Return"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());
    assert_eq!(program.num_qubits, 2);
}

#[test]
fn multiple_qubit_reindex() {
    const H: CallableId = CallableId(0);
    const MRESETZ: CallableId = CallableId(1);
    const CX: CallableId = CallableId(2);
    let mut program = Program::new();
    program.num_qubits = 2;
    program.callables.insert(H, h_decl());
    program.callables.insert(MRESETZ, mresetz_decl());
    program.callables.insert(CX, cx_decl());
    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(H, vec![Operand::Literal(Literal::Qubit(0))], None),
            Instruction::Call(
                MRESETZ,
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(0)),
                ],
                None,
            ),
            Instruction::Call(
                CX,
                vec![
                    Operand::Literal(Literal::Qubit(1)),
                    Operand::Literal(Literal::Qubit(0)),
                ],
                None,
            ),
            Instruction::Return,
        ]),
    );

    // Before
    expect![[r#"
        Block:
            Call id(0), args( Qubit(0), )
            Call id(1), args( Qubit(0), Result(0), )
            Call id(2), args( Qubit(1), Qubit(0), )
            Return"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());

    // After
    reindex_qubits(&mut program);
    expect![[r#"
        Block:
            Call id(0), args( Qubit(0), )
            Call id(3), args( Qubit(0), Result(0), )
            Call id(2), args( Qubit(1), Qubit(2), )
            Return"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());
    assert_eq!(program.num_qubits, 3);
}

#[test]
fn qubit_reindexed_multiple_times_with_mz_inserts_multiple_cx() {
    const X: CallableId = CallableId(0);
    const M: CallableId = CallableId(1);
    let mut program = Program::new();
    program.num_qubits = 1;
    program.callables.insert(X, x_decl());
    program.callables.insert(M, m_decl());
    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(X, vec![Operand::Literal(Literal::Qubit(0))], None),
            Instruction::Call(
                M,
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(0)),
                ],
                None,
            ),
            Instruction::Call(X, vec![Operand::Literal(Literal::Qubit(0))], None),
            Instruction::Call(
                M,
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(1)),
                ],
                None,
            ),
            Instruction::Call(X, vec![Operand::Literal(Literal::Qubit(0))], None),
            Instruction::Call(
                M,
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(2)),
                ],
                None,
            ),
            Instruction::Call(X, vec![Operand::Literal(Literal::Qubit(0))], None),
            Instruction::Call(
                M,
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(3)),
                ],
                None,
            ),
            Instruction::Return,
        ]),
    );

    // Before
    expect![[r#"
        Block:
            Call id(0), args( Qubit(0), )
            Call id(1), args( Qubit(0), Result(0), )
            Call id(0), args( Qubit(0), )
            Call id(1), args( Qubit(0), Result(1), )
            Call id(0), args( Qubit(0), )
            Call id(1), args( Qubit(0), Result(2), )
            Call id(0), args( Qubit(0), )
            Call id(1), args( Qubit(0), Result(3), )
            Return"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());

    // After
    reindex_qubits(&mut program);
    expect![[r#"
        Block:
            Call id(0), args( Qubit(0), )
            Call id(2), args( Qubit(0), Qubit(1), )
            Call id(1), args( Qubit(0), Result(0), )
            Call id(0), args( Qubit(1), )
            Call id(2), args( Qubit(1), Qubit(2), )
            Call id(1), args( Qubit(1), Result(1), )
            Call id(0), args( Qubit(2), )
            Call id(2), args( Qubit(2), Qubit(3), )
            Call id(1), args( Qubit(2), Result(2), )
            Call id(0), args( Qubit(3), )
            Call id(1), args( Qubit(3), Result(3), )
            Return"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());
    assert_eq!(program.num_qubits, 4);
}

#[test]
#[should_panic(expected = "Reindexing qubits across multiple blocks is not supported")]
fn qubit_reindexed_across_branches() {
    const X: CallableId = CallableId(0);
    const M: CallableId = CallableId(1);
    const READ_RESULT: CallableId = CallableId(2);
    let mut program = Program::new();
    program.num_qubits = 1;
    program.num_results = 3;
    program.callables.insert(X, x_decl());
    program.callables.insert(M, m_decl());
    program.callables.insert(READ_RESULT, read_result_decl());

    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(X, vec![Operand::Literal(Literal::Qubit(0))], None),
            Instruction::Call(
                M,
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(0)),
                ],
                None,
            ),
            Instruction::Call(
                READ_RESULT,
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
            Instruction::Call(X, vec![Operand::Literal(Literal::Qubit(0))], None),
            Instruction::Call(
                M,
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(1)),
                ],
                None,
            ),
            Instruction::Jump(BlockId(3)),
        ]),
    );
    program.blocks.insert(
        BlockId(2),
        Block(vec![
            Instruction::Call(
                M,
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(2)),
                ],
                None,
            ),
            Instruction::Jump(BlockId(3)),
        ]),
    );
    program.blocks.insert(
        BlockId(3),
        Block(vec![
            Instruction::Call(X, vec![Operand::Literal(Literal::Qubit(0))], None),
            Instruction::Return,
        ]),
    );

    // Before
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: __quantum__qis__x__body
                    call_type: Regular
                    input_type:
                        [0]: Qubit
                    output_type: <VOID>
                    body: <NONE>
                Callable 1: Callable:
                    name: __quantum__qis__m__body
                    call_type: Measurement
                    input_type:
                        [0]: Qubit
                        [1]: Result
                    output_type: <VOID>
                    body: <NONE>
                Callable 2: Callable:
                    name: __quantum__rt__read_result
                    call_type: Readout
                    input_type:
                        [0]: Result
                    output_type: Boolean
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Call id(0), args( Qubit(0), )
                    Call id(1), args( Qubit(0), Result(0), )
                    Variable(0, Boolean) = Call id(2), args( Result(0), )
                    Branch Variable(0, Boolean), 1, 2
                Block 1: Block:
                    Call id(0), args( Qubit(0), )
                    Call id(1), args( Qubit(0), Result(1), )
                    Jump(3)
                Block 2: Block:
                    Call id(1), args( Qubit(0), Result(2), )
                    Jump(3)
                Block 3: Block:
                    Call id(0), args( Qubit(0), )
                    Return
            config: Config:
                capabilities: Base
            num_qubits: 1
            num_results: 3
            tags:
    "#]]
    .assert_eq(&program.to_string());

    // After
    reindex_qubits(&mut program);
}
