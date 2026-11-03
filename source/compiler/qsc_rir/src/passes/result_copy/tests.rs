// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;
use qsc_data_structures::target::Profile;

use super::copy_results;
use crate::{
    builder,
    passes::{check_and_transform, check_types},
    rir::{
        Block, BlockId, Callable, CallableId, CallableType, Instruction, Literal, Operand, Prim,
        Program, Ty, Variable, VariableId,
    },
};

const READ: CallableId = CallableId(1);
const WRITE: CallableId = CallableId(2);

fn result(id: u32) -> Operand {
    Operand::Literal(Literal::Result(id))
}

fn variable(id: u32) -> Variable {
    Variable {
        variable_id: VariableId(id),
        ty: Ty::Prim(Prim::Result),
    }
}

fn program(mut instructions: Vec<Instruction>) -> Program {
    instructions.push(Instruction::Return(None));
    let mut program = Program::with_blocks(vec![(BlockId(0), Block(instructions))]);
    program.config.capabilities = Profile::Adaptive.into();
    program.num_results = 2;
    program.callables.insert(READ, builder::read_result_decl());
    program
        .callables
        .insert(WRITE, builder::write_result_decl());
    program
}

fn transform(program: &mut Program) {
    check_types(program);
    copy_results(program);
    check_types(program);
}

fn body(program: &Program) -> String {
    program.get_block(BlockId(0)).to_string()
}

fn add_body(program: &mut Program, id: u32, instructions: Vec<Instruction>) {
    program.callables.insert(
        CallableId(id),
        Callable {
            name: format!("body_{id}"),
            input_type: vec![],
            input_vars: vec![],
            output_type: None,
            body: Some(BlockId(id)),
            call_type: CallableType::Regular,
        },
    );
    program.blocks.insert(BlockId(id), Block(instructions));
}

fn call(id: u32) -> Instruction {
    Instruction::Call(CallableId(id), vec![], None, None)
}

