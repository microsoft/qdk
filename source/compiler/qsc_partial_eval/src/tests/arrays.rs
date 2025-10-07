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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[122-133] scope=0 scope_package_id=2 scope_span=[68-153] callable=Main
                Call id(1), args( Qubit(1), Result(1), ) !dbg package_id=2 span=[135-146] scope=0 scope_package_id=2 scope_span=[68-153] callable=Main
                Call id(2), args( Integer(2), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(0), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(1), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]"#]],
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[109-119] scope=0 scope_package_id=2 scope_span=[66-151] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[136-144] scope=0 scope_package_id=2 scope_span=[66-151] callable=Main
                Variable(1, Boolean) = Store Variable(0, Boolean) !dbg package_id=2 span=[136-144] scope=0 scope_package_id=2 scope_span=[66-151] callable=Main
                Call id(3), args( Integer(2), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(4), args( Bool(true), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(4), args( Variable(1, Boolean), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]"#]],
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[104-114] scope=0 scope_package_id=2 scope_span=[68-131] callable=Main
                Call id(2), args( Integer(2), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(0), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(0), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]"#]],
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[134-145] scope=0 scope_package_id=2 scope_span=[66-185] callable=Main
                Call id(1), args( Qubit(1), Result(1), ) !dbg package_id=2 span=[147-158] scope=0 scope_package_id=2 scope_span=[66-185] callable=Main
                Call id(2), args( Result(1), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]"#]],
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[134-145] scope=0 scope_package_id=2 scope_span=[66-186] callable=Main
                Call id(1), args( Qubit(1), Result(1), ) !dbg package_id=2 span=[147-158] scope=0 scope_package_id=2 scope_span=[66-186] callable=Main
                Call id(2), args( Result(1), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]"#]],
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[169-180] scope=0 scope_package_id=2 scope_span=[68-259] callable=Main
                Call id(1), args( Qubit(1), Result(1), ) !dbg package_id=2 span=[182-193] scope=0 scope_package_id=2 scope_span=[68-259] callable=Main
                Call id(1), args( Qubit(2), Result(2), ) !dbg package_id=2 span=[195-206] scope=0 scope_package_id=2 scope_span=[68-259] callable=Main
                Call id(1), args( Qubit(3), Result(3), ) !dbg package_id=2 span=[208-219] scope=0 scope_package_id=2 scope_span=[68-259] callable=Main
                Call id(1), args( Qubit(4), Result(4), ) !dbg package_id=2 span=[221-232] scope=0 scope_package_id=2 scope_span=[68-259] callable=Main
                Call id(2), args( Integer(3), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(0), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(2), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(4), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]"#]],
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[143-154] scope=0 scope_package_id=2 scope_span=[68-204] callable=Main
                Call id(1), args( Qubit(1), Result(1), ) !dbg package_id=2 span=[156-167] scope=0 scope_package_id=2 scope_span=[68-204] callable=Main
                Call id(1), args( Qubit(2), Result(2), ) !dbg package_id=2 span=[169-180] scope=0 scope_package_id=2 scope_span=[68-204] callable=Main
                Call id(2), args( Integer(2), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(0), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(1), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]"#]],
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[143-154] scope=0 scope_package_id=2 scope_span=[68-204] callable=Main
                Call id(1), args( Qubit(1), Result(1), ) !dbg package_id=2 span=[156-167] scope=0 scope_package_id=2 scope_span=[68-204] callable=Main
                Call id(1), args( Qubit(2), Result(2), ) !dbg package_id=2 span=[169-180] scope=0 scope_package_id=2 scope_span=[68-204] callable=Main
                Call id(2), args( Integer(2), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(1), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(2), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]"#]],
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[169-180] scope=0 scope_package_id=2 scope_span=[68-259] callable=Main
                Call id(1), args( Qubit(1), Result(1), ) !dbg package_id=2 span=[182-193] scope=0 scope_package_id=2 scope_span=[68-259] callable=Main
                Call id(1), args( Qubit(2), Result(2), ) !dbg package_id=2 span=[195-206] scope=0 scope_package_id=2 scope_span=[68-259] callable=Main
                Call id(1), args( Qubit(3), Result(3), ) !dbg package_id=2 span=[208-219] scope=0 scope_package_id=2 scope_span=[68-259] callable=Main
                Call id(1), args( Qubit(4), Result(4), ) !dbg package_id=2 span=[221-232] scope=0 scope_package_id=2 scope_span=[68-259] callable=Main
                Call id(2), args( Integer(3), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(0), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(2), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(4), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]"#]],
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[156-167] scope=0 scope_package_id=2 scope_span=[68-231] callable=Main
                Call id(1), args( Qubit(1), Result(1), ) !dbg package_id=2 span=[169-180] scope=0 scope_package_id=2 scope_span=[68-231] callable=Main
                Call id(1), args( Qubit(2), Result(2), ) !dbg package_id=2 span=[182-193] scope=0 scope_package_id=2 scope_span=[68-231] callable=Main
                Call id(1), args( Qubit(3), Result(3), ) !dbg package_id=2 span=[214-225] scope=0 scope_package_id=2 scope_span=[68-231] callable=Main
                Call id(2), args( Integer(3), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(0), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(3), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(2), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]"#]],
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[231-242] scope=0 scope_package_id=2 scope_span=[68-372] callable=Main
                Call id(1), args( Qubit(1), Result(1), ) !dbg package_id=2 span=[244-255] scope=0 scope_package_id=2 scope_span=[68-372] callable=Main
                Call id(1), args( Qubit(2), Result(2), ) !dbg package_id=2 span=[257-268] scope=0 scope_package_id=2 scope_span=[68-372] callable=Main
                Call id(1), args( Qubit(3), Result(3), ) !dbg package_id=2 span=[270-281] scope=0 scope_package_id=2 scope_span=[68-372] callable=Main
                Call id(1), args( Qubit(4), Result(4), ) !dbg package_id=2 span=[283-294] scope=0 scope_package_id=2 scope_span=[68-372] callable=Main
                Call id(1), args( Qubit(5), Result(5), ) !dbg package_id=2 span=[322-335] scope=0 scope_package_id=2 scope_span=[68-372] callable=Main
                Call id(1), args( Qubit(6), Result(6), ) !dbg package_id=2 span=[337-350] scope=0 scope_package_id=2 scope_span=[68-372] callable=Main
                Call id(1), args( Qubit(7), Result(7), ) !dbg package_id=2 span=[352-365] scope=0 scope_package_id=2 scope_span=[68-372] callable=Main
                Call id(2), args( Integer(5), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(5), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(1), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(6), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(3), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(7), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]"#]],
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[169-180] scope=0 scope_package_id=2 scope_span=[68-262] callable=Main
                Call id(1), args( Qubit(1), Result(1), ) !dbg package_id=2 span=[182-193] scope=0 scope_package_id=2 scope_span=[68-262] callable=Main
                Call id(1), args( Qubit(2), Result(2), ) !dbg package_id=2 span=[195-206] scope=0 scope_package_id=2 scope_span=[68-262] callable=Main
                Call id(1), args( Qubit(3), Result(3), ) !dbg package_id=2 span=[231-242] scope=0 scope_package_id=2 scope_span=[68-262] callable=Main
                Call id(1), args( Qubit(4), Result(4), ) !dbg package_id=2 span=[244-255] scope=0 scope_package_id=2 scope_span=[68-262] callable=Main
                Call id(2), args( Integer(3), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(3), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(4), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(2), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]"#]],
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[169-180] scope=0 scope_package_id=2 scope_span=[68-262] callable=Main
                Call id(1), args( Qubit(1), Result(1), ) !dbg package_id=2 span=[182-193] scope=0 scope_package_id=2 scope_span=[68-262] callable=Main
                Call id(1), args( Qubit(2), Result(2), ) !dbg package_id=2 span=[195-206] scope=0 scope_package_id=2 scope_span=[68-262] callable=Main
                Call id(1), args( Qubit(3), Result(3), ) !dbg package_id=2 span=[231-242] scope=0 scope_package_id=2 scope_span=[68-262] callable=Main
                Call id(1), args( Qubit(4), Result(4), ) !dbg package_id=2 span=[244-255] scope=0 scope_package_id=2 scope_span=[68-262] callable=Main
                Call id(2), args( Integer(3), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(0), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(3), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(4), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]"#]],
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[169-180] scope=0 scope_package_id=2 scope_span=[68-265] callable=Main
                Call id(1), args( Qubit(1), Result(1), ) !dbg package_id=2 span=[182-193] scope=0 scope_package_id=2 scope_span=[68-265] callable=Main
                Call id(1), args( Qubit(2), Result(2), ) !dbg package_id=2 span=[195-206] scope=0 scope_package_id=2 scope_span=[68-265] callable=Main
                Call id(1), args( Qubit(3), Result(3), ) !dbg package_id=2 span=[234-245] scope=0 scope_package_id=2 scope_span=[68-265] callable=Main
                Call id(1), args( Qubit(4), Result(4), ) !dbg package_id=2 span=[247-258] scope=0 scope_package_id=2 scope_span=[68-265] callable=Main
                Call id(2), args( Integer(3), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(3), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(1), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(4), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]"#]],
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
                    Variable(2, Integer) = Store Integer(0)
                    Variable(2, Integer) = Store Integer(1)
                    Variable(2, Integer) = Store Integer(2)
                    Call id(3), args( Integer(1), Tag(0, 3), )
                    Return
            config: Config:
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations | BackwardsBranching | HigherLevelConstructs | QubitReset)
            num_qubits: 2
            num_results: 2
            tags:
                [0]: 0_i
    "#]].assert_eq(&program.to_string());
}
