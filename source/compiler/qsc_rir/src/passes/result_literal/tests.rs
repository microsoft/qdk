// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::too_many_lines, clippy::needless_raw_string_hashes)]

use expect_test::expect;

use crate::rir::{
    Block, BlockId, Callable, CallableId, CallableType, Instruction, Literal, Operand, Prim,
    Program, Ty, Variable, VariableId,
};

use super::transform_result_literals;

const RUNTIME_RESULT: Variable = Variable {
    variable_id: VariableId(0),
    ty: Ty::Prim(Prim::Result),
};

fn result_array_program(operands: Vec<Operand>) -> Program {
    let mut program = Program::new();
    program.callables.insert(
        CallableId(0),
        Callable {
            name: "main".to_string(),
            input_type: Vec::new(),
            output_type: None,
            body: Some(BlockId(0)),
            input_vars: Vec::new(),
            call_type: CallableType::Regular,
        },
    );
    program
        .callables
        .insert(CallableId(1), crate::builder::initialize_decl());
    let array = Variable::new_array(VariableId(1), operands.len(), Prim::Result);
    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(
                CallableId(1),
                vec![Operand::Literal(Literal::NullPointer)],
                None,
                None,
            ),
            Instruction::Store(Operand::Literal(Literal::Result(0)), RUNTIME_RESULT),
            Instruction::StoreArray(operands, array),
            Instruction::Return(None),
        ]),
    );
    program.num_results = 1;
    program
}

