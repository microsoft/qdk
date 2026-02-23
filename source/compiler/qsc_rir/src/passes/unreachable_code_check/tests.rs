// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::rir::{
    Block, BlockId, Callable, CallableId, CallableType, InstructionKind, Literal, Operand, Program,
    Ty, Variable, VariableId,
};

use super::{check_unreachable_blocks, check_unreachable_callable, check_unreachable_instrs};

#[test]
#[should_panic(expected = "BlockId(0) does not end with a terminator instruction")]
fn test_check_unreachable_instrs_panics_on_missing_terminator() {
    let mut program = Program::new();
    program.blocks.insert(
        BlockId(0),
        Block::from_instruction_kinds(vec![InstructionKind::BitwiseNot(
            Operand::Literal(Literal::Bool(true)),
            Variable {
                variable_id: VariableId(0),
                ty: Ty::Boolean,
            },
        )]),
    );
    check_unreachable_instrs(&program);
}

#[test]
fn test_check_unreachable_instrs_succeeds_on_terminator() {
    let mut program = Program::new();
    program.blocks.insert(
        BlockId(0),
        Block::from_instruction_kinds(vec![InstructionKind::Return]),
    );
    check_unreachable_instrs(&program);
}

#[test]
fn test_check_unreachable_instrs_succeeds_on_terminator_after_other_instrs() {
    let mut program = Program::new();
    program.blocks.insert(
        BlockId(0),
        Block::from_instruction_kinds(vec![
            InstructionKind::BitwiseNot(
                Operand::Literal(Literal::Bool(true)),
                Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                },
            ),
            InstructionKind::Return,
        ]),
    );
    check_unreachable_instrs(&program);
}

#[test]
#[should_panic(expected = "BlockId(0) has unreachable instructions after a terminator")]
fn test_check_unreachable_instrs_panics_on_unreachable_instrs_after_terminator() {
    let mut program = Program::new();
    program.blocks.insert(
        BlockId(0),
        Block::from_instruction_kinds(vec![
            InstructionKind::Return,
            InstructionKind::BitwiseNot(
                Operand::Literal(Literal::Bool(true)),
                Variable {
                    variable_id: VariableId(0),
                    ty: Ty::Boolean,
                },
            ),
        ]),
    );
    check_unreachable_instrs(&program);
}

#[test]
fn test_check_unreachable_blocks_succeeds_on_no_unreachable_blocks() {
    let mut program = Program::new();
    program.callables.insert(
        CallableId(0),
        Callable {
            name: "test".to_string(),
            input_type: vec![],
            output_type: None,
            body: Some(BlockId(0)),
            call_type: CallableType::Regular,
        },
    );
    program.blocks.insert(
        BlockId(0),
        Block::from_instruction_kinds(vec![InstructionKind::Return]),
    );
    check_unreachable_blocks(&program);
}

#[test]
#[should_panic(expected = "Unreachable blocks found: [BlockId(1)]")]
fn test_check_unreachable_blocks_panics_on_unreachable_block() {
    let mut program = Program::new();
    program.callables.insert(
        CallableId(0),
        Callable {
            name: "test".to_string(),
            input_type: vec![],
            output_type: None,
            body: Some(BlockId(0)),
            call_type: CallableType::Regular,
        },
    );
    program.blocks.insert(
        BlockId(0),
        Block::from_instruction_kinds(vec![InstructionKind::Return]),
    );
    program.blocks.insert(
        BlockId(1),
        Block::from_instruction_kinds(vec![InstructionKind::Return]),
    );
    check_unreachable_blocks(&program);
}

