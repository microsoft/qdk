// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(
    clippy::needless_raw_string_hashes,
    clippy::similar_names,
    clippy::too_many_lines
)]

use super::{
    assert_block_instructions, assert_callable, assert_error, get_partial_evaluation_error,
    get_rir_program,
};
use expect_test::expect;
use indoc::indoc;
use qsc_rir::rir::{BlockId, CallableId};

#[test]
fn runtime_index_update_selects_three_one_two_or_zero_three_two_without_bounds_checks() {
    use qsc_rir::rir::{ConditionCode, Instruction, Literal, Operand};
    let program = get_rir_program(indoc! {r#"
        @EntryPoint() operation Main() : Int[] {
            use q = Qubit();
            let index = MResetZ(q) == Zero ? 0 | 1;
            [0, 1, 2] w/ index <- 3
        }
    "#});
    let instructions: Vec<_> = program.blocks.values().flat_map(|block| &block.0).collect();
    let outputs: Vec<_> = instructions
        .iter()
        .filter_map(|instruction| {
            if let Instruction::Call(id, args, _, _) = instruction
                && program.get_callable(*id).name == "__quantum__rt__int_record_output"
            {
                Some(args[0])
            } else {
                None
            }
        })
        .collect();
    assert_eq!(outputs.len(), 3);
    let mut runtime_index = None;
    for (position, output) in outputs.iter().enumerate() {
        let Operand::Variable(result) = output else {
            panic!("updated element must be dynamic")
        };
        assert!(instructions.iter().any(|instruction| matches!(instruction,
            Instruction::Store(Operand::Literal(Literal::Integer(value)), variable)
                if *value == i64::try_from(position).expect("small array position") && variable == result)));
        let condition = instructions
            .iter()
            .find_map(|instruction| {
                if let Instruction::Icmp(
                    ConditionCode::Eq,
                    index,
                    Operand::Literal(Literal::Integer(value)),
                    condition,
                ) = instruction
                    && *value == i64::try_from(position).expect("small array position")
                {
                    if let Some(previous) = runtime_index {
                        assert_eq!(previous, *index);
                    }
                    runtime_index = Some(*index);
                    Some(*condition)
                } else {
                    None
                }
            })
            .expect("each element is selected by the original runtime index");
        let (selected, continuation) = instructions
            .iter()
            .find_map(|instruction| {
                if let Instruction::Branch(variable, yes, no, _) = instruction
                    && *variable == condition
                {
                    Some((*yes, *no))
                } else {
                    None
                }
            })
            .expect("element update branch");
        assert!(matches!(program.get_block(selected).0.as_slice(),
            [Instruction::Store(Operand::Literal(Literal::Integer(3)), actual),
             Instruction::Jump(next)] if actual == result && *next == continuation));
    }
    assert!(matches!(runtime_index, Some(Operand::Variable(_))));
    assert_eq!(
        instructions
            .iter()
            .filter(|instruction| matches!(
                instruction,
                Instruction::Icmp(
                    ConditionCode::Eq,
                    _,
                    Operand::Literal(Literal::Integer(_)),
                    _
                )
            ))
            .count(),
        3
    );
    assert!(
        program
            .callables
            .values()
            .all(|callable| !callable.name.contains("fail"))
    );
}

#[test]
fn array_with_dynamic_content() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result[] {
                use (q0, q1) = (Qubit(), Qubit());
                [MResetZ(q0), MResetZ(q1)]
            }
        }
    "#});
    let mresetz_callable_id = CallableId(1);
    assert_callable(
        &program,
        mresetz_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let array_output_recording_callable_id = CallableId(2);
    assert_callable(
        &program,
        array_output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__qis__mresetz__body
                call_type: Measurement
                input_type:
                    [0]: Qubit
                    [1]: Result
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let result_output_recording_callable_id = CallableId(3);
    assert_callable(
        &program,
        result_output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__array_record_output
                call_type: OutputRecording
                input_type:
                    [0]: Integer
                    [1]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), Result(0), )
                Call id(2), args( Qubit(1), Result(1), )
                Call id(3), args( Integer(2), Tag(0, 3), )
                Call id(4), args( Result(0), Tag(1, 5), )
                Call id(4), args( Result(1), Tag(2, 5), )
                Return Integer(0)"#]],
    );
}