#[test]
fn store_array_zero_literals_share_one_initialized_result() {
    let mut program = result_array_program(vec![
        Operand::Variable(RUNTIME_RESULT),
        Operand::Literal(Literal::ResultLit(false)),
        Operand::Literal(Literal::ResultLit(false)),
    ]);

    transform_result_literals(&mut program);

    assert_eq!(program.num_results, 2);
    expect![[r#"
        Block:
            Call id(1), args( Pointer, )
            Call id(2), args( Bool(false), Result(1), )
            Variable(0, Result) = Store Result(0)
            Variable(1, Array(3, Result)) = StoreArray [Variable(0, Result), Result(1), Result(1)]
            Return"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());
}

#[test]
fn store_array_one_literals_share_one_initialized_result() {
    let mut program = result_array_program(vec![
        Operand::Literal(Literal::ResultLit(true)),
        Operand::Variable(RUNTIME_RESULT),
        Operand::Literal(Literal::ResultLit(true)),
    ]);

    transform_result_literals(&mut program);

    assert_eq!(program.num_results, 3);
    expect![[r#"
        Block:
            Call id(1), args( Pointer, )
            Call id(2), args( Bool(true), Result(2), )
            Variable(0, Result) = Store Result(0)
            Variable(1, Array(3, Result)) = StoreArray [Result(2), Variable(0, Result), Result(2)]
            Return"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());
}

#[test]
fn store_array_zero_and_one_literals_use_distinct_initialized_results() {
    let mut program = result_array_program(vec![
        Operand::Variable(RUNTIME_RESULT),
        Operand::Literal(Literal::ResultLit(false)),
        Operand::Literal(Literal::ResultLit(true)),
        Operand::Literal(Literal::ResultLit(false)),
        Operand::Literal(Literal::ResultLit(true)),
    ]);

    transform_result_literals(&mut program);

    assert_eq!(program.num_results, 3);
    expect![[r#"
        Block:
            Call id(1), args( Pointer, )
            Call id(2), args( Bool(false), Result(1), )
            Call id(2), args( Bool(true), Result(2), )
            Variable(0, Result) = Store Result(0)
            Variable(1, Array(5, Result)) = StoreArray [Variable(0, Result), Result(1), Result(2), Result(1), Result(2)]
            Return"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());
}

#[test]
fn program_with_no_result_literals_unchanged() {
    let mut program = Program::new();
    program.callables.insert(
        CallableId(0),
        Callable {
            name: "main".to_string(),
            input_type: Vec::new(),
            output_type: None,
            body: Some(BlockId(0)),
            input_vars: Vec::new(),
            call_type: CallableType::Regular,
        },
    );
    // Add a call to callable id 1 to mock the initialize call unconditionally added by
    // partial eval. All RIR programs are expected to have this call in the entry point,
    // and the result literal transform pass looks for it to determine the instruction
    // insertion location.
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "initialize".to_string(),
            input_type: Vec::new(),
            output_type: None,
            body: None,
            input_vars: Vec::new(),
            call_type: CallableType::Regular,
        },
    );
    program.callables.insert(
        CallableId(2),
        Callable {
            name: "read_result".to_string(),
            input_type: vec![Ty::Prim(Prim::Result)],
            output_type: Some(Ty::Prim(Prim::Boolean)),
            body: None,
            input_vars: vec![VariableId(0)],
            call_type: CallableType::Regular,
        },
    );
    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(CallableId(1), Vec::new(), None, None),
            Instruction::Call(
                CallableId(2),
                vec![Operand::Literal(Literal::Result(0))],
                Some(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                None,
            ),
        ]),
    );
    program.num_results = 1;

    // Before
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: <VOID>
                    body: 0
                Callable 1: Callable:
                    name: initialize
                    call_type: Regular
                    input_type: <VOID>
                    output_type: <VOID>
                    body: <NONE>
                Callable 2: Callable:
                    name: read_result
                    call_type: Regular
                    input_type:
                        [0]: Result
                    input_vars:
                        [0]: 0
                    output_type: Boolean
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Call id(1), args( )
                    Variable(0, Boolean) = Call id(2), args( Result(0), )
            config: Config:
                capabilities: Base
            num_qubits: 0
            num_results: 1
            tags:
    "#]]
    .assert_eq(&program.to_string());

    transform_result_literals(&mut program);
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: <VOID>
                    body: 0
                Callable 1: Callable:
                    name: initialize
                    call_type: Regular
                    input_type: <VOID>
                    output_type: <VOID>
                    body: <NONE>
                Callable 2: Callable:
                    name: read_result
                    call_type: Regular
                    input_type:
                        [0]: Result
                    input_vars:
                        [0]: 0
                    output_type: Boolean
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Call id(1), args( )
                    Variable(0, Boolean) = Call id(2), args( Result(0), )
            config: Config:
                capabilities: Base
            num_qubits: 0
            num_results: 1
            tags:
    "#]]
    .assert_eq(&program.to_string());
}

#[test]
fn program_with_result_literal_zero_adds_single_write_result_call() {
    let mut program = Program::new();
    program.callables.insert(
        CallableId(0),
        Callable {
            name: "main".to_string(),
            input_type: Vec::new(),
            output_type: None,
            body: Some(BlockId(0)),
            input_vars: Vec::new(),
            call_type: CallableType::Regular,
        },
    );
    // Add a call to callable id 1 to mock the initialize call unconditionally added by
    // partial eval. All RIR programs are expected to have this call in the entry point,
    // and the result literal transform pass looks for it to determine the instruction
    // insertion location.
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "initialize".to_string(),
            input_type: Vec::new(),
            output_type: None,
            body: None,
            input_vars: Vec::new(),
            call_type: CallableType::Regular,
        },
    );
    program.callables.insert(
        CallableId(2),
        Callable {
            name: "read_result".to_string(),
            input_type: vec![Ty::Prim(Prim::Result)],
            output_type: Some(Ty::Prim(Prim::Boolean)),
            body: None,
            input_vars: vec![VariableId(0)],
            call_type: CallableType::Regular,
        },
    );
    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(CallableId(1), Vec::new(), None, None),
            Instruction::Call(
                CallableId(2),
                vec![Operand::Literal(Literal::Result(0))],
                Some(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                None,
            ),
            Instruction::Call(
                CallableId(2),
                vec![Operand::Literal(Literal::ResultLit(false))],
                Some(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                None,
            ),
        ]),
    );
    program.num_results = 1;

    // Before
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: <VOID>
                    body: 0
                Callable 1: Callable:
                    name: initialize
                    call_type: Regular
                    input_type: <VOID>
                    output_type: <VOID>
                    body: <NONE>
                Callable 2: Callable:
                    name: read_result
                    call_type: Regular
                    input_type:
                        [0]: Result
                    input_vars:
                        [0]: 0
                    output_type: Boolean
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Call id(1), args( )
                    Variable(0, Boolean) = Call id(2), args( Result(0), )
                    Variable(1, Boolean) = Call id(2), args( ResultLit(false), )
            config: Config:
                capabilities: Base
            num_qubits: 0
            num_results: 1
            tags:
    "#]]
    .assert_eq(&program.to_string());

    transform_result_literals(&mut program);
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: <VOID>
                    body: 0
                Callable 1: Callable:
                    name: initialize
                    call_type: Regular
                    input_type: <VOID>
                    output_type: <VOID>
                    body: <NONE>
                Callable 2: Callable:
                    name: read_result
                    call_type: Regular
                    input_type:
                        [0]: Result
                    input_vars:
                        [0]: 0
                    output_type: Boolean
                    body: <NONE>
                Callable 3: Callable:
                    name: __quantum__rt__write_result
                    call_type: Regular
                    input_type:
                        [0]: Boolean
                        [1]: Result
                    output_type: <VOID>
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Call id(1), args( )
                    Call id(3), args( Bool(false), Result(1), )
                    Variable(0, Boolean) = Call id(2), args( Result(0), )
                    Variable(1, Boolean) = Call id(2), args( Result(1), )
            config: Config:
                capabilities: Base
            num_qubits: 0
            num_results: 2
            tags:
    "#]]
    .assert_eq(&program.to_string());
}

