// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::needless_raw_string_hashes, clippy::similar_names)]

use super::{
    assert_block_instructions, assert_blocks, assert_callable, assert_error,
    get_partial_evaluation_error, get_rir_program,
};
use expect_test::expect;
use indoc::indoc;
use qsc_rir::rir::{BlockId, CallableId};

#[test]
fn unitary_call_within_an_if_with_classical_condition_within_a_for_loop() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            operation op(q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                for idx in 0..5 {
                    if idx % 2 == 0 {
                        op(q);
                    }
                }
            }
        }
        "#,
    });

    let op_callable_id = CallableId(1);
    assert_callable(
        &program,
        op_callable_id,
        &expect![[r#"
        Callable:
            name: op
            call_type: Regular
            input_type:
                [0]: Qubit
            output_type: <VOID>
            body: <NONE>"#]],
    );
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Variable(0, Integer) = Store Integer(0) !dbg package_id=2 span=[165-169]
                Call id(1), args( Qubit(0), ) !dbg package_id=2 span=[218-220]
                Variable(0, Integer) = Store Integer(1) !dbg package_id=2 span=[165-169]
                Variable(0, Integer) = Store Integer(2) !dbg package_id=2 span=[165-169]
                Call id(1), args( Qubit(0), ) !dbg package_id=2 span=[218-220]
                Variable(0, Integer) = Store Integer(3) !dbg package_id=2 span=[165-169]
                Variable(0, Integer) = Store Integer(4) !dbg package_id=2 span=[165-169]
                Call id(1), args( Qubit(0), ) !dbg package_id=2 span=[218-220]
                Variable(0, Integer) = Store Integer(5) !dbg package_id=2 span=[165-169]
                Variable(0, Integer) = Store Integer(6) !dbg package_id=2 span=[165-169]
                Call id(2), args( Integer(0), Pointer, ) !dbg package_id=2 span=[105-109]
                Return !dbg package_id=2 span=[105-109]"#]],
    );
}

#[test]
fn unitary_call_within_an_if_with_classical_condition_within_a_while_loop() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            operation op(q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                mutable idx = 0;
                while idx <= 5 {
                    if idx % 2 == 0 {
                        op(q);
                    }
                    set idx += 1;
                }
            }
        }
        "#,
    });

    let op_callable_id = CallableId(1);
    assert_callable(
        &program,
        op_callable_id,
        &expect![[r#"
        Callable:
            name: op
            call_type: Regular
            input_type:
                [0]: Qubit
            output_type: <VOID>
            body: <NONE>"#]],
    );
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Variable(0, Integer) = Store Integer(0) !dbg package_id=2 span=[162-165]
                Call id(1), args( Qubit(0), ) !dbg package_id=2 span=[242-244]
                Variable(0, Integer) = Store Integer(1) !dbg package_id=2 span=[279-282]
                Variable(0, Integer) = Store Integer(2) !dbg package_id=2 span=[279-282]
                Call id(1), args( Qubit(0), ) !dbg package_id=2 span=[242-244]
                Variable(0, Integer) = Store Integer(3) !dbg package_id=2 span=[279-282]
                Variable(0, Integer) = Store Integer(4) !dbg package_id=2 span=[279-282]
                Call id(1), args( Qubit(0), ) !dbg package_id=2 span=[242-244]
                Variable(0, Integer) = Store Integer(5) !dbg package_id=2 span=[279-282]
                Variable(0, Integer) = Store Integer(6) !dbg package_id=2 span=[279-282]
                Call id(2), args( Integer(0), Pointer, ) !dbg package_id=2 span=[105-109]
                Return !dbg package_id=2 span=[105-109]"#]],
    );
}

