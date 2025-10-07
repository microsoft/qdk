// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(
    clippy::needless_raw_string_hashes,
    clippy::similar_names,
    clippy::too_many_lines
)]

use super::{
    assert_block_instructions, assert_blocks, assert_callable, assert_error,
    get_partial_evaluation_error, get_rir_program,
};
use expect_test::expect;
use indoc::indoc;
use qsc_rir::rir::{BlockId, CallableId};

#[test]
fn assigning_result_literal_updates_value() {
    // This test verifies that the result value is updated using the fact that a program that returns a result literal
    // value will raise an error in partial evaluation.
    let error = get_partial_evaluation_error(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                mutable r = MResetZ(q);
                set r = One;
                r
            }
        }
    "#});
    assert_error(
        &error,
        &expect![
            "OutputResultLiteral(PackageSpan { package: PackageId(2), span: Span { lo: 50, hi: 54 } })"
        ],
    );
}

#[test]
fn assigning_result_register_updates_value() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                mutable r = Zero;
                set r = MResetZ(q);
                r
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
    let output_recording_callable_id = CallableId(2);
    assert_callable(
        &program,
        output_recording_callable_id,
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[135-145] scope=0 scope_package_id=2 scope_span=[66-162] callable=Main
                Call id(2), args( Result(0), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]"#]],
    );
}

#[test]
fn assigning_classical_bool_updates_value_and_adds_store_instructions() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Bool {
                use q = Qubit(); // Needed to make `Main` hybrid.
                mutable b = true;
                set b = false;
                b
            }
        }
    "#});
    let output_recording_callable_id = CallableId(1);
    assert_callable(
        &program,
        output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Variable(0, Boolean) = Store Bool(true) !dbg package_id=2 span=[140-141] scope=0 scope_package_id=2 scope_span=[64-188] callable=Main
                Variable(0, Boolean) = Store Bool(false) !dbg package_id=2 span=[162-163] scope=0 scope_package_id=2 scope_span=[64-188] callable=Main
                Call id(1), args( Bool(false), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]"#]],
    );
}

#[test]
fn assigning_dynamic_bool_updates_value_and_adds_store_instructions() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Bool {
                use q = Qubit();
                mutable b = false;
                set b = MResetZ(q) == One;
                b
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
    let readout_callable_id = CallableId(2);
    assert_callable(
        &program,
        readout_callable_id,
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
    let output_recording_callable_id = CallableId(3);
    assert_callable(
        &program,
        output_recording_callable_id,
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
                Variable(0, Boolean) = Store Bool(false) !dbg package_id=2 span=[107-108] scope=0 scope_package_id=2 scope_span=[64-168] callable=Main
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[134-144] scope=0 scope_package_id=2 scope_span=[64-168] callable=Main
                Variable(1, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[134-151] scope=0 scope_package_id=2 scope_span=[64-168] callable=Main
                Variable(2, Boolean) = Store Variable(1, Boolean) !dbg package_id=2 span=[134-151] scope=0 scope_package_id=2 scope_span=[64-168] callable=Main
                Variable(0, Boolean) = Store Variable(2, Boolean) !dbg package_id=2 span=[130-131] scope=0 scope_package_id=2 scope_span=[64-168] callable=Main
                Variable(3, Boolean) = Store Variable(0, Boolean) !dbg package_id=2 span=[161-162] scope=0 scope_package_id=2 scope_span=[64-168] callable=Main
                Call id(3), args( Variable(3, Boolean), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]"#]],
    );
}

#[test]
fn assigning_classical_int_updates_value_and_adds_store_instructions() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit(); // Needed to make `Main` hybrid.
                mutable i = 0;
                set i = 1;
                i
            }
        }
    "#});
    let output_recording_callable_id = CallableId(1);
    assert_callable(
        &program,
        output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Variable(0, Integer) = Store Integer(0) !dbg package_id=2 span=[139-140] scope=0 scope_package_id=2 scope_span=[63-180] callable=Main
                Variable(0, Integer) = Store Integer(1) !dbg package_id=2 span=[158-159] scope=0 scope_package_id=2 scope_span=[63-180] callable=Main
                Call id(1), args( Integer(1), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]"#]],
    );
}

#[test]
fn assigning_dynamic_int_updates_value_and_adds_store_instructions() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                mutable i = 0;
                set i = MResetZ(q) == One ? 1 | 2;
                i
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
    let readout_callable_id = CallableId(2);
    assert_callable(
        &program,
        readout_callable_id,
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
    let output_recording_callable_id = CallableId(3);
    assert_callable(
        &program,
        output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__read_result
                call_type: Readout
                input_type:
                    [0]: Result
                output_type: Boolean
                body: <NONE>"#]],
    );
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Variable(0, Integer) = Store Integer(0) !dbg package_id=2 span=[106-107] scope=0 scope_package_id=2 scope_span=[63-171] callable=Main
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[129-139] scope=0 scope_package_id=2 scope_span=[63-171] callable=Main
                Variable(1, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[129-146] scope=0 scope_package_id=2 scope_span=[63-171] callable=Main
                Variable(2, Boolean) = Store Variable(1, Boolean) !dbg package_id=2 span=[129-146] scope=0 scope_package_id=2 scope_span=[63-171] callable=Main
                Branch Variable(2, Boolean), 2, 3 !dbg package_id=2 span=[129-146] scope=0 scope_package_id=2 scope_span=[63-171] callable=Main
            Block 1:Block:
                Variable(0, Integer) = Store Variable(3, Integer) !dbg package_id=2 span=[125-126] scope=0 scope_package_id=2 scope_span=[63-171] callable=Main
                Variable(4, Integer) = Store Variable(0, Integer) !dbg package_id=2 span=[164-165] scope=0 scope_package_id=2 scope_span=[63-171] callable=Main
                Call id(3), args( Variable(4, Integer), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]
            Block 2:Block:
                Variable(3, Integer) = Store Integer(1) !dbg package_id=2 span=[149-150] scope=0 scope_package_id=2 scope_span=[63-171] callable=Main
                Jump(1) !dbg package_id=2 span=[149-150] scope=0 scope_package_id=2 scope_span=[63-171] callable=Main
            Block 3:Block:
                Variable(3, Integer) = Store Integer(2) !dbg package_id=2 span=[153-154] scope=0 scope_package_id=2 scope_span=[63-171] callable=Main
                Jump(1) !dbg package_id=2 span=[153-154] scope=0 scope_package_id=2 scope_span=[63-171] callable=Main"#]],
    );
}

#[test]
fn assigning_classical_bool_within_dynamic_if_expression_adds_store_instruction() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Bool {
                use q = Qubit();
                mutable b = false;
                if MResetZ(q) == One {
                    set b = true;
                }
                b
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
    let readout_callable_id = CallableId(2);
    assert_callable(
        &program,
        readout_callable_id,
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
    let output_recording_callable_id = CallableId(3);
    assert_callable(
        &program,
        output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__read_result
                call_type: Readout
                input_type:
                    [0]: Result
                output_type: Boolean
                body: <NONE>"#]],
    );
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Variable(0, Boolean) = Store Bool(false) !dbg package_id=2 span=[107-108] scope=0 scope_package_id=2 scope_span=[64-200] callable=Main
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[129-139] scope=0 scope_package_id=2 scope_span=[64-200] callable=Main
                Variable(1, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[129-146] scope=0 scope_package_id=2 scope_span=[64-200] callable=Main
                Variable(2, Boolean) = Store Variable(1, Boolean) !dbg package_id=2 span=[129-146] scope=0 scope_package_id=2 scope_span=[64-200] callable=Main
                Branch Variable(2, Boolean), 2, 1 !dbg package_id=2 span=[129-146] scope=0 scope_package_id=2 scope_span=[64-200] callable=Main
            Block 1:Block:
                Variable(3, Boolean) = Store Variable(0, Boolean) !dbg package_id=2 span=[193-194] scope=0 scope_package_id=2 scope_span=[64-200] callable=Main
                Call id(3), args( Variable(3, Boolean), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]
            Block 2:Block:
                Variable(0, Boolean) = Store Bool(true) !dbg package_id=2 span=[165-166] scope=1 scope_package_id=2 scope_span=[147-184] callable=Main
                Jump(1) !dbg package_id=2 span=[147-184] scope=0 scope_package_id=2 scope_span=[64-200] callable=Main"#]],
    );
}

#[test]
fn assigning_classical_int_within_dynamic_if_else_expression_adds_store_instruction() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                mutable i = 0;
                if MResetZ(q) == Zero {
                    set i = 1;
                } else {
                    set i = 2;
                }
                i
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
    let readout_callable_id = CallableId(2);
    assert_callable(
        &program,
        readout_callable_id,
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
    let output_recording_callable_id = CallableId(3);
    assert_callable(
        &program,
        output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__read_result
                call_type: Readout
                input_type:
                    [0]: Result
                output_type: Boolean
                body: <NONE>"#]],
    );
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Variable(0, Integer) = Store Integer(0) !dbg package_id=2 span=[106-107] scope=0 scope_package_id=2 scope_span=[63-233] callable=Main
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[124-134] scope=0 scope_package_id=2 scope_span=[63-233] callable=Main
                Variable(1, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[124-142] scope=0 scope_package_id=2 scope_span=[63-233] callable=Main
                Variable(2, Boolean) = Icmp Eq, Variable(1, Boolean), Bool(false) !dbg package_id=2 span=[124-142] scope=0 scope_package_id=2 scope_span=[63-233] callable=Main
                Branch Variable(2, Boolean), 2, 3 !dbg package_id=2 span=[124-142] scope=0 scope_package_id=2 scope_span=[63-233] callable=Main
            Block 1:Block:
                Variable(3, Integer) = Store Variable(0, Integer) !dbg package_id=2 span=[226-227] scope=0 scope_package_id=2 scope_span=[63-233] callable=Main
                Call id(3), args( Variable(3, Integer), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]
            Block 2:Block:
                Variable(0, Integer) = Store Integer(1) !dbg package_id=2 span=[161-162] scope=1 scope_package_id=2 scope_span=[143-177] callable=Main
                Jump(1) !dbg package_id=2 span=[143-177] scope=0 scope_package_id=2 scope_span=[63-233] callable=Main
            Block 3:Block:
                Variable(0, Integer) = Store Integer(2) !dbg package_id=2 span=[201-202] scope=2 scope_package_id=2 scope_span=[183-217] callable=Main
                Jump(1) !dbg package_id=2 span=[178-217] scope=0 scope_package_id=2 scope_span=[63-233] callable=Main"#]],
    );
}

#[test]
fn assigning_result_literal_within_dynamic_if_expression_produces_error() {
    let error = get_partial_evaluation_error(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                mutable r = Zero;
                if MResetZ(q) == One {
                    set r = One;
                }
                r
            }
        }
    "#});
    assert_error(
        &error,
        &expect![[
            r#"Unexpected("re-assignment within a dynamic branch is unsupported for type Result", PackageSpan { package: PackageId(2), span: Span { lo: 166, hi: 167 } })"#
        ]],
    );
}