#[test]
fn array_with_hybrid_content() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Bool[] {
                use q = Qubit();
                let r = MResetZ(q);
                [true, r == One]
            }
        }
    "#});
    let mresetz_callable_id = CallableId(1);
    assert_callable(
        &program,
        mresetz_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let array_output_recording_callable_id = CallableId(2);
    assert_callable(
        &program,
        array_output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__qis__mresetz__body
                call_type: Measurement
                input_type:
                    [0]: Qubit
                    [1]: Result
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let boolean_output_recording_callable_id = CallableId(3);
    assert_callable(
        &program,
        boolean_output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__read_result
                call_type: Readout
                input_type:
                    [0]: Result
                output_type: Boolean
                body: <NONE>"#]],
    );
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), Result(0), )
                Variable(0, Boolean) = Call id(3), args( Result(0), )
                Variable(1, Boolean) = Store Variable(0, Boolean)
                Call id(4), args( Integer(2), Tag(0, 3), )
                Call id(5), args( Bool(true), Tag(1, 5), )
                Call id(5), args( Variable(1, Boolean), Tag(2, 5), )
                Return Integer(0)"#]],
    );
}

#[test]
fn array_repeat_with_dynamic_content() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result[] {
                use q = Qubit();
                [MResetZ(q), size = 2]
            }
        }
    "#});
    let mresetz_callable_id = CallableId(1);
    assert_callable(
        &program,
        mresetz_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let array_output_recording_callable_id = CallableId(2);
    assert_callable(
        &program,
        array_output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__qis__mresetz__body
                call_type: Measurement
                input_type:
                    [0]: Qubit
                    [1]: Result
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let result_output_recording_callable_id = CallableId(3);
    assert_callable(
        &program,
        result_output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__array_record_output
                call_type: OutputRecording
                input_type:
                    [0]: Integer
                    [1]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), Result(0), )
                Call id(3), args( Integer(2), Tag(0, 3), )
                Call id(4), args( Result(0), Tag(1, 5), )
                Call id(4), args( Result(0), Tag(2, 5), )
                Return Integer(0)"#]],
    );
}

#[test]
fn result_array_value_at_index() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result {
                use (q0, q1) = (Qubit(), Qubit());
                let results = [MResetZ(q0), MResetZ(q1)];
                results[1]
            }
        }
    "#});
    let measurement_callable_id = CallableId(1);
    assert_callable(
        &program,
        measurement_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let result_output_recording_callable_id = CallableId(2);
    assert_callable(
        &program,
        result_output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__qis__mresetz__body
                call_type: Measurement
                input_type:
                    [0]: Qubit
                    [1]: Result
                output_type: <VOID>
                body: <NONE>"#]],
    );
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), Result(0), )
                Call id(2), args( Qubit(1), Result(1), )
                Call id(3), args( Result(1), Tag(0, 3), )
                Return Integer(0)"#]],
    );
}

#[test]
fn result_array_value_at_negative_index_works() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result {
                use (q0, q1) = (Qubit(), Qubit());
                let results = [MResetZ(q0), MResetZ(q1)];
                results[-1]
            }
        }
    "#});
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), Result(0), )
                Call id(2), args( Qubit(1), Result(1), )
                Call id(3), args( Result(1), Tag(0, 3), )
                Return Integer(0)"#]],
    );
}

#[test]
fn result_array_value_at_index_out_of_bounds_raises_error() {
    let error = get_partial_evaluation_error(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result {
                use (q0, q1) = (Qubit(), Qubit());
                let results = [MResetZ(q0), MResetZ(q1)];
                results[2]
            }
        }
    "#});
    assert_error(
        &error,
        &expect![[
            r#"EvaluationFailed("index out of range: 2", PackageSpan { package: PackageId(2), span: Span { lo: 177, hi: 178 } })"#
        ]],
    );
}

