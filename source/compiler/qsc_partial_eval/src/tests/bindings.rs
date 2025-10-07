// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(
    clippy::needless_raw_string_hashes,
    clippy::similar_names,
    clippy::too_many_lines
)]

use super::{assert_block_instructions, assert_blocks, assert_callable, get_rir_program};
use expect_test::expect;
use indoc::indoc;
use qsc_rir::rir::{BlockId, CallableId};

#[test]
fn immutable_result_binding_does_not_generate_store_instruction() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                let r = MResetZ(q);
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[109-119] scope=0 scope_package_id=2 scope_span=[66-136] callable=Main
                Call id(2), args( Result(0), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]"#]],
    );
}

#[test]
fn mutable_result_binding_does_not_generate_store_instruction() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                mutable r = MResetZ(q);
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[113-123] scope=0 scope_package_id=2 scope_span=[66-140] callable=Main
                Call id(2), args( Result(0), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]"#]],
    );
}

#[test]
fn immutable_bool_binding_does_not_generate_store_instruction() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Bool {
                use q = Qubit();
                let b = MResetZ(q) == One;
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
    let read_result_callable_id = CallableId(2);
    assert_callable(
        &program,
        read_result_callable_id,
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[107-117] scope=0 scope_package_id=2 scope_span=[64-141] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[107-124] scope=0 scope_package_id=2 scope_span=[64-141] callable=Main
                Variable(1, Boolean) = Store Variable(0, Boolean) !dbg package_id=2 span=[107-124] scope=0 scope_package_id=2 scope_span=[64-141] callable=Main
                Variable(2, Boolean) = Store Variable(1, Boolean) !dbg package_id=2 span=[103-104] scope=0 scope_package_id=2 scope_span=[64-141] callable=Main
                Variable(3, Boolean) = Store Variable(2, Boolean) !dbg package_id=2 span=[134-135] scope=0 scope_package_id=2 scope_span=[64-141] callable=Main
                Call id(3), args( Variable(3, Boolean), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]"#]],
    );
}

#[test]
fn mutable_bool_binding_generates_store_instruction() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Bool {
                use q = Qubit();
                mutable b = MResetZ(q) == One;
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
    let read_result_callable_id = CallableId(2);
    assert_callable(
        &program,
        read_result_callable_id,
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[111-121] scope=0 scope_package_id=2 scope_span=[64-145] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[111-128] scope=0 scope_package_id=2 scope_span=[64-145] callable=Main
                Variable(1, Boolean) = Store Variable(0, Boolean) !dbg package_id=2 span=[111-128] scope=0 scope_package_id=2 scope_span=[64-145] callable=Main
                Variable(2, Boolean) = Store Variable(1, Boolean) !dbg package_id=2 span=[107-108] scope=0 scope_package_id=2 scope_span=[64-145] callable=Main
                Variable(3, Boolean) = Store Variable(2, Boolean) !dbg package_id=2 span=[138-139] scope=0 scope_package_id=2 scope_span=[64-145] callable=Main
                Call id(3), args( Variable(3, Boolean), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]"#]],
    );
}

#[test]
fn immutable_int_binding_does_not_generate_store_instruction() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                let i = MResetZ(q) == One ? 0 | 1;
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
    let read_result_callable_id = CallableId(2);
    assert_callable(
        &program,
        read_result_callable_id,
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[106-116] scope=0 scope_package_id=2 scope_span=[63-148] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[106-123] scope=0 scope_package_id=2 scope_span=[63-148] callable=Main
                Variable(1, Boolean) = Store Variable(0, Boolean) !dbg package_id=2 span=[106-123] scope=0 scope_package_id=2 scope_span=[63-148] callable=Main
                Branch Variable(1, Boolean), 2, 3 !dbg package_id=2 span=[106-123] scope=0 scope_package_id=2 scope_span=[63-148] callable=Main
            Block 1:Block:
                Variable(3, Integer) = Store Variable(2, Integer) !dbg package_id=2 span=[102-103] scope=0 scope_package_id=2 scope_span=[63-148] callable=Main
                Variable(4, Integer) = Store Variable(3, Integer) !dbg package_id=2 span=[141-142] scope=0 scope_package_id=2 scope_span=[63-148] callable=Main
                Call id(3), args( Variable(4, Integer), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]
            Block 2:Block:
                Variable(2, Integer) = Store Integer(0) !dbg package_id=2 span=[126-127] scope=0 scope_package_id=2 scope_span=[63-148] callable=Main
                Jump(1) !dbg package_id=2 span=[126-127] scope=0 scope_package_id=2 scope_span=[63-148] callable=Main
            Block 3:Block:
                Variable(2, Integer) = Store Integer(1) !dbg package_id=2 span=[130-131] scope=0 scope_package_id=2 scope_span=[63-148] callable=Main
                Jump(1) !dbg package_id=2 span=[130-131] scope=0 scope_package_id=2 scope_span=[63-148] callable=Main"#]],
    );
}