#[test]
fn array_of_results_update_element_at_index_with_dynamic_content() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result[] {
                use (q0, q1) = (Qubit(), Qubit());
                mutable arr = [MResetZ(q0), Zero];
                set arr w/= 1 <- MResetZ(q1);
                arr
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[136-147] scope=0 scope_package_id=2 scope_span=[68-211] callable=Main
                Call id(1), args( Qubit(1), Result(1), ) !dbg package_id=2 span=[181-192] scope=0 scope_package_id=2 scope_span=[68-211] callable=Main
                Call id(2), args( Integer(2), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(0), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(1), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]"#]],
    );
}

#[test]
fn array_of_bools_update_element_at_index_with_dynamic_content() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Bool[] {
                use (q0, q1) = (Qubit(), Qubit());
                mutable arr = [MResetZ(q0) == Zero, true];
                set arr w/= 1 <- MResetZ(q1) == One;
                arr
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
    let readout_callable_id = CallableId(2);
    assert_callable(
        &program,
        readout_callable_id,
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
    let array_output_recording_callable_id = CallableId(3);
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
    let bool_output_recording_callable_id = CallableId(4);
    assert_callable(
        &program,
        bool_output_recording_callable_id,
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[134-145] scope=0 scope_package_id=2 scope_span=[66-224] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[134-153] scope=0 scope_package_id=2 scope_span=[66-224] callable=Main
                Variable(1, Boolean) = Icmp Eq, Variable(0, Boolean), Bool(false) !dbg package_id=2 span=[134-153] scope=0 scope_package_id=2 scope_span=[66-224] callable=Main
                Call id(1), args( Qubit(1), Result(1), ) !dbg package_id=2 span=[187-198] scope=0 scope_package_id=2 scope_span=[66-224] callable=Main
                Variable(2, Boolean) = Call id(2), args( Result(1), ) !dbg package_id=2 span=[187-205] scope=0 scope_package_id=2 scope_span=[66-224] callable=Main
                Variable(3, Boolean) = Store Variable(2, Boolean) !dbg package_id=2 span=[187-205] scope=0 scope_package_id=2 scope_span=[66-224] callable=Main
                Call id(3), args( Integer(2), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(4), args( Variable(1, Boolean), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(4), args( Variable(3, Boolean), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]"#]],
    );
}

#[test]
fn array_of_results_update_element_at_negative_index_raises_error() {
    let error = get_partial_evaluation_error(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result[] {
                use (q0, q1) = (Qubit(), Qubit());
                mutable arr = [MResetZ(q0), Zero];
                set arr w/= -1 <- MResetZ(q1);
                arr
            }
        }
    "#});
    assert_error(
        &error,
        &expect![[
            r#"EvaluationFailed("negative integers cannot be used here: -1", PackageSpan { package: PackageId(2), span: Span { lo: 176, hi: 178 } })"#
        ]],
    );
}

#[test]
fn array_of_results_update_element_at_out_of_bounds_index_raises_error() {
    let error = get_partial_evaluation_error(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result[] {
                use (q0, q1) = (Qubit(), Qubit());
                mutable arr = [MResetZ(q0), Zero];
                set arr w/= 2 <- MResetZ(q1);
                arr
            }
        }
    "#});
    assert_error(
        &error,
        &expect![[
            r#"EvaluationFailed("index out of range: 2", PackageSpan { package: PackageId(2), span: Span { lo: 176, hi: 177 } })"#
        ]],
    );
}

#[test]
fn array_of_results_update_slice_with_explicit_range() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result[] {
                use (q0, q1, q2, q3, q4) = (Qubit(), Qubit(), Qubit(), Qubit(), Qubit());
                use (aux0, aux1, aux2) = (Qubit(), Qubit(), Qubit());
                mutable a = [MResetZ(q0), MResetZ(q1), MResetZ(q2), MResetZ(q3), MResetZ(q4)];
                set a w/= 0..2..4 <- [MResetZ(aux0), MResetZ(aux1), MResetZ(aux2)];
                a
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[235-246] scope=0 scope_package_id=2 scope_span=[68-392] callable=Main
                Call id(1), args( Qubit(1), Result(1), ) !dbg package_id=2 span=[248-259] scope=0 scope_package_id=2 scope_span=[68-392] callable=Main
                Call id(1), args( Qubit(2), Result(2), ) !dbg package_id=2 span=[261-272] scope=0 scope_package_id=2 scope_span=[68-392] callable=Main
                Call id(1), args( Qubit(3), Result(3), ) !dbg package_id=2 span=[274-285] scope=0 scope_package_id=2 scope_span=[68-392] callable=Main
                Call id(1), args( Qubit(4), Result(4), ) !dbg package_id=2 span=[287-298] scope=0 scope_package_id=2 scope_span=[68-392] callable=Main
                Call id(1), args( Qubit(5), Result(5), ) !dbg package_id=2 span=[331-344] scope=0 scope_package_id=2 scope_span=[68-392] callable=Main
                Call id(1), args( Qubit(6), Result(6), ) !dbg package_id=2 span=[346-359] scope=0 scope_package_id=2 scope_span=[68-392] callable=Main
                Call id(1), args( Qubit(7), Result(7), ) !dbg package_id=2 span=[361-374] scope=0 scope_package_id=2 scope_span=[68-392] callable=Main
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
fn array_of_results_update_slice_with_open_start_range() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result[] {
                use (q0, q1, q2, q3, q4) = (Qubit(), Qubit(), Qubit(), Qubit(), Qubit());
                mutable a = [MResetZ(q0), MResetZ(q1), MResetZ(q2)];
                set a w/= ...2 <- [MResetZ(q3), MResetZ(q4)];
                a
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[173-184] scope=0 scope_package_id=2 scope_span=[68-282] callable=Main
                Call id(1), args( Qubit(1), Result(1), ) !dbg package_id=2 span=[186-197] scope=0 scope_package_id=2 scope_span=[68-282] callable=Main
                Call id(1), args( Qubit(2), Result(2), ) !dbg package_id=2 span=[199-210] scope=0 scope_package_id=2 scope_span=[68-282] callable=Main
                Call id(1), args( Qubit(3), Result(3), ) !dbg package_id=2 span=[240-251] scope=0 scope_package_id=2 scope_span=[68-282] callable=Main
                Call id(1), args( Qubit(4), Result(4), ) !dbg package_id=2 span=[253-264] scope=0 scope_package_id=2 scope_span=[68-282] callable=Main
                Call id(2), args( Integer(3), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(3), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(4), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(2), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]"#]],
    );
}

#[test]
fn array_of_results_update_slice_with_open_ended_range() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result[] {
                use (q0, q1, q2, q3, q4) = (Qubit(), Qubit(), Qubit(), Qubit(), Qubit());
                mutable a = [MResetZ(q0), MResetZ(q1), MResetZ(q2)];
                set a w/= 1... <- [MResetZ(q3), MResetZ(q4)];
                a
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[173-184] scope=0 scope_package_id=2 scope_span=[68-282] callable=Main
                Call id(1), args( Qubit(1), Result(1), ) !dbg package_id=2 span=[186-197] scope=0 scope_package_id=2 scope_span=[68-282] callable=Main
                Call id(1), args( Qubit(2), Result(2), ) !dbg package_id=2 span=[199-210] scope=0 scope_package_id=2 scope_span=[68-282] callable=Main
                Call id(1), args( Qubit(3), Result(3), ) !dbg package_id=2 span=[240-251] scope=0 scope_package_id=2 scope_span=[68-282] callable=Main
                Call id(1), args( Qubit(4), Result(4), ) !dbg package_id=2 span=[253-264] scope=0 scope_package_id=2 scope_span=[68-282] callable=Main
                Call id(2), args( Integer(3), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(0), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(3), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(4), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]"#]],
    );
}

#[test]
fn array_of_results_update_slice_with_open_two_step_range() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result[] {
                use (q0, q1, q2, q3, q4) = (Qubit(), Qubit(), Qubit(), Qubit(), Qubit());
                mutable a = [MResetZ(q0), MResetZ(q1), MResetZ(q2)];
                set a w/= ...2... <- [MResetZ(q3), MResetZ(q4)];
                a
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[173-184] scope=0 scope_package_id=2 scope_span=[68-285] callable=Main
                Call id(1), args( Qubit(1), Result(1), ) !dbg package_id=2 span=[186-197] scope=0 scope_package_id=2 scope_span=[68-285] callable=Main
                Call id(1), args( Qubit(2), Result(2), ) !dbg package_id=2 span=[199-210] scope=0 scope_package_id=2 scope_span=[68-285] callable=Main
                Call id(1), args( Qubit(3), Result(3), ) !dbg package_id=2 span=[243-254] scope=0 scope_package_id=2 scope_span=[68-285] callable=Main
                Call id(1), args( Qubit(4), Result(4), ) !dbg package_id=2 span=[256-267] scope=0 scope_package_id=2 scope_span=[68-285] callable=Main
                Call id(2), args( Integer(3), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(3), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(1), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(4), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]"#]],
    );
}

#[test]
fn array_of_results_update_slice_with_out_of_bounds_range_raises_error() {
    let error = get_partial_evaluation_error(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result[] {
                use (q0, q1, q2, q3) = (Qubit(), Qubit(), Qubit(), Qubit());
                mutable a = [MResetZ(q0), MResetZ(q1), MResetZ(q2)];
                set a w/= 1..3 <- [MResetZ(q0), MResetZ(q1), MResetZ(q2)];
                a
            }
        }
    "#});
    assert_error(
        &error,
        &expect![[
            r#"EvaluationFailed("index out of range: 3", PackageSpan { package: PackageId(2), span: Span { lo: 218, hi: 222 } })"#
        ]],
    );
}

#[test]
fn empty_array_of_results_in_place_concatenation() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result[] {
                use (q0, q1) = (Qubit(), Qubit());
                mutable results = [];
                set results += [MResetZ(q0)];
                set results += [MResetZ(q1)];
                results
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[167-178] scope=0 scope_package_id=2 scope_span=[68-240] callable=Main
                Call id(1), args( Qubit(1), Result(1), ) !dbg package_id=2 span=[205-216] scope=0 scope_package_id=2 scope_span=[68-240] callable=Main
                Call id(2), args( Integer(2), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(0), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(1), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]"#]],
    );
}