#[test]
fn program_with_result_literal_one_adds_single_write_result_call() {
    let mut program = Program::new();
    program.callables.insert(
        CallableId(0),
        Callable {
            name: "main".to_string(),
            input_type: Vec::new(),
            output_type: None,
            body: Some(BlockId(0)),
            input_vars: Vec::new(),
            call_type: CallableType::Regular,
        },
    );
    // Add a call to callable id 1 to mock the initialize call unconditionally added by
    // partial eval. All RIR programs are expected to have this call in the entry point,
    // and the result literal transform pass looks for it to determine the instruction
    // insertion location.
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "initialize".to_string(),
            input_type: Vec::new(),
            output_type: None,
            body: None,
            input_vars: Vec::new(),
            call_type: CallableType::Regular,
        },
    );
    program.callables.insert(
        CallableId(2),
        Callable {
            name: "read_result".to_string(),
            input_type: vec![Ty::Prim(Prim::Result)],
            output_type: Some(Ty::Prim(Prim::Boolean)),
            body: None,
            input_vars: vec![VariableId(0)],
            call_type: CallableType::Regular,
        },
    );
    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(CallableId(1), Vec::new(), None, None),
            Instruction::Call(
                CallableId(2),
                vec![Operand::Literal(Literal::Result(0))],
                Some(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                None,
            ),
            Instruction::Call(
                CallableId(2),
                vec![Operand::Literal(Literal::ResultLit(true))],
                Some(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                None,
            ),
        ]),
    );
    program.num_results = 1;

    // Before
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: <VOID>
                    body: 0
                Callable 1: Callable:
                    name: initialize
                    call_type: Regular
                    input_type: <VOID>
                    output_type: <VOID>
                    body: <NONE>
                Callable 2: Callable:
                    name: read_result
                    call_type: Regular
                    input_type:
                        [0]: Result
                    input_vars:
                        [0]: 0
                    output_type: Boolean
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Call id(1), args( )
                    Variable(0, Boolean) = Call id(2), args( Result(0), )
                    Variable(1, Boolean) = Call id(2), args( ResultLit(true), )
            config: Config:
                capabilities: Base
            num_qubits: 0
            num_results: 1
            tags:
    "#]]
    .assert_eq(&program.to_string());

    transform_result_literals(&mut program);
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: <VOID>
                    body: 0
                Callable 1: Callable:
                    name: initialize
                    call_type: Regular
                    input_type: <VOID>
                    output_type: <VOID>
                    body: <NONE>
                Callable 2: Callable:
                    name: read_result
                    call_type: Regular
                    input_type:
                        [0]: Result
                    input_vars:
                        [0]: 0
                    output_type: Boolean
                    body: <NONE>
                Callable 3: Callable:
                    name: __quantum__rt__write_result
                    call_type: Regular
                    input_type:
                        [0]: Boolean
                        [1]: Result
                    output_type: <VOID>
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Call id(1), args( )
                    Call id(3), args( Bool(true), Result(2), )
                    Variable(0, Boolean) = Call id(2), args( Result(0), )
                    Variable(1, Boolean) = Call id(2), args( Result(2), )
            config: Config:
                capabilities: Base
            num_qubits: 0
            num_results: 3
            tags:
    "#]]
    .assert_eq(&program.to_string());
}