#[test]
fn test_check_unreachable_blocks_succeeds_on_no_unreachable_blocks_with_branch() {
    let mut program = Program::new();
    program.callables.insert(
        CallableId(0),
        Callable {
            name: "test".to_string(),
            input_type: vec![],
            output_type: None,
            body: Some(BlockId(0)),
            call_type: CallableType::Regular,
        },
    );
    program.blocks.insert(
        BlockId(0),
        Block::from_instruction_kinds(vec![InstructionKind::Branch(
            Variable {
                variable_id: VariableId(0),
                ty: Ty::Boolean,
            },
            BlockId(1),
            BlockId(2),
        )]),
    );
    program.blocks.insert(
        BlockId(1),
        Block::from_instruction_kinds(vec![InstructionKind::Return]),
    );
    program.blocks.insert(
        BlockId(2),
        Block::from_instruction_kinds(vec![InstructionKind::Return]),
    );
    check_unreachable_blocks(&program);
}

#[test]
fn test_check_unreachable_blocks_succeeds_on_no_unreachable_blocks_with_jump() {
    let mut program = Program::new();
    program.callables.insert(
        CallableId(0),
        Callable {
            name: "test".to_string(),
            input_type: vec![],
            output_type: None,
            body: Some(BlockId(0)),
            call_type: CallableType::Regular,
        },
    );
    program.blocks.insert(
        BlockId(0),
        Block::from_instruction_kinds(vec![InstructionKind::Jump(BlockId(1))]),
    );
    program.blocks.insert(
        BlockId(1),
        Block::from_instruction_kinds(vec![InstructionKind::Return]),
    );
    check_unreachable_blocks(&program);
}

#[test]
#[should_panic(expected = "Unreachable blocks found: [BlockId(2)]")]
fn test_check_unreachable_blocks_panics_on_unreachable_block_with_branch() {
    let mut program = Program::new();
    program.callables.insert(
        CallableId(0),
        Callable {
            name: "test".to_string(),
            input_type: vec![],
            output_type: None,
            body: Some(BlockId(0)),
            call_type: CallableType::Regular,
        },
    );
    program.blocks.insert(
        BlockId(0),
        Block::from_instruction_kinds(vec![InstructionKind::Branch(
            Variable {
                variable_id: VariableId(0),
                ty: Ty::Boolean,
            },
            BlockId(1),
            BlockId(1),
        )]),
    );
    program.blocks.insert(
        BlockId(1),
        Block::from_instruction_kinds(vec![InstructionKind::Return]),
    );
    program.blocks.insert(
        BlockId(2),
        Block::from_instruction_kinds(vec![InstructionKind::Return]),
    );
    check_unreachable_blocks(&program);
}

#[test]
fn test_check_unreachable_callable_succeeds_on_no_unreachable_callables() {
    let mut program = Program::new();
    program.entry = CallableId(0);
    program.callables.insert(
        CallableId(0),
        Callable {
            name: "test".to_string(),
            input_type: vec![],
            output_type: None,
            body: Some(BlockId(0)),
            call_type: CallableType::Regular,
        },
    );
    program.blocks.insert(
        BlockId(0),
        Block::from_instruction_kinds(vec![InstructionKind::Return]),
    );
    check_unreachable_callable(&program);
}

#[test]
#[should_panic(expected = "Unreachable callables found: [CallableId(1)]")]
fn test_check_unreachable_callable_panics_on_unreachable_callable() {
    let mut program = Program::new();
    program.entry = CallableId(0);
    program.callables.insert(
        CallableId(0),
        Callable {
            name: "test".to_string(),
            input_type: vec![],
            output_type: None,
            body: Some(BlockId(0)),
            call_type: CallableType::Regular,
        },
    );
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "test".to_string(),
            input_type: vec![],
            output_type: None,
            body: Some(BlockId(1)),
            call_type: CallableType::Regular,
        },
    );
    program.blocks.insert(
        BlockId(0),
        Block::from_instruction_kinds(vec![InstructionKind::Return]),
    );
    program.blocks.insert(
        BlockId(1),
        Block::from_instruction_kinds(vec![InstructionKind::Return]),
    );
    check_unreachable_callable(&program);
}