#[test]
fn non_empty_array_of_results_in_place_concatenation() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result[] {
                use (q0, q1) = (Qubit(), Qubit());
                mutable results = [MResetZ(q0)];
                set results += [MResetZ(q1)];
                results
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[140-151] scope=0 scope_package_id=2 scope_span=[68-213] callable=Main
                Call id(1), args( Qubit(1), Result(1), ) !dbg package_id=2 span=[178-189] scope=0 scope_package_id=2 scope_span=[68-213] callable=Main
                Call id(2), args( Integer(2), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(0), Pointer, ) !dbg package_id=2 span=[50-54]
                Call id(3), args( Result(1), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]"#]],
    );
}

#[test]
fn logical_and_assign_with_lhs_classical_true_is_optimized_as_store() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Bool {
                use q = Qubit();
                let a = MResetZ(q) == One;
                mutable b = true;
                set b and= a;
                b
            }
        }
        "#,
    });
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
    let readout_callable_id = CallableId(2);
    assert_callable(
        &program,
        readout_callable_id,
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
    let output_record_id = CallableId(3);
    assert_callable(
        &program,
        output_record_id,
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[107-117] scope=0 scope_package_id=2 scope_span=[64-189] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[107-124] scope=0 scope_package_id=2 scope_span=[64-189] callable=Main
                Variable(1, Boolean) = Store Variable(0, Boolean) !dbg package_id=2 span=[107-124] scope=0 scope_package_id=2 scope_span=[64-189] callable=Main
                Variable(2, Boolean) = Store Variable(1, Boolean) !dbg package_id=2 span=[103-104] scope=0 scope_package_id=2 scope_span=[64-189] callable=Main
                Variable(3, Boolean) = Store Bool(true) !dbg package_id=2 span=[142-143] scope=0 scope_package_id=2 scope_span=[64-189] callable=Main
                Variable(3, Boolean) = Store Variable(2, Boolean) !dbg package_id=2 span=[164-165] scope=0 scope_package_id=2 scope_span=[64-189] callable=Main
                Variable(4, Boolean) = Store Variable(3, Boolean) !dbg package_id=2 span=[182-183] scope=0 scope_package_id=2 scope_span=[64-189] callable=Main
                Call id(3), args( Variable(4, Boolean), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]"#]],
    );
}

#[test]
fn logical_and_assign_with_lhs_classical_false_short_circuits_evaluation() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Bool {
                use q = Qubit();
                let a = MResetZ(q) == One;
                mutable b = false;
                set b and= a;
                b
            }
        }
        "#,
    });
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
    let readout_callable_id = CallableId(2);
    assert_callable(
        &program,
        readout_callable_id,
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
    let output_record_id = CallableId(3);
    assert_callable(
        &program,
        output_record_id,
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[107-117] scope=0 scope_package_id=2 scope_span=[64-190] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[107-124] scope=0 scope_package_id=2 scope_span=[64-190] callable=Main
                Variable(1, Boolean) = Store Variable(0, Boolean) !dbg package_id=2 span=[107-124] scope=0 scope_package_id=2 scope_span=[64-190] callable=Main
                Variable(2, Boolean) = Store Variable(1, Boolean) !dbg package_id=2 span=[103-104] scope=0 scope_package_id=2 scope_span=[64-190] callable=Main
                Variable(3, Boolean) = Store Bool(false) !dbg package_id=2 span=[142-143] scope=0 scope_package_id=2 scope_span=[64-190] callable=Main
                Variable(3, Boolean) = Store Bool(false) !dbg package_id=2 span=[165-166] scope=0 scope_package_id=2 scope_span=[64-190] callable=Main
                Call id(3), args( Bool(false), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]"#]],
    );
}

#[test]
fn logical_and_assign_with_dynamic_lhs_and_dynamic_rhs_short_circuits_when_rhs_is_false() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Bool {
                use q = Qubit();
                mutable b = MResetZ(q) != One;
                set b and= MResetZ(q) != One;
                b
            }
        }
        "#,
    });
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
    let readout_callable_id = CallableId(2);
    assert_callable(
        &program,
        readout_callable_id,
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
    let output_record_id = CallableId(3);
    assert_callable(
        &program,
        output_record_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__read_result
                call_type: Readout
                input_type:
                    [0]: Result
                output_type: Boolean
                body: <NONE>"#]],
    );
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[111-121] scope=0 scope_package_id=2 scope_span=[64-183] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[111-128] scope=0 scope_package_id=2 scope_span=[64-183] callable=Main
                Variable(1, Boolean) = Icmp Ne, Variable(0, Boolean), Bool(true) !dbg package_id=2 span=[111-128] scope=0 scope_package_id=2 scope_span=[64-183] callable=Main
                Variable(2, Boolean) = Store Variable(1, Boolean) !dbg package_id=2 span=[107-108] scope=0 scope_package_id=2 scope_span=[64-183] callable=Main
                Variable(3, Boolean) = Store Bool(false) !dbg package_id=2 span=[149-166] scope=0 scope_package_id=2 scope_span=[64-183] callable=Main
                Branch Variable(2, Boolean), 2, 1 !dbg package_id=2 span=[149-166] scope=0 scope_package_id=2 scope_span=[64-183] callable=Main
            Block 1:Block:
                Variable(2, Boolean) = Store Variable(3, Boolean) !dbg package_id=2 span=[142-143] scope=0 scope_package_id=2 scope_span=[64-183] callable=Main
                Variable(6, Boolean) = Store Variable(2, Boolean) !dbg package_id=2 span=[176-177] scope=0 scope_package_id=2 scope_span=[64-183] callable=Main
                Call id(3), args( Variable(6, Boolean), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]
            Block 2:Block:
                Call id(1), args( Qubit(0), Result(1), ) !dbg package_id=2 span=[149-159] scope=0 scope_package_id=2 scope_span=[64-183] callable=Main
                Variable(4, Boolean) = Call id(2), args( Result(1), ) !dbg package_id=2 span=[149-166] scope=0 scope_package_id=2 scope_span=[64-183] callable=Main
                Variable(5, Boolean) = Icmp Ne, Variable(4, Boolean), Bool(true) !dbg package_id=2 span=[149-166] scope=0 scope_package_id=2 scope_span=[64-183] callable=Main
                Variable(3, Boolean) = Store Variable(5, Boolean) !dbg package_id=2 span=[149-166] scope=0 scope_package_id=2 scope_span=[64-183] callable=Main
                Jump(1) !dbg package_id=2 span=[149-166] scope=0 scope_package_id=2 scope_span=[64-183] callable=Main"#]],
    );
}

#[test]
fn logical_or_assign_with_lhs_classical_true_short_circuits_evaluation() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Bool {
                use q = Qubit();
                let a = MResetZ(q) == One;
                mutable b = true;
                set b or= a;
                b
            }
        }
        "#,
    });
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
    let readout_callable_id = CallableId(2);
    assert_callable(
        &program,
        readout_callable_id,
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
    let output_record_id = CallableId(3);
    assert_callable(
        &program,
        output_record_id,
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[107-117] scope=0 scope_package_id=2 scope_span=[64-188] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[107-124] scope=0 scope_package_id=2 scope_span=[64-188] callable=Main
                Variable(1, Boolean) = Store Variable(0, Boolean) !dbg package_id=2 span=[107-124] scope=0 scope_package_id=2 scope_span=[64-188] callable=Main
                Variable(2, Boolean) = Store Variable(1, Boolean) !dbg package_id=2 span=[103-104] scope=0 scope_package_id=2 scope_span=[64-188] callable=Main
                Variable(3, Boolean) = Store Bool(true) !dbg package_id=2 span=[142-143] scope=0 scope_package_id=2 scope_span=[64-188] callable=Main
                Variable(3, Boolean) = Store Bool(true) !dbg package_id=2 span=[164-165] scope=0 scope_package_id=2 scope_span=[64-188] callable=Main
                Call id(3), args( Bool(true), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]"#]],
    );
}