#[test]
fn unitary_call_within_an_if_with_classical_condition_within_a_repeat_until_loop() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            operation op(q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                mutable idx = 0;
                repeat {
                    if idx % 2 == 0 {
                        op(q);
                    }
                    set idx += 1;
                } until idx > 5;
            }
        }
        "#,
    });

    let op_callable_id = CallableId(1);
    assert_callable(
        &program,
        op_callable_id,
        &expect![[r#"
        Callable:
            name: op
            call_type: Regular
            input_type:
                [0]: Qubit
            output_type: <VOID>
            body: <NONE>"#]],
    );
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Variable(0, Integer) = Store Integer(0) !dbg package_id=2 span=[162-165]
                Variable(1, Boolean) = Store Bool(true) !dbg package_id=2 span=[297-304]
                Call id(1), args( Qubit(0), ) !dbg package_id=2 span=[234-236]
                Variable(0, Integer) = Store Integer(1) !dbg package_id=2 span=[271-274]
                Variable(1, Boolean) = Store Bool(true) !dbg package_id=2 span=[297-304]
                Variable(0, Integer) = Store Integer(2) !dbg package_id=2 span=[271-274]
                Variable(1, Boolean) = Store Bool(true) !dbg package_id=2 span=[297-304]
                Call id(1), args( Qubit(0), ) !dbg package_id=2 span=[234-236]
                Variable(0, Integer) = Store Integer(3) !dbg package_id=2 span=[271-274]
                Variable(1, Boolean) = Store Bool(true) !dbg package_id=2 span=[297-304]
                Variable(0, Integer) = Store Integer(4) !dbg package_id=2 span=[271-274]
                Variable(1, Boolean) = Store Bool(true) !dbg package_id=2 span=[297-304]
                Call id(1), args( Qubit(0), ) !dbg package_id=2 span=[234-236]
                Variable(0, Integer) = Store Integer(5) !dbg package_id=2 span=[271-274]
                Variable(1, Boolean) = Store Bool(true) !dbg package_id=2 span=[297-304]
                Variable(0, Integer) = Store Integer(6) !dbg package_id=2 span=[271-274]
                Variable(1, Boolean) = Store Bool(false) !dbg package_id=2 span=[297-304]
                Call id(2), args( Integer(0), Pointer, ) !dbg package_id=2 span=[105-109]
                Return !dbg package_id=2 span=[105-109]"#]],
    );
}

#[test]
fn boolean_assign_and_update_with_classical_value_within_an_if_with_dynamic_condition() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Bool {
                use qubit = Qubit();
                mutable b = true;
                if MResetZ(qubit) == One {
                    set b and= false;
                }
                return b;
            }
        }
        "#,
    });
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Variable(0, Boolean) = Store Bool(true) !dbg package_id=2 span=[111-112]
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=1 span=[182985-183014]
                Variable(1, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[132-153]
                Variable(2, Boolean) = Store Variable(1, Boolean) !dbg package_id=2 span=[132-153]
                Branch Variable(2, Boolean), 2, 1 !dbg package_id=2 span=[132-153]
            Block 1:Block:
                Variable(3, Boolean) = Store Variable(0, Boolean) !dbg package_id=2 span=[211-212]
                Call id(3), args( Variable(3, Boolean), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]
            Block 2:Block:
                Variable(0, Boolean) = Store Bool(false) !dbg package_id=2 span=[172-173]
                Jump(1) !dbg package_id=2 span=[154-195]"#]],
    );
}

#[test]
fn integer_assign_and_update_with_classical_value_within_an_if_with_dynamic_condition() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                use qubit = Qubit();
                mutable i = 1;
                if MResetZ(qubit) == One {
                    set i |||= 1 <<< 2;
                }
                return i;
            }
        }
        "#,
    });
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Variable(0, Integer) = Store Integer(1) !dbg package_id=2 span=[110-111]
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=1 span=[182985-183014]
                Variable(1, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[128-149]
                Variable(2, Boolean) = Store Variable(1, Boolean) !dbg package_id=2 span=[128-149]
                Branch Variable(2, Boolean), 2, 1 !dbg package_id=2 span=[128-149]
            Block 1:Block:
                Variable(3, Integer) = Store Variable(0, Integer) !dbg package_id=2 span=[209-210]
                Call id(3), args( Variable(3, Integer), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]
            Block 2:Block:
                Variable(0, Integer) = Store Integer(5) !dbg package_id=2 span=[168-169]
                Jump(1) !dbg package_id=2 span=[150-193]"#]],
    );
}

