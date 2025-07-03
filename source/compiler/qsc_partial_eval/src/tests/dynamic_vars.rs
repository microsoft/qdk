// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::needless_raw_string_hashes, clippy::similar_names)]

use super::{assert_blocks, assert_callable, get_rir_program};
use expect_test::expect;
use indoc::indoc;
use qsc_rir::rir::CallableId;

#[test]
fn dynamic_int_from_if_expression_with_single_measurement_comparison_and_classical_blocks() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                let r = QIR.Intrinsic.__quantum__qis__mresetz__body(q);
                let b = if r == Zero { 0 } else { 1 };
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
                Variable(3, Integer) = Store Variable(2, Integer)
                Call id(3), args( Integer(0), EmptyTag, )
                Return
            Block 2:Block:
                Variable(2, Integer) = Store Integer(0)
                Jump(1)
            Block 3:Block:
                Variable(2, Integer) = Store Integer(1)
                Jump(1)"#]],
    );
}

#[test]
fn dynamic_int_from_if_expression_with_single_measurement_comparison_and_non_classical_blocks() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            operation OpA(q : Qubit) : Unit { body intrinsic; }
            operation OpB(q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                use (q0, q1) = (Qubit(), Qubit());
                let r = QIR.Intrinsic.__quantum__qis__mresetz__body(q0);
                let b = if r == Zero {
                    OpA(q1);
                    0
                } else {
                    OpB(q1);
                    1
                };
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
            name: OpA
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
                name: OpB
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
                Branch Variable(1, Boolean), 2, 3
            Block 1:Block:
                Variable(3, Integer) = Store Variable(2, Integer)
                Call id(5), args( Integer(0), EmptyTag, )
                Return
            Block 2:Block:
                Call id(3), args( Qubit(1), )
                Variable(2, Integer) = Store Integer(0)
                Jump(1)
            Block 3:Block:
                Call id(4), args( Qubit(1), )
                Variable(2, Integer) = Store Integer(1)
                Jump(1)"#]],
    );
}

#[test]
fn dynamic_var_across_if_else_static_in_both_branches_constant_folded() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                mutable value = 0;
                use q = Qubit();
                let cond = MResetZ(q);
                if cond == Zero {
                    value -= 1;
                } else {
                    value += 1;
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
                Variable(0, Integer) = Store Integer(0)
                Call id(1), args( Qubit(0), Result(0), )
                Variable(1, Boolean) = Call id(2), args( Result(0), )
                Variable(2, Boolean) = Icmp Eq, Variable(1, Boolean), Bool(false)
                Branch Variable(2, Boolean), 2, 3
            Block 1:Block:
                Call id(3), args( Integer(0), EmptyTag, )
                Return
            Block 2:Block:
                Variable(0, Integer) = Store Integer(-1)
                Jump(1)
            Block 3:Block:
                Variable(0, Integer) = Store Integer(1)
                Jump(1)"#]],
    );
}

#[test]
fn dynamic_var_across_if_else_in_loop_constant_folded_in_first_iteration() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                mutable value = 0;
                use q = Qubit();
                let cond = MResetZ(q);
                for _ in 0..1 {
                    if cond == Zero {
                        value -= 1;
                    } else {
                        value += 1;
                    }
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
                Variable(0, Integer) = Store Integer(0)
                Call id(1), args( Qubit(0), Result(0), )
                Variable(1, Integer) = Store Integer(0)
                Variable(2, Boolean) = Call id(2), args( Result(0), )
                Variable(3, Boolean) = Icmp Eq, Variable(2, Boolean), Bool(false)
                Branch Variable(3, Boolean), 2, 3
            Block 1:Block:
                Variable(1, Integer) = Store Integer(1)
                Variable(4, Boolean) = Call id(2), args( Result(0), )
                Variable(5, Boolean) = Icmp Eq, Variable(4, Boolean), Bool(false)
                Branch Variable(5, Boolean), 5, 6
            Block 2:Block:
                Variable(0, Integer) = Store Integer(-1)
                Jump(1)
            Block 3:Block:
                Variable(0, Integer) = Store Integer(1)
                Jump(1)
            Block 4:Block:
                Variable(1, Integer) = Store Integer(2)
                Call id(3), args( Integer(0), EmptyTag, )
                Return
            Block 5:Block:
                Variable(6, Integer) = Sub Variable(0, Integer), Integer(1)
                Variable(0, Integer) = Store Variable(6, Integer)
                Jump(4)
            Block 6:Block:
                Variable(7, Integer) = Add Variable(0, Integer), Integer(1)
                Variable(0, Integer) = Store Variable(7, Integer)
                Jump(4)"#]],
    );
}