#[test]
fn logical_or_assign_with_lhs_classical_false_is_optimized_as_store() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Bool {
                use q = Qubit();
                let a = MResetZ(q) == One;
                mutable b = false;
                set b or= a;
                b
            }
        }
        "#,
    });
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
    let readout_callable_id = CallableId(2);
    assert_callable(
        &program,
        readout_callable_id,
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
    let output_record_id = CallableId(3);
    assert_callable(
        &program,
        output_record_id,
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[107-117] scope=0 scope_package_id=2 scope_span=[64-189] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[107-124] scope=0 scope_package_id=2 scope_span=[64-189] callable=Main
                Variable(1, Boolean) = Store Variable(0, Boolean) !dbg package_id=2 span=[107-124] scope=0 scope_package_id=2 scope_span=[64-189] callable=Main
                Variable(2, Boolean) = Store Variable(1, Boolean) !dbg package_id=2 span=[103-104] scope=0 scope_package_id=2 scope_span=[64-189] callable=Main
                Variable(3, Boolean) = Store Bool(false) !dbg package_id=2 span=[142-143] scope=0 scope_package_id=2 scope_span=[64-189] callable=Main
                Variable(3, Boolean) = Store Variable(2, Boolean) !dbg package_id=2 span=[165-166] scope=0 scope_package_id=2 scope_span=[64-189] callable=Main
                Variable(4, Boolean) = Store Variable(3, Boolean) !dbg package_id=2 span=[182-183] scope=0 scope_package_id=2 scope_span=[64-189] callable=Main
                Call id(3), args( Variable(4, Boolean), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]"#]],
    );
}

#[test]
fn logical_or_assign_with_dynamic_lhs_and_dynamic_rhs_short_circuits_when_rhs_is_true() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Bool {
                use q = Qubit();
                mutable b = MResetZ(q) != One;
                set b or= MResetZ(q) != One;
                b
            }
        }
        "#,
    });
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
    let readout_callable_id = CallableId(2);
    assert_callable(
        &program,
        readout_callable_id,
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
    let output_record_id = CallableId(3);
    assert_callable(
        &program,
        output_record_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__read_result
                call_type: Readout
                input_type:
                    [0]: Result
                output_type: Boolean
                body: <NONE>"#]],
    );
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[111-121] scope=0 scope_package_id=2 scope_span=[64-182] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[111-128] scope=0 scope_package_id=2 scope_span=[64-182] callable=Main
                Variable(1, Boolean) = Icmp Ne, Variable(0, Boolean), Bool(true) !dbg package_id=2 span=[111-128] scope=0 scope_package_id=2 scope_span=[64-182] callable=Main
                Variable(2, Boolean) = Store Variable(1, Boolean) !dbg package_id=2 span=[107-108] scope=0 scope_package_id=2 scope_span=[64-182] callable=Main
                Variable(3, Boolean) = Store Bool(true) !dbg package_id=2 span=[148-165] scope=0 scope_package_id=2 scope_span=[64-182] callable=Main
                Branch Variable(2, Boolean), 1, 2 !dbg package_id=2 span=[148-165] scope=0 scope_package_id=2 scope_span=[64-182] callable=Main
            Block 1:Block:
                Variable(2, Boolean) = Store Variable(3, Boolean) !dbg package_id=2 span=[142-143] scope=0 scope_package_id=2 scope_span=[64-182] callable=Main
                Variable(6, Boolean) = Store Variable(2, Boolean) !dbg package_id=2 span=[175-176] scope=0 scope_package_id=2 scope_span=[64-182] callable=Main
                Call id(3), args( Variable(6, Boolean), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]
            Block 2:Block:
                Call id(1), args( Qubit(0), Result(1), ) !dbg package_id=2 span=[148-158] scope=0 scope_package_id=2 scope_span=[64-182] callable=Main
                Variable(4, Boolean) = Call id(2), args( Result(1), ) !dbg package_id=2 span=[148-165] scope=0 scope_package_id=2 scope_span=[64-182] callable=Main
                Variable(5, Boolean) = Icmp Ne, Variable(4, Boolean), Bool(true) !dbg package_id=2 span=[148-165] scope=0 scope_package_id=2 scope_span=[64-182] callable=Main
                Variable(3, Boolean) = Store Variable(5, Boolean) !dbg package_id=2 span=[148-165] scope=0 scope_package_id=2 scope_span=[64-182] callable=Main
                Jump(1) !dbg package_id=2 span=[148-165] scope=0 scope_package_id=2 scope_span=[64-182] callable=Main"#]],
    );
}

#[test]
fn integer_assign_add_with_lhs_classical_integer_and_rhs_dynamic_integer() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                mutable i = 0;
                set i += MResetZ(q) == Zero ? 0 | 1;
                i
            }
        }
        "#,
    });
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
    let readout_callable_id = CallableId(2);
    assert_callable(
        &program,
        readout_callable_id,
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
    let output_record_id = CallableId(3);
    assert_callable(
        &program,
        output_record_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__read_result
                call_type: Readout
                input_type:
                    [0]: Result
                output_type: Boolean
                body: <NONE>"#]],
    );
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Variable(0, Integer) = Store Integer(0) !dbg package_id=2 span=[106-107] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[130-140] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Variable(1, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[130-148] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Variable(2, Boolean) = Icmp Eq, Variable(1, Boolean), Bool(false) !dbg package_id=2 span=[130-148] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Branch Variable(2, Boolean), 2, 3 !dbg package_id=2 span=[130-148] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
            Block 1:Block:
                Variable(4, Integer) = Add Integer(0), Variable(3, Integer) !dbg package_id=2 span=[121-156] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Variable(0, Integer) = Store Variable(4, Integer) !dbg package_id=2 span=[125-126] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Variable(5, Integer) = Store Variable(0, Integer) !dbg package_id=2 span=[166-167] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Call id(3), args( Variable(5, Integer), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]
            Block 2:Block:
                Variable(3, Integer) = Store Integer(0) !dbg package_id=2 span=[151-152] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Jump(1) !dbg package_id=2 span=[151-152] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
            Block 3:Block:
                Variable(3, Integer) = Store Integer(1) !dbg package_id=2 span=[155-156] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Jump(1) !dbg package_id=2 span=[155-156] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main"#]],
    );
}

#[test]
fn integer_assign_sub_with_lhs_dynamic_integer_and_rhs_classical_integer() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                mutable i = MResetZ(q) == Zero ? 0 | 1;
                set i -= 1;
                i
            }
        }
        "#,
    });
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
    let readout_callable_id = CallableId(2);
    assert_callable(
        &program,
        readout_callable_id,
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
    let output_record_id = CallableId(3);
    assert_callable(
        &program,
        output_record_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__read_result
                call_type: Readout
                input_type:
                    [0]: Result
                output_type: Boolean
                body: <NONE>"#]],
    );
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[110-120] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[110-128] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Variable(1, Boolean) = Icmp Eq, Variable(0, Boolean), Bool(false) !dbg package_id=2 span=[110-128] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Branch Variable(1, Boolean), 2, 3 !dbg package_id=2 span=[110-128] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
            Block 1:Block:
                Variable(3, Integer) = Store Variable(2, Integer) !dbg package_id=2 span=[106-107] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Variable(4, Integer) = Sub Variable(3, Integer), Integer(1) !dbg package_id=2 span=[146-156] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Variable(3, Integer) = Store Variable(4, Integer) !dbg package_id=2 span=[150-151] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Variable(5, Integer) = Store Variable(3, Integer) !dbg package_id=2 span=[166-167] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Call id(3), args( Variable(5, Integer), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]
            Block 2:Block:
                Variable(2, Integer) = Store Integer(0) !dbg package_id=2 span=[131-132] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Jump(1) !dbg package_id=2 span=[131-132] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
            Block 3:Block:
                Variable(2, Integer) = Store Integer(1) !dbg package_id=2 span=[135-136] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Jump(1) !dbg package_id=2 span=[135-136] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main"#]],
    );
}