#[test]
fn mutable_int_binding_does_generate_store_instruction() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                mutable i = MResetZ(q) == One ? 0 | 1;
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
    let read_result_callable_id = CallableId(2);
    assert_callable(
        &program,
        read_result_callable_id,
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[110-120] scope=0 scope_package_id=2 scope_span=[63-152] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[110-127] scope=0 scope_package_id=2 scope_span=[63-152] callable=Main
                Variable(1, Boolean) = Store Variable(0, Boolean) !dbg package_id=2 span=[110-127] scope=0 scope_package_id=2 scope_span=[63-152] callable=Main
                Branch Variable(1, Boolean), 2, 3 !dbg package_id=2 span=[110-127] scope=0 scope_package_id=2 scope_span=[63-152] callable=Main
            Block 1:Block:
                Variable(3, Integer) = Store Variable(2, Integer) !dbg package_id=2 span=[106-107] scope=0 scope_package_id=2 scope_span=[63-152] callable=Main
                Variable(4, Integer) = Store Variable(3, Integer) !dbg package_id=2 span=[145-146] scope=0 scope_package_id=2 scope_span=[63-152] callable=Main
                Call id(3), args( Variable(4, Integer), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]
            Block 2:Block:
                Variable(2, Integer) = Store Integer(0) !dbg package_id=2 span=[130-131] scope=0 scope_package_id=2 scope_span=[63-152] callable=Main
                Jump(1) !dbg package_id=2 span=[130-131] scope=0 scope_package_id=2 scope_span=[63-152] callable=Main
            Block 3:Block:
                Variable(2, Integer) = Store Integer(1) !dbg package_id=2 span=[134-135] scope=0 scope_package_id=2 scope_span=[63-152] callable=Main
                Jump(1) !dbg package_id=2 span=[134-135] scope=0 scope_package_id=2 scope_span=[63-152] callable=Main"#]],
    );
}

#[test]
fn mutable_variable_in_outer_scope_set_to_mutable_from_inner_scope() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                mutable i = 0;
                if MResetZ(q) == One {
                    mutable j = 1;
                    set i = j;
                }
                else {
                    set i = 2;
                }
                return i;
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
    let read_result_callable_id = CallableId(2);
    assert_callable(
        &program,
        read_result_callable_id,
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
                Variable(0, Integer) = Store Integer(0) !dbg package_id=2 span=[106-107] scope=0 scope_package_id=2 scope_span=[63-275] callable=Main
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[124-134] scope=0 scope_package_id=2 scope_span=[63-275] callable=Main
                Variable(1, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[124-141] scope=0 scope_package_id=2 scope_span=[63-275] callable=Main
                Variable(2, Boolean) = Store Variable(1, Boolean) !dbg package_id=2 span=[124-141] scope=0 scope_package_id=2 scope_span=[63-275] callable=Main
                Branch Variable(2, Boolean), 2, 3 !dbg package_id=2 span=[124-141] scope=0 scope_package_id=2 scope_span=[63-275] callable=Main
            Block 1:Block:
                Variable(4, Integer) = Store Variable(0, Integer) !dbg package_id=2 span=[267-268] scope=3 scope_package_id=2 scope_span=[260-268] callable=Main
                Call id(3), args( Variable(4, Integer), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]
            Block 2:Block:
                Variable(3, Integer) = Store Integer(1) !dbg package_id=2 span=[164-165] scope=1 scope_package_id=2 scope_span=[142-203] callable=Main
                Variable(0, Integer) = Store Integer(1) !dbg package_id=2 span=[187-188] scope=1 scope_package_id=2 scope_span=[142-203] callable=Main
                Jump(1) !dbg package_id=2 span=[142-203] scope=0 scope_package_id=2 scope_span=[63-275] callable=Main
            Block 3:Block:
                Variable(0, Integer) = Store Integer(2) !dbg package_id=2 span=[235-236] scope=2 scope_package_id=2 scope_span=[217-251] callable=Main
                Jump(1) !dbg package_id=2 span=[212-251] scope=0 scope_package_id=2 scope_span=[63-275] callable=Main"#]],
    );
}

#[test]
fn mutable_double_binding_does_generate_store_instruction() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Double {
                use q = Qubit();
                mutable d = MResetZ(q) == One ? 0.1 | 1.1;
                d
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
    let read_result_callable_id = CallableId(2);
    assert_callable(
        &program,
        read_result_callable_id,
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[113-123] scope=0 scope_package_id=2 scope_span=[66-159] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[113-130] scope=0 scope_package_id=2 scope_span=[66-159] callable=Main
                Variable(1, Boolean) = Store Variable(0, Boolean) !dbg package_id=2 span=[113-130] scope=0 scope_package_id=2 scope_span=[66-159] callable=Main
                Branch Variable(1, Boolean), 2, 3 !dbg package_id=2 span=[113-130] scope=0 scope_package_id=2 scope_span=[66-159] callable=Main
            Block 1:Block:
                Variable(3, Double) = Store Variable(2, Double) !dbg package_id=2 span=[109-110] scope=0 scope_package_id=2 scope_span=[66-159] callable=Main
                Variable(4, Double) = Store Variable(3, Double) !dbg package_id=2 span=[152-153] scope=0 scope_package_id=2 scope_span=[66-159] callable=Main
                Call id(3), args( Variable(4, Double), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]
            Block 2:Block:
                Variable(2, Double) = Store Double(0.1) !dbg package_id=2 span=[133-136] scope=0 scope_package_id=2 scope_span=[66-159] callable=Main
                Jump(1) !dbg package_id=2 span=[133-136] scope=0 scope_package_id=2 scope_span=[66-159] callable=Main
            Block 3:Block:
                Variable(2, Double) = Store Double(1.1) !dbg package_id=2 span=[139-142] scope=0 scope_package_id=2 scope_span=[66-159] callable=Main
                Jump(1) !dbg package_id=2 span=[139-142] scope=0 scope_package_id=2 scope_span=[66-159] callable=Main"#]],
    );
}
