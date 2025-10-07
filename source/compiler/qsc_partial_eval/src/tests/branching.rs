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
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Qubit(0), ) !dbg package_id=2 span=[0-0] scope=1 scope_package_id=2 scope_span=[163-194] callable=Main
                Call id(2), args( Integer(0), Pointer, ) !dbg package_id=2 span=[106-110]
                Return !dbg package_id=2 span=[106-110]"#]],
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
                Call id(1), args( Integer(0), Pointer, ) !dbg package_id=2 span=[106-110]
                Return !dbg package_id=2 span=[106-110]"#]],
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
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Qubit(0), ) !dbg package_id=2 span=[0-0] scope=1 scope_package_id=2 scope_span=[219-250] callable=Main
                Call id(2), args( Integer(0), Pointer, ) !dbg package_id=2 span=[162-166]
                Return !dbg package_id=2 span=[162-166]"#]],
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
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Qubit(0), ) !dbg package_id=2 span=[0-0] scope=2 scope_package_id=2 scope_span=[257-288] callable=Main
                Call id(2), args( Integer(0), Pointer, ) !dbg package_id=2 span=[162-166]
                Return !dbg package_id=2 span=[162-166]"#]],
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
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Qubit(0), ) !dbg package_id=2 span=[0-0] scope=2 scope_package_id=2 scope_span=[318-349] callable=Main
                Call id(2), args( Integer(0), Pointer, ) !dbg package_id=2 span=[218-222]
                Return !dbg package_id=2 span=[218-222]"#]],
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
    let op_a_callable_id = CallableId(3);
    assert_callable(
        &program,
        op_a_callable_id,
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[0-0] scope=0 scope_package_id=2 scope_span=[120-269] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[222-231] scope=0 scope_package_id=2 scope_span=[120-269] callable=Main
                Variable(1, Boolean) = Icmp Eq, Variable(0, Boolean), Bool(false) !dbg package_id=2 span=[222-231] scope=0 scope_package_id=2 scope_span=[120-269] callable=Main
                Branch Variable(1, Boolean), 2, 1 !dbg package_id=2 span=[222-231] scope=0 scope_package_id=2 scope_span=[120-269] callable=Main
            Block 1:Block:
                Call id(4), args( Integer(0), Pointer, ) !dbg package_id=2 span=[106-110]
                Return !dbg package_id=2 span=[106-110]
            Block 2:Block:
                Call id(3), args( Qubit(0), ) !dbg package_id=2 span=[0-0] scope=1 scope_package_id=2 scope_span=[232-263] callable=Main
                Jump(1) !dbg package_id=2 span=[232-263] scope=0 scope_package_id=2 scope_span=[120-269] callable=Main"#]],
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
    let op_a_callable_id = CallableId(3);
    assert_callable(
        &program,
        op_a_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__read_result
                call_type: Readout
                input_type:
                    [0]: Result
                output_type: Boolean
                body: <NONE>"#]],
    );
    let op_b_callable_id = CallableId(4);
    assert_callable(
        &program,
        op_b_callable_id,
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[0-0] scope=0 scope_package_id=2 scope_span=[176-361] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[278-286] scope=0 scope_package_id=2 scope_span=[176-361] callable=Main
                Variable(1, Boolean) = Store Variable(0, Boolean) !dbg package_id=2 span=[278-286] scope=0 scope_package_id=2 scope_span=[176-361] callable=Main
                Branch Variable(1, Boolean), 2, 3 !dbg package_id=2 span=[278-286] scope=0 scope_package_id=2 scope_span=[176-361] callable=Main
            Block 1:Block:
                Call id(5), args( Integer(0), Pointer, ) !dbg package_id=2 span=[162-166]
                Return !dbg package_id=2 span=[162-166]
            Block 2:Block:
                Call id(3), args( Qubit(0), ) !dbg package_id=2 span=[0-0] scope=1 scope_package_id=2 scope_span=[287-318] callable=Main
                Jump(1) !dbg package_id=2 span=[287-318] scope=0 scope_package_id=2 scope_span=[176-361] callable=Main
            Block 3:Block:
                Call id(4), args( Qubit(0), ) !dbg package_id=2 span=[0-0] scope=2 scope_package_id=2 scope_span=[324-355] callable=Main
                Jump(1) !dbg package_id=2 span=[319-355] scope=0 scope_package_id=2 scope_span=[176-361] callable=Main"#]],
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
    let op_a_callable_id = CallableId(3);
    assert_callable(
        &program,
        op_a_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__read_result
                call_type: Readout
                input_type:
                    [0]: Result
                output_type: Boolean
                body: <NONE>"#]],
    );
    let op_b_callable_id = CallableId(4);
    assert_callable(
        &program,
        op_b_callable_id,
        &expect![[r#"
            Callable:
                name: opA
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[0-0] scope=0 scope_package_id=2 scope_span=[232-567] callable=Main
                Call id(1), args( Qubit(1), Result(1), ) !dbg package_id=2 span=[0-0] scope=0 scope_package_id=2 scope_span=[232-567] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[433-442] scope=0 scope_package_id=2 scope_span=[232-567] callable=Main
                Variable(1, Boolean) = Store Variable(0, Boolean) !dbg package_id=2 span=[433-442] scope=0 scope_package_id=2 scope_span=[232-567] callable=Main
                Branch Variable(1, Boolean), 2, 3 !dbg package_id=2 span=[433-442] scope=0 scope_package_id=2 scope_span=[232-567] callable=Main
            Block 1:Block:
                Call id(6), args( Integer(0), Pointer, ) !dbg package_id=2 span=[218-222]
                Return !dbg package_id=2 span=[218-222]
            Block 2:Block:
                Call id(3), args( Qubit(2), ) !dbg package_id=2 span=[0-0] scope=1 scope_package_id=2 scope_span=[443-475] callable=Main
                Jump(1) !dbg package_id=2 span=[443-475] scope=0 scope_package_id=2 scope_span=[232-567] callable=Main
            Block 3:Block:
                Variable(2, Boolean) = Call id(2), args( Result(1), ) !dbg package_id=2 span=[481-490] scope=0 scope_package_id=2 scope_span=[232-567] callable=Main
                Variable(3, Boolean) = Store Variable(2, Boolean) !dbg package_id=2 span=[481-490] scope=0 scope_package_id=2 scope_span=[232-567] callable=Main
                Branch Variable(3, Boolean), 5, 6 !dbg package_id=2 span=[481-490] scope=0 scope_package_id=2 scope_span=[232-567] callable=Main
            Block 4:Block:
                Jump(1) !dbg package_id=2 span=[476-561] scope=0 scope_package_id=2 scope_span=[232-567] callable=Main
            Block 5:Block:
                Call id(4), args( Qubit(2), ) !dbg package_id=2 span=[0-0] scope=2 scope_package_id=2 scope_span=[491-523] callable=Main
                Jump(4) !dbg package_id=2 span=[491-523] scope=0 scope_package_id=2 scope_span=[232-567] callable=Main
            Block 6:Block:
                Call id(5), args( Qubit(2), ) !dbg package_id=2 span=[0-0] scope=3 scope_package_id=2 scope_span=[529-561] callable=Main
                Jump(4) !dbg package_id=2 span=[524-561] scope=0 scope_package_id=2 scope_span=[232-567] callable=Main"#]],
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
    let op_a_callable_id = CallableId(3);
    assert_callable(
        &program,
        op_a_callable_id,
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[0-0] scope=0 scope_package_id=2 scope_span=[120-309] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[222-231] scope=0 scope_package_id=2 scope_span=[120-309] callable=Main
                Variable(1, Boolean) = Icmp Eq, Variable(0, Boolean), Bool(false) !dbg package_id=2 span=[222-231] scope=0 scope_package_id=2 scope_span=[120-309] callable=Main
                Branch Variable(1, Boolean), 2, 1 !dbg package_id=2 span=[222-231] scope=0 scope_package_id=2 scope_span=[120-309] callable=Main
            Block 1:Block:
                Call id(4), args( Integer(0), Pointer, ) !dbg package_id=2 span=[106-110]
                Return !dbg package_id=2 span=[106-110]
            Block 2:Block:
                Call id(3), args( Qubit(0), ) !dbg package_id=2 span=[0-0] scope=2 scope_package_id=2 scope_span=[254-293] callable=Main
                Jump(1) !dbg package_id=2 span=[232-303] scope=0 scope_package_id=2 scope_span=[120-309] callable=Main"#]],
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

    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[0-0] scope=0 scope_package_id=2 scope_span=[120-310] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[222-231] scope=0 scope_package_id=2 scope_span=[120-310] callable=Main
                Variable(1, Boolean) = Icmp Eq, Variable(0, Boolean), Bool(false) !dbg package_id=2 span=[222-231] scope=0 scope_package_id=2 scope_span=[120-310] callable=Main
                Branch Variable(1, Boolean), 2, 1 !dbg package_id=2 span=[222-231] scope=0 scope_package_id=2 scope_span=[120-310] callable=Main
            Block 1:Block:
                Call id(3), args( Integer(0), Pointer, ) !dbg package_id=2 span=[106-110]
                Return !dbg package_id=2 span=[106-110]
            Block 2:Block:
                Jump(1) !dbg package_id=2 span=[232-304] scope=0 scope_package_id=2 scope_span=[120-310] callable=Main"#]],
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
    let op_a_callable_id = CallableId(3);
    assert_callable(
        &program,
        op_a_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__read_result
                call_type: Readout
                input_type:
                    [0]: Result
                output_type: Boolean
                body: <NONE>"#]],
    );
    let op_b_callable_id = CallableId(4);
    assert_callable(
        &program,
        op_b_callable_id,
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[0-0] scope=0 scope_package_id=2 scope_span=[176-401] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[278-286] scope=0 scope_package_id=2 scope_span=[176-401] callable=Main
                Variable(1, Boolean) = Store Variable(0, Boolean) !dbg package_id=2 span=[278-286] scope=0 scope_package_id=2 scope_span=[176-401] callable=Main
                Branch Variable(1, Boolean), 2, 3 !dbg package_id=2 span=[278-286] scope=0 scope_package_id=2 scope_span=[176-401] callable=Main
            Block 1:Block:
                Call id(5), args( Integer(0), Pointer, ) !dbg package_id=2 span=[162-166]
                Return !dbg package_id=2 span=[162-166]
            Block 2:Block:
                Call id(3), args( Qubit(0), ) !dbg package_id=2 span=[0-0] scope=1 scope_package_id=2 scope_span=[287-318] callable=Main
                Jump(1) !dbg package_id=2 span=[287-318] scope=0 scope_package_id=2 scope_span=[176-401] callable=Main
            Block 3:Block:
                Call id(4), args( Qubit(0), ) !dbg package_id=2 span=[0-0] scope=3 scope_package_id=2 scope_span=[346-385] callable=Main
                Jump(1) !dbg package_id=2 span=[319-395] scope=0 scope_package_id=2 scope_span=[176-401] callable=Main"#]],
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
    let op_a_callable_id = CallableId(3);
    assert_callable(
        &program,
        op_a_callable_id,
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[0-0] scope=0 scope_package_id=2 scope_span=[176-402] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[278-286] scope=0 scope_package_id=2 scope_span=[176-402] callable=Main
                Variable(1, Boolean) = Store Variable(0, Boolean) !dbg package_id=2 span=[278-286] scope=0 scope_package_id=2 scope_span=[176-402] callable=Main
                Branch Variable(1, Boolean), 2, 3 !dbg package_id=2 span=[278-286] scope=0 scope_package_id=2 scope_span=[176-402] callable=Main
            Block 1:Block:
                Call id(4), args( Integer(0), Pointer, ) !dbg package_id=2 span=[162-166]
                Return !dbg package_id=2 span=[162-166]
            Block 2:Block:
                Call id(3), args( Qubit(0), ) !dbg package_id=2 span=[0-0] scope=1 scope_package_id=2 scope_span=[287-318] callable=Main
                Jump(1) !dbg package_id=2 span=[287-318] scope=0 scope_package_id=2 scope_span=[176-402] callable=Main
            Block 3:Block:
                Jump(1) !dbg package_id=2 span=[319-396] scope=0 scope_package_id=2 scope_span=[176-402] callable=Main"#]],
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
    let op_a_callable_id = CallableId(3);
    assert_callable(
        &program,
        op_a_callable_id,
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[0-0] scope=0 scope_package_id=2 scope_span=[120-415] callable=Main
                Call id(1), args( Qubit(1), Result(1), ) !dbg package_id=2 span=[0-0] scope=0 scope_package_id=2 scope_span=[120-415] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[321-331] scope=0 scope_package_id=2 scope_span=[120-415] callable=Main
                Variable(1, Boolean) = Icmp Eq, Variable(0, Boolean), Bool(false) !dbg package_id=2 span=[321-331] scope=0 scope_package_id=2 scope_span=[120-415] callable=Main
                Branch Variable(1, Boolean), 2, 1 !dbg package_id=2 span=[321-331] scope=0 scope_package_id=2 scope_span=[120-415] callable=Main
            Block 1:Block:
                Call id(4), args( Integer(0), Pointer, ) !dbg package_id=2 span=[106-110]
                Return !dbg package_id=2 span=[106-110]
            Block 2:Block:
                Variable(2, Boolean) = Call id(2), args( Result(1), ) !dbg package_id=2 span=[349-358] scope=1 scope_package_id=2 scope_span=[332-409] callable=Main
                Variable(3, Boolean) = Store Variable(2, Boolean) !dbg package_id=2 span=[349-358] scope=1 scope_package_id=2 scope_span=[332-409] callable=Main
                Branch Variable(3, Boolean), 4, 3 !dbg package_id=2 span=[349-358] scope=1 scope_package_id=2 scope_span=[332-409] callable=Main
            Block 3:Block:
                Jump(1) !dbg package_id=2 span=[332-409] scope=0 scope_package_id=2 scope_span=[120-415] callable=Main
            Block 4:Block:
                Call id(3), args( Qubit(2), ) !dbg package_id=2 span=[0-0] scope=2 scope_package_id=2 scope_span=[359-399] callable=Main
                Jump(3) !dbg package_id=2 span=[359-399] scope=1 scope_package_id=2 scope_span=[332-409] callable=Main"#]],
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
    let op_a_callable_id = CallableId(3);
    assert_callable(
        &program,
        op_a_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__read_result
                call_type: Readout
                input_type:
                    [0]: Result
                output_type: Boolean
                body: <NONE>"#]],
    );
    let op_b_callable_id = CallableId(4);
    assert_callable(
        &program,
        op_b_callable_id,
        &expect![[r#"
            Callable:
                name: opA
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
                name: opB
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[0-0] scope=0 scope_package_id=2 scope_span=[288-758] callable=Main
                Call id(1), args( Qubit(1), Result(1), ) !dbg package_id=2 span=[0-0] scope=0 scope_package_id=2 scope_span=[288-758] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[489-499] scope=0 scope_package_id=2 scope_span=[288-758] callable=Main
                Variable(1, Boolean) = Icmp Eq, Variable(0, Boolean), Bool(false) !dbg package_id=2 span=[489-499] scope=0 scope_package_id=2 scope_span=[288-758] callable=Main
                Branch Variable(1, Boolean), 2, 6 !dbg package_id=2 span=[489-499] scope=0 scope_package_id=2 scope_span=[288-758] callable=Main
            Block 1:Block:
                Call id(7), args( Integer(0), Pointer, ) !dbg package_id=2 span=[274-278]
                Return !dbg package_id=2 span=[274-278]
            Block 2:Block:
                Variable(2, Boolean) = Call id(2), args( Result(1), ) !dbg package_id=2 span=[517-527] scope=1 scope_package_id=2 scope_span=[500-624] callable=Main
                Variable(3, Boolean) = Icmp Eq, Variable(2, Boolean), Bool(false) !dbg package_id=2 span=[517-527] scope=1 scope_package_id=2 scope_span=[500-624] callable=Main
                Branch Variable(3, Boolean), 4, 5 !dbg package_id=2 span=[517-527] scope=1 scope_package_id=2 scope_span=[500-624] callable=Main
            Block 3:Block:
                Jump(1) !dbg package_id=2 span=[500-624] scope=0 scope_package_id=2 scope_span=[288-758] callable=Main
            Block 4:Block:
                Call id(3), args( Qubit(2), ) !dbg package_id=2 span=[0-0] scope=2 scope_package_id=2 scope_span=[528-568] callable=Main
                Jump(3) !dbg package_id=2 span=[528-568] scope=1 scope_package_id=2 scope_span=[500-624] callable=Main
            Block 5:Block:
                Call id(4), args( Qubit(2), ) !dbg package_id=2 span=[0-0] scope=3 scope_package_id=2 scope_span=[574-614] callable=Main
                Jump(3) !dbg package_id=2 span=[569-614] scope=1 scope_package_id=2 scope_span=[500-624] callable=Main
            Block 6:Block:
                Variable(4, Boolean) = Call id(2), args( Result(1), ) !dbg package_id=2 span=[647-656] scope=4 scope_package_id=2 scope_span=[630-752] callable=Main
                Variable(5, Boolean) = Store Variable(4, Boolean) !dbg package_id=2 span=[647-656] scope=4 scope_package_id=2 scope_span=[630-752] callable=Main
                Branch Variable(5, Boolean), 8, 9 !dbg package_id=2 span=[647-656] scope=4 scope_package_id=2 scope_span=[630-752] callable=Main
            Block 7:Block:
                Jump(1) !dbg package_id=2 span=[625-752] scope=0 scope_package_id=2 scope_span=[288-758] callable=Main
            Block 8:Block:
                Call id(5), args( Qubit(2), ) !dbg package_id=2 span=[0-0] scope=5 scope_package_id=2 scope_span=[656-696] callable=Main
                Jump(7) !dbg package_id=2 span=[656-696] scope=4 scope_package_id=2 scope_span=[630-752] callable=Main
            Block 9:Block:
                Call id(6), args( Qubit(2), ) !dbg package_id=2 span=[0-0] scope=6 scope_package_id=2 scope_span=[702-742] callable=Main
                Jump(7) !dbg package_id=2 span=[697-742] scope=4 scope_package_id=2 scope_span=[630-752] callable=Main"#]],
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
    let op_a_callable_id = CallableId(3);
    assert_callable(
        &program,
        op_a_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__read_result
                call_type: Readout
                input_type:
                    [0]: Result
                output_type: Boolean
                body: <NONE>"#]],
    );
    let op_b_callable_id = CallableId(4);
    assert_callable(
        &program,
        op_b_callable_id,
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[0-0] scope=0 scope_package_id=2 scope_span=[176-341] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[278-287] scope=0 scope_package_id=2 scope_span=[176-341] callable=Main
                Variable(1, Boolean) = Icmp Eq, Variable(0, Boolean), Bool(false) !dbg package_id=2 span=[278-287] scope=0 scope_package_id=2 scope_span=[176-341] callable=Main
                Branch Variable(1, Boolean), 2, 1 !dbg package_id=2 span=[278-287] scope=0 scope_package_id=2 scope_span=[176-341] callable=Main
            Block 1:Block:
                Call id(4), args( Qubit(0), ) !dbg package_id=2 span=[0-0] scope=0 scope_package_id=2 scope_span=[176-341] callable=Main
                Call id(5), args( Integer(0), Pointer, ) !dbg package_id=2 span=[162-166]
                Return !dbg package_id=2 span=[162-166]
            Block 2:Block:
                Call id(3), args( Qubit(0), ) !dbg package_id=2 span=[0-0] scope=1 scope_package_id=2 scope_span=[288-319] callable=Main
                Jump(1) !dbg package_id=2 span=[288-319] scope=0 scope_package_id=2 scope_span=[176-341] callable=Main"#]],
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
    let op_a_callable_id = CallableId(3);
    assert_callable(
        &program,
        op_a_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__read_result
                call_type: Readout
                input_type:
                    [0]: Result
                output_type: Boolean
                body: <NONE>"#]],
    );
    let op_b_callable_id = CallableId(4);
    assert_callable(
        &program,
        op_b_callable_id,
        &expect![[r#"
            Callable:
                name: opA
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[0-0] scope=0 scope_package_id=2 scope_span=[232-433] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[334-342] scope=0 scope_package_id=2 scope_span=[232-433] callable=Main
                Variable(1, Boolean) = Store Variable(0, Boolean) !dbg package_id=2 span=[334-342] scope=0 scope_package_id=2 scope_span=[232-433] callable=Main
                Branch Variable(1, Boolean), 2, 3 !dbg package_id=2 span=[334-342] scope=0 scope_package_id=2 scope_span=[232-433] callable=Main
            Block 1:Block:
                Call id(5), args( Qubit(0), ) !dbg package_id=2 span=[0-0] scope=0 scope_package_id=2 scope_span=[232-433] callable=Main
                Call id(6), args( Integer(0), Pointer, ) !dbg package_id=2 span=[218-222]
                Return !dbg package_id=2 span=[218-222]
            Block 2:Block:
                Call id(3), args( Qubit(0), ) !dbg package_id=2 span=[0-0] scope=1 scope_package_id=2 scope_span=[343-374] callable=Main
                Jump(1) !dbg package_id=2 span=[343-374] scope=0 scope_package_id=2 scope_span=[232-433] callable=Main
            Block 3:Block:
                Call id(4), args( Qubit(0), ) !dbg package_id=2 span=[0-0] scope=2 scope_package_id=2 scope_span=[380-411] callable=Main
                Jump(1) !dbg package_id=2 span=[375-411] scope=0 scope_package_id=2 scope_span=[232-433] callable=Main"#]],
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
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let x_callable_id = CallableId(2);
    assert_callable(
        &program,
        x_callable_id,
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
    let record_array_callable_id = CallableId(3);
    assert_callable(
        &program,
        record_array_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__qis__x__body
                call_type: Regular
                input_type:
                    [0]: Qubit
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let record_result_callable_id = CallableId(4);
    assert_callable(
        &program,
        record_result_callable_id,
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
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[153-163] scope=0 scope_package_id=2 scope_span=[43-323] callable=Main
                Call id(2), args( Qubit(1), ) !dbg package_id=2 span=[251-255] scope=1 scope_package_id=2 scope_span=[241-262] callable=Main
                Call id(1), args( Qubit(1), Result(1), ) !dbg package_id=2 span=[293-303] scope=0 scope_package_id=2 scope_span=[43-323] callable=Main
                Call id(3), args( Integer(2), Pointer, ) !dbg package_id=2 span=[25-29]
                Call id(4), args( Result(0), Pointer, ) !dbg package_id=2 span=[25-29]
                Call id(4), args( Result(1), Pointer, ) !dbg package_id=2 span=[25-29]
                Return !dbg package_id=2 span=[25-29]"#]],
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
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let x_callable_id = CallableId(2);
    assert_callable(
        &program,
        x_callable_id,
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
    let record_array_callable_id = CallableId(3);
    assert_callable(
        &program,
        record_array_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__qis__x__body
                call_type: Regular
                input_type:
                    [0]: Qubit
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let record_result_callable_id = CallableId(4);
    assert_callable(
        &program,
        record_result_callable_id,
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
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[153-163] scope=0 scope_package_id=2 scope_span=[43-322] callable=Main
                Call id(2), args( Qubit(1), ) !dbg package_id=2 span=[250-254] scope=1 scope_package_id=2 scope_span=[240-261] callable=Main
                Call id(1), args( Qubit(1), Result(1), ) !dbg package_id=2 span=[292-302] scope=0 scope_package_id=2 scope_span=[43-322] callable=Main
                Call id(3), args( Integer(2), Pointer, ) !dbg package_id=2 span=[25-29]
                Call id(4), args( Result(0), Pointer, ) !dbg package_id=2 span=[25-29]
                Call id(4), args( Result(1), Pointer, ) !dbg package_id=2 span=[25-29]
                Return !dbg package_id=2 span=[25-29]"#]],
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
    let x_callable_id = CallableId(3);
    assert_callable(
        &program,
        x_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__read_result
                call_type: Readout
                input_type:
                    [0]: Result
                output_type: Boolean
                body: <NONE>"#]],
    );
    let record_array_callable_id = CallableId(4);
    assert_callable(
        &program,
        record_array_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__qis__x__body
                call_type: Regular
                input_type:
                    [0]: Qubit
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let record_result_callable_id = CallableId(5);
    assert_callable(
        &program,
        record_result_callable_id,
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
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[153-163] scope=0 scope_package_id=2 scope_span=[43-324] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[218-241] scope=0 scope_package_id=2 scope_span=[43-324] callable=Main
                Variable(1, Boolean) = Icmp Eq, Variable(0, Boolean), Bool(false) !dbg package_id=2 span=[218-241] scope=0 scope_package_id=2 scope_span=[43-324] callable=Main
                Branch Variable(1, Boolean), 2, 1 !dbg package_id=2 span=[218-241] scope=0 scope_package_id=2 scope_span=[43-324] callable=Main
            Block 1:Block:
                Call id(1), args( Qubit(1), Result(1), ) !dbg package_id=2 span=[294-304] scope=0 scope_package_id=2 scope_span=[43-324] callable=Main
                Call id(4), args( Integer(2), Pointer, ) !dbg package_id=2 span=[25-29]
                Call id(5), args( Result(0), Pointer, ) !dbg package_id=2 span=[25-29]
                Call id(5), args( Result(1), Pointer, ) !dbg package_id=2 span=[25-29]
                Return !dbg package_id=2 span=[25-29]
            Block 2:Block:
                Call id(3), args( Qubit(1), ) !dbg package_id=2 span=[252-256] scope=1 scope_package_id=2 scope_span=[242-263] callable=Main
                Jump(1) !dbg package_id=2 span=[242-263] scope=0 scope_package_id=2 scope_span=[43-324] callable=Main"#]],
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
    let x_callable_id = CallableId(3);
    assert_callable(
        &program,
        x_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__read_result
                call_type: Readout
                input_type:
                    [0]: Result
                output_type: Boolean
                body: <NONE>"#]],
    );
    let record_array_callable_id = CallableId(4);
    assert_callable(
        &program,
        record_array_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__qis__x__body
                call_type: Regular
                input_type:
                    [0]: Qubit
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let record_bool_callable_id = CallableId(5);
    assert_callable(
        &program,
        record_bool_callable_id,
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
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[139-149] scope=0 scope_package_id=2 scope_span=[41-304] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[139-156] scope=0 scope_package_id=2 scope_span=[41-304] callable=Main
                Variable(1, Boolean) = Store Variable(0, Boolean) !dbg package_id=2 span=[139-156] scope=0 scope_package_id=2 scope_span=[41-304] callable=Main
                Call id(3), args( Qubit(1), ) !dbg package_id=2 span=[239-243] scope=1 scope_package_id=2 scope_span=[229-250] callable=Main
                Call id(1), args( Qubit(1), Result(1), ) !dbg package_id=2 span=[274-284] scope=0 scope_package_id=2 scope_span=[41-304] callable=Main
                Variable(2, Boolean) = Call id(2), args( Result(1), ) !dbg package_id=2 span=[274-291] scope=0 scope_package_id=2 scope_span=[41-304] callable=Main
                Variable(3, Boolean) = Store Variable(2, Boolean) !dbg package_id=2 span=[274-291] scope=0 scope_package_id=2 scope_span=[41-304] callable=Main
                Call id(4), args( Integer(2), Pointer, ) !dbg package_id=2 span=[25-29]
                Call id(5), args( Variable(1, Boolean), Pointer, ) !dbg package_id=2 span=[25-29]
                Call id(5), args( Variable(3, Boolean), Pointer, ) !dbg package_id=2 span=[25-29]
                Return !dbg package_id=2 span=[25-29]"#]],
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
    let x_callable_id = CallableId(3);
    assert_callable(
        &program,
        x_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__read_result
                call_type: Readout
                input_type:
                    [0]: Result
                output_type: Boolean
                body: <NONE>"#]],
    );
    let record_array_callable_id = CallableId(4);
    assert_callable(
        &program,
        record_array_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__qis__x__body
                call_type: Regular
                input_type:
                    [0]: Qubit
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let record_bool_callable_id = CallableId(5);
    assert_callable(
        &program,
        record_bool_callable_id,
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
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[139-149] scope=0 scope_package_id=2 scope_span=[41-296] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[139-156] scope=0 scope_package_id=2 scope_span=[41-296] callable=Main
                Variable(1, Boolean) = Store Variable(0, Boolean) !dbg package_id=2 span=[139-156] scope=0 scope_package_id=2 scope_span=[41-296] callable=Main
                Branch Variable(1, Boolean), 2, 1 !dbg package_id=2 span=[212-220] scope=0 scope_package_id=2 scope_span=[41-296] callable=Main
            Block 1:Block:
                Call id(1), args( Qubit(1), Result(1), ) !dbg package_id=2 span=[266-276] scope=0 scope_package_id=2 scope_span=[41-296] callable=Main
                Variable(2, Boolean) = Call id(2), args( Result(1), ) !dbg package_id=2 span=[266-283] scope=0 scope_package_id=2 scope_span=[41-296] callable=Main
                Variable(3, Boolean) = Store Variable(2, Boolean) !dbg package_id=2 span=[266-283] scope=0 scope_package_id=2 scope_span=[41-296] callable=Main
                Call id(4), args( Integer(2), Pointer, ) !dbg package_id=2 span=[25-29]
                Call id(5), args( Variable(1, Boolean), Pointer, ) !dbg package_id=2 span=[25-29]
                Call id(5), args( Variable(3, Boolean), Pointer, ) !dbg package_id=2 span=[25-29]
                Return !dbg package_id=2 span=[25-29]
            Block 2:Block:
                Call id(3), args( Qubit(1), ) !dbg package_id=2 span=[231-235] scope=1 scope_package_id=2 scope_span=[221-242] callable=Main
                Jump(1) !dbg package_id=2 span=[221-242] scope=0 scope_package_id=2 scope_span=[41-296] callable=Main"#]],
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
    let x_callable_id = CallableId(3);
    assert_callable(
        &program,
        x_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__read_result
                call_type: Readout
                input_type:
                    [0]: Result
                output_type: Boolean
                body: <NONE>"#]],
    );
    let record_array_callable_id = CallableId(4);
    assert_callable(
        &program,
        record_array_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__qis__x__body
                call_type: Regular
                input_type:
                    [0]: Qubit
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let record_int_callable_id = CallableId(5);
    assert_callable(
        &program,
        record_int_callable_id,
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
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[136-146] scope=0 scope_package_id=2 scope_span=[40-324] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[136-154] scope=0 scope_package_id=2 scope_span=[40-324] callable=Main
                Variable(1, Boolean) = Icmp Eq, Variable(0, Boolean), Bool(false) !dbg package_id=2 span=[136-154] scope=0 scope_package_id=2 scope_span=[40-324] callable=Main
                Branch Variable(1, Boolean), 2, 3 !dbg package_id=2 span=[136-154] scope=0 scope_package_id=2 scope_span=[40-324] callable=Main
            Block 1:Block:
                Call id(3), args( Qubit(1), ) !dbg package_id=2 span=[244-248] scope=1 scope_package_id=2 scope_span=[234-255] callable=Main
                Call id(1), args( Qubit(1), Result(1), ) !dbg package_id=2 span=[282-292] scope=0 scope_package_id=2 scope_span=[40-324] callable=Main
                Variable(3, Boolean) = Call id(2), args( Result(1), ) !dbg package_id=2 span=[282-300] scope=0 scope_package_id=2 scope_span=[40-324] callable=Main
                Variable(4, Boolean) = Icmp Eq, Variable(3, Boolean), Bool(false) !dbg package_id=2 span=[282-300] scope=0 scope_package_id=2 scope_span=[40-324] callable=Main
                Branch Variable(4, Boolean), 5, 6 !dbg package_id=2 span=[282-300] scope=0 scope_package_id=2 scope_span=[40-324] callable=Main
            Block 2:Block:
                Variable(2, Integer) = Store Integer(0) !dbg package_id=2 span=[157-158] scope=0 scope_package_id=2 scope_span=[40-324] callable=Main
                Jump(1) !dbg package_id=2 span=[157-158] scope=0 scope_package_id=2 scope_span=[40-324] callable=Main
            Block 3:Block:
                Variable(2, Integer) = Store Integer(1) !dbg package_id=2 span=[161-162] scope=0 scope_package_id=2 scope_span=[40-324] callable=Main
                Jump(1) !dbg package_id=2 span=[161-162] scope=0 scope_package_id=2 scope_span=[40-324] callable=Main
            Block 4:Block:
                Call id(4), args( Integer(2), Pointer, ) !dbg package_id=2 span=[25-29]
                Call id(5), args( Variable(2, Integer), Pointer, ) !dbg package_id=2 span=[25-29]
                Call id(5), args( Variable(5, Integer), Pointer, ) !dbg package_id=2 span=[25-29]
                Return !dbg package_id=2 span=[25-29]
            Block 5:Block:
                Variable(5, Integer) = Store Integer(0) !dbg package_id=2 span=[303-304] scope=0 scope_package_id=2 scope_span=[40-324] callable=Main
                Jump(4) !dbg package_id=2 span=[303-304] scope=0 scope_package_id=2 scope_span=[40-324] callable=Main
            Block 6:Block:
                Variable(5, Integer) = Store Integer(1) !dbg package_id=2 span=[307-308] scope=0 scope_package_id=2 scope_span=[40-324] callable=Main
                Jump(4) !dbg package_id=2 span=[307-308] scope=0 scope_package_id=2 scope_span=[40-324] callable=Main"#]],
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
    let x_callable_id = CallableId(3);
    assert_callable(
        &program,
        x_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__read_result
                call_type: Readout
                input_type:
                    [0]: Result
                output_type: Boolean
                body: <NONE>"#]],
    );
    let record_array_callable_id = CallableId(4);
    assert_callable(
        &program,
        record_array_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__qis__x__body
                call_type: Regular
                input_type:
                    [0]: Qubit
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let record_int_callable_id = CallableId(5);
    assert_callable(
        &program,
        record_int_callable_id,
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
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[136-146] scope=0 scope_package_id=2 scope_span=[40-325] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[136-154] scope=0 scope_package_id=2 scope_span=[40-325] callable=Main
                Variable(1, Boolean) = Icmp Eq, Variable(0, Boolean), Bool(false) !dbg package_id=2 span=[136-154] scope=0 scope_package_id=2 scope_span=[40-325] callable=Main
                Branch Variable(1, Boolean), 2, 3 !dbg package_id=2 span=[136-154] scope=0 scope_package_id=2 scope_span=[40-325] callable=Main
            Block 1:Block:
                Variable(3, Boolean) = Icmp Eq, Variable(2, Integer), Integer(0) !dbg package_id=2 span=[218-234] scope=0 scope_package_id=2 scope_span=[40-325] callable=Main
                Branch Variable(3, Boolean), 5, 4 !dbg package_id=2 span=[218-234] scope=0 scope_package_id=2 scope_span=[40-325] callable=Main
            Block 2:Block:
                Variable(2, Integer) = Store Integer(0) !dbg package_id=2 span=[157-158] scope=0 scope_package_id=2 scope_span=[40-325] callable=Main
                Jump(1) !dbg package_id=2 span=[157-158] scope=0 scope_package_id=2 scope_span=[40-325] callable=Main
            Block 3:Block:
                Variable(2, Integer) = Store Integer(1) !dbg package_id=2 span=[161-162] scope=0 scope_package_id=2 scope_span=[40-325] callable=Main
                Jump(1) !dbg package_id=2 span=[161-162] scope=0 scope_package_id=2 scope_span=[40-325] callable=Main
            Block 4:Block:
                Call id(1), args( Qubit(1), Result(1), ) !dbg package_id=2 span=[283-293] scope=0 scope_package_id=2 scope_span=[40-325] callable=Main
                Variable(4, Boolean) = Call id(2), args( Result(1), ) !dbg package_id=2 span=[283-301] scope=0 scope_package_id=2 scope_span=[40-325] callable=Main
                Variable(5, Boolean) = Icmp Eq, Variable(4, Boolean), Bool(false) !dbg package_id=2 span=[283-301] scope=0 scope_package_id=2 scope_span=[40-325] callable=Main
                Branch Variable(5, Boolean), 7, 8 !dbg package_id=2 span=[283-301] scope=0 scope_package_id=2 scope_span=[40-325] callable=Main
            Block 5:Block:
                Call id(3), args( Qubit(1), ) !dbg package_id=2 span=[245-249] scope=1 scope_package_id=2 scope_span=[235-256] callable=Main
                Jump(4) !dbg package_id=2 span=[235-256] scope=0 scope_package_id=2 scope_span=[40-325] callable=Main
            Block 6:Block:
                Call id(4), args( Integer(2), Pointer, ) !dbg package_id=2 span=[25-29]
                Call id(5), args( Variable(2, Integer), Pointer, ) !dbg package_id=2 span=[25-29]
                Call id(5), args( Variable(6, Integer), Pointer, ) !dbg package_id=2 span=[25-29]
                Return !dbg package_id=2 span=[25-29]
            Block 7:Block:
                Variable(6, Integer) = Store Integer(0) !dbg package_id=2 span=[304-305] scope=0 scope_package_id=2 scope_span=[40-325] callable=Main
                Jump(6) !dbg package_id=2 span=[304-305] scope=0 scope_package_id=2 scope_span=[40-325] callable=Main
            Block 8:Block:
                Variable(6, Integer) = Store Integer(1) !dbg package_id=2 span=[308-309] scope=0 scope_package_id=2 scope_span=[40-325] callable=Main
                Jump(6) !dbg package_id=2 span=[308-309] scope=0 scope_package_id=2 scope_span=[40-325] callable=Main"#]],
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
    let x_callable_id = CallableId(3);
    assert_callable(
        &program,
        x_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__read_result
                call_type: Readout
                input_type:
                    [0]: Result
                output_type: Boolean
                body: <NONE>"#]],
    );
    let record_array_callable_id = CallableId(4);
    assert_callable(
        &program,
        record_array_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__qis__x__body
                call_type: Regular
                input_type:
                    [0]: Qubit
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let record_int_callable_id = CallableId(5);
    assert_callable(
        &program,
        record_int_callable_id,
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
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[141-151] scope=0 scope_package_id=2 scope_span=[43-335] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[141-159] scope=0 scope_package_id=2 scope_span=[43-335] callable=Main
                Variable(1, Boolean) = Icmp Eq, Variable(0, Boolean), Bool(false) !dbg package_id=2 span=[141-159] scope=0 scope_package_id=2 scope_span=[43-335] callable=Main
                Branch Variable(1, Boolean), 2, 3 !dbg package_id=2 span=[141-159] scope=0 scope_package_id=2 scope_span=[43-335] callable=Main
            Block 1:Block:
                Call id(3), args( Qubit(1), ) !dbg package_id=2 span=[253-257] scope=1 scope_package_id=2 scope_span=[243-264] callable=Main
                Call id(1), args( Qubit(1), Result(1), ) !dbg package_id=2 span=[290-300] scope=0 scope_package_id=2 scope_span=[43-335] callable=Main
                Variable(3, Boolean) = Call id(2), args( Result(1), ) !dbg package_id=2 span=[290-308] scope=0 scope_package_id=2 scope_span=[43-335] callable=Main
                Variable(4, Boolean) = Icmp Eq, Variable(3, Boolean), Bool(false) !dbg package_id=2 span=[290-308] scope=0 scope_package_id=2 scope_span=[43-335] callable=Main
                Branch Variable(4, Boolean), 5, 6 !dbg package_id=2 span=[290-308] scope=0 scope_package_id=2 scope_span=[43-335] callable=Main
            Block 2:Block:
                Variable(2, Double) = Store Double(0.1) !dbg package_id=2 span=[162-165] scope=0 scope_package_id=2 scope_span=[43-335] callable=Main
                Jump(1) !dbg package_id=2 span=[162-165] scope=0 scope_package_id=2 scope_span=[43-335] callable=Main
            Block 3:Block:
                Variable(2, Double) = Store Double(1.1) !dbg package_id=2 span=[168-171] scope=0 scope_package_id=2 scope_span=[43-335] callable=Main
                Jump(1) !dbg package_id=2 span=[168-171] scope=0 scope_package_id=2 scope_span=[43-335] callable=Main
            Block 4:Block:
                Call id(4), args( Integer(2), Pointer, ) !dbg package_id=2 span=[25-29]
                Call id(5), args( Variable(2, Double), Pointer, ) !dbg package_id=2 span=[25-29]
                Call id(5), args( Variable(5, Double), Pointer, ) !dbg package_id=2 span=[25-29]
                Return !dbg package_id=2 span=[25-29]
            Block 5:Block:
                Variable(5, Double) = Store Double(0.1) !dbg package_id=2 span=[311-314] scope=0 scope_package_id=2 scope_span=[43-335] callable=Main
                Jump(4) !dbg package_id=2 span=[311-314] scope=0 scope_package_id=2 scope_span=[43-335] callable=Main
            Block 6:Block:
                Variable(5, Double) = Store Double(1.1) !dbg package_id=2 span=[317-320] scope=0 scope_package_id=2 scope_span=[43-335] callable=Main
                Jump(4) !dbg package_id=2 span=[317-320] scope=0 scope_package_id=2 scope_span=[43-335] callable=Main"#]],
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
    let x_callable_id = CallableId(3);
    assert_callable(
        &program,
        x_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__read_result
                call_type: Readout
                input_type:
                    [0]: Result
                output_type: Boolean
                body: <NONE>"#]],
    );
    let record_array_callable_id = CallableId(4);
    assert_callable(
        &program,
        record_array_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__qis__x__body
                call_type: Regular
                input_type:
                    [0]: Qubit
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let record_int_callable_id = CallableId(5);
    assert_callable(
        &program,
        record_int_callable_id,
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
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[141-151] scope=0 scope_package_id=2 scope_span=[43-336] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[141-159] scope=0 scope_package_id=2 scope_span=[43-336] callable=Main
                Variable(1, Boolean) = Icmp Eq, Variable(0, Boolean), Bool(false) !dbg package_id=2 span=[141-159] scope=0 scope_package_id=2 scope_span=[43-336] callable=Main
                Branch Variable(1, Boolean), 2, 3 !dbg package_id=2 span=[141-159] scope=0 scope_package_id=2 scope_span=[43-336] callable=Main
            Block 1:Block:
                Variable(3, Boolean) = Fcmp Oeq, Variable(2, Double), Double(0) !dbg package_id=2 span=[226-243] scope=0 scope_package_id=2 scope_span=[43-336] callable=Main
                Branch Variable(3, Boolean), 5, 4 !dbg package_id=2 span=[226-243] scope=0 scope_package_id=2 scope_span=[43-336] callable=Main
            Block 2:Block:
                Variable(2, Double) = Store Double(0.1) !dbg package_id=2 span=[162-165] scope=0 scope_package_id=2 scope_span=[43-336] callable=Main
                Jump(1) !dbg package_id=2 span=[162-165] scope=0 scope_package_id=2 scope_span=[43-336] callable=Main
            Block 3:Block:
                Variable(2, Double) = Store Double(1.1) !dbg package_id=2 span=[168-171] scope=0 scope_package_id=2 scope_span=[43-336] callable=Main
                Jump(1) !dbg package_id=2 span=[168-171] scope=0 scope_package_id=2 scope_span=[43-336] callable=Main
            Block 4:Block:
                Call id(1), args( Qubit(1), Result(1), ) !dbg package_id=2 span=[291-301] scope=0 scope_package_id=2 scope_span=[43-336] callable=Main
                Variable(4, Boolean) = Call id(2), args( Result(1), ) !dbg package_id=2 span=[291-309] scope=0 scope_package_id=2 scope_span=[43-336] callable=Main
                Variable(5, Boolean) = Icmp Eq, Variable(4, Boolean), Bool(false) !dbg package_id=2 span=[291-309] scope=0 scope_package_id=2 scope_span=[43-336] callable=Main
                Branch Variable(5, Boolean), 7, 8 !dbg package_id=2 span=[291-309] scope=0 scope_package_id=2 scope_span=[43-336] callable=Main
            Block 5:Block:
                Call id(3), args( Qubit(1), ) !dbg package_id=2 span=[254-258] scope=1 scope_package_id=2 scope_span=[244-265] callable=Main
                Jump(4) !dbg package_id=2 span=[244-265] scope=0 scope_package_id=2 scope_span=[43-336] callable=Main
            Block 6:Block:
                Call id(4), args( Integer(2), Pointer, ) !dbg package_id=2 span=[25-29]
                Call id(5), args( Variable(2, Double), Pointer, ) !dbg package_id=2 span=[25-29]
                Call id(5), args( Variable(6, Double), Pointer, ) !dbg package_id=2 span=[25-29]
                Return !dbg package_id=2 span=[25-29]
            Block 7:Block:
                Variable(6, Double) = Store Double(0.1) !dbg package_id=2 span=[312-315] scope=0 scope_package_id=2 scope_span=[43-336] callable=Main
                Jump(6) !dbg package_id=2 span=[312-315] scope=0 scope_package_id=2 scope_span=[43-336] callable=Main
            Block 8:Block:
                Variable(6, Double) = Store Double(1.1) !dbg package_id=2 span=[318-321] scope=0 scope_package_id=2 scope_span=[43-336] callable=Main
                Jump(6) !dbg package_id=2 span=[318-321] scope=0 scope_package_id=2 scope_span=[43-336] callable=Main"#]],
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[167-177] scope=3 scope_package_id=2 scope_span=[133-180] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[44-52] scope=0 scope_package_id=2 scope_span=[35-95] callable=Choose
                Variable(1, Boolean) = Store Variable(0, Boolean) !dbg package_id=2 span=[44-52] scope=0 scope_package_id=2 scope_span=[35-95] callable=Choose
                Branch Variable(1, Boolean), 2, 3 !dbg package_id=2 span=[44-52] scope=0 scope_package_id=2 scope_span=[35-95] callable=Choose
            Block 1:Block:
                Variable(3, Integer) = Store Variable(2, Integer) !dbg package_id=2 span=[160-178] scope=3 scope_package_id=2 scope_span=[133-180] callable=Main
                Call id(3), args( Variable(3, Integer), Pointer, ) !dbg package_id=2 span=[120-124]
                Return !dbg package_id=2 span=[120-124]
            Block 2:Block:
                Variable(2, Integer) = Store Integer(1) !dbg package_id=2 span=[53-70] scope=0 scope_package_id=2 scope_span=[35-95] callable=Choose
                Jump(1) !dbg package_id=2 span=[53-70] scope=0 scope_package_id=2 scope_span=[35-95] callable=Choose
            Block 3:Block:
                Variable(2, Integer) = Store Integer(0) !dbg package_id=2 span=[71-93] scope=0 scope_package_id=2 scope_span=[35-95] callable=Choose
                Jump(1) !dbg package_id=2 span=[71-93] scope=0 scope_package_id=2 scope_span=[35-95] callable=Choose"#]],
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