#[test]
fn literal_stores_write_bits_into_one_destination_register() {
    let mut program = program(vec![
        Instruction::Store(Operand::Literal(Literal::ResultLit(false)), variable(0)),
        Instruction::Store(Operand::Literal(Literal::ResultLit(true)), variable(0)),
    ]);
    transform(&mut program);
    assert_eq!(program.num_results, 3);
    expect![[r#"
        Block:
            Call id(2), args( Bool(false), Result(2), )
            Variable(0, Result) = Store Result(2)
            Call id(2), args( Bool(true), Result(2), )
            Variable(0, Result) = Store Result(2)
            Return"#]]
    .assert_eq(&body(&program));
}

#[test]
fn measurement_operand_is_read_before_writing_private_storage() {
    let mut program = program(vec![Instruction::Store(result(0), variable(0))]);
    transform(&mut program);
    assert_eq!(program.num_results, 3);
    expect![[r#"
        Block:
            Variable(1, Boolean) = Call id(1), args( Result(0), )
            Call id(2), args( Variable(1, Boolean), Result(2), )
            Variable(0, Result) = Store Result(2)
            Return"#]]
    .assert_eq(&body(&program));
}

#[test]
fn alias_store_receives_distinct_storage_before_source_reassignment() {
    let mut program = program(vec![
        Instruction::Store(result(0), variable(0)),
        Instruction::Store(Operand::Variable(variable(0)), variable(1)),
        Instruction::Store(result(1), variable(0)),
    ]);
    transform(&mut program);
    assert_eq!(program.num_results, 4);
    expect![[r#"
        Block:
            Variable(2, Boolean) = Call id(1), args( Result(0), )
            Call id(2), args( Variable(2, Boolean), Result(2), )
            Variable(0, Result) = Store Result(2)
            Variable(3, Boolean) = Call id(1), args( Variable(0, Result), )
            Call id(2), args( Variable(3, Boolean), Result(3), )
            Variable(1, Result) = Store Result(3)
            Variable(4, Boolean) = Call id(1), args( Result(1), )
            Call id(2), args( Variable(4, Boolean), Result(2), )
            Variable(0, Result) = Store Result(2)
            Return"#]]
    .assert_eq(&body(&program));
}

#[test]
fn self_assignment_reads_before_overwriting_its_register() {
    let mut program = program(vec![
        Instruction::Store(result(0), variable(0)),
        Instruction::Store(Operand::Variable(variable(0)), variable(0)),
    ]);
    transform(&mut program);
    assert_eq!(program.num_results, 3);
    expect![[r#"
        Block:
            Variable(1, Boolean) = Call id(1), args( Result(0), )
            Call id(2), args( Variable(1, Boolean), Result(2), )
            Variable(0, Result) = Store Result(2)
            Variable(2, Boolean) = Call id(1), args( Variable(0, Result), )
            Call id(2), args( Variable(2, Boolean), Result(2), )
            Variable(0, Result) = Store Result(2)
            Return"#]]
    .assert_eq(&body(&program));
}

#[test]
fn array_elements_receive_distinct_storage_including_duplicate_sources() {
    let array = Variable::new_array(VariableId(0), 3, Prim::Result);
    let mut program = program(vec![Instruction::StoreArray(
        vec![
            result(0),
            result(0),
            Operand::Literal(Literal::ResultLit(true)),
        ],
        array,
    )]);
    transform(&mut program);
    assert_eq!(program.num_results, 5);
    expect![[r#"
        Block:
            Variable(1, Boolean) = Call id(1), args( Result(0), )
            Variable(2, Boolean) = Call id(1), args( Result(0), )
            Call id(2), args( Variable(1, Boolean), Result(2), )
            Call id(2), args( Variable(2, Boolean), Result(3), )
            Call id(2), args( Bool(true), Result(4), )
            Variable(0, Array(3, Result)) = StoreArray [Result(2), Result(3), Result(4)]
            Return"#]]
    .assert_eq(&body(&program));
}

#[test]
fn array_swap_reads_all_old_elements_before_any_destination_write() {
    let array = Variable::new_array(VariableId(0), 2, Prim::Result);
    let mut program = program(vec![
        Instruction::StoreArray(vec![result(0), result(1)], array),
        Instruction::Index(
            Operand::Variable(array),
            Operand::Literal(Literal::Integer(1)),
            variable(1),
        ),
        Instruction::Index(
            Operand::Variable(array),
            Operand::Literal(Literal::Integer(0)),
            variable(2),
        ),
        Instruction::StoreArray(
            vec![
                Operand::Variable(variable(1)),
                Operand::Variable(variable(2)),
            ],
            array,
        ),
    ]);
    transform(&mut program);
    assert_eq!(program.num_results, 4);
    expect![[r#"
        Block:
            Variable(3, Boolean) = Call id(1), args( Result(0), )
            Variable(4, Boolean) = Call id(1), args( Result(1), )
            Call id(2), args( Variable(3, Boolean), Result(2), )
            Call id(2), args( Variable(4, Boolean), Result(3), )
            Variable(0, Array(2, Result)) = StoreArray [Result(2), Result(3)]
            Variable(1, Result) = Index Variable(0, Array(2, Result)), Integer(1)
            Variable(2, Result) = Index Variable(0, Array(2, Result)), Integer(0)
            Variable(5, Boolean) = Call id(1), args( Variable(1, Result), )
            Variable(6, Boolean) = Call id(1), args( Variable(2, Result), )
            Call id(2), args( Variable(5, Boolean), Result(2), )
            Call id(2), args( Variable(6, Boolean), Result(3), )
            Variable(0, Array(2, Result)) = StoreArray [Result(2), Result(3)]
            Return"#]]
    .assert_eq(&body(&program));
}

#[test]
fn empty_result_array_adds_no_registers_or_instructions() {
    let mut program = program(vec![Instruction::StoreArray(
        vec![],
        Variable::new_array(VariableId(0), 0, Prim::Result),
    )]);
    let original = program.to_string();
    transform(&mut program);
    assert_eq!(program.to_string(), original);
}

#[test]
fn non_result_stores_remain_unchanged() {
    let mut program = program(vec![
        Instruction::Store(
            Operand::Literal(Literal::Integer(7)),
            Variable::new_integer(VariableId(0)),
        ),
        Instruction::StoreArray(
            vec![Operand::Literal(Literal::Qubit(0))],
            Variable::new_array(VariableId(1), 1, Prim::Qubit),
        ),
    ]);
    let original = program.to_string();
    transform(&mut program);
    assert_eq!(program.to_string(), original);
}

#[test]
fn lower_profile_pipeline_does_not_add_result_copies() {
    let mut program = program(vec![
        Instruction::Store(result(0), variable(0)),
        Instruction::Call(
            READ,
            vec![Operand::Variable(variable(0))],
            Some(Variable::new_boolean(VariableId(1))),
            None,
        ),
    ]);
    program.config.capabilities = Profile::AdaptiveRIF.into();
    program.callables.remove(WRITE);
    check_and_transform(&mut program);
    check_types(&program);
    assert_eq!(program.num_results, 2);
    assert!(!body(&program).contains("Call id(2)"));
}

#[test]
fn fresh_ids_follow_sparse_callable_inputs_and_result_references() {
    let mut program = program(vec![Instruction::Store(
        Operand::Variable(variable(50)),
        variable(3),
    )]);
    let callable = program
        .callables
        .get_mut(CallableId(0))
        .expect("entry exists");
    callable.input_type = vec![Ty::Prim(Prim::Result)];
    callable.input_vars = vec![VariableId(50)];
    program
        .blocks
        .get_mut(BlockId(0))
        .expect("entry exists")
        .0
        .insert(
            0,
            Instruction::Call(
                READ,
                vec![result(60)],
                Some(Variable::new_boolean(VariableId(7))),
                None,
            ),
        );
    transform(&mut program);
    assert_eq!(program.num_results, 62);
    expect![[r#"
        Block:
            Variable(7, Boolean) = Call id(1), args( Result(60), )
            Variable(51, Boolean) = Call id(1), args( Variable(50, Result), )
            Call id(2), args( Variable(51, Boolean), Result(61), )
            Variable(3, Result) = Store Result(61)
            Return"#]]
    .assert_eq(&body(&program));
}

#[test]
fn custom_measurement_outputs_are_not_reused_for_copy_destinations() {
    let mut program = program(vec![
        Instruction::Call(
            CallableId(9),
            vec![Operand::Literal(Literal::Qubit(0)), result(0), result(1)],
            None,
            None,
        ),
        Instruction::StoreArray(
            vec![result(0), result(1)],
            Variable::new_array(VariableId(0), 2, Prim::Result),
        ),
    ]);
    program.callables.insert(
        CallableId(9),
        Callable {
            name: "custom_measure".into(),
            input_type: vec![
                Ty::Prim(Prim::Qubit),
                Ty::Prim(Prim::Result),
                Ty::Prim(Prim::Result),
            ],
            input_vars: vec![],
            output_type: None,
            body: None,
            call_type: CallableType::Measurement,
        },
    );
    transform(&mut program);
    assert_eq!(program.num_results, 4);
    expect![[r#"
        Block:
            Call id(9), args( Qubit(0), Result(0), Result(1), )
            Variable(1, Boolean) = Call id(1), args( Result(0), )
            Variable(2, Boolean) = Call id(1), args( Result(1), )
            Call id(2), args( Variable(1, Boolean), Result(2), )
            Call id(2), args( Variable(2, Boolean), Result(3), )
            Variable(0, Array(2, Result)) = StoreArray [Result(2), Result(3)]
            Return"#]]
    .assert_eq(&body(&program));
}

#[test]
fn stores_in_loop_blocks_reuse_the_same_destination_register() {
    let mut program = program(vec![Instruction::Store(result(0), variable(0))]);
    program.get_block_mut(BlockId(0)).0.pop();
    program
        .get_block_mut(BlockId(0))
        .0
        .push(Instruction::Jump(BlockId(1)));
    program.blocks.insert(
        BlockId(1),
        Block(vec![
            Instruction::Store(result(1), variable(0)),
            Instruction::Jump(BlockId(1)),
        ]),
    );
    transform(&mut program);
    assert_eq!(program.num_results, 3);
    expect![[r#"
        Block:
            Variable(2, Boolean) = Call id(1), args( Result(1), )
            Call id(2), args( Variable(2, Boolean), Result(2), )
            Variable(0, Result) = Store Result(2)
            Jump(1)"#]]
    .assert_eq(&program.get_block(BlockId(1)).to_string());
}

#[test]
fn directly_recursive_body_is_left_unchanged() {
    let mut program = program(vec![Instruction::Store(result(0), variable(0)), call(0)]);
    let original = program.to_string();
    transform(&mut program);
    assert_eq!(program.to_string(), original);
}

#[test]
fn mutually_recursive_bodies_are_left_unchanged() {
    let mut program = program(vec![Instruction::Store(result(0), variable(0)), call(3)]);
    add_body(
        &mut program,
        3,
        vec![
            Instruction::Store(result(1), variable(1)),
            call(0),
            Instruction::Return(None),
        ],
    );
    let original = program.to_string();
    transform(&mut program);
    assert_eq!(program.to_string(), original);
}

#[test]
fn nonrecursive_caller_is_copied_while_recursive_callee_is_unchanged() {
    let mut program = program(vec![Instruction::Store(result(0), variable(0)), call(3)]);
    add_body(
        &mut program,
        3,
        vec![
            Instruction::Store(result(1), variable(1)),
            call(3),
            Instruction::Return(None),
        ],
    );
    let recursive = program.get_block(BlockId(3)).to_string();
    transform(&mut program);
    assert_eq!(program.get_block(BlockId(3)).to_string(), recursive);
    assert_eq!(program.num_results, 3);
    expect![[r#"
        Block:
            Variable(2, Boolean) = Call id(1), args( Result(0), )
            Call id(2), args( Variable(2, Boolean), Result(2), )
            Variable(0, Result) = Store Result(2)
            Call id(3), args( )
            Return"#]]
    .assert_eq(&body(&program));
}

#[test]
fn shared_block_ownership_does_not_acquire_private_result_storage() {
    let mut program = program(vec![Instruction::Store(result(0), variable(0))]);
    let shared = program.get_callable(CallableId(0)).clone();
    program.callables.insert(CallableId(3), shared);
    let original = program.to_string();
    transform(&mut program);
    assert_eq!(program.to_string(), original);
}