#[test]
fn dynamic_var_within_if_else_in_loop_constant_folded_in_every_iteration() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                let cond = MResetZ(q);
                for _ in 0..1 {
                    mutable value = 0;
                    if cond == Zero {
                        value -= 1;
                    } else {
                        value += 1;
                    }
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
                Call id(1), args( Qubit(0), Result(0), )
                Variable(0, Integer) = Store Integer(0)
                Variable(1, Integer) = Store Integer(0)
                Variable(2, Boolean) = Call id(2), args( Result(0), )
                Variable(3, Boolean) = Icmp Eq, Variable(2, Boolean), Bool(false)
                Branch Variable(3, Boolean), 2, 3
            Block 1:Block:
                Variable(0, Integer) = Store Integer(1)
                Variable(4, Integer) = Store Integer(0)
                Variable(5, Boolean) = Call id(2), args( Result(0), )
                Variable(6, Boolean) = Icmp Eq, Variable(5, Boolean), Bool(false)
                Branch Variable(6, Boolean), 5, 6
            Block 2:Block:
                Variable(1, Integer) = Store Integer(-1)
                Jump(1)
            Block 3:Block:
                Variable(1, Integer) = Store Integer(1)
                Jump(1)
            Block 4:Block:
                Variable(0, Integer) = Store Integer(2)
                Call id(3), args( Integer(0), EmptyTag, )
                Return
            Block 5:Block:
                Variable(4, Integer) = Store Integer(-1)
                Jump(4)
            Block 6:Block:
                Variable(4, Integer) = Store Integer(1)
                Jump(4)"#]],
    );
}

#[test]
fn dynamic_var_updated_twice_in_same_branch_constant_folded() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                mutable value = 0;
                use q = Qubit();
                let cond = MResetZ(q);
                if cond == Zero {
                    value -= 1;
                    value += 3;
                } else {
                    value += 1;
                    value -= 3;
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
                Variable(0, Integer) = Store Integer(0)
                Call id(1), args( Qubit(0), Result(0), )
                Variable(1, Boolean) = Call id(2), args( Result(0), )
                Variable(2, Boolean) = Icmp Eq, Variable(1, Boolean), Bool(false)
                Branch Variable(2, Boolean), 2, 3
            Block 1:Block:
                Call id(3), args( Integer(0), EmptyTag, )
                Return
            Block 2:Block:
                Variable(0, Integer) = Store Integer(-1)
                Variable(0, Integer) = Store Integer(2)
                Jump(1)
            Block 3:Block:
                Variable(0, Integer) = Store Integer(1)
                Variable(0, Integer) = Store Integer(-2)
                Jump(1)"#]],
    );
}

#[test]
fn dynamic_var_updated_to_same_value_in_different_branches_constant_folded_after_if() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                mutable value = 0;
                use q = Qubit();
                let cond = MResetZ(q);
                if cond == Zero {
                    value -= 1;
                    value += 2;
                } else {
                    value += 1;
                    value /= 1;
                }
                value += 1;
            }
        }
        "#,
    });

    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Variable(0, Integer) = Store Integer(0)
                Call id(1), args( Qubit(0), Result(0), )
                Variable(1, Boolean) = Call id(2), args( Result(0), )
                Variable(2, Boolean) = Icmp Eq, Variable(1, Boolean), Bool(false)
                Branch Variable(2, Boolean), 2, 3
            Block 1:Block:
                Variable(0, Integer) = Store Integer(2)
                Call id(3), args( Integer(0), EmptyTag, )
                Return
            Block 2:Block:
                Variable(0, Integer) = Store Integer(-1)
                Variable(0, Integer) = Store Integer(1)
                Jump(1)
            Block 3:Block:
                Variable(0, Integer) = Store Integer(1)
                Variable(0, Integer) = Store Integer(1)
                Jump(1)"#]],
    );
}