#[test]
fn result_array_slice_with_explicit_range() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result[] {
                use (q0, q1, q2, q3, q4) = (Qubit(), Qubit(), Qubit(), Qubit(), Qubit());
                let a = [MResetZ(q0), MResetZ(q1), MResetZ(q2), MResetZ(q3), MResetZ(q4)];
                a[0..2..4]
            }
        }
    "#});
    let measurement_callable_id = CallableId(1);
    assert_callable(
        &program,
        measurement_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let array_output_recording_callable_id = CallableId(2);
    assert_callable(
        &program,
        array_output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__qis__mresetz__body
                call_type: Measurement
                input_type:
                    [0]: Qubit
                    [1]: Result
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let result_output_recording_callable_id = CallableId(3);
    assert_callable(
        &program,
        result_output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__array_record_output
                call_type: OutputRecording
                input_type:
                    [0]: Integer
                    [1]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), Result(0), )
                Call id(2), args( Qubit(1), Result(1), )
                Call id(2), args( Qubit(2), Result(2), )
                Call id(2), args( Qubit(3), Result(3), )
                Call id(2), args( Qubit(4), Result(4), )
                Call id(3), args( Integer(3), Tag(0, 3), )
                Call id(4), args( Result(0), Tag(1, 5), )
                Call id(4), args( Result(2), Tag(2, 5), )
                Call id(4), args( Result(4), Tag(3, 5), )
                Return Integer(0)"#]],
    );
}

#[test]
fn result_array_slice_with_open_start_range() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result[] {
                use (q0, q1, q2) = (Qubit(), Qubit(), Qubit());
                let a = [MResetZ(q0), MResetZ(q1), MResetZ(q2)];
                a[...1]
            }
        }
    "#});
    let measurement_callable_id = CallableId(1);
    assert_callable(
        &program,
        measurement_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let array_output_recording_callable_id = CallableId(2);
    assert_callable(
        &program,
        array_output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__qis__mresetz__body
                call_type: Measurement
                input_type:
                    [0]: Qubit
                    [1]: Result
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let result_output_recording_callable_id = CallableId(3);
    assert_callable(
        &program,
        result_output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__array_record_output
                call_type: OutputRecording
                input_type:
                    [0]: Integer
                    [1]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), Result(0), )
                Call id(2), args( Qubit(1), Result(1), )
                Call id(2), args( Qubit(2), Result(2), )
                Call id(3), args( Integer(2), Tag(0, 3), )
                Call id(4), args( Result(0), Tag(1, 5), )
                Call id(4), args( Result(1), Tag(2, 5), )
                Return Integer(0)"#]],
    );
}

#[test]
fn result_array_slice_with_open_ended_range() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result[] {
                use (q0, q1, q2) = (Qubit(), Qubit(), Qubit());
                let a = [MResetZ(q0), MResetZ(q1), MResetZ(q2)];
                a[1...]
            }
        }
    "#});
    let measurement_callable_id = CallableId(1);
    assert_callable(
        &program,
        measurement_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let array_output_recording_callable_id = CallableId(2);
    assert_callable(
        &program,
        array_output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__qis__mresetz__body
                call_type: Measurement
                input_type:
                    [0]: Qubit
                    [1]: Result
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let result_output_recording_callable_id = CallableId(3);
    assert_callable(
        &program,
        result_output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__array_record_output
                call_type: OutputRecording
                input_type:
                    [0]: Integer
                    [1]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), Result(0), )
                Call id(2), args( Qubit(1), Result(1), )
                Call id(2), args( Qubit(2), Result(2), )
                Call id(3), args( Integer(2), Tag(0, 3), )
                Call id(4), args( Result(1), Tag(1, 5), )
                Call id(4), args( Result(2), Tag(2, 5), )
                Return Integer(0)"#]],
    );
}