#[test]
fn integer_assign_mul_with_lhs_dynamic_integer_and_rhs_dynamic_integer() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                mutable i = MResetZ(q) == Zero ? 0 | 1;
                set i *= MResetZ(q) == Zero ? 1 | 0;
                i
            }
        }
        "#,
    });
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
    let readout_callable_id = CallableId(2);
    assert_callable(
        &program,
        readout_callable_id,
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
    let output_record_id = CallableId(3);
    assert_callable(
        &program,
        output_record_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__read_result
                call_type: Readout
                input_type:
                    [0]: Result
                output_type: Boolean
                body: <NONE>"#]],
    );
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[110-120] scope=0 scope_package_id=2 scope_span=[63-198] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[110-128] scope=0 scope_package_id=2 scope_span=[63-198] callable=Main
                Variable(1, Boolean) = Icmp Eq, Variable(0, Boolean), Bool(false) !dbg package_id=2 span=[110-128] scope=0 scope_package_id=2 scope_span=[63-198] callable=Main
                Branch Variable(1, Boolean), 2, 3 !dbg package_id=2 span=[110-128] scope=0 scope_package_id=2 scope_span=[63-198] callable=Main
            Block 1:Block:
                Variable(3, Integer) = Store Variable(2, Integer) !dbg package_id=2 span=[106-107] scope=0 scope_package_id=2 scope_span=[63-198] callable=Main
                Call id(1), args( Qubit(0), Result(1), ) !dbg package_id=2 span=[155-165] scope=0 scope_package_id=2 scope_span=[63-198] callable=Main
                Variable(4, Boolean) = Call id(2), args( Result(1), ) !dbg package_id=2 span=[155-173] scope=0 scope_package_id=2 scope_span=[63-198] callable=Main
                Variable(5, Boolean) = Icmp Eq, Variable(4, Boolean), Bool(false) !dbg package_id=2 span=[155-173] scope=0 scope_package_id=2 scope_span=[63-198] callable=Main
                Branch Variable(5, Boolean), 5, 6 !dbg package_id=2 span=[155-173] scope=0 scope_package_id=2 scope_span=[63-198] callable=Main
            Block 2:Block:
                Variable(2, Integer) = Store Integer(0) !dbg package_id=2 span=[131-132] scope=0 scope_package_id=2 scope_span=[63-198] callable=Main
                Jump(1) !dbg package_id=2 span=[131-132] scope=0 scope_package_id=2 scope_span=[63-198] callable=Main
            Block 3:Block:
                Variable(2, Integer) = Store Integer(1) !dbg package_id=2 span=[135-136] scope=0 scope_package_id=2 scope_span=[63-198] callable=Main
                Jump(1) !dbg package_id=2 span=[135-136] scope=0 scope_package_id=2 scope_span=[63-198] callable=Main
            Block 4:Block:
                Variable(7, Integer) = Mul Variable(3, Integer), Variable(6, Integer) !dbg package_id=2 span=[146-181] scope=0 scope_package_id=2 scope_span=[63-198] callable=Main
                Variable(3, Integer) = Store Variable(7, Integer) !dbg package_id=2 span=[150-151] scope=0 scope_package_id=2 scope_span=[63-198] callable=Main
                Variable(8, Integer) = Store Variable(3, Integer) !dbg package_id=2 span=[191-192] scope=0 scope_package_id=2 scope_span=[63-198] callable=Main
                Call id(3), args( Variable(8, Integer), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]
            Block 5:Block:
                Variable(6, Integer) = Store Integer(1) !dbg package_id=2 span=[176-177] scope=0 scope_package_id=2 scope_span=[63-198] callable=Main
                Jump(4) !dbg package_id=2 span=[176-177] scope=0 scope_package_id=2 scope_span=[63-198] callable=Main
            Block 6:Block:
                Variable(6, Integer) = Store Integer(0) !dbg package_id=2 span=[180-181] scope=0 scope_package_id=2 scope_span=[63-198] callable=Main
                Jump(4) !dbg package_id=2 span=[180-181] scope=0 scope_package_id=2 scope_span=[63-198] callable=Main"#]],
    );
}

#[test]
fn integer_assign_div_with_lhs_classical_integer_and_rhs_dynamic_integer() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                mutable i = 0;
                set i /= MResetZ(q) == Zero ? 0 | 1;
                i
            }
        }
        "#,
    });
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
    let readout_callable_id = CallableId(2);
    assert_callable(
        &program,
        readout_callable_id,
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
    let output_record_id = CallableId(3);
    assert_callable(
        &program,
        output_record_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__read_result
                call_type: Readout
                input_type:
                    [0]: Result
                output_type: Boolean
                body: <NONE>"#]],
    );
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Variable(0, Integer) = Store Integer(0) !dbg package_id=2 span=[106-107] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[130-140] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Variable(1, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[130-148] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Variable(2, Boolean) = Icmp Eq, Variable(1, Boolean), Bool(false) !dbg package_id=2 span=[130-148] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Branch Variable(2, Boolean), 2, 3 !dbg package_id=2 span=[130-148] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
            Block 1:Block:
                Variable(4, Integer) = Sdiv Integer(0), Variable(3, Integer) !dbg package_id=2 span=[121-156] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Variable(0, Integer) = Store Variable(4, Integer) !dbg package_id=2 span=[125-126] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Variable(5, Integer) = Store Variable(0, Integer) !dbg package_id=2 span=[166-167] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Call id(3), args( Variable(5, Integer), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]
            Block 2:Block:
                Variable(3, Integer) = Store Integer(0) !dbg package_id=2 span=[151-152] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Jump(1) !dbg package_id=2 span=[151-152] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
            Block 3:Block:
                Variable(3, Integer) = Store Integer(1) !dbg package_id=2 span=[155-156] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Jump(1) !dbg package_id=2 span=[155-156] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main"#]],
    );
}

#[test]
fn integer_assign_mod_with_lhs_dynamic_integer_and_rhs_classical_integer() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                mutable i = MResetZ(q) == Zero ? 0 | 1;
                set i %= 1;
                i
            }
        }
        "#,
    });
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
    let readout_callable_id = CallableId(2);
    assert_callable(
        &program,
        readout_callable_id,
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
    let output_record_id = CallableId(3);
    assert_callable(
        &program,
        output_record_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__read_result
                call_type: Readout
                input_type:
                    [0]: Result
                output_type: Boolean
                body: <NONE>"#]],
    );
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[110-120] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[110-128] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Variable(1, Boolean) = Icmp Eq, Variable(0, Boolean), Bool(false) !dbg package_id=2 span=[110-128] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Branch Variable(1, Boolean), 2, 3 !dbg package_id=2 span=[110-128] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
            Block 1:Block:
                Variable(3, Integer) = Store Variable(2, Integer) !dbg package_id=2 span=[106-107] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Variable(4, Integer) = Srem Variable(3, Integer), Integer(1) !dbg package_id=2 span=[146-156] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Variable(3, Integer) = Store Variable(4, Integer) !dbg package_id=2 span=[150-151] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Variable(5, Integer) = Store Variable(3, Integer) !dbg package_id=2 span=[166-167] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Call id(3), args( Variable(5, Integer), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]
            Block 2:Block:
                Variable(2, Integer) = Store Integer(0) !dbg package_id=2 span=[131-132] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Jump(1) !dbg package_id=2 span=[131-132] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
            Block 3:Block:
                Variable(2, Integer) = Store Integer(1) !dbg package_id=2 span=[135-136] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Jump(1) !dbg package_id=2 span=[135-136] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main"#]],
    );
}

#[test]
fn integer_assign_exp_with_lhs_classical_integer_and_rhs_dynamic_integer_raises_error() {
    let error = get_partial_evaluation_error(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                mutable i = 0;
                set i ^= MResetZ(q) == Zero ? 0 | 1;
                i
            }
        }
        "#,
    });
    assert_error(
        &error,
        &expect![[
            r#"Unexpected("exponent must be a classical integer", PackageSpan { package: PackageId(2), span: Span { lo: 121, hi: 156 } })"#
        ]],
    );
}

#[test]
fn integer_assign_exp_with_lhs_classical_integer_and_rhs_classical_negative_integer_raises_error() {
    let error = get_partial_evaluation_error(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                mutable i = 0;
                set i ^= -1;
                i
            }
        }
        "#,
    });
    assert_error(
        &error,
        &expect![[
            r#"EvaluationFailed("negative integers cannot be used here: -1", PackageSpan { package: PackageId(2), span: Span { lo: 130, hi: 132 } })"#
        ]],
    );
}

#[test]
fn integer_assign_exp_with_lhs_dynamic_integer_and_rhs_classical_zero_integer() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                mutable i = MResetZ(q) == Zero ? 0 | 1;
                set i ^= 0;
                i
            }
        }
        "#,
    });
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
    let readout_callable_id = CallableId(2);
    assert_callable(
        &program,
        readout_callable_id,
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
    let output_record_id = CallableId(3);
    assert_callable(
        &program,
        output_record_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__read_result
                call_type: Readout
                input_type:
                    [0]: Result
                output_type: Boolean
                body: <NONE>"#]],
    );
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[110-120] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[110-128] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Variable(1, Boolean) = Icmp Eq, Variable(0, Boolean), Bool(false) !dbg package_id=2 span=[110-128] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Branch Variable(1, Boolean), 2, 3 !dbg package_id=2 span=[110-128] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
            Block 1:Block:
                Variable(3, Integer) = Store Variable(2, Integer) !dbg package_id=2 span=[106-107] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Variable(4, Integer) = Store Integer(1) !dbg package_id=2 span=[146-156] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Variable(3, Integer) = Store Variable(4, Integer) !dbg package_id=2 span=[150-151] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Variable(5, Integer) = Store Variable(3, Integer) !dbg package_id=2 span=[166-167] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Call id(3), args( Variable(5, Integer), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]
            Block 2:Block:
                Variable(2, Integer) = Store Integer(0) !dbg package_id=2 span=[131-132] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Jump(1) !dbg package_id=2 span=[131-132] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
            Block 3:Block:
                Variable(2, Integer) = Store Integer(1) !dbg package_id=2 span=[135-136] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Jump(1) !dbg package_id=2 span=[135-136] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main"#]],
    );
}

#[test]
fn integer_assign_exp_with_lhs_dynamic_integer_and_rhs_classical_positive_integer() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                mutable i = MResetZ(q) == Zero ? 0 | 1;
                set i ^= 3;
                i
            }
        }
        "#,
    });
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
    let readout_callable_id = CallableId(2);
    assert_callable(
        &program,
        readout_callable_id,
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
    let output_record_id = CallableId(3);
    assert_callable(
        &program,
        output_record_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__read_result
                call_type: Readout
                input_type:
                    [0]: Result
                output_type: Boolean
                body: <NONE>"#]],
    );
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[110-120] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[110-128] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Variable(1, Boolean) = Icmp Eq, Variable(0, Boolean), Bool(false) !dbg package_id=2 span=[110-128] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Branch Variable(1, Boolean), 2, 3 !dbg package_id=2 span=[110-128] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
            Block 1:Block:
                Variable(3, Integer) = Store Variable(2, Integer) !dbg package_id=2 span=[106-107] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Variable(4, Integer) = Store Integer(1) !dbg package_id=2 span=[146-156] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Variable(5, Integer) = Mul Variable(4, Integer), Variable(3, Integer) !dbg package_id=2 span=[146-156] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Variable(6, Integer) = Mul Variable(5, Integer), Variable(3, Integer) !dbg package_id=2 span=[146-156] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Variable(7, Integer) = Mul Variable(6, Integer), Variable(3, Integer) !dbg package_id=2 span=[146-156] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Variable(3, Integer) = Store Variable(7, Integer) !dbg package_id=2 span=[150-151] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Variable(8, Integer) = Store Variable(3, Integer) !dbg package_id=2 span=[166-167] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Call id(3), args( Variable(8, Integer), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]
            Block 2:Block:
                Variable(2, Integer) = Store Integer(0) !dbg package_id=2 span=[131-132] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Jump(1) !dbg package_id=2 span=[131-132] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
            Block 3:Block:
                Variable(2, Integer) = Store Integer(1) !dbg package_id=2 span=[135-136] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main
                Jump(1) !dbg package_id=2 span=[135-136] scope=0 scope_package_id=2 scope_span=[63-173] callable=Main"#]],
    );
}

