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
            name: __quantum__qis__mresetz__body
            call_type: Measurement
            input_type:
                [0]: Qubit
                [1]: Result
            output_type: <VOID>
            body: <NONE>"#]],
    );
    let array_output_recording_callable_id = CallableId(2);
    assert_callable(
        &program,
        array_output_recording_callable_id,
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
    let result_output_recording_callable_id = CallableId(3);
    assert_callable(
        &program,
        result_output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__result_record_output
                call_type: OutputRecording
                input_type:
                    [0]: Result
                    [1]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Qubit(0), Result(0), )
                Call id(1), args( Qubit(1), Result(1), )
                Call id(2), args( Integer(2), EmptyTag, )
                Call id(3), args( Result(0), Tag(0, 5), )
                Call id(3), args( Result(1), Tag(1, 5), )
                Return"#]],
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
            name: __quantum__qis__mresetz__body
            call_type: Measurement
            input_type:
                [0]: Qubit
                [1]: Result
            output_type: <VOID>
            body: <NONE>"#]],
    );
    let array_output_recording_callable_id = CallableId(2);
    assert_callable(
        &program,
        array_output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__read_result
                call_type: Readout
                input_type:
                    [0]: Result
                output_type: Boolean
                body: <NONE>"#]],
    );
    let boolean_output_recording_callable_id = CallableId(3);
    assert_callable(
        &program,
        boolean_output_recording_callable_id,
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
                Call id(1), args( Qubit(0), Result(0), )
                Variable(0, Boolean) = Call id(2), args( Result(0), )
                Variable(1, Boolean) = Store Variable(0, Boolean)
                Call id(3), args( Integer(2), EmptyTag, )
                Call id(4), args( Bool(true), Tag(0, 5), )
                Call id(4), args( Variable(1, Boolean), Tag(1, 5), )
                Return"#]],
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
            name: __quantum__qis__mresetz__body
            call_type: Measurement
            input_type:
                [0]: Qubit
                [1]: Result
            output_type: <VOID>
            body: <NONE>"#]],
    );
    let array_output_recording_callable_id = CallableId(2);
    assert_callable(
        &program,
        array_output_recording_callable_id,
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
    let result_output_recording_callable_id = CallableId(3);
    assert_callable(
        &program,
        result_output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__result_record_output
                call_type: OutputRecording
                input_type:
                    [0]: Result
                    [1]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Qubit(0), Result(0), )
                Call id(2), args( Integer(2), EmptyTag, )
                Call id(3), args( Result(0), Tag(0, 5), )
                Call id(3), args( Result(0), Tag(1, 5), )
                Return"#]],
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
                name: __quantum__qis__mresetz__body
                call_type: Measurement
                input_type:
                    [0]: Qubit
                    [1]: Result
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let result_output_recording_callable_id = CallableId(2);
    assert_callable(
        &program,
        result_output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__result_record_output
                call_type: OutputRecording
                input_type:
                    [0]: Result
                    [1]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Qubit(0), Result(0), )
                Call id(1), args( Qubit(1), Result(1), )
                Call id(2), args( Result(1), Tag(0, 3), )
                Return"#]],
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
                Call id(1), args( Qubit(0), Result(0), )
                Call id(1), args( Qubit(1), Result(1), )
                Call id(2), args( Result(1), Pointer, )
                Return"#]],
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
                name: __quantum__qis__mresetz__body
                call_type: Measurement
                input_type:
                    [0]: Qubit
                    [1]: Result
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let array_output_recording_callable_id = CallableId(2);
    assert_callable(
        &program,
        array_output_recording_callable_id,
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
    let result_output_recording_callable_id = CallableId(3);
    assert_callable(
        &program,
        result_output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__result_record_output
                call_type: OutputRecording
                input_type:
                    [0]: Result
                    [1]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Qubit(0), Result(0), )
                Call id(1), args( Qubit(1), Result(1), )
                Call id(1), args( Qubit(2), Result(2), )
                Call id(1), args( Qubit(3), Result(3), )
                Call id(1), args( Qubit(4), Result(4), )
                Call id(2), args( Integer(3), EmptyTag, )
                Call id(3), args( Result(0), Tag(0, 5), )
                Call id(3), args( Result(2), Tag(1, 5), )
                Call id(3), args( Result(4), Tag(2, 5), )
                Return"#]],
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
                name: __quantum__qis__mresetz__body
                call_type: Measurement
                input_type:
                    [0]: Qubit
                    [1]: Result
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let array_output_recording_callable_id = CallableId(2);
    assert_callable(
        &program,
        array_output_recording_callable_id,
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
    let result_output_recording_callable_id = CallableId(3);
    assert_callable(
        &program,
        result_output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__result_record_output
                call_type: OutputRecording
                input_type:
                    [0]: Result
                    [1]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Qubit(0), Result(0), )
                Call id(1), args( Qubit(1), Result(1), )
                Call id(1), args( Qubit(2), Result(2), )
                Call id(2), args( Integer(2), EmptyTag, )
                Call id(3), args( Result(0), Tag(0, 5), )
                Call id(3), args( Result(1), Tag(1, 5), )
                Return"#]],
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
                name: __quantum__qis__mresetz__body
                call_type: Measurement
                input_type:
                    [0]: Qubit
                    [1]: Result
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let array_output_recording_callable_id = CallableId(2);
    assert_callable(
        &program,
        array_output_recording_callable_id,
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
    let result_output_recording_callable_id = CallableId(3);
    assert_callable(
        &program,
        result_output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__result_record_output
                call_type: OutputRecording
                input_type:
                    [0]: Result
                    [1]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Qubit(0), Result(0), )
                Call id(1), args( Qubit(1), Result(1), )
                Call id(1), args( Qubit(2), Result(2), )
                Call id(2), args( Integer(2), EmptyTag, )
                Call id(3), args( Result(1), Tag(0, 5), )
                Call id(3), args( Result(2), Tag(1, 5), )
                Return"#]],
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
                name: __quantum__qis__mresetz__body
                call_type: Measurement
                input_type:
                    [0]: Qubit
                    [1]: Result
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let array_output_recording_callable_id = CallableId(2);
    assert_callable(
        &program,
        array_output_recording_callable_id,
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
    let result_output_recording_callable_id = CallableId(3);
    assert_callable(
        &program,
        result_output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__result_record_output
                call_type: OutputRecording
                input_type:
                    [0]: Result
                    [1]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Qubit(0), Result(0), )
                Call id(1), args( Qubit(1), Result(1), )
                Call id(1), args( Qubit(2), Result(2), )
                Call id(1), args( Qubit(3), Result(3), )
                Call id(1), args( Qubit(4), Result(4), )
                Call id(2), args( Integer(3), EmptyTag, )
                Call id(3), args( Result(0), Tag(0, 5), )
                Call id(3), args( Result(2), Tag(1, 5), )
                Call id(3), args( Result(4), Tag(2, 5), )
                Return"#]],
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
                name: __quantum__qis__mresetz__body
                call_type: Measurement
                input_type:
                    [0]: Qubit
                    [1]: Result
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let array_output_recording_callable_id = CallableId(2);
    assert_callable(
        &program,
        array_output_recording_callable_id,
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
    let result_output_recording_callable_id = CallableId(3);
    assert_callable(
        &program,
        result_output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__result_record_output
                call_type: OutputRecording
                input_type:
                    [0]: Result
                    [1]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Qubit(0), Result(0), )
                Call id(1), args( Qubit(1), Result(1), )
                Call id(1), args( Qubit(2), Result(2), )
                Call id(1), args( Qubit(3), Result(3), )
                Call id(2), args( Integer(3), EmptyTag, )
                Call id(3), args( Result(0), Tag(0, 5), )
                Call id(3), args( Result(3), Tag(1, 5), )
                Call id(3), args( Result(2), Tag(2, 5), )
                Return"#]],
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
                name: __quantum__qis__mresetz__body
                call_type: Measurement
                input_type:
                    [0]: Qubit
                    [1]: Result
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let array_output_recording_callable_id = CallableId(2);
    assert_callable(
        &program,
        array_output_recording_callable_id,
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
    let result_output_recording_callable_id = CallableId(3);
    assert_callable(
        &program,
        result_output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__result_record_output
                call_type: OutputRecording
                input_type:
                    [0]: Result
                    [1]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Qubit(0), Result(0), )
                Call id(1), args( Qubit(1), Result(1), )
                Call id(1), args( Qubit(2), Result(2), )
                Call id(1), args( Qubit(3), Result(3), )
                Call id(1), args( Qubit(4), Result(4), )
                Call id(1), args( Qubit(5), Result(5), )
                Call id(1), args( Qubit(6), Result(6), )
                Call id(1), args( Qubit(7), Result(7), )
                Call id(2), args( Integer(5), EmptyTag, )
                Call id(3), args( Result(5), Tag(0, 5), )
                Call id(3), args( Result(1), Tag(1, 5), )
                Call id(3), args( Result(6), Tag(2, 5), )
                Call id(3), args( Result(3), Tag(3, 5), )
                Call id(3), args( Result(7), Tag(4, 5), )
                Return"#]],
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
                name: __quantum__qis__mresetz__body
                call_type: Measurement
                input_type:
                    [0]: Qubit
                    [1]: Result
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let array_output_recording_callable_id = CallableId(2);
    assert_callable(
        &program,
        array_output_recording_callable_id,
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
    let result_output_recording_callable_id = CallableId(3);
    assert_callable(
        &program,
        result_output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__result_record_output
                call_type: OutputRecording
                input_type:
                    [0]: Result
                    [1]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Qubit(0), Result(0), )
                Call id(1), args( Qubit(1), Result(1), )
                Call id(1), args( Qubit(2), Result(2), )
                Call id(1), args( Qubit(3), Result(3), )
                Call id(1), args( Qubit(4), Result(4), )
                Call id(2), args( Integer(3), EmptyTag, )
                Call id(3), args( Result(3), Tag(0, 5), )
                Call id(3), args( Result(4), Tag(1, 5), )
                Call id(3), args( Result(2), Tag(2, 5), )
                Return"#]],
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
                name: __quantum__qis__mresetz__body
                call_type: Measurement
                input_type:
                    [0]: Qubit
                    [1]: Result
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let array_output_recording_callable_id = CallableId(2);
    assert_callable(
        &program,
        array_output_recording_callable_id,
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
    let result_output_recording_callable_id = CallableId(3);
    assert_callable(
        &program,
        result_output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__result_record_output
                call_type: OutputRecording
                input_type:
                    [0]: Result
                    [1]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Qubit(0), Result(0), )
                Call id(1), args( Qubit(1), Result(1), )
                Call id(1), args( Qubit(2), Result(2), )
                Call id(1), args( Qubit(3), Result(3), )
                Call id(1), args( Qubit(4), Result(4), )
                Call id(2), args( Integer(3), EmptyTag, )
                Call id(3), args( Result(0), Tag(0, 5), )
                Call id(3), args( Result(3), Tag(1, 5), )
                Call id(3), args( Result(4), Tag(2, 5), )
                Return"#]],
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
                name: __quantum__qis__mresetz__body
                call_type: Measurement
                input_type:
                    [0]: Qubit
                    [1]: Result
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let array_output_recording_callable_id = CallableId(2);
    assert_callable(
        &program,
        array_output_recording_callable_id,
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
    let result_output_recording_callable_id = CallableId(3);
    assert_callable(
        &program,
        result_output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__result_record_output
                call_type: OutputRecording
                input_type:
                    [0]: Result
                    [1]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Qubit(0), Result(0), )
                Call id(1), args( Qubit(1), Result(1), )
                Call id(1), args( Qubit(2), Result(2), )
                Call id(1), args( Qubit(3), Result(3), )
                Call id(1), args( Qubit(4), Result(4), )
                Call id(2), args( Integer(3), EmptyTag, )
                Call id(3), args( Result(3), Tag(0, 5), )
                Call id(3), args( Result(1), Tag(1, 5), )
                Call id(3), args( Result(4), Tag(2, 5), )
                Return"#]],
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