#[test]
fn result_array_slice_with_open_two_step_range() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result[] {
                use (q0, q1, q2, q3, q4) = (Qubit(), Qubit(), Qubit(), Qubit(), Qubit());
                let a = [MResetZ(q0), MResetZ(q1), MResetZ(q2), MResetZ(q3), MResetZ(q4)];
                a[...2...]
            }
        }
    "#});
    let measurement_callable_id = CallableId(1);
    assert_callable(
        &program,
        measurement_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let array_output_recording_callable_id = CallableId(2);
    assert_callable(
        &program,
        array_output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__qis__mresetz__body
                call_type: Measurement
                input_type:
                    [0]: Qubit
                    [1]: Result
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let result_output_recording_callable_id = CallableId(3);
    assert_callable(
        &program,
        result_output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__array_record_output
                call_type: OutputRecording
                input_type:
                    [0]: Integer
                    [1]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), Result(0), )
                Call id(2), args( Qubit(1), Result(1), )
                Call id(2), args( Qubit(2), Result(2), )
                Call id(2), args( Qubit(3), Result(3), )
                Call id(2), args( Qubit(4), Result(4), )
                Call id(3), args( Integer(3), Tag(0, 3), )
                Call id(4), args( Result(0), Tag(1, 5), )
                Call id(4), args( Result(2), Tag(2, 5), )
                Call id(4), args( Result(4), Tag(3, 5), )
                Return Integer(0)"#]],
    );
}

#[test]
fn result_array_slice_with_out_of_bounds_range_raises_error() {
    let error = get_partial_evaluation_error(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result[] {
                use (q0, q1, q2, q3) = (Qubit(), Qubit(), Qubit(), Qubit());
                let a = [MResetZ(q0), MResetZ(q1), MResetZ(q2)];
                a[1..3]
            }
        }
    "#});
    assert_error(
        &error,
        &expect![[
            r#"EvaluationFailed("index out of range: 3", PackageSpan { package: PackageId(2), span: Span { lo: 206, hi: 210 } })"#
        ]],
    );
}

#[test]
fn result_array_copy_and_update_with_single_index() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result[] {
                use (q0, q1, q2, q3) = (Qubit(), Qubit(), Qubit(), Qubit());
                let a = [MResetZ(q0), MResetZ(q1), MResetZ(q2)];
                a w/ 1 <- MResetZ(q3)
            }
        }
    "#});
    let measurement_callable_id = CallableId(1);
    assert_callable(
        &program,
        measurement_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let array_output_recording_callable_id = CallableId(2);
    assert_callable(
        &program,
        array_output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__qis__mresetz__body
                call_type: Measurement
                input_type:
                    [0]: Qubit
                    [1]: Result
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let result_output_recording_callable_id = CallableId(3);
    assert_callable(
        &program,
        result_output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__array_record_output
                call_type: OutputRecording
                input_type:
                    [0]: Integer
                    [1]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), Result(0), )
                Call id(2), args( Qubit(1), Result(1), )
                Call id(2), args( Qubit(2), Result(2), )
                Call id(2), args( Qubit(3), Result(3), )
                Call id(3), args( Integer(3), Tag(0, 3), )
                Call id(4), args( Result(0), Tag(1, 5), )
                Call id(4), args( Result(3), Tag(2, 5), )
                Call id(4), args( Result(2), Tag(3, 5), )
                Return Integer(0)"#]],
    );
}

#[test]
fn result_array_copy_and_update_with_single_negative_index_raises_error() {
    let error = get_partial_evaluation_error(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result[] {
                use (q0, q1, q2, q3) = (Qubit(), Qubit(), Qubit(), Qubit());
                let a = [MResetZ(q0), MResetZ(q1), MResetZ(q2)];
                a w/ -1 <- MResetZ(q3)
            }
        }
    "#});
    assert_error(
        &error,
        &expect![[
            r#"EvaluationFailed("negative integers cannot be used here: -1", PackageSpan { package: PackageId(2), span: Span { lo: 209, hi: 211 } })"#
        ]],
    );
}

#[test]
fn result_array_copy_and_update_with_single_out_of_bounds_index_raises_error() {
    let error = get_partial_evaluation_error(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result[] {
                use (q0, q1, q2, q3) = (Qubit(), Qubit(), Qubit(), Qubit());
                let a = [MResetZ(q0), MResetZ(q1), MResetZ(q2)];
                a w/ 3 <- MResetZ(q3)
            }
        }
    "#});
    assert_error(
        &error,
        &expect![[
            r#"EvaluationFailed("index out of range: 3", PackageSpan { package: PackageId(2), span: Span { lo: 209, hi: 210 } })"#
        ]],
    );
}