#[test]
fn test_check_unreachable_callable_succeeds_on_no_unreachable_callables_with_call() {
    let mut program = Program::new();
    program.entry = CallableId(0);
    program.callables.insert(
        CallableId(0),
        Callable {
            name: "test".to_string(),
            input_type: vec![],
            output_type: None,
            body: Some(BlockId(0)),
            call_type: CallableType::Regular,
        },
    );
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "test".to_string(),
            input_type: vec![],
            output_type: None,
            body: Some(BlockId(1)),
            call_type: CallableType::Regular,
        },
    );
    program.blocks.insert(
        BlockId(0),
        Block::from_instruction_kinds(vec![InstructionKind::Call(CallableId(1), Vec::new(), None)]),
    );
    program.blocks.insert(
        BlockId(1),
        Block::from_instruction_kinds(vec![InstructionKind::Return]),
    );
    check_unreachable_callable(&program);
}

#[test]
fn test_check_unreachable_callable_succeeds_on_no_unreachable_callables_with_nested_call() {
    let mut program = Program::new();
    program.entry = CallableId(0);
    program.callables.insert(
        CallableId(0),
        Callable {
            name: "test".to_string(),
            input_type: vec![],
            output_type: None,
            body: Some(BlockId(0)),
            call_type: CallableType::Regular,
        },
    );
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "test".to_string(),
            input_type: vec![],
            output_type: None,
            body: Some(BlockId(1)),
            call_type: CallableType::Regular,
        },
    );
    program.callables.insert(
        CallableId(2),
        Callable {
            name: "test".to_string(),
            input_type: vec![],
            output_type: None,
            body: None,
            call_type: CallableType::Regular,
        },
    );
    program.blocks.insert(
        BlockId(0),
        Block::from_instruction_kinds(vec![InstructionKind::Call(CallableId(1), Vec::new(), None)]),
    );
    program.blocks.insert(
        BlockId(1),
        Block::from_instruction_kinds(vec![InstructionKind::Call(CallableId(2), Vec::new(), None)]),
    );
    check_unreachable_callable(&program);
}

#[test]
#[should_panic(expected = "Unreachable callables found: [CallableId(2)]")]
fn test_check_unreachable_callable_panics_on_unreachable_callable_with_nested_call() {
    let mut program = Program::new();
    program.entry = CallableId(0);
    program.callables.insert(
        CallableId(0),
        Callable {
            name: "test".to_string(),
            input_type: vec![],
            output_type: None,
            body: Some(BlockId(0)),
            call_type: CallableType::Regular,
        },
    );
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "test".to_string(),
            input_type: vec![],
            output_type: None,
            body: Some(BlockId(1)),
            call_type: CallableType::Regular,
        },
    );
    program.callables.insert(
        CallableId(2),
        Callable {
            name: "test".to_string(),
            input_type: vec![],
            output_type: None,
            body: None,
            call_type: CallableType::Regular,
        },
    );
    program.blocks.insert(
        BlockId(0),
        Block::from_instruction_kinds(vec![InstructionKind::Call(CallableId(1), Vec::new(), None)]),
    );
    program.blocks.insert(
        BlockId(1),
        Block::from_instruction_kinds(vec![InstructionKind::Return]),
    );
    check_unreachable_callable(&program);
}

#[test]
fn test_check_unreachable_callable_succeeds_on_no_unreachable_callables_with_call_in_successor_block()
 {
    let mut program = Program::new();
    program.entry = CallableId(0);
    program.callables.insert(
        CallableId(0),
        Callable {
            name: "test".to_string(),
            input_type: vec![],
            output_type: None,
            body: Some(BlockId(0)),
            call_type: CallableType::Regular,
        },
    );
    program.callables.insert(
        CallableId(1),
        Callable {
            name: "test".to_string(),
            input_type: vec![],
            output_type: None,
            body: None,
            call_type: CallableType::Regular,
        },
    );
    program.blocks.insert(
        BlockId(0),
        Block::from_instruction_kinds(vec![InstructionKind::Jump(BlockId(1))]),
    );
    program.blocks.insert(
        BlockId(1),
        Block::from_instruction_kinds(vec![InstructionKind::Call(CallableId(1), Vec::new(), None)]),
    );
    check_unreachable_callable(&program);
}
