// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(
    clippy::needless_raw_string_hashes,
    clippy::similar_names,
    clippy::too_many_lines
)]

use super::{
    assert_block_instructions, assert_blocks, assert_callable, assert_error,
    get_partial_evaluation_error, get_partial_evaluation_error_with_capabilities, get_rir_program,
    get_rir_program_with_adaptive_profile,
};
use expect_test::expect;
use indoc::indoc;
use qsc_data_structures::target::Profile;
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

#[test]
fn mutable_fixed_size_array_dynamic_index_update() {
    let program = get_rir_program_with_adaptive_profile(indoc! {r#"
        @EntryPoint()
        operation Main() : Int[] {
            use qs = Qubit[4];
            mutable arr = [0, size = Length(qs)];
            for idx in 0..Length(qs)-1 {
                if M(qs[idx]) == One {
                    arr[idx] = 1;
                }
            }
            arr
        }
    "#});
    assert_blocks(
        &program,
        &expect![[r#"
        Blocks:
        Block 0:Block:
            Call id(1), args( Pointer, )
            Variable(0, Integer) = Store Integer(0)
            Variable(0, Integer) = Store Integer(1)
            Variable(0, Integer) = Store Integer(2)
            Variable(0, Integer) = Store Integer(3)
            Variable(0, Integer) = Store Integer(4)
            Variable(1, Array(4, Integer)) = StoreArray [Integer(0), Integer(0), Integer(0), Integer(0)]
            Variable(2, Integer) = Store Integer(0)
            Jump(1)
        Block 1:Block:
            Variable(3, Boolean) = Icmp Sle, Variable(2, Integer), Integer(3)
            Variable(4, Boolean) = Store Bool(true)
            Branch Variable(3, Boolean), 3, 4
        Block 2:Block:
            Variable(9, Array(4, Integer)) = CopyArray Variable(1, Array(4, Integer))
            Variable(10, Integer) = Index Variable(9, Array(4, Integer)), Integer(0)
            Variable(11, Integer) = Index Variable(9, Array(4, Integer)), Integer(1)
            Variable(12, Integer) = Index Variable(9, Array(4, Integer)), Integer(2)
            Variable(13, Integer) = Index Variable(9, Array(4, Integer)), Integer(3)
            Call id(4), args( Integer(4), Tag(0, 3), )
            Call id(5), args( Variable(10, Integer), Tag(1, 5), )
            Call id(5), args( Variable(11, Integer), Tag(2, 5), )
            Call id(5), args( Variable(12, Integer), Tag(3, 5), )
            Call id(5), args( Variable(13, Integer), Tag(4, 5), )
            Return Integer(0)
        Block 3:Block:
            Branch Variable(4, Boolean), 5, 2
        Block 4:Block:
            Variable(4, Boolean) = Store Bool(false)
            Jump(3)
        Block 5:Block:
            Variable(5, Qubit) = Index Array(0), Variable(2, Integer)
            Call id(2), args( Variable(5, Qubit), Result(0), )
            Variable(6, Boolean) = Call id(3), args( Result(0), )
            Variable(7, Boolean) = Store Variable(6, Boolean)
            Branch Variable(7, Boolean), 7, 6
        Block 6:Block:
            Variable(8, Integer) = Add Variable(2, Integer), Integer(1)
            Variable(2, Integer) = Store Variable(8, Integer)
            Jump(1)
        Block 7:Block:
            StoreIndex Integer(1), Variable(2, Integer), Variable(1, Array(4, Integer))
            Jump(6)"#]],
    );
}

#[test]
fn mutable_fixed_size_array_dynamic_range_update() {
    let error = get_partial_evaluation_error_with_capabilities(
        indoc! {r#"
        @EntryPoint()
        operation Main() : Bool[] {
            use qs = Qubit[2];
            mutable arr = [false, size = 4];
            arr[...2...] = Std.Convert.ResultArrayAsBoolArray(MResetEachZ(qs));
            arr
        }
    "#},
        Profile::Adaptive.into(),
    );
    expect![[r#"
        Unimplemented(
            "range indexing for mutation of fixed size array",
            PackageSpan {
                package: PackageId(
                    2,
                ),
                span: Span {
                    lo: 111,
                    hi: 118,
                },
            },
        )
    "#]]
    .assert_debug_eq(&error);
}

#[test]
fn mutable_fixed_size_array_length_emitted_as_constant() {
    let program = get_rir_program_with_adaptive_profile(indoc! {r#"
        @EntryPoint()
        operation Main() : (Int[], Int) {
            use qs = Qubit[4];
            mutable arr = [0, size = Length(qs)];
            for idx in 0..Length(qs)-1 {
                if M(qs[idx]) == One {
                    arr[idx] = 1;
                }
            }
            (arr, Length(arr))
        }
    "#});
    assert_blocks(
        &program,
        &expect![[r#"
        Blocks:
        Block 0:Block:
            Call id(1), args( Pointer, )
            Variable(0, Integer) = Store Integer(0)
            Variable(0, Integer) = Store Integer(1)
            Variable(0, Integer) = Store Integer(2)
            Variable(0, Integer) = Store Integer(3)
            Variable(0, Integer) = Store Integer(4)
            Variable(1, Array(4, Integer)) = StoreArray [Integer(0), Integer(0), Integer(0), Integer(0)]
            Variable(2, Integer) = Store Integer(0)
            Jump(1)
        Block 1:Block:
            Variable(3, Boolean) = Icmp Sle, Variable(2, Integer), Integer(3)
            Variable(4, Boolean) = Store Bool(true)
            Branch Variable(3, Boolean), 3, 4
        Block 2:Block:
            Variable(9, Integer) = Index Variable(1, Array(4, Integer)), Integer(0)
            Variable(10, Integer) = Index Variable(1, Array(4, Integer)), Integer(1)
            Variable(11, Integer) = Index Variable(1, Array(4, Integer)), Integer(2)
            Variable(12, Integer) = Index Variable(1, Array(4, Integer)), Integer(3)
            Call id(4), args( Integer(2), Tag(0, 3), )
            Call id(5), args( Integer(4), Tag(1, 5), )
            Call id(6), args( Variable(9, Integer), Tag(2, 7), )
            Call id(6), args( Variable(10, Integer), Tag(3, 7), )
            Call id(6), args( Variable(11, Integer), Tag(4, 7), )
            Call id(6), args( Variable(12, Integer), Tag(5, 7), )
            Call id(6), args( Integer(4), Tag(6, 5), )
            Return Integer(0)
        Block 3:Block:
            Branch Variable(4, Boolean), 5, 2
        Block 4:Block:
            Variable(4, Boolean) = Store Bool(false)
            Jump(3)
        Block 5:Block:
            Variable(5, Qubit) = Index Array(0), Variable(2, Integer)
            Call id(2), args( Variable(5, Qubit), Result(0), )
            Variable(6, Boolean) = Call id(3), args( Result(0), )
            Variable(7, Boolean) = Store Variable(6, Boolean)
            Branch Variable(7, Boolean), 7, 6
        Block 6:Block:
            Variable(8, Integer) = Add Variable(2, Integer), Integer(1)
            Variable(2, Integer) = Store Variable(8, Integer)
            Jump(1)
        Block 7:Block:
            StoreIndex Integer(1), Variable(2, Integer), Variable(1, Array(4, Integer))
            Jump(6)"#]],
    );
}

#[test]
fn mutable_fixed_size_array_length_usable_in_ranges() {
    let program = get_rir_program_with_adaptive_profile(indoc! {r#"
        @EntryPoint()
        operation Main() : (Int[], Int) {
            use qs = Qubit[4];
            mutable arr = [0, size = Length(qs)];
            for idx in 0..Length(arr)-1 {
                if M(qs[idx]) == One {
                    arr[idx] = 1;
                }
            }
            (arr, Length(arr))
        }
    "#});
    assert_blocks(
        &program,
        &expect![[r#"
        Blocks:
        Block 0:Block:
            Call id(1), args( Pointer, )
            Variable(0, Integer) = Store Integer(0)
            Variable(0, Integer) = Store Integer(1)
            Variable(0, Integer) = Store Integer(2)
            Variable(0, Integer) = Store Integer(3)
            Variable(0, Integer) = Store Integer(4)
            Variable(1, Array(4, Integer)) = StoreArray [Integer(0), Integer(0), Integer(0), Integer(0)]
            Variable(2, Integer) = Store Integer(0)
            Jump(1)
        Block 1:Block:
            Variable(3, Boolean) = Icmp Sle, Variable(2, Integer), Integer(3)
            Variable(4, Boolean) = Store Bool(true)
            Branch Variable(3, Boolean), 3, 4
        Block 2:Block:
            Variable(9, Integer) = Index Variable(1, Array(4, Integer)), Integer(0)
            Variable(10, Integer) = Index Variable(1, Array(4, Integer)), Integer(1)
            Variable(11, Integer) = Index Variable(1, Array(4, Integer)), Integer(2)
            Variable(12, Integer) = Index Variable(1, Array(4, Integer)), Integer(3)
            Call id(4), args( Integer(2), Tag(0, 3), )
            Call id(5), args( Integer(4), Tag(1, 5), )
            Call id(6), args( Variable(9, Integer), Tag(2, 7), )
            Call id(6), args( Variable(10, Integer), Tag(3, 7), )
            Call id(6), args( Variable(11, Integer), Tag(4, 7), )
            Call id(6), args( Variable(12, Integer), Tag(5, 7), )
            Call id(6), args( Integer(4), Tag(6, 5), )
            Return Integer(0)
        Block 3:Block:
            Branch Variable(4, Boolean), 5, 2
        Block 4:Block:
            Variable(4, Boolean) = Store Bool(false)
            Jump(3)
        Block 5:Block:
            Variable(5, Qubit) = Index Array(0), Variable(2, Integer)
            Call id(2), args( Variable(5, Qubit), Result(0), )
            Variable(6, Boolean) = Call id(3), args( Result(0), )
            Variable(7, Boolean) = Store Variable(6, Boolean)
            Branch Variable(7, Boolean), 7, 6
        Block 6:Block:
            Variable(8, Integer) = Add Variable(2, Integer), Integer(1)
            Variable(2, Integer) = Store Variable(8, Integer)
            Jump(1)
        Block 7:Block:
            StoreIndex Integer(1), Variable(2, Integer), Variable(1, Array(4, Integer))
            Jump(6)"#]],
    );
}

#[test]
fn mutable_fixed_size_array_slicing() {
    let program = get_rir_program_with_adaptive_profile(indoc! {r#"
        @EntryPoint()
        operation Main() : Int[] {
            use qs = Qubit[4];
            mutable arr = [0, size = Length(qs)];
            for idx in 0..Length(qs)-1 {
                if M(qs[idx]) == One {
                    arr[idx] = 1;
                }
            }
            arr[...2...]
        }
    "#});
    assert_blocks(
        &program,
        &expect![[r#"
        Blocks:
        Block 0:Block:
            Call id(1), args( Pointer, )
            Variable(0, Integer) = Store Integer(0)
            Variable(0, Integer) = Store Integer(1)
            Variable(0, Integer) = Store Integer(2)
            Variable(0, Integer) = Store Integer(3)
            Variable(0, Integer) = Store Integer(4)
            Variable(1, Array(4, Integer)) = StoreArray [Integer(0), Integer(0), Integer(0), Integer(0)]
            Variable(2, Integer) = Store Integer(0)
            Jump(1)
        Block 1:Block:
            Variable(3, Boolean) = Icmp Sle, Variable(2, Integer), Integer(3)
            Variable(4, Boolean) = Store Bool(true)
            Branch Variable(3, Boolean), 3, 4
        Block 2:Block:
            Variable(9, Array(2, Integer)) = SliceArray Variable(1, Array(4, Integer)), 0, 2, 3
            Variable(10, Array(2, Integer)) = CopyArray Variable(9, Array(2, Integer))
            Variable(11, Integer) = Index Variable(10, Array(2, Integer)), Integer(0)
            Variable(12, Integer) = Index Variable(10, Array(2, Integer)), Integer(1)
            Call id(4), args( Integer(2), Tag(0, 3), )
            Call id(5), args( Variable(11, Integer), Tag(1, 5), )
            Call id(5), args( Variable(12, Integer), Tag(2, 5), )
            Return Integer(0)
        Block 3:Block:
            Branch Variable(4, Boolean), 5, 2
        Block 4:Block:
            Variable(4, Boolean) = Store Bool(false)
            Jump(3)
        Block 5:Block:
            Variable(5, Qubit) = Index Array(0), Variable(2, Integer)
            Call id(2), args( Variable(5, Qubit), Result(0), )
            Variable(6, Boolean) = Call id(3), args( Result(0), )
            Variable(7, Boolean) = Store Variable(6, Boolean)
            Branch Variable(7, Boolean), 7, 6
        Block 6:Block:
            Variable(8, Integer) = Add Variable(2, Integer), Integer(1)
            Variable(2, Integer) = Store Variable(8, Integer)
            Jump(1)
        Block 7:Block:
            StoreIndex Integer(1), Variable(2, Integer), Variable(1, Array(4, Integer))
            Jump(6)"#]],
    );
}

#[test]
fn mutable_fixed_size_array_reverse_slicing() {
    let program = get_rir_program_with_adaptive_profile(indoc! {r#"
        @EntryPoint()
        operation Main() : Int[] {
            use qs = Qubit[4];
            mutable arr = [0, size = Length(qs)];
            for idx in 0..Length(qs)-1 {
                if M(qs[idx]) == One {
                    arr[idx] = 1;
                }
            }
            arr[...-2...]
        }
    "#});
    assert_blocks(
        &program,
        &expect![[r#"
        Blocks:
        Block 0:Block:
            Call id(1), args( Pointer, )
            Variable(0, Integer) = Store Integer(0)
            Variable(0, Integer) = Store Integer(1)
            Variable(0, Integer) = Store Integer(2)
            Variable(0, Integer) = Store Integer(3)
            Variable(0, Integer) = Store Integer(4)
            Variable(1, Array(4, Integer)) = StoreArray [Integer(0), Integer(0), Integer(0), Integer(0)]
            Variable(2, Integer) = Store Integer(0)
            Jump(1)
        Block 1:Block:
            Variable(3, Boolean) = Icmp Sle, Variable(2, Integer), Integer(3)
            Variable(4, Boolean) = Store Bool(true)
            Branch Variable(3, Boolean), 3, 4
        Block 2:Block:
            Variable(9, Array(2, Integer)) = SliceArray Variable(1, Array(4, Integer)), 3, -2, 0
            Variable(10, Array(2, Integer)) = CopyArray Variable(9, Array(2, Integer))
            Variable(11, Integer) = Index Variable(10, Array(2, Integer)), Integer(0)
            Variable(12, Integer) = Index Variable(10, Array(2, Integer)), Integer(1)
            Call id(4), args( Integer(2), Tag(0, 3), )
            Call id(5), args( Variable(11, Integer), Tag(1, 5), )
            Call id(5), args( Variable(12, Integer), Tag(2, 5), )
            Return Integer(0)
        Block 3:Block:
            Branch Variable(4, Boolean), 5, 2
        Block 4:Block:
            Variable(4, Boolean) = Store Bool(false)
            Jump(3)
        Block 5:Block:
            Variable(5, Qubit) = Index Array(0), Variable(2, Integer)
            Call id(2), args( Variable(5, Qubit), Result(0), )
            Variable(6, Boolean) = Call id(3), args( Result(0), )
            Variable(7, Boolean) = Store Variable(6, Boolean)
            Branch Variable(7, Boolean), 7, 6
        Block 6:Block:
            Variable(8, Integer) = Add Variable(2, Integer), Integer(1)
            Variable(2, Integer) = Store Variable(8, Integer)
            Jump(1)
        Block 7:Block:
            StoreIndex Integer(1), Variable(2, Integer), Variable(1, Array(4, Integer))
            Jump(6)"#]],
    );
}

#[test]
fn immutable_array_dynamic_content_generates_store_array() {
    let program = get_rir_program_with_adaptive_profile(indoc! {r#"
        operation Main() : Bool[] {
            use qs = Qubit[3];
            mutable arr = [false, size = Length(qs) + 1];
            let res = Std.Convert.ResultArrayAsBoolArray(MResetEachZ(qs));
            for i in 0..Length(res)-1 {
                arr[i + 1] = res[i];
            }
            arr
        }
    "#});
    assert_blocks(
        &program,
        &expect![[r#"
        Blocks:
        Block 0:Block:
            Call id(1), args( Pointer, )
            Variable(0, Integer) = Store Integer(0)
            Variable(0, Integer) = Store Integer(1)
            Variable(0, Integer) = Store Integer(2)
            Variable(0, Integer) = Store Integer(3)
            Variable(1, Array(4, Boolean)) = StoreArray [Bool(false), Bool(false), Bool(false), Bool(false)]
            Variable(2, Integer) = Store Integer(0)
            Call id(2), args( Qubit(0), Result(0), )
            Variable(2, Integer) = Store Integer(1)
            Call id(2), args( Qubit(1), Result(1), )
            Variable(2, Integer) = Store Integer(2)
            Call id(2), args( Qubit(2), Result(2), )
            Variable(2, Integer) = Store Integer(3)
            Variable(3, Integer) = Store Integer(0)
            Variable(4, Boolean) = Call id(3), args( Result(0), )
            Variable(5, Boolean) = Store Variable(4, Boolean)
            Variable(3, Integer) = Store Integer(1)
            Variable(6, Boolean) = Call id(3), args( Result(1), )
            Variable(7, Boolean) = Store Variable(6, Boolean)
            Variable(3, Integer) = Store Integer(2)
            Variable(8, Boolean) = Call id(3), args( Result(2), )
            Variable(9, Boolean) = Store Variable(8, Boolean)
            Variable(3, Integer) = Store Integer(3)
            Variable(10, Array(3, Boolean)) = StoreArray [Variable(5, Boolean), Variable(7, Boolean), Variable(9, Boolean)]
            Variable(11, Integer) = Store Integer(0)
            Jump(1)
        Block 1:Block:
            Variable(12, Boolean) = Icmp Sle, Variable(11, Integer), Integer(2)
            Variable(13, Boolean) = Store Bool(true)
            Branch Variable(12, Boolean), 3, 4
        Block 2:Block:
            Variable(18, Array(4, Boolean)) = CopyArray Variable(1, Array(4, Boolean))
            Variable(19, Boolean) = Index Variable(18, Array(4, Boolean)), Integer(0)
            Variable(20, Boolean) = Index Variable(18, Array(4, Boolean)), Integer(1)
            Variable(21, Boolean) = Index Variable(18, Array(4, Boolean)), Integer(2)
            Variable(22, Boolean) = Index Variable(18, Array(4, Boolean)), Integer(3)
            Call id(4), args( Integer(4), Tag(0, 3), )
            Call id(5), args( Variable(19, Boolean), Tag(1, 5), )
            Call id(5), args( Variable(20, Boolean), Tag(2, 5), )
            Call id(5), args( Variable(21, Boolean), Tag(3, 5), )
            Call id(5), args( Variable(22, Boolean), Tag(4, 5), )
            Return Integer(0)
        Block 3:Block:
            Branch Variable(13, Boolean), 5, 2
        Block 4:Block:
            Variable(13, Boolean) = Store Bool(false)
            Jump(3)
        Block 5:Block:
            Variable(14, Integer) = Add Variable(11, Integer), Integer(1)
            Variable(15, Integer) = Store Variable(14, Integer)
            Variable(16, Boolean) = Index Variable(10, Array(3, Boolean)), Variable(11, Integer)
            StoreIndex Variable(16, Boolean), Variable(15, Integer), Variable(1, Array(4, Boolean))
            Variable(17, Integer) = Add Variable(11, Integer), Integer(1)
            Variable(11, Integer) = Store Variable(17, Integer)
            Jump(1)"#]],
    );
}
