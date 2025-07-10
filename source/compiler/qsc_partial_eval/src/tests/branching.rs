// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(
    clippy::needless_raw_string_hashes,
    clippy::similar_names,
    clippy::too_many_lines
)]

use super::{
    assert_blocks, assert_callable, assert_error, get_partial_evaluation_error, get_rir_program,
};
use expect_test::expect;
use indoc::indoc;
use qsc_rir::rir::CallableId;

#[test]
fn if_expression_with_true_condition() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            operation opA(q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                if true {
                    opA(q);
                }
            }
        }
        "#,
    });
    let op_a_callable_id = CallableId(1);
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
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Qubit(0), )
                Call id(2), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn if_expression_with_false_condition() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            operation opA(q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                if false {
                    opA(q);
                }
            }
        }
        "#,
    });
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn if_else_expression_with_true_condition() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            operation opA(q : Qubit) : Unit { body intrinsic; }
            operation opB(q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                if true {
                    opA(q);
                } else {
                    opB(q);
                }
            }
        }
        "#,
    });
    let op_a_callable_id = CallableId(1);
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
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Qubit(0), )
                Call id(2), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn if_else_expression_with_false_condition() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            operation opA(q : Qubit) : Unit { body intrinsic; }
            operation opB(q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                if false {
                    opA(q);
                } else {
                    opB(q);
                }
            }
        }
        "#,
    });
    let op_b_callable_id = CallableId(1);
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
                Call id(1), args( Qubit(0), )
                Call id(2), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn if_elif_else_expression_with_true_elif_condition() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            operation opA(q : Qubit) : Unit { body intrinsic; }
            operation opB(q : Qubit) : Unit { body intrinsic; }
            operation opC(q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                if false {
                    opA(q);
                } elif true {
                    opB(q);
                } else {
                    opC(q);
                }
            }
        }
        "#,
    });
    let op_b_callable_id = CallableId(1);
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
                Call id(1), args( Qubit(0), )
                Call id(2), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn if_expression_with_dynamic_condition() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            operation opA(q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                let r = QIR.Intrinsic.__quantum__qis__mresetz__body(q);
                if r == Zero {
                    opA(q);
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

    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Qubit(0), Result(0), )
                Variable(0, Boolean) = Call id(2), args( Result(0), )
                Variable(1, Boolean) = Icmp Eq, Variable(0, Boolean), Bool(false)
                Branch Variable(1, Boolean), 2, 1
            Block 1:Block:
                Call id(4), args( Integer(0), EmptyTag, )
                Return
            Block 2:Block:
                Call id(3), args( Qubit(0), )
                Jump(1)"#]],
    );
}

#[test]
fn if_else_expression_with_dynamic_condition() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            operation opA(q : Qubit) : Unit { body intrinsic; }
            operation opB(q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                let r = QIR.Intrinsic.__quantum__qis__mresetz__body(q);
                if r == One {
                    opA(q);
                } else {
                    opB(q);
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
                Branch Variable(1, Boolean), 2, 3
            Block 1:Block:
                Call id(5), args( Integer(0), EmptyTag, )
                Return
            Block 2:Block:
                Call id(3), args( Qubit(0), )
                Jump(1)
            Block 3:Block:
                Call id(4), args( Qubit(0), )
                Jump(1)"#]],
    );
}