#[test]
fn result_array_copy_and_update_with_explicit_range() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result[] {
                use (q0, q1, q2, q3, q4) = (Qubit(), Qubit(), Qubit(), Qubit(), Qubit());
                use (aux0, aux1, aux2) = (Qubit(), Qubit(), Qubit());
                let a = [MResetZ(q0), MResetZ(q1), MResetZ(q2), MResetZ(q3), MResetZ(q4)];
                a w/ 0..2..4 <- [MResetZ(aux0), MResetZ(aux1), MResetZ(aux2)]
            }
        }
    "#});
    let measurement_callable_id = CallableId(1);
    assert_callable(
        &program,
        measurement_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let array_output_recording_callable_id = CallableId(2);
    assert_callable(
        &program,
        array_output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__qis__mresetz__body
                call_type: Measurement
                input_type:
                    [0]: Qubit
                    [1]: Result
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let result_output_recording_callable_id = CallableId(3);
    assert_callable(
        &program,
        result_output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__array_record_output
                call_type: OutputRecording
                input_type:
                    [0]: Integer
                    [1]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), Result(0), )
                Call id(2), args( Qubit(1), Result(1), )
                Call id(2), args( Qubit(2), Result(2), )
                Call id(2), args( Qubit(3), Result(3), )
                Call id(2), args( Qubit(4), Result(4), )
                Call id(2), args( Qubit(5), Result(5), )
                Call id(2), args( Qubit(6), Result(6), )
                Call id(2), args( Qubit(7), Result(7), )
                Call id(3), args( Integer(5), Tag(0, 3), )
                Call id(4), args( Result(5), Tag(1, 5), )
                Call id(4), args( Result(1), Tag(2, 5), )
                Call id(4), args( Result(6), Tag(3, 5), )
                Call id(4), args( Result(3), Tag(4, 5), )
                Call id(4), args( Result(7), Tag(5, 5), )
                Return Integer(0)"#]],
    );
}

#[test]
fn result_array_copy_and_update_with_open_start_range() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result[] {
                use (q0, q1, q2, q3, q4) = (Qubit(), Qubit(), Qubit(), Qubit(), Qubit());
                let a = [MResetZ(q0), MResetZ(q1), MResetZ(q2)];
                a w/ ...2 <- [MResetZ(q3), MResetZ(q4)]
            }
        }
    "#});
    let measurement_callable_id = CallableId(1);
    assert_callable(
        &program,
        measurement_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let array_output_recording_callable_id = CallableId(2);
    assert_callable(
        &program,
        array_output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__qis__mresetz__body
                call_type: Measurement
                input_type:
                    [0]: Qubit
                    [1]: Result
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let result_output_recording_callable_id = CallableId(3);
    assert_callable(
        &program,
        result_output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__array_record_output
                call_type: OutputRecording
                input_type:
                    [0]: Integer
                    [1]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), Result(0), )
                Call id(2), args( Qubit(1), Result(1), )
                Call id(2), args( Qubit(2), Result(2), )
                Call id(2), args( Qubit(3), Result(3), )
                Call id(2), args( Qubit(4), Result(4), )
                Call id(3), args( Integer(3), Tag(0, 3), )
                Call id(4), args( Result(3), Tag(1, 5), )
                Call id(4), args( Result(4), Tag(2, 5), )
                Call id(4), args( Result(2), Tag(3, 5), )
                Return Integer(0)"#]],
    );
}

#[test]
fn result_array_copy_and_update_with_open_ended_range() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result[] {
                use (q0, q1, q2, q3, q4) = (Qubit(), Qubit(), Qubit(), Qubit(), Qubit());
                let a = [MResetZ(q0), MResetZ(q1), MResetZ(q2)];
                a w/ 1... <- [MResetZ(q3), MResetZ(q4)]
            }
        }
    "#});
    let measurement_callable_id = CallableId(1);
    assert_callable(
        &program,
        measurement_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let array_output_recording_callable_id = CallableId(2);
    assert_callable(
        &program,
        array_output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__qis__mresetz__body
                call_type: Measurement
                input_type:
                    [0]: Qubit
                    [1]: Result
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let result_output_recording_callable_id = CallableId(3);
    assert_callable(
        &program,
        result_output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__array_record_output
                call_type: OutputRecording
                input_type:
                    [0]: Integer
                    [1]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), Result(0), )
                Call id(2), args( Qubit(1), Result(1), )
                Call id(2), args( Qubit(2), Result(2), )
                Call id(2), args( Qubit(3), Result(3), )
                Call id(2), args( Qubit(4), Result(4), )
                Call id(3), args( Integer(3), Tag(0, 3), )
                Call id(4), args( Result(0), Tag(1, 5), )
                Call id(4), args( Result(3), Tag(2, 5), )
                Call id(4), args( Result(4), Tag(3, 5), )
                Return Integer(0)"#]],
    );
}