#[test]
fn integer_assign_exp_with_lhs_dynamic_integer_and_rhs_dynamic_integer_raises_error() {
    let error = get_partial_evaluation_error(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                mutable i = MResetZ(q) == Zero ? 0 | 1;
                set i ^= MResetZ(q) == Zero ? 1 | 0;
                i
            }
        }
        "#,
    });
    assert_error(
        &error,
        &expect![[
            r#"Unexpected("exponent must be a classical integer", PackageSpan { package: PackageId(2), span: Span { lo: 146, hi: 181 } })"#
        ]],
    );
}

#[test]
fn integer_assign_bitwise_and_with_lhs_dynamic_integer_and_rhs_dynamic_integer() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                mutable i = MResetZ(q) == Zero ? 0 | 1;
                set i &&&= MResetZ(q) == Zero ? 1 | 0;
                i
            }
        }
        "#,
    });
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
    let readout_callable_id = CallableId(2);
    assert_callable(
        &program,
        readout_callable_id,
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
    let output_record_id = CallableId(3);
    assert_callable(
        &program,
        output_record_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__read_result
                call_type: Readout
                input_type:
                    [0]: Result
                output_type: Boolean
                body: <NONE>"#]],
    );
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[110-120] scope=0 scope_package_id=2 scope_span=[63-200] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[110-128] scope=0 scope_package_id=2 scope_span=[63-200] callable=Main
                Variable(1, Boolean) = Icmp Eq, Variable(0, Boolean), Bool(false) !dbg package_id=2 span=[110-128] scope=0 scope_package_id=2 scope_span=[63-200] callable=Main
                Branch Variable(1, Boolean), 2, 3 !dbg package_id=2 span=[110-128] scope=0 scope_package_id=2 scope_span=[63-200] callable=Main
            Block 1:Block:
                Variable(3, Integer) = Store Variable(2, Integer) !dbg package_id=2 span=[106-107] scope=0 scope_package_id=2 scope_span=[63-200] callable=Main
                Call id(1), args( Qubit(0), Result(1), ) !dbg package_id=2 span=[157-167] scope=0 scope_package_id=2 scope_span=[63-200] callable=Main
                Variable(4, Boolean) = Call id(2), args( Result(1), ) !dbg package_id=2 span=[157-175] scope=0 scope_package_id=2 scope_span=[63-200] callable=Main
                Variable(5, Boolean) = Icmp Eq, Variable(4, Boolean), Bool(false) !dbg package_id=2 span=[157-175] scope=0 scope_package_id=2 scope_span=[63-200] callable=Main
                Branch Variable(5, Boolean), 5, 6 !dbg package_id=2 span=[157-175] scope=0 scope_package_id=2 scope_span=[63-200] callable=Main
            Block 2:Block:
                Variable(2, Integer) = Store Integer(0) !dbg package_id=2 span=[131-132] scope=0 scope_package_id=2 scope_span=[63-200] callable=Main
                Jump(1) !dbg package_id=2 span=[131-132] scope=0 scope_package_id=2 scope_span=[63-200] callable=Main
            Block 3:Block:
                Variable(2, Integer) = Store Integer(1) !dbg package_id=2 span=[135-136] scope=0 scope_package_id=2 scope_span=[63-200] callable=Main
                Jump(1) !dbg package_id=2 span=[135-136] scope=0 scope_package_id=2 scope_span=[63-200] callable=Main
            Block 4:Block:
                Variable(7, Integer) = BitwiseAnd Variable(3, Integer), Variable(6, Integer) !dbg package_id=2 span=[146-183] scope=0 scope_package_id=2 scope_span=[63-200] callable=Main
                Variable(3, Integer) = Store Variable(7, Integer) !dbg package_id=2 span=[150-151] scope=0 scope_package_id=2 scope_span=[63-200] callable=Main
                Variable(8, Integer) = Store Variable(3, Integer) !dbg package_id=2 span=[193-194] scope=0 scope_package_id=2 scope_span=[63-200] callable=Main
                Call id(3), args( Variable(8, Integer), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]
            Block 5:Block:
                Variable(6, Integer) = Store Integer(1) !dbg package_id=2 span=[178-179] scope=0 scope_package_id=2 scope_span=[63-200] callable=Main
                Jump(4) !dbg package_id=2 span=[178-179] scope=0 scope_package_id=2 scope_span=[63-200] callable=Main
            Block 6:Block:
                Variable(6, Integer) = Store Integer(0) !dbg package_id=2 span=[182-183] scope=0 scope_package_id=2 scope_span=[63-200] callable=Main
                Jump(4) !dbg package_id=2 span=[182-183] scope=0 scope_package_id=2 scope_span=[63-200] callable=Main"#]],
    );
}

#[test]
fn integer_assign_bitwise_or_with_lhs_classical_integer_and_rhs_dynamic_integer() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                mutable i = 0;
                set i |||= MResetZ(q) == Zero ? 0 | 1;
                i
            }
        }
        "#,
    });
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
    let readout_callable_id = CallableId(2);
    assert_callable(
        &program,
        readout_callable_id,
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
    let output_record_id = CallableId(3);
    assert_callable(
        &program,
        output_record_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__read_result
                call_type: Readout
                input_type:
                    [0]: Result
                output_type: Boolean
                body: <NONE>"#]],
    );
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Variable(0, Integer) = Store Integer(0) !dbg package_id=2 span=[106-107] scope=0 scope_package_id=2 scope_span=[63-175] callable=Main
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[132-142] scope=0 scope_package_id=2 scope_span=[63-175] callable=Main
                Variable(1, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[132-150] scope=0 scope_package_id=2 scope_span=[63-175] callable=Main
                Variable(2, Boolean) = Icmp Eq, Variable(1, Boolean), Bool(false) !dbg package_id=2 span=[132-150] scope=0 scope_package_id=2 scope_span=[63-175] callable=Main
                Branch Variable(2, Boolean), 2, 3 !dbg package_id=2 span=[132-150] scope=0 scope_package_id=2 scope_span=[63-175] callable=Main
            Block 1:Block:
                Variable(4, Integer) = BitwiseOr Integer(0), Variable(3, Integer) !dbg package_id=2 span=[121-158] scope=0 scope_package_id=2 scope_span=[63-175] callable=Main
                Variable(0, Integer) = Store Variable(4, Integer) !dbg package_id=2 span=[125-126] scope=0 scope_package_id=2 scope_span=[63-175] callable=Main
                Variable(5, Integer) = Store Variable(0, Integer) !dbg package_id=2 span=[168-169] scope=0 scope_package_id=2 scope_span=[63-175] callable=Main
                Call id(3), args( Variable(5, Integer), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]
            Block 2:Block:
                Variable(3, Integer) = Store Integer(0) !dbg package_id=2 span=[153-154] scope=0 scope_package_id=2 scope_span=[63-175] callable=Main
                Jump(1) !dbg package_id=2 span=[153-154] scope=0 scope_package_id=2 scope_span=[63-175] callable=Main
            Block 3:Block:
                Variable(3, Integer) = Store Integer(1) !dbg package_id=2 span=[157-158] scope=0 scope_package_id=2 scope_span=[63-175] callable=Main
                Jump(1) !dbg package_id=2 span=[157-158] scope=0 scope_package_id=2 scope_span=[63-175] callable=Main"#]],
    );
}

#[test]
fn integer_bitwise_xor_with_lhs_dynamic_integer_and_rhs_classical_integer() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                mutable i = MResetZ(q) == Zero ? 0 | 1;
                set i ^^^= 1;
                i
            }
        }
        "#,
    });
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
    let readout_callable_id = CallableId(2);
    assert_callable(
        &program,
        readout_callable_id,
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
    let output_record_id = CallableId(3);
    assert_callable(
        &program,
        output_record_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__read_result
                call_type: Readout
                input_type:
                    [0]: Result
                output_type: Boolean
                body: <NONE>"#]],
    );
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[110-120] scope=0 scope_package_id=2 scope_span=[63-175] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[110-128] scope=0 scope_package_id=2 scope_span=[63-175] callable=Main
                Variable(1, Boolean) = Icmp Eq, Variable(0, Boolean), Bool(false) !dbg package_id=2 span=[110-128] scope=0 scope_package_id=2 scope_span=[63-175] callable=Main
                Branch Variable(1, Boolean), 2, 3 !dbg package_id=2 span=[110-128] scope=0 scope_package_id=2 scope_span=[63-175] callable=Main
            Block 1:Block:
                Variable(3, Integer) = Store Variable(2, Integer) !dbg package_id=2 span=[106-107] scope=0 scope_package_id=2 scope_span=[63-175] callable=Main
                Variable(4, Integer) = BitwiseXor Variable(3, Integer), Integer(1) !dbg package_id=2 span=[146-158] scope=0 scope_package_id=2 scope_span=[63-175] callable=Main
                Variable(3, Integer) = Store Variable(4, Integer) !dbg package_id=2 span=[150-151] scope=0 scope_package_id=2 scope_span=[63-175] callable=Main
                Variable(5, Integer) = Store Variable(3, Integer) !dbg package_id=2 span=[168-169] scope=0 scope_package_id=2 scope_span=[63-175] callable=Main
                Call id(3), args( Variable(5, Integer), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]
            Block 2:Block:
                Variable(2, Integer) = Store Integer(0) !dbg package_id=2 span=[131-132] scope=0 scope_package_id=2 scope_span=[63-175] callable=Main
                Jump(1) !dbg package_id=2 span=[131-132] scope=0 scope_package_id=2 scope_span=[63-175] callable=Main
            Block 3:Block:
                Variable(2, Integer) = Store Integer(1) !dbg package_id=2 span=[135-136] scope=0 scope_package_id=2 scope_span=[63-175] callable=Main
                Jump(1) !dbg package_id=2 span=[135-136] scope=0 scope_package_id=2 scope_span=[63-175] callable=Main"#]],
    );
}