#[test]
fn if_elif_else_expression_with_dynamic_condition() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            operation opA(q : Qubit) : Unit { body intrinsic; }
            operation opB(q : Qubit) : Unit { body intrinsic; }
            operation opC(q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                use (q0, q1, q2) = (Qubit(), Qubit(), Qubit());
                let r0 = QIR.Intrinsic.__quantum__qis__mresetz__body(q0);
                let r1 = QIR.Intrinsic.__quantum__qis__mresetz__body(q1);
                if r0 == One {
                    opA(q2);
                } elif r1 == One {
                    opB(q2);
                } else {
                    opC(q2);
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
    let op_c_callable_id = CallableId(5);
    assert_callable(
        &program,
        op_c_callable_id,
        &expect![[r#"
        Callable:
            name: opC
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
                Call id(1), args( Qubit(1), Result(1), )
                Variable(0, Boolean) = Call id(2), args( Result(0), )
                Variable(1, Boolean) = Store Variable(0, Boolean)
                Branch Variable(1, Boolean), 2, 3
            Block 1:Block:
                Call id(6), args( Integer(0), EmptyTag, )
                Return
            Block 2:Block:
                Call id(3), args( Qubit(2), )
                Jump(1)
            Block 3:Block:
                Variable(2, Boolean) = Call id(2), args( Result(1), )
                Variable(3, Boolean) = Store Variable(2, Boolean)
                Branch Variable(3, Boolean), 5, 6
            Block 4:Block:
                Jump(1)
            Block 5:Block:
                Call id(4), args( Qubit(2), )
                Jump(4)
            Block 6:Block:
                Call id(5), args( Qubit(2), )
                Jump(4)"#]],
    );
}

#[test]
fn if_expression_with_dynamic_condition_and_nested_if_expression_with_true_condition() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            operation opA(q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                let r = QIR.Intrinsic.__quantum__qis__mresetz__body(q);
                if r == Zero {
                    if true {
                        opA(q);
                    }
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

    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Qubit(0), Result(0), )
                Variable(0, Boolean) = Call id(2), args( Result(0), )
                Variable(1, Boolean) = Icmp Eq, Variable(0, Boolean), Bool(false)
                Branch Variable(1, Boolean), 2, 1
            Block 1:Block:
                Call id(4), args( Integer(0), EmptyTag, )
                Return
            Block 2:Block:
                Call id(3), args( Qubit(0), )
                Jump(1)"#]],
    );
}

#[test]
fn if_expression_with_dynamic_condition_and_nested_if_expression_with_false_condition() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            operation opA(q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                let r = QIR.Intrinsic.__quantum__qis__mresetz__body(q);
                if r == Zero {
                    if false {
                        opA(q);
                    }
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

    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Qubit(0), Result(0), )
                Variable(0, Boolean) = Call id(2), args( Result(0), )
                Variable(1, Boolean) = Icmp Eq, Variable(0, Boolean), Bool(false)
                Branch Variable(1, Boolean), 2, 1
            Block 1:Block:
                Call id(3), args( Integer(0), EmptyTag, )
                Return
            Block 2:Block:
                Jump(1)"#]],
    );
}

#[test]
fn if_else_expression_with_dynamic_condition_and_nested_if_expression_with_true_condition() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            operation opA(q : Qubit) : Unit { body intrinsic; }
            operation opB(q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                let r = QIR.Intrinsic.__quantum__qis__mresetz__body(q);
                if r == One {
                    opA(q);
                } else {
                    if true {
                        opB(q);
                    }
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
                Branch Variable(1, Boolean), 2, 3
            Block 1:Block:
                Call id(5), args( Integer(0), EmptyTag, )
                Return
            Block 2:Block:
                Call id(3), args( Qubit(0), )
                Jump(1)
            Block 3:Block:
                Call id(4), args( Qubit(0), )
                Jump(1)"#]],
    );
}

#[test]
fn if_else_expression_with_dynamic_condition_and_nested_if_expression_with_false_condition() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            operation opA(q : Qubit) : Unit { body intrinsic; }
            operation opB(q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                let r = QIR.Intrinsic.__quantum__qis__mresetz__body(q);
                if r == One {
                    opA(q);
                } else {
                    if false {
                        opB(q);
                    }
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

    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Qubit(0), Result(0), )
                Variable(0, Boolean) = Call id(2), args( Result(0), )
                Variable(1, Boolean) = Store Variable(0, Boolean)
                Branch Variable(1, Boolean), 2, 3
            Block 1:Block:
                Call id(4), args( Integer(0), EmptyTag, )
                Return
            Block 2:Block:
                Call id(3), args( Qubit(0), )
                Jump(1)
            Block 3:Block:
                Jump(1)"#]],
    );
}