#[test]
fn dynamic_var_updated_in_nested_branches_constant_folded_when_value_matches_across_all_branches() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                mutable value = 0;
                use q = Qubit();
                let cond = MResetZ(q);
                if cond == Zero {
                    value -= 1;
                    if cond == Zero {
                        value += 2;
                    } else {
                        value -= 2;
                        value /= 3;
                        value *= -1;
                }
                } else {
                    value += 1;
                    if cond == Zero {
                        value += 2;
                        value /= 3;
                    } else {
                        value -= 2;
                        value *= -1;
                    }
                }
                return value;
            }
        }
        "#,
    });

    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Variable(0, Integer) = Store Integer(0)
                Call id(1), args( Qubit(0), Result(0), )
                Variable(1, Boolean) = Call id(2), args( Result(0), )
                Variable(2, Boolean) = Icmp Eq, Variable(1, Boolean), Bool(false)
                Branch Variable(2, Boolean), 2, 6
            Block 1:Block:
                Call id(3), args( Integer(1), Tag(0, 3), )
                Return
            Block 2:Block:
                Variable(0, Integer) = Store Integer(-1)
                Variable(3, Boolean) = Call id(2), args( Result(0), )
                Variable(4, Boolean) = Icmp Eq, Variable(3, Boolean), Bool(false)
                Branch Variable(4, Boolean), 4, 5
            Block 3:Block:
                Jump(1)
            Block 4:Block:
                Variable(0, Integer) = Store Integer(1)
                Jump(3)
            Block 5:Block:
                Variable(0, Integer) = Store Integer(-3)
                Variable(0, Integer) = Store Integer(-1)
                Variable(0, Integer) = Store Integer(1)
                Jump(3)
            Block 6:Block:
                Variable(0, Integer) = Store Integer(1)
                Variable(5, Boolean) = Call id(2), args( Result(0), )
                Variable(6, Boolean) = Icmp Eq, Variable(5, Boolean), Bool(false)
                Branch Variable(6, Boolean), 8, 9
            Block 7:Block:
                Jump(1)
            Block 8:Block:
                Variable(0, Integer) = Store Integer(3)
                Variable(0, Integer) = Store Integer(1)
                Jump(7)
            Block 9:Block:
                Variable(0, Integer) = Store Integer(-1)
                Variable(0, Integer) = Store Integer(1)
                Jump(7)"#]],
    );
}

#[test]
fn dynamic_var_set_to_static_after_dynamism_still_constant_folded() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                mutable value = 0;
                use q = Qubit();
                let cond = MResetZ(q);
                if cond == Zero {
                    value -= 1;
                }
                value *= 2;
                value = 3;
                value += 1;
            }
        }
        "#,
    });

    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Variable(0, Integer) = Store Integer(0)
                Call id(1), args( Qubit(0), Result(0), )
                Variable(1, Boolean) = Call id(2), args( Result(0), )
                Variable(2, Boolean) = Icmp Eq, Variable(1, Boolean), Bool(false)
                Branch Variable(2, Boolean), 2, 1
            Block 1:Block:
                Variable(3, Integer) = Mul Variable(0, Integer), Integer(2)
                Variable(0, Integer) = Store Variable(3, Integer)
                Variable(0, Integer) = Store Integer(3)
                Variable(0, Integer) = Store Integer(4)
                Call id(3), args( Integer(0), EmptyTag, )
                Return
            Block 2:Block:
                Variable(0, Integer) = Store Integer(-1)
                Jump(1)"#]],
    );
}

#[test]
fn dynamic_var_updated_in_loop_constant_folded_when_every_iteration_results_in_same_value() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                mutable value = 0;
                use q = Qubit();
                let cond = MResetZ(q);
                for _ in 0..1 {
                    if cond == Zero {
                        value -= 1;
                        value ^= 2;
                    } else {
                        value += 1;
                    }
                    value -= 1;
                }
                return value
            }
        }
        "#,
    });

    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Variable(0, Integer) = Store Integer(0)
                Call id(1), args( Qubit(0), Result(0), )
                Variable(1, Integer) = Store Integer(0)
                Variable(2, Boolean) = Call id(2), args( Result(0), )
                Variable(3, Boolean) = Icmp Eq, Variable(2, Boolean), Bool(false)
                Branch Variable(3, Boolean), 2, 3
            Block 1:Block:
                Variable(0, Integer) = Store Integer(0)
                Variable(1, Integer) = Store Integer(1)
                Variable(4, Boolean) = Call id(2), args( Result(0), )
                Variable(5, Boolean) = Icmp Eq, Variable(4, Boolean), Bool(false)
                Branch Variable(5, Boolean), 5, 6
            Block 2:Block:
                Variable(0, Integer) = Store Integer(-1)
                Variable(0, Integer) = Store Integer(1)
                Jump(1)
            Block 3:Block:
                Variable(0, Integer) = Store Integer(1)
                Jump(1)
            Block 4:Block:
                Variable(0, Integer) = Store Integer(0)
                Variable(1, Integer) = Store Integer(2)
                Call id(3), args( Integer(0), Tag(0, 3), )
                Return
            Block 5:Block:
                Variable(0, Integer) = Store Integer(-1)
                Variable(0, Integer) = Store Integer(1)
                Jump(4)
            Block 6:Block:
                Variable(0, Integer) = Store Integer(1)
                Jump(4)"#]],
    );
}