#[test]
fn integer_assign_bitwise_left_shift_with_lhs_dynamic_integer_and_rhs_dynamic_integer() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                mutable i = MResetZ(q) == Zero ? 0 | 1;
                set i <<<= MResetZ(q) == Zero ? 1 | 0;
                i
            }
        }
        "#,
    });
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
    let readout_callable_id = CallableId(2);
    assert_callable(
        &program,
        readout_callable_id,
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
    let output_record_id = CallableId(3);
    assert_callable(
        &program,
        output_record_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__read_result
                call_type: Readout
                input_type:
                    [0]: Result
                output_type: Boolean
                body: <NONE>"#]],
    );
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[110-120] scope=0 scope_package_id=2 scope_span=[63-200] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[110-128] scope=0 scope_package_id=2 scope_span=[63-200] callable=Main
                Variable(1, Boolean) = Icmp Eq, Variable(0, Boolean), Bool(false) !dbg package_id=2 span=[110-128] scope=0 scope_package_id=2 scope_span=[63-200] callable=Main
                Branch Variable(1, Boolean), 2, 3 !dbg package_id=2 span=[110-128] scope=0 scope_package_id=2 scope_span=[63-200] callable=Main
            Block 1:Block:
                Variable(3, Integer) = Store Variable(2, Integer) !dbg package_id=2 span=[106-107] scope=0 scope_package_id=2 scope_span=[63-200] callable=Main
                Call id(1), args( Qubit(0), Result(1), ) !dbg package_id=2 span=[157-167] scope=0 scope_package_id=2 scope_span=[63-200] callable=Main
                Variable(4, Boolean) = Call id(2), args( Result(1), ) !dbg package_id=2 span=[157-175] scope=0 scope_package_id=2 scope_span=[63-200] callable=Main
                Variable(5, Boolean) = Icmp Eq, Variable(4, Boolean), Bool(false) !dbg package_id=2 span=[157-175] scope=0 scope_package_id=2 scope_span=[63-200] callable=Main
                Branch Variable(5, Boolean), 5, 6 !dbg package_id=2 span=[157-175] scope=0 scope_package_id=2 scope_span=[63-200] callable=Main
            Block 2:Block:
                Variable(2, Integer) = Store Integer(0) !dbg package_id=2 span=[131-132] scope=0 scope_package_id=2 scope_span=[63-200] callable=Main
                Jump(1) !dbg package_id=2 span=[131-132] scope=0 scope_package_id=2 scope_span=[63-200] callable=Main
            Block 3:Block:
                Variable(2, Integer) = Store Integer(1) !dbg package_id=2 span=[135-136] scope=0 scope_package_id=2 scope_span=[63-200] callable=Main
                Jump(1) !dbg package_id=2 span=[135-136] scope=0 scope_package_id=2 scope_span=[63-200] callable=Main
            Block 4:Block:
                Variable(7, Integer) = Shl Variable(3, Integer), Variable(6, Integer) !dbg package_id=2 span=[146-183] scope=0 scope_package_id=2 scope_span=[63-200] callable=Main
                Variable(3, Integer) = Store Variable(7, Integer) !dbg package_id=2 span=[150-151] scope=0 scope_package_id=2 scope_span=[63-200] callable=Main
                Variable(8, Integer) = Store Variable(3, Integer) !dbg package_id=2 span=[193-194] scope=0 scope_package_id=2 scope_span=[63-200] callable=Main
                Call id(3), args( Variable(8, Integer), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]
            Block 5:Block:
                Variable(6, Integer) = Store Integer(1) !dbg package_id=2 span=[178-179] scope=0 scope_package_id=2 scope_span=[63-200] callable=Main
                Jump(4) !dbg package_id=2 span=[178-179] scope=0 scope_package_id=2 scope_span=[63-200] callable=Main
            Block 6:Block:
                Variable(6, Integer) = Store Integer(0) !dbg package_id=2 span=[182-183] scope=0 scope_package_id=2 scope_span=[63-200] callable=Main
                Jump(4) !dbg package_id=2 span=[182-183] scope=0 scope_package_id=2 scope_span=[63-200] callable=Main"#]],
    );
}

#[test]
fn integer_assign_bitwise_right_shift_with_lhs_classical_integer_and_rhs_dynamic_integer() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                mutable i = 0;
                set i >>>= MResetZ(q) == Zero ? 0 | 1;
                i
            }
        }
        "#,
    });
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
    let readout_callable_id = CallableId(2);
    assert_callable(
        &program,
        readout_callable_id,
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
    let output_record_id = CallableId(3);
    assert_callable(
        &program,
        output_record_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__read_result
                call_type: Readout
                input_type:
                    [0]: Result
                output_type: Boolean
                body: <NONE>"#]],
    );
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Variable(0, Integer) = Store Integer(0) !dbg package_id=2 span=[106-107] scope=0 scope_package_id=2 scope_span=[63-175] callable=Main
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[132-142] scope=0 scope_package_id=2 scope_span=[63-175] callable=Main
                Variable(1, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[132-150] scope=0 scope_package_id=2 scope_span=[63-175] callable=Main
                Variable(2, Boolean) = Icmp Eq, Variable(1, Boolean), Bool(false) !dbg package_id=2 span=[132-150] scope=0 scope_package_id=2 scope_span=[63-175] callable=Main
                Branch Variable(2, Boolean), 2, 3 !dbg package_id=2 span=[132-150] scope=0 scope_package_id=2 scope_span=[63-175] callable=Main
            Block 1:Block:
                Variable(4, Integer) = Ashr Integer(0), Variable(3, Integer) !dbg package_id=2 span=[121-158] scope=0 scope_package_id=2 scope_span=[63-175] callable=Main
                Variable(0, Integer) = Store Variable(4, Integer) !dbg package_id=2 span=[125-126] scope=0 scope_package_id=2 scope_span=[63-175] callable=Main
                Variable(5, Integer) = Store Variable(0, Integer) !dbg package_id=2 span=[168-169] scope=0 scope_package_id=2 scope_span=[63-175] callable=Main
                Call id(3), args( Variable(5, Integer), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]
            Block 2:Block:
                Variable(3, Integer) = Store Integer(0) !dbg package_id=2 span=[153-154] scope=0 scope_package_id=2 scope_span=[63-175] callable=Main
                Jump(1) !dbg package_id=2 span=[153-154] scope=0 scope_package_id=2 scope_span=[63-175] callable=Main
            Block 3:Block:
                Variable(3, Integer) = Store Integer(1) !dbg package_id=2 span=[157-158] scope=0 scope_package_id=2 scope_span=[63-175] callable=Main
                Jump(1) !dbg package_id=2 span=[157-158] scope=0 scope_package_id=2 scope_span=[63-175] callable=Main"#]],
    );
}

#[test]
fn double_assign_add_with_lhs_classical_double_and_rhs_dynamic_double() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Double {
                use q = Qubit();
                mutable i = 0.0;
                set i += MResetZ(q) == Zero ? 0.0 | 1.0;
                i
            }
        }
        "#,
    });
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
    let readout_callable_id = CallableId(2);
    assert_callable(
        &program,
        readout_callable_id,
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
    let output_record_id = CallableId(3);
    assert_callable(
        &program,
        output_record_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__read_result
                call_type: Readout
                input_type:
                    [0]: Result
                output_type: Boolean
                body: <NONE>"#]],
    );
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Variable(0, Double) = Store Double(0) !dbg package_id=2 span=[109-110] scope=0 scope_package_id=2 scope_span=[66-182] callable=Main
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[135-145] scope=0 scope_package_id=2 scope_span=[66-182] callable=Main
                Variable(1, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[135-153] scope=0 scope_package_id=2 scope_span=[66-182] callable=Main
                Variable(2, Boolean) = Icmp Eq, Variable(1, Boolean), Bool(false) !dbg package_id=2 span=[135-153] scope=0 scope_package_id=2 scope_span=[66-182] callable=Main
                Branch Variable(2, Boolean), 2, 3 !dbg package_id=2 span=[135-153] scope=0 scope_package_id=2 scope_span=[66-182] callable=Main
            Block 1:Block:
                Variable(4, Double) = Fadd Double(0), Variable(3, Double) !dbg package_id=2 span=[126-165] scope=0 scope_package_id=2 scope_span=[66-182] callable=Main
                Variable(0, Double) = Store Variable(4, Double) !dbg package_id=2 span=[130-131] scope=0 scope_package_id=2 scope_span=[66-182] callable=Main
                Variable(5, Double) = Store Variable(0, Double) !dbg package_id=2 span=[175-176] scope=0 scope_package_id=2 scope_span=[66-182] callable=Main
                Call id(3), args( Variable(5, Double), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]
            Block 2:Block:
                Variable(3, Double) = Store Double(0) !dbg package_id=2 span=[156-159] scope=0 scope_package_id=2 scope_span=[66-182] callable=Main
                Jump(1) !dbg package_id=2 span=[156-159] scope=0 scope_package_id=2 scope_span=[66-182] callable=Main
            Block 3:Block:
                Variable(3, Double) = Store Double(1) !dbg package_id=2 span=[162-165] scope=0 scope_package_id=2 scope_span=[66-182] callable=Main
                Jump(1) !dbg package_id=2 span=[162-165] scope=0 scope_package_id=2 scope_span=[66-182] callable=Main"#]],
    );
}

