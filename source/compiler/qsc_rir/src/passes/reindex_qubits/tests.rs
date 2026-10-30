// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::too_many_lines, clippy::needless_raw_string_hashes)]

use expect_test::expect;

use crate::{
    builder::{
        cx_decl, h_decl, m_decl, mresetz_decl, mx_decl, my_decl, read_result_decl, reset_decl,
        s_decl, x_decl,
    },
    rir::{
        Block, BlockId, CallableId, CallableType, Instruction, Literal, Operand, Prim, Program, Ty,
        Variable, VariableId,
    },
};

use super::reindex_qubits;

/// Returns the program's callables as sorted `id: name` lines.
fn callables_to_string(program: &Program) -> String {
    let mut callables = program
        .callables
        .iter()
        .map(|(id, callable)| (id.0, callable.name.clone()))
        .collect::<Vec<_>>();
    callables.sort_unstable();
    callables
        .into_iter()
        .map(|(id, name)| format!("{id}: {name}"))
        .collect::<Vec<_>>()
        .join("\n")
}

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
            Instruction::Call(X, vec![Operand::Literal(Literal::Qubit(0))], None, None),
            Instruction::Call(RESET, vec![Operand::Literal(Literal::Qubit(0))], None, None),
            Instruction::Call(X, vec![Operand::Literal(Literal::Qubit(0))], None, None),
            Instruction::Call(RESET, vec![Operand::Literal(Literal::Qubit(0))], None, None),
            Instruction::Return(None),
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
fn qubit_reindex_removes_initial_reset_without_reindexing() {
    const X: CallableId = CallableId(0);
    const RESET: CallableId = CallableId(1);
    let mut program = Program::new();
    program.num_qubits = 1;
    program.callables.insert(X, x_decl());
    program.callables.insert(RESET, reset_decl());
    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(RESET, vec![Operand::Literal(Literal::Qubit(0))], None, None),
            Instruction::Call(X, vec![Operand::Literal(Literal::Qubit(0))], None, None),
            Instruction::Return(None),
        ]),
    );

    // Before
    expect![[r#"
        Block:
            Call id(1), args( Qubit(0), )
            Call id(0), args( Qubit(0), )
            Return"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());

    // After
    reindex_qubits(&mut program);
    expect![[r#"
        Block:
            Call id(0), args( Qubit(0), )
            Return"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());
    assert_eq!(program.num_qubits, 1);

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
            Instruction::Call(X, vec![Operand::Literal(Literal::Qubit(0))], None, None),
            Instruction::Call(
                M,
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(0)),
                ],
                None,
                None,
            ),
            Instruction::Call(X, vec![Operand::Literal(Literal::Qubit(0))], None, None),
            Instruction::Call(
                M,
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(1)),
                ],
                None,
                None,
            ),
            Instruction::Return(None),
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
fn qubit_reindexed_after_mx() {
    const H: CallableId = CallableId(0);
    const MX: CallableId = CallableId(1);
    let mut program = Program::new();
    program.num_qubits = 1;
    program.callables.insert(H, h_decl());
    program.callables.insert(MX, mx_decl());
    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(H, vec![Operand::Literal(Literal::Qubit(0))], None, None),
            Instruction::Call(
                MX,
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(0)),
                ],
                None,
                None,
            ),
            Instruction::Call(
                MX,
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(1)),
                ],
                None,
                None,
            ),
            Instruction::Return(None),
        ]),
    );

    // Before
    expect![[r#"Block:
    Call id(0), args( Qubit(0), )
    Call id(1), args( Qubit(0), Result(0), )
    Call id(1), args( Qubit(0), Result(1), )
    Return"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());

    //After
    reindex_qubits(&mut program);
    expect![[r#"Block:
    Call id(0), args( Qubit(0), )
    Call id(0), args( Qubit(1), )
    Call id(2), args( Qubit(1), Qubit(0), )
    Call id(1), args( Qubit(0), Result(0), )
    Call id(1), args( Qubit(1), Result(1), )
    Return"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());
    assert_eq!(program.num_qubits, 2);

    // Callables after the pass.
    expect![[r#"
        0: __quantum__qis__h__body
        1: __quantum__qis__mx__body
        2: __quantum__qis__cx__body"#]]
    .assert_eq(&callables_to_string(&program));
}

#[test]
fn qubit_reindexed_after_my() {
    const H: CallableId = CallableId(0);
    const MY: CallableId = CallableId(1);
    let mut program = Program::new();
    program.num_qubits = 1;
    program.callables.insert(H, h_decl());
    program.callables.insert(MY, my_decl());
    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(H, vec![Operand::Literal(Literal::Qubit(0))], None, None),
            Instruction::Call(
                MY,
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(0)),
                ],
                None,
                None,
            ),
            Instruction::Call(
                MY,
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(1)),
                ],
                None,
                None,
            ),
            Instruction::Return(None),
        ]),
    );

    // Before
    expect![[r#"Block:
    Call id(0), args( Qubit(0), )
    Call id(1), args( Qubit(0), Result(0), )
    Call id(1), args( Qubit(0), Result(1), )
    Return"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());

    //After
    reindex_qubits(&mut program);
    expect![[r#"Block:
    Call id(0), args( Qubit(0), )
    Call id(4), args( Qubit(0), )
    Call id(0), args( Qubit(1), )
    Call id(2), args( Qubit(1), Qubit(0), )
    Call id(3), args( Qubit(0), )
    Call id(3), args( Qubit(1), )
    Call id(1), args( Qubit(0), Result(0), )
    Call id(1), args( Qubit(1), Result(1), )
    Return"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());
    assert_eq!(program.num_qubits, 2);

    // Callables after the pass.
    expect![[r#"
        0: __quantum__qis__h__body
        1: __quantum__qis__my__body
        2: __quantum__qis__cx__body
        3: __quantum__qis__s__body
        4: __quantum__qis__s__adj"#]]
    .assert_eq(&callables_to_string(&program));
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
            Instruction::Call(X, vec![Operand::Literal(Literal::Qubit(0))], None, None),
            Instruction::Call(
                MRESETZ,
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(0)),
                ],
                None,
                None,
            ),
            Instruction::Call(X, vec![Operand::Literal(Literal::Qubit(0))], None, None),
            Instruction::Call(
                MRESETZ,
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(1)),
                ],
                None,
                None,
            ),
            Instruction::Return(None),
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
            Instruction::Call(H, vec![Operand::Literal(Literal::Qubit(0))], None, None),
            Instruction::Call(
                MRESETZ,
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(0)),
                ],
                None,
                None,
            ),
            Instruction::Call(
                CX,
                vec![
                    Operand::Literal(Literal::Qubit(1)),
                    Operand::Literal(Literal::Qubit(0)),
                ],
                None,
                None,
            ),
            Instruction::Return(None),
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
fn multiple_qubit_reindex_with_mx_my_mz_resets() {
    const S: CallableId = CallableId(0);
    const H: CallableId = CallableId(1);
    const CX: CallableId = CallableId(2);
    const M: CallableId = CallableId(3);
    const MX: CallableId = CallableId(4);
    const MY: CallableId = CallableId(5);
    const RESET: CallableId = CallableId(6);
    let mut program = Program::new();
    program.num_qubits = 2;
    program.callables.insert(S, s_decl());
    program.callables.insert(H, h_decl());
    program.callables.insert(CX, cx_decl());
    program.callables.insert(MX, mx_decl());
    program.callables.insert(MY, my_decl());
    program.callables.insert(M, m_decl());
    program.callables.insert(RESET, reset_decl());
    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(H, vec![Operand::Literal(Literal::Qubit(0))], None, None),
            Instruction::Call(
                MX,
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(0)),
                ],
                None,
                None,
            ),
            Instruction::Call(RESET, vec![Operand::Literal(Literal::Qubit(0))], None, None),
            Instruction::Call(
                CX,
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Qubit(1)),
                ],
                None,
                None,
            ),
            Instruction::Call(S, vec![Operand::Literal(Literal::Qubit(0))], None, None),
            Instruction::Call(
                MY,
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(1)),
                ],
                None,
                None,
            ),
            Instruction::Call(
                M,
                vec![
                    Operand::Literal(Literal::Qubit(1)),
                    Operand::Literal(Literal::Result(2)),
                ],
                None,
                None,
            ),
            Instruction::Call(S, vec![Operand::Literal(Literal::Qubit(0))], None, None),
            Instruction::Call(RESET, vec![Operand::Literal(Literal::Qubit(1))], None, None),
            Instruction::Return(None),
        ]),
    );

    // Before
    expect![[r#"
        Block:
            Call id(1), args( Qubit(0), )
            Call id(4), args( Qubit(0), Result(0), )
            Call id(6), args( Qubit(0), )
            Call id(2), args( Qubit(0), Qubit(1), )
            Call id(0), args( Qubit(0), )
            Call id(5), args( Qubit(0), Result(1), )
            Call id(3), args( Qubit(1), Result(2), )
            Call id(0), args( Qubit(0), )
            Call id(6), args( Qubit(1), )
            Return"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());

    // After
    reindex_qubits(&mut program);
    expect![[r#"
        Block:
            Call id(1), args( Qubit(0), )
            Call id(4), args( Qubit(0), Result(0), )
            Call id(2), args( Qubit(2), Qubit(1), )
            Call id(0), args( Qubit(2), )
            Call id(7), args( Qubit(2), )
            Call id(1), args( Qubit(3), )
            Call id(2), args( Qubit(3), Qubit(2), )
            Call id(0), args( Qubit(2), )
            Call id(0), args( Qubit(3), )
            Call id(5), args( Qubit(2), Result(1), )
            Call id(3), args( Qubit(1), Result(2), )
            Call id(0), args( Qubit(3), )
            Return"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());
    assert_eq!(program.num_qubits, 4);

    // Callables after the pass.
    expect![[r#"
        0: __quantum__qis__s__body
        1: __quantum__qis__h__body
        2: __quantum__qis__cx__body
        3: __quantum__qis__m__body
        4: __quantum__qis__mx__body
        5: __quantum__qis__my__body
        7: __quantum__qis__s__adj"#]]
    .assert_eq(&callables_to_string(&program));
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
            Instruction::Call(X, vec![Operand::Literal(Literal::Qubit(0))], None, None),
            Instruction::Call(
                M,
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(0)),
                ],
                None,
                None,
            ),
            Instruction::Call(X, vec![Operand::Literal(Literal::Qubit(0))], None, None),
            Instruction::Call(
                M,
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(1)),
                ],
                None,
                None,
            ),
            Instruction::Call(X, vec![Operand::Literal(Literal::Qubit(0))], None, None),
            Instruction::Call(
                M,
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(2)),
                ],
                None,
                None,
            ),
            Instruction::Call(X, vec![Operand::Literal(Literal::Qubit(0))], None, None),
            Instruction::Call(
                M,
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(3)),
                ],
                None,
                None,
            ),
            Instruction::Return(None),
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
fn qubit_reindexed_multiple_times_with_mx_and_resets() {
    const X: CallableId = CallableId(0);
    const MX: CallableId = CallableId(1);
    const RESET: CallableId = CallableId(2);
    let mut program = Program::new();
    // Only Qubit(0) is used; fresh qubit IDs start after this count.
    program.num_qubits = 1;
    program.callables.insert(X, x_decl());
    program.callables.insert(MX, mx_decl());
    program.callables.insert(RESET, reset_decl());
    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(X, vec![Operand::Literal(Literal::Qubit(0))], None, None),
            Instruction::Call(
                MX,
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(0)),
                ],
                None,
                None,
            ),
            Instruction::Call(RESET, vec![Operand::Literal(Literal::Qubit(0))], None, None),
            Instruction::Call(
                MX,
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(1)),
                ],
                None,
                None,
            ),
            Instruction::Call(
                MX,
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(2)),
                ],
                None,
                None,
            ),
            Instruction::Call(X, vec![Operand::Literal(Literal::Qubit(0))], None, None),
            Instruction::Call(RESET, vec![Operand::Literal(Literal::Qubit(0))], None, None),
            Instruction::Return(None),
        ]),
    );

    // Before
    expect![[r#"
        Block:
            Call id(0), args( Qubit(0), )
            Call id(1), args( Qubit(0), Result(0), )
            Call id(2), args( Qubit(0), )
            Call id(1), args( Qubit(0), Result(1), )
            Call id(1), args( Qubit(0), Result(2), )
            Call id(0), args( Qubit(0), )
            Call id(2), args( Qubit(0), )
            Return"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());

    // After
    reindex_qubits(&mut program);
    expect![[r#"
        Block:
            Call id(0), args( Qubit(0), )
            Call id(1), args( Qubit(0), Result(0), )
            Call id(4), args( Qubit(2), )
            Call id(3), args( Qubit(2), Qubit(1), )
            Call id(1), args( Qubit(1), Result(1), )
            Call id(4), args( Qubit(3), )
            Call id(3), args( Qubit(3), Qubit(2), )
            Call id(1), args( Qubit(2), Result(2), )
            Call id(0), args( Qubit(3), )
            Return"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());
    assert_eq!(program.num_qubits, 4);

    // Callables after the pass.
    expect![[r#"
        0: __quantum__qis__x__body
        1: __quantum__qis__mx__body
        3: __quantum__qis__cx__body
        4: __quantum__qis__h__body"#]]
    .assert_eq(&callables_to_string(&program));
}

#[test]
fn qubit_reindexed_multiple_times_with_my_and_resets() {
    const S: CallableId = CallableId(0);
    const MY: CallableId = CallableId(1);
    const RESET: CallableId = CallableId(2);
    let mut program = Program::new();
    // Only Qubit(0) is used; fresh qubit IDs start after this count.
    program.num_qubits = 1;
    program.callables.insert(S, s_decl());
    program.callables.insert(MY, my_decl());
    program.callables.insert(RESET, reset_decl());
    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(
                MY,
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(0)),
                ],
                None,
                None,
            ),
            Instruction::Call(RESET, vec![Operand::Literal(Literal::Qubit(0))], None, None),
            Instruction::Call(
                MY,
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(1)),
                ],
                None,
                None,
            ),
            Instruction::Call(
                MY,
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(2)),
                ],
                None,
                None,
            ),
            Instruction::Call(S, vec![Operand::Literal(Literal::Qubit(0))], None, None),
            Instruction::Call(RESET, vec![Operand::Literal(Literal::Qubit(0))], None, None),
            Instruction::Return(None),
        ]),
    );

    // Before
    expect![[r#"
        Block:
            Call id(1), args( Qubit(0), Result(0), )
            Call id(2), args( Qubit(0), )
            Call id(1), args( Qubit(0), Result(1), )
            Call id(1), args( Qubit(0), Result(2), )
            Call id(0), args( Qubit(0), )
            Call id(2), args( Qubit(0), )
            Return"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());

    // After
    reindex_qubits(&mut program);
    expect![[r#"
        Block:
            Call id(1), args( Qubit(0), Result(0), )
            Call id(5), args( Qubit(1), )
            Call id(4), args( Qubit(2), )
            Call id(3), args( Qubit(2), Qubit(1), )
            Call id(0), args( Qubit(1), )
            Call id(0), args( Qubit(2), )
            Call id(1), args( Qubit(1), Result(1), )
            Call id(5), args( Qubit(2), )
            Call id(4), args( Qubit(3), )
            Call id(3), args( Qubit(3), Qubit(2), )
            Call id(0), args( Qubit(2), )
            Call id(0), args( Qubit(3), )
            Call id(1), args( Qubit(2), Result(2), )
            Call id(0), args( Qubit(3), )
            Return"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());
    assert_eq!(program.num_qubits, 4);

    // Callables after the pass.
    expect![[r#"
        0: __quantum__qis__s__body
        1: __quantum__qis__my__body
        3: __quantum__qis__cx__body
        4: __quantum__qis__h__body
        5: __quantum__qis__s__adj"#]]
    .assert_eq(&callables_to_string(&program));
}

#[test]
fn qubit_reindexed_multiple_times_with_mz_mx_my_resets() {
    const S: CallableId = CallableId(0);
    const H: CallableId = CallableId(1);
    const M: CallableId = CallableId(2);
    const MX: CallableId = CallableId(3);
    const MY: CallableId = CallableId(4);
    const RESET: CallableId = CallableId(5);
    const MRESETZ: CallableId = CallableId(6);
    let mut program = Program::new();
    program.num_qubits = 1;
    program.callables.insert(S, s_decl());
    program.callables.insert(H, h_decl());
    program.callables.insert(M, m_decl());
    program.callables.insert(MX, mx_decl());
    program.callables.insert(MY, my_decl());
    program.callables.insert(RESET, reset_decl());
    program.callables.insert(MRESETZ, mresetz_decl());
    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(
                MX,
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(0)),
                ],
                None,
                None,
            ),
            Instruction::Call(RESET, vec![Operand::Literal(Literal::Qubit(0))], None, None),
            Instruction::Call(S, vec![Operand::Literal(Literal::Qubit(0))], None, None),
            Instruction::Call(
                M,
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(1)),
                ],
                None,
                None,
            ),
            Instruction::Call(H, vec![Operand::Literal(Literal::Qubit(0))], None, None),
            Instruction::Call(
                MY,
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(2)),
                ],
                None,
                None,
            ),
            Instruction::Call(
                MRESETZ,
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(3)),
                ],
                None,
                None,
            ),
            Instruction::Call(H, vec![Operand::Literal(Literal::Qubit(0))], None, None),
            Instruction::Call(
                MX,
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(4)),
                ],
                None,
                None,
            ),
            Instruction::Return(None),
        ]),
    );

    // Before
    expect![[r#"
        Block:
            Call id(3), args( Qubit(0), Result(0), )
            Call id(5), args( Qubit(0), )
            Call id(0), args( Qubit(0), )
            Call id(2), args( Qubit(0), Result(1), )
            Call id(1), args( Qubit(0), )
            Call id(4), args( Qubit(0), Result(2), )
            Call id(6), args( Qubit(0), Result(3), )
            Call id(1), args( Qubit(0), )
            Call id(3), args( Qubit(0), Result(4), )
            Return"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());

    // After
    reindex_qubits(&mut program);
    expect![[r#"
        Block:
            Call id(3), args( Qubit(0), Result(0), )
            Call id(0), args( Qubit(1), )
            Call id(7), args( Qubit(1), Qubit(2), )
            Call id(2), args( Qubit(1), Result(1), )
            Call id(1), args( Qubit(2), )
            Call id(8), args( Qubit(2), )
            Call id(1), args( Qubit(3), )
            Call id(7), args( Qubit(3), Qubit(2), )
            Call id(0), args( Qubit(2), )
            Call id(0), args( Qubit(3), )
            Call id(4), args( Qubit(2), Result(2), )
            Call id(2), args( Qubit(3), Result(3), )
            Call id(1), args( Qubit(4), )
            Call id(3), args( Qubit(4), Result(4), )
            Return"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());
    assert_eq!(program.num_qubits, 5);

    // Callables after the pass.
    expect![[r#"
        0: __quantum__qis__s__body
        1: __quantum__qis__h__body
        2: __quantum__qis__m__body
        3: __quantum__qis__mx__body
        4: __quantum__qis__my__body
        7: __quantum__qis__cx__body
        8: __quantum__qis__s__adj"#]]
    .assert_eq(&callables_to_string(&program));
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
            Instruction::Call(X, vec![Operand::Literal(Literal::Qubit(0))], None, None),
            Instruction::Call(
                M,
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(0)),
                ],
                None,
                None,
            ),
            Instruction::Call(
                READ_RESULT,
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
            Instruction::Call(X, vec![Operand::Literal(Literal::Qubit(0))], None, None),
            Instruction::Call(
                M,
                vec![
                    Operand::Literal(Literal::Qubit(0)),
                    Operand::Literal(Literal::Result(1)),
                ],
                None,
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
                None,
            ),
            Instruction::Jump(BlockId(3)),
        ]),
    );
    program.blocks.insert(
        BlockId(3),
        Block(vec![
            Instruction::Call(X, vec![Operand::Literal(Literal::Qubit(0))], None, None),
            Instruction::Return(None),
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