#[test]
fn if_expression_with_dynamic_condition_and_nested_if_expression_with_dynamic_condition() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            operation opA(q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                use (q0, q1, q2) = (Qubit(), Qubit(), Qubit());
                let r0 = QIR.Intrinsic.__quantum__qis__mresetz__body(q0);
                let r1 = QIR.Intrinsic.__quantum__qis__mresetz__body(q1);
                if r0 == Zero {
                    if r1 == One {
                        opA(q2);
                    }
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

    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Qubit(0), Result(0), )
                Call id(1), args( Qubit(1), Result(1), )
                Variable(0, Boolean) = Call id(2), args( Result(0), )
                Variable(1, Boolean) = Icmp Eq, Variable(0, Boolean), Bool(false)
                Branch Variable(1, Boolean), 2, 1
            Block 1:Block:
                Call id(4), args( Integer(0), EmptyTag, )
                Return
            Block 2:Block:
                Variable(2, Boolean) = Call id(2), args( Result(1), )
                Variable(3, Boolean) = Store Variable(2, Boolean)
                Branch Variable(3, Boolean), 4, 3
            Block 3:Block:
                Jump(1)
            Block 4:Block:
                Call id(3), args( Qubit(2), )
                Jump(3)"#]],
    );
}

#[allow(clippy::too_many_lines)]
#[test]
fn doubly_nested_if_else_expressions_with_dynamic_conditions() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            operation opA(q : Qubit) : Unit { body intrinsic; }
            operation opB(q : Qubit) : Unit { body intrinsic; }
            operation opC(q : Qubit) : Unit { body intrinsic; }
            operation opD(q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                use (q0, q1, q2) = (Qubit(), Qubit(), Qubit());
                let r0 = QIR.Intrinsic.__quantum__qis__mresetz__body(q0);
                let r1 = QIR.Intrinsic.__quantum__qis__mresetz__body(q1);
                if r0 == Zero {
                    if r1 == Zero {
                        opA(q2);
                    } else {
                        opB(q2);
                    }
                } else {
                    if r1 == One{
                        opC(q2);
                    } else {
                        opD(q2);
                    }
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
    let op_c_callable_id = CallableId(5);
    assert_callable(
        &program,
        op_c_callable_id,
        &expect![[r#"
        Callable:
            name: opC
            call_type: Regular
            input_type:
                [0]: Qubit
            output_type: <VOID>
            body: <NONE>"#]],
    );
    let op_d_callable_id = CallableId(6);
    assert_callable(
        &program,
        op_d_callable_id,
        &expect![[r#"
        Callable:
            name: opD
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
                Call id(1), args( Qubit(1), Result(1), )
                Variable(0, Boolean) = Call id(2), args( Result(0), )
                Variable(1, Boolean) = Icmp Eq, Variable(0, Boolean), Bool(false)
                Branch Variable(1, Boolean), 2, 6
            Block 1:Block:
                Call id(7), args( Integer(0), EmptyTag, )
                Return
            Block 2:Block:
                Variable(2, Boolean) = Call id(2), args( Result(1), )
                Variable(3, Boolean) = Icmp Eq, Variable(2, Boolean), Bool(false)
                Branch Variable(3, Boolean), 4, 5
            Block 3:Block:
                Jump(1)
            Block 4:Block:
                Call id(3), args( Qubit(2), )
                Jump(3)
            Block 5:Block:
                Call id(4), args( Qubit(2), )
                Jump(3)
            Block 6:Block:
                Variable(4, Boolean) = Call id(2), args( Result(1), )
                Variable(5, Boolean) = Store Variable(4, Boolean)
                Branch Variable(5, Boolean), 8, 9
            Block 7:Block:
                Jump(1)
            Block 8:Block:
                Call id(5), args( Qubit(2), )
                Jump(7)
            Block 9:Block:
                Call id(6), args( Qubit(2), )
                Jump(7)"#]],
    );
}

#[test]
fn if_expression_with_dynamic_condition_and_subsequent_call_to_operation() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            operation opA(q : Qubit) : Unit { body intrinsic; }
            operation opB(q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                let r = QIR.Intrinsic.__quantum__qis__mresetz__body(q);
                if r == Zero {
                    opA(q);
                }
                opB(q);
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
                Variable(1, Boolean) = Icmp Eq, Variable(0, Boolean), Bool(false)
                Branch Variable(1, Boolean), 2, 1
            Block 1:Block:
                Call id(4), args( Qubit(0), )
                Call id(5), args( Integer(0), EmptyTag, )
                Return
            Block 2:Block:
                Call id(3), args( Qubit(0), )
                Jump(1)"#]],
    );
}

#[test]
fn if_else_expression_with_dynamic_condition_and_subsequent_call_to_operation() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            operation opA(q : Qubit) : Unit { body intrinsic; }
            operation opB(q : Qubit) : Unit { body intrinsic; }
            operation opC(q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                let r = QIR.Intrinsic.__quantum__qis__mresetz__body(q);
                if r == One {
                    opA(q);
                } else {
                    opB(q);
                }
                opC(q);
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
    let op_c_callable_id = CallableId(5);
    assert_callable(
        &program,
        op_c_callable_id,
        &expect![[r#"
        Callable:
            name: opC
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
                Branch Variable(1, Boolean), 2, 3
            Block 1:Block:
                Call id(5), args( Qubit(0), )
                Call id(6), args( Integer(0), EmptyTag, )
                Return
            Block 2:Block:
                Call id(3), args( Qubit(0), )
                Jump(1)
            Block 3:Block:
                Call id(4), args( Qubit(0), )
                Jump(1)"#]],
    );
}

#[test]
fn if_else_expression_with_result_literal_fails() {
    let error = get_partial_evaluation_error(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                MResetZ(q) == One ? One | MResetZ(q)
            }
        }
        "#,
    });

    assert_error(
        &error,
        &expect![[
            r#"Unexpected("dynamic value of type Result in conditional expression", PackageSpan { package: PackageId(2), span: Span { lo: 101, hi: 137 } })"#
        ]],
    );
}

#[test]
fn if_expression_with_classical_operand_from_hybrid_results_array_comparing_to_literal_zero() {
    let program = get_rir_program(indoc! {r#"
        @EntryPoint()
        operation Main() : Result[] {
            mutable measurements = [Zero, Zero];
            use (a, b) = (Qubit(), Qubit());
            set measurements w/= 0 <- MResetZ(a);
            // Use a static result in the condition.
            if measurements[1] == Zero {
                X(b);
            }
            set measurements w/= 1 <- MResetZ(b);
            measurements
        }
        "#
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
    let x_callable_id = CallableId(2);
    assert_callable(
        &program,
        x_callable_id,
        &expect![[r#"
        Callable:
            name: __quantum__qis__x__body
            call_type: Regular
            input_type:
                [0]: Qubit
            output_type: <VOID>
            body: <NONE>"#]],
    );
    let record_array_callable_id = CallableId(3);
    assert_callable(
        &program,
        record_array_callable_id,
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
    let record_result_callable_id = CallableId(4);
    assert_callable(
        &program,
        record_result_callable_id,
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
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Qubit(0), Result(0), )
                Call id(2), args( Qubit(1), )
                Call id(1), args( Qubit(1), Result(1), )
                Call id(3), args( Integer(2), EmptyTag, )
                Call id(4), args( Result(0), Tag(0, 5), )
                Call id(4), args( Result(1), Tag(1, 5), )
                Return"#]],
    );
}

#[test]
fn if_expression_with_classical_operand_from_hybrid_results_array_comparing_to_literal_one() {
    let program = get_rir_program(indoc! {r#"
        @EntryPoint()
        operation Main() : Result[] {
            mutable measurements = [Zero, Zero];
            use (a, b) = (Qubit(), Qubit());
            set measurements w/= 0 <- MResetZ(a);
            // Use a static result in the condition.
            if measurements[1] != One {
                X(b);
            }
            set measurements w/= 1 <- MResetZ(b);
            measurements
        }
        "#
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
    let x_callable_id = CallableId(2);
    assert_callable(
        &program,
        x_callable_id,
        &expect![[r#"
        Callable:
            name: __quantum__qis__x__body
            call_type: Regular
            input_type:
                [0]: Qubit
            output_type: <VOID>
            body: <NONE>"#]],
    );
    let record_array_callable_id = CallableId(3);
    assert_callable(
        &program,
        record_array_callable_id,
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
    let record_result_callable_id = CallableId(4);
    assert_callable(
        &program,
        record_result_callable_id,
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
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Qubit(0), Result(0), )
                Call id(2), args( Qubit(1), )
                Call id(1), args( Qubit(1), Result(1), )
                Call id(3), args( Integer(2), EmptyTag, )
                Call id(4), args( Result(0), Tag(0, 5), )
                Call id(4), args( Result(1), Tag(1, 5), )
                Return"#]],
    );
}

#[test]
fn if_expression_with_dynamic_operand_from_hybrid_results_array() {
    let program = get_rir_program(indoc! {r#"
        @EntryPoint()
        operation Main() : Result[] {
            mutable measurements = [Zero, Zero];
            use (a, b) = (Qubit(), Qubit());
            set measurements w/= 0 <- MResetZ(a);
            // Use a dynamic result in the condition.
            if measurements[0] == Zero {
                X(b);
            }
            set measurements w/= 1 <- MResetZ(b);
            measurements
        }
        "#
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
    let x_callable_id = CallableId(3);
    assert_callable(
        &program,
        x_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__qis__x__body
                call_type: Regular
                input_type:
                    [0]: Qubit
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let record_array_callable_id = CallableId(4);
    assert_callable(
        &program,
        record_array_callable_id,
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
    let record_result_callable_id = CallableId(5);
    assert_callable(
        &program,
        record_result_callable_id,
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
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Qubit(0), Result(0), )
                Variable(0, Boolean) = Call id(2), args( Result(0), )
                Variable(1, Boolean) = Icmp Eq, Variable(0, Boolean), Bool(false)
                Branch Variable(1, Boolean), 2, 1
            Block 1:Block:
                Call id(1), args( Qubit(1), Result(1), )
                Call id(4), args( Integer(2), EmptyTag, )
                Call id(5), args( Result(0), Tag(0, 5), )
                Call id(5), args( Result(1), Tag(1, 5), )
                Return
            Block 2:Block:
                Call id(3), args( Qubit(1), )
                Jump(1)"#]],
    );
}

#[test]
fn if_expression_with_classical_operand_from_hybrid_booleans_array() {
    let program = get_rir_program(indoc! {r#"
        @EntryPoint()
        operation Main() : Bool[] {
            mutable flags = [false, false];
            use (a, b) = (Qubit(), Qubit());
            set flags w/= 0 <- MResetZ(a) == One;
            // Use a static Boolean in the condition.
            if flags[1] == false {
                X(b);
            }
            set flags w/= 1 <- MResetZ(b) == One;
            flags
        }
        "#
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
    let x_callable_id = CallableId(3);
    assert_callable(
        &program,
        x_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__qis__x__body
                call_type: Regular
                input_type:
                    [0]: Qubit
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let record_array_callable_id = CallableId(4);
    assert_callable(
        &program,
        record_array_callable_id,
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
    let record_bool_callable_id = CallableId(5);
    assert_callable(
        &program,
        record_bool_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__bool_record_output
                call_type: OutputRecording
                input_type:
                    [0]: Boolean
                    [1]: Pointer
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
                Call id(3), args( Qubit(1), )
                Call id(1), args( Qubit(1), Result(1), )
                Variable(2, Boolean) = Call id(2), args( Result(1), )
                Variable(3, Boolean) = Store Variable(2, Boolean)
                Call id(4), args( Integer(2), EmptyTag, )
                Call id(5), args( Variable(1, Boolean), Tag(0, 5), )
                Call id(5), args( Variable(3, Boolean), Tag(1, 5), )
                Return"#]],
    );
}

#[test]
fn if_expression_with_dynamic_operand_from_hybrid_booleans_array() {
    let program = get_rir_program(indoc! {r#"
        @EntryPoint()
        operation Main() : Bool[] {
            mutable flags = [false, false];
            use (a, b) = (Qubit(), Qubit());
            set flags w/= 0 <- MResetZ(a) == One;
            // Use a dynamic Boolean in the condition.
            if flags[0] {
                X(b);
            }
            set flags w/= 1 <- MResetZ(b) == One;
            flags
        }
        "#
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
    let x_callable_id = CallableId(3);
    assert_callable(
        &program,
        x_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__qis__x__body
                call_type: Regular
                input_type:
                    [0]: Qubit
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let record_array_callable_id = CallableId(4);
    assert_callable(
        &program,
        record_array_callable_id,
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
    let record_bool_callable_id = CallableId(5);
    assert_callable(
        &program,
        record_bool_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__bool_record_output
                call_type: OutputRecording
                input_type:
                    [0]: Boolean
                    [1]: Pointer
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
                Branch Variable(1, Boolean), 2, 1
            Block 1:Block:
                Call id(1), args( Qubit(1), Result(1), )
                Variable(2, Boolean) = Call id(2), args( Result(1), )
                Variable(3, Boolean) = Store Variable(2, Boolean)
                Call id(4), args( Integer(2), EmptyTag, )
                Call id(5), args( Variable(1, Boolean), Tag(0, 5), )
                Call id(5), args( Variable(3, Boolean), Tag(1, 5), )
                Return
            Block 2:Block:
                Call id(3), args( Qubit(1), )
                Jump(1)"#]],
    );
}

#[test]
fn if_expression_with_classical_operand_from_hybrid_integers_array() {
    let program = get_rir_program(indoc! {r#"
        @EntryPoint()
        operation Main() : Int[] {
            mutable integers = [0, 0];
            use (a, b) = (Qubit(), Qubit());
            set integers w/= 0 <- MResetZ(a) == Zero ? 0 | 1;
            // Use a static integer in the condition.
            if integers[1] == 0 {
                X(b);
            }
            set integers w/= 1 <- MResetZ(b) == Zero ? 0 | 1;
            integers
        }
        "#
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
    let x_callable_id = CallableId(3);
    assert_callable(
        &program,
        x_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__qis__x__body
                call_type: Regular
                input_type:
                    [0]: Qubit
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let record_array_callable_id = CallableId(4);
    assert_callable(
        &program,
        record_array_callable_id,
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
    let record_int_callable_id = CallableId(5);
    assert_callable(
        &program,
        record_int_callable_id,
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
                Call id(1), args( Qubit(0), Result(0), )
                Variable(0, Boolean) = Call id(2), args( Result(0), )
                Variable(1, Boolean) = Icmp Eq, Variable(0, Boolean), Bool(false)
                Branch Variable(1, Boolean), 2, 3
            Block 1:Block:
                Call id(3), args( Qubit(1), )
                Call id(1), args( Qubit(1), Result(1), )
                Variable(3, Boolean) = Call id(2), args( Result(1), )
                Variable(4, Boolean) = Icmp Eq, Variable(3, Boolean), Bool(false)
                Branch Variable(4, Boolean), 5, 6
            Block 2:Block:
                Variable(2, Integer) = Store Integer(0)
                Jump(1)
            Block 3:Block:
                Variable(2, Integer) = Store Integer(1)
                Jump(1)
            Block 4:Block:
                Call id(4), args( Integer(2), EmptyTag, )
                Call id(5), args( Variable(2, Integer), Tag(0, 5), )
                Call id(5), args( Variable(5, Integer), Tag(1, 5), )
                Return
            Block 5:Block:
                Variable(5, Integer) = Store Integer(0)
                Jump(4)
            Block 6:Block:
                Variable(5, Integer) = Store Integer(1)
                Jump(4)"#]],
    );
}

#[test]
fn if_expression_with_dynamic_operand_from_hybrid_integers_array() {
    let program = get_rir_program(indoc! {r#"
        @EntryPoint()
        operation Main() : Int[] {
            mutable integers = [0, 0];
            use (a, b) = (Qubit(), Qubit());
            set integers w/= 0 <- MResetZ(a) == Zero ? 0 | 1;
            // Use a dynamic integer in the condition.
            if integers[0] == 0 {
                X(b);
            }
            set integers w/= 1 <- MResetZ(b) == Zero ? 0 | 1;
            integers
        }
        "#
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
    let x_callable_id = CallableId(3);
    assert_callable(
        &program,
        x_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__qis__x__body
                call_type: Regular
                input_type:
                    [0]: Qubit
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let record_array_callable_id = CallableId(4);
    assert_callable(
        &program,
        record_array_callable_id,
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
    let record_int_callable_id = CallableId(5);
    assert_callable(
        &program,
        record_int_callable_id,
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
                Call id(1), args( Qubit(0), Result(0), )
                Variable(0, Boolean) = Call id(2), args( Result(0), )
                Variable(1, Boolean) = Icmp Eq, Variable(0, Boolean), Bool(false)
                Branch Variable(1, Boolean), 2, 3
            Block 1:Block:
                Variable(3, Boolean) = Icmp Eq, Variable(2, Integer), Integer(0)
                Branch Variable(3, Boolean), 5, 4
            Block 2:Block:
                Variable(2, Integer) = Store Integer(0)
                Jump(1)
            Block 3:Block:
                Variable(2, Integer) = Store Integer(1)
                Jump(1)
            Block 4:Block:
                Call id(1), args( Qubit(1), Result(1), )
                Variable(4, Boolean) = Call id(2), args( Result(1), )
                Variable(5, Boolean) = Icmp Eq, Variable(4, Boolean), Bool(false)
                Branch Variable(5, Boolean), 7, 8
            Block 5:Block:
                Call id(3), args( Qubit(1), )
                Jump(4)
            Block 6:Block:
                Call id(4), args( Integer(2), EmptyTag, )
                Call id(5), args( Variable(2, Integer), Tag(0, 5), )
                Call id(5), args( Variable(6, Integer), Tag(1, 5), )
                Return
            Block 7:Block:
                Variable(6, Integer) = Store Integer(0)
                Jump(6)
            Block 8:Block:
                Variable(6, Integer) = Store Integer(1)
                Jump(6)"#]],
    );
}

#[test]
fn if_expression_with_classical_operand_from_hybrid_doubles_array() {
    let program = get_rir_program(indoc! {r#"
        @EntryPoint()
        operation Main() : Double[] {
            mutable doubles = [0.0, 0.0];
            use (a, b) = (Qubit(), Qubit());
            set doubles w/= 0 <- MResetZ(a) == Zero ? 0.1 | 1.1;
            // Use a static double in the condition.
            if doubles[1] == 0.0 {
                X(b);
            }
            set doubles w/= 1 <- MResetZ(b) == Zero ? 0.1 | 1.1;
            doubles
        }
        "#
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
    let x_callable_id = CallableId(3);
    assert_callable(
        &program,
        x_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__qis__x__body
                call_type: Regular
                input_type:
                    [0]: Qubit
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let record_array_callable_id = CallableId(4);
    assert_callable(
        &program,
        record_array_callable_id,
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
    let record_int_callable_id = CallableId(5);
    assert_callable(
        &program,
        record_int_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__double_record_output
                call_type: OutputRecording
                input_type:
                    [0]: Double
                    [1]: Pointer
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
                Variable(1, Boolean) = Icmp Eq, Variable(0, Boolean), Bool(false)
                Branch Variable(1, Boolean), 2, 3
            Block 1:Block:
                Call id(3), args( Qubit(1), )
                Call id(1), args( Qubit(1), Result(1), )
                Variable(3, Boolean) = Call id(2), args( Result(1), )
                Variable(4, Boolean) = Icmp Eq, Variable(3, Boolean), Bool(false)
                Branch Variable(4, Boolean), 5, 6
            Block 2:Block:
                Variable(2, Double) = Store Double(0.1)
                Jump(1)
            Block 3:Block:
                Variable(2, Double) = Store Double(1.1)
                Jump(1)
            Block 4:Block:
                Call id(4), args( Integer(2), EmptyTag, )
                Call id(5), args( Variable(2, Double), Tag(0, 5), )
                Call id(5), args( Variable(5, Double), Tag(1, 5), )
                Return
            Block 5:Block:
                Variable(5, Double) = Store Double(0.1)
                Jump(4)
            Block 6:Block:
                Variable(5, Double) = Store Double(1.1)
                Jump(4)"#]],
    );
}

#[test]
fn if_expression_with_dynamic_operand_from_hybrid_doubles_array() {
    let program = get_rir_program(indoc! {r#"
        @EntryPoint()
        operation Main() : Double[] {
            mutable doubles = [0.0, 0.0];
            use (a, b) = (Qubit(), Qubit());
            set doubles w/= 0 <- MResetZ(a) == Zero ? 0.1 | 1.1;
            // Use a dynamic double in the condition.
            if doubles[0] == 0.0 {
                X(b);
            }
            set doubles w/= 1 <- MResetZ(b) == Zero ? 0.1 | 1.1;
            doubles
        }
        "#
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
    let x_callable_id = CallableId(3);
    assert_callable(
        &program,
        x_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__qis__x__body
                call_type: Regular
                input_type:
                    [0]: Qubit
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let record_array_callable_id = CallableId(4);
    assert_callable(
        &program,
        record_array_callable_id,
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
    let record_int_callable_id = CallableId(5);
    assert_callable(
        &program,
        record_int_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__double_record_output
                call_type: OutputRecording
                input_type:
                    [0]: Double
                    [1]: Pointer
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
                Variable(1, Boolean) = Icmp Eq, Variable(0, Boolean), Bool(false)
                Branch Variable(1, Boolean), 2, 3
            Block 1:Block:
                Variable(3, Boolean) = Fcmp Oeq, Variable(2, Double), Double(0)
                Branch Variable(3, Boolean), 5, 4
            Block 2:Block:
                Variable(2, Double) = Store Double(0.1)
                Jump(1)
            Block 3:Block:
                Variable(2, Double) = Store Double(1.1)
                Jump(1)
            Block 4:Block:
                Call id(1), args( Qubit(1), Result(1), )
                Variable(4, Boolean) = Call id(2), args( Result(1), )
                Variable(5, Boolean) = Icmp Eq, Variable(4, Boolean), Bool(false)
                Branch Variable(5, Boolean), 7, 8
            Block 5:Block:
                Call id(3), args( Qubit(1), )
                Jump(4)
            Block 6:Block:
                Call id(4), args( Integer(2), EmptyTag, )
                Call id(5), args( Variable(2, Double), Tag(0, 5), )
                Call id(5), args( Variable(6, Double), Tag(1, 5), )
                Return
            Block 7:Block:
                Variable(6, Double) = Store Double(0.1)
                Jump(6)
            Block 8:Block:
                Variable(6, Double) = Store Double(1.1)
                Jump(6)"#]],
    );
}

#[test]
fn if_expression_with_implicit_return_in_callable_supported() {
    let program = get_rir_program(indoc! {r#"
        function Choose(r : Result) : Int {
            if r == One {
                1
            } else {
                0
            }
        }
        @EntryPoint()
        operation Main() : Int {
            use q = Qubit();
            Choose(MResetZ(q))
        }
        "#
    });

    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Qubit(0), Result(0), )
                Variable(0, Boolean) = Call id(2), args( Result(0), )
                Variable(1, Boolean) = Store Variable(0, Boolean)
                Branch Variable(1, Boolean), 2, 3
            Block 1:Block:
                Variable(3, Integer) = Store Variable(2, Integer)
                Call id(3), args( Variable(3, Integer), Tag(0, 3), )
                Return
            Block 2:Block:
                Variable(2, Integer) = Store Integer(1)
                Jump(1)
            Block 3:Block:
                Variable(2, Integer) = Store Integer(0)
                Jump(1)"#]],
    );
}

#[test]
fn if_expression_with_explicit_return_in_callable_fails() {
    let error = get_partial_evaluation_error(indoc! {r#"
        function Choose(r : Result) : Int {
            if r == One {
                return 1;
            } else {
                return 0;
            }
        }
        @EntryPoint()
        operation Main() : Int {
            use q = Qubit();
            Choose(MResetZ(q))
        }
        "#
    });

    assert_error(
        &error,
        &expect![[
            r#"Unimplemented("early return", PackageSpan { package: PackageId(2), span: Span { lo: 53, hi: 78 } })"#
        ]],
    );
}

#[test]
fn if_expression_with_explicit_return_in_one_branch_and_fallthrough_else_in_callable_fails() {
    let error = get_partial_evaluation_error(indoc! {r#"
        function Choose(r : Result) : Int {
            if r == One {
                return 1;
            }

            return 3;
        }
        @EntryPoint()
        operation Main() : Int {
            use q = Qubit();
            Choose(MResetZ(q))
        }
        "#
    });

    assert_error(
        &error,
        &expect![[
            r#"Unimplemented("early return", PackageSpan { package: PackageId(2), span: Span { lo: 53, hi: 78 } })"#
        ]],
    );
}