#[test]
fn immutable_bind_of_dynamic_var_should_be_point_in_time_copy() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : (Int, Int) {
                mutable value = 0;
                use q = Qubit();
                let cond = MResetZ(q);
                if cond == Zero {
                    value -= 1;
                }
                let copy = value;
                value += 1;
                (copy, value)
            }
        }
        "#,
    });

    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Variable(0, Integer) = Store Integer(0)
                Call id(1), args( Qubit(0), Result(0), )
                Variable(1, Boolean) = Call id(2), args( Result(0), )
                Variable(2, Boolean) = Icmp Eq, Variable(1, Boolean), Bool(false)
                Branch Variable(2, Boolean), 2, 1
            Block 1:Block:
                Variable(3, Integer) = Store Variable(0, Integer)
                Variable(4, Integer) = Add Variable(0, Integer), Integer(1)
                Variable(0, Integer) = Store Variable(4, Integer)
                Call id(3), args( Integer(2), EmptyTag, )
                Call id(4), args( Variable(3, Integer), Tag(0, 5), )
                Call id(4), args( Variable(0, Integer), Tag(1, 5), )
                Return
            Block 2:Block:
                Variable(0, Integer) = Store Integer(-1)
                Jump(1)"#]],
    );
}

#[test]
fn dynamic_double_from_if_expression_with_single_measurement_comparison_and_classical_blocks() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                let r = QIR.Intrinsic.__quantum__qis__mresetz__body(q);
                let b = if r == Zero { 0.1 } else { 1.1 };
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
                Variable(3, Double) = Store Variable(2, Double)
                Call id(3), args( Integer(0), EmptyTag, )
                Return
            Block 2:Block:
                Variable(2, Double) = Store Double(0.1)
                Jump(1)
            Block 3:Block:
                Variable(2, Double) = Store Double(1.1)
                Jump(1)"#]],
    );
}

#[test]
fn dynamic_double_from_if_expression_with_single_measurement_comparison_and_non_classical_blocks() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            operation OpA(q : Qubit) : Unit { body intrinsic; }
            operation OpB(q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                use (q0, q1) = (Qubit(), Qubit());
                let r = QIR.Intrinsic.__quantum__qis__mresetz__body(q0);
                let b = if r == Zero {
                    OpA(q1);
                    0.1
                } else {
                    OpB(q1);
                    1.1
                };
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
            name: OpA
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
                name: OpB
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
                Branch Variable(1, Boolean), 2, 3
            Block 1:Block:
                Variable(3, Double) = Store Variable(2, Double)
                Call id(5), args( Integer(0), EmptyTag, )
                Return
            Block 2:Block:
                Call id(3), args( Qubit(1), )
                Variable(2, Double) = Store Double(0.1)
                Jump(1)
            Block 3:Block:
                Call id(4), args( Qubit(1), )
                Variable(2, Double) = Store Double(1.1)
                Jump(1)"#]],
    );
}

#[test]
fn dynamic_double_from_if_expression_with_single_measurement_comparison_pass_dynamic_double_to_intrinsic()
 {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            operation OpA(theta: Double, q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                use (q0, q1) = (Qubit(), Qubit());
                let r = QIR.Intrinsic.__quantum__qis__mresetz__body(q0);
                let b = if r == Zero {
                    0.1
                } else {
                    1.1
                };
                OpA(b, q1);
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
                name: OpA
                call_type: Regular
                input_type:
                    [0]: Double
                    [1]: Qubit
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let op_b_callable_id = CallableId(4);
    assert_callable(
        &program,
        op_b_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__tuple_record_output
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
                Variable(3, Double) = Store Variable(2, Double)
                Call id(3), args( Variable(3, Double), Qubit(1), )
                Call id(4), args( Integer(0), EmptyTag, )
                Return
            Block 2:Block:
                Variable(2, Double) = Store Double(0.1)
                Jump(1)
            Block 3:Block:
                Variable(2, Double) = Store Double(1.1)
                Jump(1)"#]],
    );
}