#[test]
fn program_with_both_result_literal_zero_and_one_adds_two_write_result_calls() {
    let mut program = Program::new();
    program.callables.insert(
        CallableId(0),
        Callable {
            name: "main".to_string(),
            input_type: Vec::new(),
            output_type: None,
            body: Some(BlockId(0)),
            input_vars: Vec::new(),
            call_type: CallableType::Regular,
        },
    );
    // Add a call to callable id 1 to mock the initialize call unconditionally added by
    // partial eval. All RIR programs are expected to have this call in the entry point,
    // and the result literal transform pass looks for it to determine the instruction
    // insertion location.
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "initialize".to_string(),
            input_type: Vec::new(),
            output_type: None,
            body: None,
            input_vars: Vec::new(),
            call_type: CallableType::Regular,
        },
    );
    program.callables.insert(
        CallableId(2),
        Callable {
            name: "read_result".to_string(),
            input_type: vec![Ty::Prim(Prim::Result)],
            output_type: Some(Ty::Prim(Prim::Boolean)),
            body: None,
            input_vars: vec![VariableId(0)],
            call_type: CallableType::Regular,
        },
    );
    program.blocks.insert(
        BlockId(0),
        Block(vec![
            Instruction::Call(CallableId(1), Vec::new(), None, None),
            Instruction::Call(
                CallableId(2),
                vec![Operand::Literal(Literal::Result(0))],
                Some(Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                None,
            ),
            Instruction::Call(
                CallableId(2),
                vec![Operand::Literal(Literal::ResultLit(false))],
                Some(Variable {
                    variable_id: VariableId(1),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                None,
            ),
            Instruction::Call(
                CallableId(2),
                vec![Operand::Literal(Literal::ResultLit(true))],
                Some(Variable {
                    variable_id: VariableId(2),
                    ty: Ty::Prim(Prim::Boolean),
                }),
                None,
            ),
        ]),
    );
    program.num_results = 1;

    // Before
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: <VOID>
                    body: 0
                Callable 1: Callable:
                    name: initialize
                    call_type: Regular
                    input_type: <VOID>
                    output_type: <VOID>
                    body: <NONE>
                Callable 2: Callable:
                    name: read_result
                    call_type: Regular
                    input_type:
                        [0]: Result
                    input_vars:
                        [0]: 0
                    output_type: Boolean
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Call id(1), args( )
                    Variable(0, Boolean) = Call id(2), args( Result(0), )
                    Variable(1, Boolean) = Call id(2), args( ResultLit(false), )
                    Variable(2, Boolean) = Call id(2), args( ResultLit(true), )
            config: Config:
                capabilities: Base
            num_qubits: 0
            num_results: 1
            tags:
    "#]]
    .assert_eq(&program.to_string());

    transform_result_literals(&mut program);
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: <VOID>
                    body: 0
                Callable 1: Callable:
                    name: initialize
                    call_type: Regular
                    input_type: <VOID>
                    output_type: <VOID>
                    body: <NONE>
                Callable 2: Callable:
                    name: read_result
                    call_type: Regular
                    input_type:
                        [0]: Result
                    input_vars:
                        [0]: 0
                    output_type: Boolean
                    body: <NONE>
                Callable 3: Callable:
                    name: __quantum__rt__write_result
                    call_type: Regular
                    input_type:
                        [0]: Boolean
                        [1]: Result
                    output_type: <VOID>
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Call id(1), args( )
                    Call id(3), args( Bool(false), Result(1), )
                    Call id(3), args( Bool(true), Result(2), )
                    Variable(0, Boolean) = Call id(2), args( Result(0), )
                    Variable(1, Boolean) = Call id(2), args( Result(1), )
                    Variable(2, Boolean) = Call id(2), args( Result(2), )
            config: Config:
                capabilities: Base
            num_qubits: 0
            num_results: 3
            tags:
    "#]]
    .assert_eq(&program.to_string());
}
