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
                Variable(0, Integer) = Store Integer(0)
                Call id(1), args( Qubit(0), )
                Variable(0, Integer) = Store Integer(1)
                Variable(0, Integer) = Store Integer(2)
                Call id(1), args( Qubit(0), )
                Variable(0, Integer) = Store Integer(3)
                Variable(0, Integer) = Store Integer(4)
                Call id(1), args( Qubit(0), )
                Variable(0, Integer) = Store Integer(5)
                Variable(0, Integer) = Store Integer(6)
                Call id(2), args( Integer(0), EmptyTag, )
                Return"#]],
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
                Variable(0, Integer) = Store Integer(0)
                Call id(1), args( Qubit(0), )
                Variable(0, Integer) = Store Integer(1)
                Variable(0, Integer) = Store Integer(2)
                Call id(1), args( Qubit(0), )
                Variable(0, Integer) = Store Integer(3)
                Variable(0, Integer) = Store Integer(4)
                Call id(1), args( Qubit(0), )
                Variable(0, Integer) = Store Integer(5)
                Variable(0, Integer) = Store Integer(6)
                Call id(2), args( Integer(0), EmptyTag, )
                Return"#]],
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
                Variable(0, Integer) = Store Integer(0)
                Variable(1, Boolean) = Store Bool(true)
                Call id(1), args( Qubit(0), )
                Variable(0, Integer) = Store Integer(1)
                Variable(1, Boolean) = Store Bool(true)
                Variable(0, Integer) = Store Integer(2)
                Variable(1, Boolean) = Store Bool(true)
                Call id(1), args( Qubit(0), )
                Variable(0, Integer) = Store Integer(3)
                Variable(1, Boolean) = Store Bool(true)
                Variable(0, Integer) = Store Integer(4)
                Variable(1, Boolean) = Store Bool(true)
                Call id(1), args( Qubit(0), )
                Variable(0, Integer) = Store Integer(5)
                Variable(1, Boolean) = Store Bool(true)
                Variable(0, Integer) = Store Integer(6)
                Variable(1, Boolean) = Store Bool(false)
                Call id(2), args( Integer(0), EmptyTag, )
                Return"#]],
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
                Variable(0, Boolean) = Store Bool(true)
                Call id(1), args( Qubit(0), Result(0), )
                Variable(1, Boolean) = Call id(2), args( Result(0), )
                Variable(2, Boolean) = Store Variable(1, Boolean)
                Branch Variable(2, Boolean), 2, 1
            Block 1:Block:
                Variable(3, Boolean) = Store Variable(0, Boolean)
                Call id(3), args( Variable(3, Boolean), Tag(0, 3), )
                Return
            Block 2:Block:
                Variable(0, Boolean) = Store Bool(false)
                Jump(1)"#]],
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
                Variable(0, Integer) = Store Integer(1)
                Call id(1), args( Qubit(0), Result(0), )
                Variable(1, Boolean) = Call id(2), args( Result(0), )
                Variable(2, Boolean) = Store Variable(1, Boolean)
                Branch Variable(2, Boolean), 2, 1
            Block 1:Block:
                Variable(3, Integer) = Store Variable(0, Integer)
                Call id(3), args( Variable(3, Integer), Tag(0, 3), )
                Return
            Block 2:Block:
                Variable(0, Integer) = Store Integer(5)
                Jump(1)"#]],
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
                name: __quantum__rt__read_result
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
                Variable(0, Integer) = Store Integer(0)
                Variable(1, Integer) = Store Integer(0)
                Call id(1), args( Qubit(0), Result(0), )
                Variable(2, Boolean) = Call id(2), args( Result(0), )
                Variable(3, Boolean) = Store Variable(2, Boolean)
                Branch Variable(3, Boolean), 2, 1
            Block 1:Block:
                Variable(1, Integer) = Store Integer(1)
                Call id(1), args( Qubit(0), Result(1), )
                Variable(4, Boolean) = Call id(2), args( Result(1), )
                Variable(5, Boolean) = Store Variable(4, Boolean)
                Branch Variable(5, Boolean), 4, 3
            Block 2:Block:
                Variable(0, Integer) = Store Integer(1)
                Jump(1)
            Block 3:Block:
                Variable(1, Integer) = Store Integer(2)
                Variable(7, Integer) = Store Variable(0, Integer)
                Call id(3), args( Variable(7, Integer), Tag(0, 3), )
                Return
            Block 4:Block:
                Variable(6, Integer) = BitwiseOr Variable(0, Integer), Integer(2)
                Variable(0, Integer) = Store Variable(6, Integer)
                Jump(3)"#]],
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
                Variable(1, Integer) = Store Integer(100)
                Variable(400, Integer) = Store Variable(0, Integer)
                Call id(3), args( Variable(400, Integer), Tag(0, 3), )
                Return"#]],
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
                name: __quantum__rt__read_result
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
                Call id(1), args( Qubit(0), Result(0), )
                Variable(0, Boolean) = Call id(2), args( Result(0), )
                Variable(1, Boolean) = Store Variable(0, Boolean)
                Variable(2, Boolean) = Store Bool(false)
                Branch Variable(1, Boolean), 2, 1
            Block 1:Block:
                Branch Variable(2, Boolean), 4, 5
            Block 2:Block:
                Call id(1), args( Qubit(1), Result(1), )
                Variable(3, Boolean) = Call id(2), args( Result(1), )
                Variable(4, Boolean) = Store Variable(3, Boolean)
                Variable(2, Boolean) = Store Variable(4, Boolean)
                Jump(1)
            Block 3:Block:
                Call id(5), args( Integer(0), EmptyTag, )
                Return
            Block 4:Block:
                Call id(3), args( Qubit(2), )
                Jump(3)
            Block 5:Block:
                Call id(4), args( Qubit(2), )
                Jump(3)"#]],
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
                name: __quantum__rt__read_result
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
                Call id(1), args( Qubit(0), Result(0), )
                Variable(0, Boolean) = Call id(2), args( Result(0), )
                Variable(1, Boolean) = Store Variable(0, Boolean)
                Variable(2, Boolean) = Store Bool(true)
                Branch Variable(1, Boolean), 1, 2
            Block 1:Block:
                Branch Variable(2, Boolean), 4, 5
            Block 2:Block:
                Call id(1), args( Qubit(1), Result(1), )
                Variable(3, Boolean) = Call id(2), args( Result(1), )
                Variable(4, Boolean) = Store Variable(3, Boolean)
                Variable(2, Boolean) = Store Variable(4, Boolean)
                Jump(1)
            Block 3:Block:
                Call id(5), args( Integer(0), EmptyTag, )
                Return
            Block 4:Block:
                Call id(3), args( Qubit(2), )
                Jump(3)
            Block 5:Block:
                Call id(4), args( Qubit(2), )
                Jump(3)"#]],
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