#[test]
fn result_array_copy_and_update_with_open_two_step_range() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result[] {
                use (q0, q1, q2, q3, q4) = (Qubit(), Qubit(), Qubit(), Qubit(), Qubit());
                let a = [MResetZ(q0), MResetZ(q1), MResetZ(q2)];
                a w/ ...2... <- [MResetZ(q3), MResetZ(q4)]
            }
        }
    "#});
    let measurement_callable_id = CallableId(1);
    assert_callable(
        &program,
        measurement_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let array_output_recording_callable_id = CallableId(2);
    assert_callable(
        &program,
        array_output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__qis__mresetz__body
                call_type: Measurement
                input_type:
                    [0]: Qubit
                    [1]: Result
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let result_output_recording_callable_id = CallableId(3);
    assert_callable(
        &program,
        result_output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__array_record_output
                call_type: OutputRecording
                input_type:
                    [0]: Integer
                    [1]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), Result(0), )
                Call id(2), args( Qubit(1), Result(1), )
                Call id(2), args( Qubit(2), Result(2), )
                Call id(2), args( Qubit(3), Result(3), )
                Call id(2), args( Qubit(4), Result(4), )
                Call id(3), args( Integer(3), Tag(0, 3), )
                Call id(4), args( Result(3), Tag(1, 5), )
                Call id(4), args( Result(1), Tag(2, 5), )
                Call id(4), args( Result(4), Tag(3, 5), )
                Return Integer(0)"#]],
    );
}

#[test]
fn result_array_copy_and_update_with_out_of_bounds_range_raises_error() {
    let error = get_partial_evaluation_error(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result[] {
                use (q0, q1, q2, q3) = (Qubit(), Qubit(), Qubit(), Qubit());
                let a = [MResetZ(q0), MResetZ(q1), MResetZ(q2)];
                a w/ 1..3 <- [MResetZ(q0), MResetZ(q1), MResetZ(q2)]
            }
        }
    "#});
    assert_error(
        &error,
        &expect![[
            r#"EvaluationFailed("index out of range: 3", PackageSpan { package: PackageId(2), span: Span { lo: 209, hi: 213 } })"#
        ]],
    );
}

#[test]
fn result_array_index_range_returns_length_as_end() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                use qs = Qubit[2];
                let results = MResetEachZ(qs);
                Std.Arrays.IndexRange(results).End
            }
        }
    "#});
    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 0
                Callable 1: Callable:
                    name: __quantum__rt__initialize
                    call_type: Regular
                    input_type:
                        [0]: Pointer
                    output_type: <VOID>
                    body: <NONE>
                Callable 2: Callable:
                    name: __quantum__qis__mresetz__body
                    call_type: Measurement
                    input_type:
                        [0]: Qubit
                        [1]: Result
                    output_type: <VOID>
                    body: <NONE>
                Callable 3: Callable:
                    name: __quantum__rt__int_record_output
                    call_type: OutputRecording
                    input_type:
                        [0]: Integer
                        [1]: Pointer
                    output_type: <VOID>
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Call id(1), args( Pointer, )
                    Variable(0, Integer) = Store Integer(0)
                    Variable(0, Integer) = Store Integer(1)
                    Variable(0, Integer) = Store Integer(2)
                    Variable(1, Integer) = Store Integer(0)
                    Call id(2), args( Qubit(0), Result(0), )
                    Variable(1, Integer) = Store Integer(1)
                    Call id(2), args( Qubit(1), Result(1), )
                    Variable(1, Integer) = Store Integer(2)
                    Call id(3), args( Integer(1), Tag(0, 3), )
                    Return Integer(0)
            config: Config:
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations)
            num_qubits: 2
            num_results: 2
            tags:
                [0]: 0_i
    "#]].assert_eq(&program.to_string());
}