#[test]
fn integer_assign_with_hybrid_value_within_an_if_with_dynamic_condition() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                use qubit = Qubit();
                mutable i = 0;
                for idxBit in 0..1{
                    if (MResetZ(qubit) == One) {
                        set i |||= 1 <<< idxBit;
                    }
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
                name: __quantum__qis__mresetz__body
                call_type: Measurement
                input_type:
                    [0]: Qubit
                    [1]: Result
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let readout_callable_id = CallableId(2);
    assert_callable(
        &program,
        readout_callable_id,
        &expect![[r#"
        Callable:
            name: __quantum__qis__read_result__body
            call_type: Readout
            input_type:
                [0]: Result
            output_type: Boolean
            body: <NONE>"#]],
    );
    let output_record_id = CallableId(3);
    assert_callable(
        &program,
        output_record_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__int_record_output
                call_type: OutputRecording
                input_type:
                    [0]: Integer
                    [1]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Variable(0, Integer) = Store Integer(0) !dbg package_id=2 span=[110-111]
                Variable(1, Integer) = Store Integer(0) !dbg package_id=2 span=[139-143]
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=1 span=[182985-183014]
                Variable(2, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[161-182]
                Variable(3, Boolean) = Store Variable(2, Boolean) !dbg package_id=2 span=[161-182]
                Branch Variable(3, Boolean), 2, 1 !dbg package_id=2 span=[161-182]
            Block 1:Block:
                Variable(1, Integer) = Store Integer(1) !dbg package_id=2 span=[139-143]
                Call id(1), args( Qubit(0), Result(1), ) !dbg package_id=1 span=[182985-183014]
                Variable(4, Boolean) = Call id(2), args( Result(1), ) !dbg package_id=2 span=[161-182]
                Variable(5, Boolean) = Store Variable(4, Boolean) !dbg package_id=2 span=[161-182]
                Branch Variable(5, Boolean), 4, 3 !dbg package_id=2 span=[161-182]
            Block 2:Block:
                Variable(0, Integer) = Store Integer(1) !dbg package_id=2 span=[206-207]
                Jump(1) !dbg package_id=2 span=[184-240]
            Block 3:Block:
                Variable(1, Integer) = Store Integer(2) !dbg package_id=2 span=[139-143]
                Variable(7, Integer) = Store Variable(0, Integer) !dbg package_id=2 span=[266-267]
                Call id(3), args( Variable(7, Integer), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]
            Block 4:Block:
                Variable(6, Integer) = BitwiseOr Variable(0, Integer), Integer(2) !dbg package_id=2 span=[202-225]
                Variable(0, Integer) = Store Variable(6, Integer) !dbg package_id=2 span=[206-207]
                Jump(3) !dbg package_id=2 span=[184-240]"#]],
    );
}

#[test]
fn large_loop_with_inner_if_completes_eval_and_transform() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                mutable i = 0;
                for idx in 0..99 {
                    if i == 0 {
                        if MResetZ(q) == One {
                            set i += 1;
                        }
                    }
                }
                return i;
            }
        }
        "#,
    });

    // Program is expected to be too large to reasonably print out here, so instead verify the last block
    // and the total number of blocks.
    assert_eq!(program.blocks.iter().count(), 399);
    assert_block_instructions(
        &program,
        BlockId(395),
        &expect![[r#"
            Block:
                Variable(1, Integer) = Store Integer(100) !dbg package_id=2 span=[132-137]
                Variable(400, Integer) = Store Variable(0, Integer) !dbg package_id=2 span=[292-293]
                Call id(3), args( Variable(400, Integer), Pointer, ) !dbg package_id=2 span=[50-54]
                Return !dbg package_id=2 span=[50-54]"#]],
    );
}

#[test]
fn if_else_expression_with_dynamic_logical_and_condition() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            operation opA(q : Qubit) : Unit { body intrinsic; }
            operation opB(q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                use (q0, q1, q2) = (Qubit(), Qubit(), Qubit());
                if MResetZ(q0) == One and MResetZ(q1) == One {
                    opA(q2);
                } else {
                    opB(q2);
                }
            }
        }
        "#,
    });

    // Verify the callables added to the program.
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
    let read_result_callable_id = CallableId(2);
    assert_callable(
        &program,
        read_result_callable_id,
        &expect![[r#"
        Callable:
            name: __quantum__qis__read_result__body
            call_type: Readout
            input_type:
                [0]: Result
            output_type: Boolean
            body: <NONE>"#]],
    );
    let op_a_callable_id = CallableId(3);
    assert_callable(
        &program,
        op_a_callable_id,
        &expect![[r#"
        Callable:
            name: opA
            call_type: Regular
            input_type:
                [0]: Qubit
            output_type: <VOID>
            body: <NONE>"#]],
    );
    let op_b_callable_id = CallableId(4);
    assert_callable(
        &program,
        op_b_callable_id,
        &expect![[r#"
        Callable:
            name: opB
            call_type: Regular
            input_type:
                [0]: Qubit
            output_type: <VOID>
            body: <NONE>"#]],
    );

    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=1 span=[182985-183014]
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[245-263]
                Variable(1, Boolean) = Store Variable(0, Boolean) !dbg package_id=2 span=[245-263]
                Variable(2, Boolean) = Store Bool(false) !dbg package_id=2 span=[268-286]
                Branch Variable(1, Boolean), 2, 1 !dbg package_id=2 span=[268-286]
            Block 1:Block:
                Branch Variable(2, Boolean), 4, 5 !dbg package_id=2 span=[245-286]
            Block 2:Block:
                Call id(1), args( Qubit(1), Result(1), ) !dbg package_id=1 span=[182985-183014]
                Variable(3, Boolean) = Call id(2), args( Result(1), ) !dbg package_id=2 span=[268-286]
                Variable(4, Boolean) = Store Variable(3, Boolean) !dbg package_id=2 span=[268-286]
                Variable(2, Boolean) = Store Variable(4, Boolean) !dbg package_id=2 span=[268-286]
                Jump(1) !dbg package_id=2 span=[268-286]
            Block 3:Block:
                Call id(5), args( Integer(0), Pointer, ) !dbg package_id=2 span=[162-166]
                Return !dbg package_id=2 span=[162-166]
            Block 4:Block:
                Call id(3), args( Qubit(2), ) !dbg package_id=2 span=[301-304]
                Jump(3) !dbg package_id=2 span=[287-319]
            Block 5:Block:
                Call id(4), args( Qubit(2), ) !dbg package_id=2 span=[339-342]
                Jump(3) !dbg package_id=2 span=[320-357]"#]],
    );
}

#[test]
fn if_else_expression_with_dynamic_logical_or_condition() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            operation opA(q : Qubit) : Unit { body intrinsic; }
            operation opB(q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                use (q0, q1, q2) = (Qubit(), Qubit(), Qubit());
                if MResetZ(q0) == One or MResetZ(q1) == One {
                    opA(q2);
                } else {
                    opB(q2);
                }
            }
        }
        "#,
    });

    // Verify the callables added to the program.
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
    let read_result_callable_id = CallableId(2);
    assert_callable(
        &program,
        read_result_callable_id,
        &expect![[r#"
        Callable:
            name: __quantum__qis__read_result__body
            call_type: Readout
            input_type:
                [0]: Result
            output_type: Boolean
            body: <NONE>"#]],
    );
    let op_a_callable_id = CallableId(3);
    assert_callable(
        &program,
        op_a_callable_id,
        &expect![[r#"
        Callable:
            name: opA
            call_type: Regular
            input_type:
                [0]: Qubit
            output_type: <VOID>
            body: <NONE>"#]],
    );
    let op_b_callable_id = CallableId(4);
    assert_callable(
        &program,
        op_b_callable_id,
        &expect![[r#"
        Callable:
            name: opB
            call_type: Regular
            input_type:
                [0]: Qubit
            output_type: <VOID>
            body: <NONE>"#]],
    );

    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=1 span=[182985-183014]
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[245-263]
                Variable(1, Boolean) = Store Variable(0, Boolean) !dbg package_id=2 span=[245-263]
                Variable(2, Boolean) = Store Bool(true) !dbg package_id=2 span=[267-285]
                Branch Variable(1, Boolean), 1, 2 !dbg package_id=2 span=[267-285]
            Block 1:Block:
                Branch Variable(2, Boolean), 4, 5 !dbg package_id=2 span=[245-285]
            Block 2:Block:
                Call id(1), args( Qubit(1), Result(1), ) !dbg package_id=1 span=[182985-183014]
                Variable(3, Boolean) = Call id(2), args( Result(1), ) !dbg package_id=2 span=[267-285]
                Variable(4, Boolean) = Store Variable(3, Boolean) !dbg package_id=2 span=[267-285]
                Variable(2, Boolean) = Store Variable(4, Boolean) !dbg package_id=2 span=[267-285]
                Jump(1) !dbg package_id=2 span=[267-285]
            Block 3:Block:
                Call id(5), args( Integer(0), Pointer, ) !dbg package_id=2 span=[162-166]
                Return !dbg package_id=2 span=[162-166]
            Block 4:Block:
                Call id(3), args( Qubit(2), ) !dbg package_id=2 span=[300-303]
                Jump(3) !dbg package_id=2 span=[286-318]
            Block 5:Block:
                Call id(4), args( Qubit(2), ) !dbg package_id=2 span=[338-341]
                Jump(3) !dbg package_id=2 span=[319-356]"#]],
    );
}

#[test]
fn evaluation_error_within_stdlib_yield_correct_package_span() {
    let error = get_partial_evaluation_error(indoc! {
        r#"
        namespace Test {
            import Std.Arrays.*;
            @EntryPoint()
            operation Main() : Result[] {
                use qs = Qubit[1];
                let rs = ForEach(MResetZ, qs);
                return rs;
            }
        }
        "#,
    });
    assert_error(
        &error,
        &expect![
            "UnexpectedDynamicValue(PackageSpan { package: PackageId(1), span: Span { lo: 13910, hi: 13925 } })"
        ],
    );
}