#[test]
fn double_assign_sub_with_lhs_dynamic_double_and_rhs_classical_double() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Double {
                use q = Qubit();
                mutable i = MResetZ(q) == Zero ? 0.0 | 1.0;
                set i -= 1.0;
                i
            }
        }
        "#,
    });
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
    let readout_callable_id = CallableId(2);
    assert_callable(
        &program,
        readout_callable_id,
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
    let output_record_id = CallableId(3);
    assert_callable(
        &program,
        output_record_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__read_result
                call_type: Readout
                input_type:
                    [0]: Result
                output_type: Boolean
                body: <NONE>"#]],
    );
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[113-123] scope=0 scope_package_id=2 scope_span=[66-182] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[113-131] scope=0 scope_package_id=2 scope_span=[66-182] callable=Main
                Variable(1, Boolean) = Icmp Eq, Variable(0, Boolean), Bool(false) !dbg package_id=2 span=[113-131] scope=0 scope_package_id=2 scope_span=[66-182] callable=Main
                Branch Variable(1, Boolean), 2, 3 !dbg package_id=2 span=[113-131] scope=0 scope_package_id=2 scope_span=[66-182] callable=Main
            Block 1:Block:
                Variable(3, Double) = Store Variable(2, Double) !dbg package_id=2 span=[109-110] scope=0 scope_package_id=2 scope_span=[66-182] callable=Main
                Variable(4, Double) = Fsub Variable(3, Double), Double(1) !dbg package_id=2 span=[153-165] scope=0 scope_package_id=2 scope_span=[66-182] callable=Main
                Variable(3, Double) = Store Variable(4, Double) !dbg package_id=2 span=[157-158] scope=0 scope_package_id=2 scope_span=[66-182] callable=Main
                Variable(5, Double) = Store Variable(3, Double) !dbg package_id=2 span=[175-176] scope=0 scope_package_id=2 scope_span=[66-182] callable=Main
                Call id(3), args( Variable(5, Double), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]
            Block 2:Block:
                Variable(2, Double) = Store Double(0) !dbg package_id=2 span=[134-137] scope=0 scope_package_id=2 scope_span=[66-182] callable=Main
                Jump(1) !dbg package_id=2 span=[134-137] scope=0 scope_package_id=2 scope_span=[66-182] callable=Main
            Block 3:Block:
                Variable(2, Double) = Store Double(1) !dbg package_id=2 span=[140-143] scope=0 scope_package_id=2 scope_span=[66-182] callable=Main
                Jump(1) !dbg package_id=2 span=[140-143] scope=0 scope_package_id=2 scope_span=[66-182] callable=Main"#]],
    );
}

#[test]
fn double_assign_mul_with_lhs_dynamic_double_and_rhs_dynamic_double() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Double {
                use q = Qubit();
                mutable i = MResetZ(q) == Zero ? 0.0 | 1.0;
                set i *= MResetZ(q) == Zero ? 1.1 | 0.1;
                i
            }
        }
        "#,
    });
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
    let readout_callable_id = CallableId(2);
    assert_callable(
        &program,
        readout_callable_id,
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
    let output_record_id = CallableId(3);
    assert_callable(
        &program,
        output_record_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__read_result
                call_type: Readout
                input_type:
                    [0]: Result
                output_type: Boolean
                body: <NONE>"#]],
    );
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[113-123] scope=0 scope_package_id=2 scope_span=[66-209] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[113-131] scope=0 scope_package_id=2 scope_span=[66-209] callable=Main
                Variable(1, Boolean) = Icmp Eq, Variable(0, Boolean), Bool(false) !dbg package_id=2 span=[113-131] scope=0 scope_package_id=2 scope_span=[66-209] callable=Main
                Branch Variable(1, Boolean), 2, 3 !dbg package_id=2 span=[113-131] scope=0 scope_package_id=2 scope_span=[66-209] callable=Main
            Block 1:Block:
                Variable(3, Double) = Store Variable(2, Double) !dbg package_id=2 span=[109-110] scope=0 scope_package_id=2 scope_span=[66-209] callable=Main
                Call id(1), args( Qubit(0), Result(1), ) !dbg package_id=2 span=[162-172] scope=0 scope_package_id=2 scope_span=[66-209] callable=Main
                Variable(4, Boolean) = Call id(2), args( Result(1), ) !dbg package_id=2 span=[162-180] scope=0 scope_package_id=2 scope_span=[66-209] callable=Main
                Variable(5, Boolean) = Icmp Eq, Variable(4, Boolean), Bool(false) !dbg package_id=2 span=[162-180] scope=0 scope_package_id=2 scope_span=[66-209] callable=Main
                Branch Variable(5, Boolean), 5, 6 !dbg package_id=2 span=[162-180] scope=0 scope_package_id=2 scope_span=[66-209] callable=Main
            Block 2:Block:
                Variable(2, Double) = Store Double(0) !dbg package_id=2 span=[134-137] scope=0 scope_package_id=2 scope_span=[66-209] callable=Main
                Jump(1) !dbg package_id=2 span=[134-137] scope=0 scope_package_id=2 scope_span=[66-209] callable=Main
            Block 3:Block:
                Variable(2, Double) = Store Double(1) !dbg package_id=2 span=[140-143] scope=0 scope_package_id=2 scope_span=[66-209] callable=Main
                Jump(1) !dbg package_id=2 span=[140-143] scope=0 scope_package_id=2 scope_span=[66-209] callable=Main
            Block 4:Block:
                Variable(7, Double) = Fmul Variable(3, Double), Variable(6, Double) !dbg package_id=2 span=[153-192] scope=0 scope_package_id=2 scope_span=[66-209] callable=Main
                Variable(3, Double) = Store Variable(7, Double) !dbg package_id=2 span=[157-158] scope=0 scope_package_id=2 scope_span=[66-209] callable=Main
                Variable(8, Double) = Store Variable(3, Double) !dbg package_id=2 span=[202-203] scope=0 scope_package_id=2 scope_span=[66-209] callable=Main
                Call id(3), args( Variable(8, Double), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]
            Block 5:Block:
                Variable(6, Double) = Store Double(1.1) !dbg package_id=2 span=[183-186] scope=0 scope_package_id=2 scope_span=[66-209] callable=Main
                Jump(4) !dbg package_id=2 span=[183-186] scope=0 scope_package_id=2 scope_span=[66-209] callable=Main
            Block 6:Block:
                Variable(6, Double) = Store Double(0.1) !dbg package_id=2 span=[189-192] scope=0 scope_package_id=2 scope_span=[66-209] callable=Main
                Jump(4) !dbg package_id=2 span=[189-192] scope=0 scope_package_id=2 scope_span=[66-209] callable=Main"#]],
    );
}

#[test]
fn double_assign_div_with_lhs_classical_double_and_rhs_dynamic_double() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Double {
                use q = Qubit();
                mutable i = 0.0;
                set i /= MResetZ(q) == Zero ? 0.0 | 1.0;
                i
            }
        }
        "#,
    });
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
    let readout_callable_id = CallableId(2);
    assert_callable(
        &program,
        readout_callable_id,
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
    let output_record_id = CallableId(3);
    assert_callable(
        &program,
        output_record_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__read_result
                call_type: Readout
                input_type:
                    [0]: Result
                output_type: Boolean
                body: <NONE>"#]],
    );
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Variable(0, Double) = Store Double(0) !dbg package_id=2 span=[109-110] scope=0 scope_package_id=2 scope_span=[66-182] callable=Main
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[135-145] scope=0 scope_package_id=2 scope_span=[66-182] callable=Main
                Variable(1, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[135-153] scope=0 scope_package_id=2 scope_span=[66-182] callable=Main
                Variable(2, Boolean) = Icmp Eq, Variable(1, Boolean), Bool(false) !dbg package_id=2 span=[135-153] scope=0 scope_package_id=2 scope_span=[66-182] callable=Main
                Branch Variable(2, Boolean), 2, 3 !dbg package_id=2 span=[135-153] scope=0 scope_package_id=2 scope_span=[66-182] callable=Main
            Block 1:Block:
                Variable(4, Double) = Fdiv Double(0), Variable(3, Double) !dbg package_id=2 span=[126-165] scope=0 scope_package_id=2 scope_span=[66-182] callable=Main
                Variable(0, Double) = Store Variable(4, Double) !dbg package_id=2 span=[130-131] scope=0 scope_package_id=2 scope_span=[66-182] callable=Main
                Variable(5, Double) = Store Variable(0, Double) !dbg package_id=2 span=[175-176] scope=0 scope_package_id=2 scope_span=[66-182] callable=Main
                Call id(3), args( Variable(5, Double), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]
            Block 2:Block:
                Variable(3, Double) = Store Double(0) !dbg package_id=2 span=[156-159] scope=0 scope_package_id=2 scope_span=[66-182] callable=Main
                Jump(1) !dbg package_id=2 span=[156-159] scope=0 scope_package_id=2 scope_span=[66-182] callable=Main
            Block 3:Block:
                Variable(3, Double) = Store Double(1) !dbg package_id=2 span=[162-165] scope=0 scope_package_id=2 scope_span=[66-182] callable=Main
                Jump(1) !dbg package_id=2 span=[162-165] scope=0 scope_package_id=2 scope_span=[66-182] callable=Main"#]],
    );
}
