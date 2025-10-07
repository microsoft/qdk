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
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), Result(0), )
                Variable(0, Boolean) = Call id(3), args( Result(0), )
                Variable(1, Boolean) = Icmp Eq, Variable(0, Boolean), Bool(false)
                Branch Variable(1, Boolean), 2, 3
            Block 1:Block:
                Variable(3, Integer) = Store Variable(2, Integer)
                Call id(4), args( Integer(0), EmptyTag, )
                Return
            Block 2:Block:
                Variable(2, Integer) = Store Integer(0) !dbg package_id=2 span=[184-189] scope=0 scope_package_id=2 scope_span=[64-207] callable=Main
                Jump(1) !dbg package_id=2 span=[184-189] scope=0 scope_package_id=2 scope_span=[64-207] callable=Main
            Block 3:Block:
                Variable(2, Integer) = Store Integer(1) !dbg package_id=2 span=[190-200] scope=0 scope_package_id=2 scope_span=[64-207] callable=Main
                Jump(1) !dbg package_id=2 span=[190-200] scope=0 scope_package_id=2 scope_span=[64-207] callable=Main"#]],
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
                name: OpA
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
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), Result(0), )
                Variable(0, Boolean) = Call id(3), args( Result(0), )
                Variable(1, Boolean) = Icmp Eq, Variable(0, Boolean), Bool(false)
                Branch Variable(1, Boolean), 2, 3
            Block 1:Block:
                Variable(3, Integer) = Store Variable(2, Integer)
                Call id(6), args( Integer(0), EmptyTag, )
                Return
            Block 2:Block:
                Call id(4), args( Qubit(1), )
                Variable(2, Integer) = Store Integer(0)
                Jump(1)
            Block 3:Block:
                Call id(5), args( Qubit(1), )
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
                Call id(1), args( Pointer, )
                Variable(0, Integer) = Store Integer(0)
                Call id(2), args( Qubit(0), Result(0), )
                Variable(1, Boolean) = Call id(3), args( Result(0), )
                Variable(2, Boolean) = Icmp Eq, Variable(1, Boolean), Bool(false)
                Branch Variable(2, Boolean), 2, 3
            Block 1:Block:
                Call id(4), args( Integer(0), EmptyTag, )
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
                Call id(1), args( Pointer, )
                Variable(0, Integer) = Store Integer(0)
                Call id(2), args( Qubit(0), Result(0), )
                Variable(1, Integer) = Store Integer(0)
                Variable(2, Boolean) = Call id(3), args( Result(0), )
                Variable(3, Boolean) = Icmp Eq, Variable(2, Boolean), Bool(false)
                Branch Variable(3, Boolean), 2, 3
            Block 1:Block:
                Variable(1, Integer) = Store Integer(1)
                Variable(4, Boolean) = Call id(3), args( Result(0), )
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
                Call id(4), args( Integer(0), EmptyTag, )
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
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), Result(0), )
                Variable(0, Integer) = Store Integer(0)
                Variable(1, Integer) = Store Integer(0)
                Variable(2, Boolean) = Call id(3), args( Result(0), )
                Variable(3, Boolean) = Icmp Eq, Variable(2, Boolean), Bool(false)
                Branch Variable(3, Boolean), 2, 3
            Block 1:Block:
                Variable(0, Integer) = Store Integer(1)
                Variable(4, Integer) = Store Integer(0)
                Variable(5, Boolean) = Call id(3), args( Result(0), )
                Variable(6, Boolean) = Icmp Eq, Variable(5, Boolean), Bool(false)
                Branch Variable(6, Boolean), 5, 6
            Block 2:Block:
                Variable(1, Integer) = Store Integer(-1) !dbg package_id=2 span=[223-228] scope=3 scope_package_id=2 scope_span=[205-248] discriminator=1 callable=Main
                Jump(1) !dbg package_id=2 span=[205-248] scope=2 scope_package_id=2 scope_span=[144-307] discriminator=1 callable=Main
            Block 3:Block:
                Variable(1, Integer) = Store Integer(1) !dbg package_id=2 span=[272-277] scope=4 scope_package_id=2 scope_span=[254-297] discriminator=1 callable=Main
                Jump(1) !dbg package_id=2 span=[249-297] scope=2 scope_package_id=2 scope_span=[144-307] discriminator=1 callable=Main
            Block 4:Block:
                Variable(0, Integer) = Store Integer(2)
                Call id(4), args( Integer(0), EmptyTag, )
                Return
            Block 5:Block:
                Variable(4, Integer) = Store Integer(-1) !dbg package_id=2 span=[223-228] scope=3 scope_package_id=2 scope_span=[205-248] discriminator=2 callable=Main
                Jump(4) !dbg package_id=2 span=[205-248] scope=2 scope_package_id=2 scope_span=[144-307] discriminator=2 callable=Main
            Block 6:Block:
                Variable(4, Integer) = Store Integer(1) !dbg package_id=2 span=[272-277] scope=4 scope_package_id=2 scope_span=[254-297] discriminator=2 callable=Main
                Jump(4) !dbg package_id=2 span=[249-297] scope=2 scope_package_id=2 scope_span=[144-307] discriminator=2 callable=Main"#]],
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
                Call id(1), args( Pointer, )
                Variable(0, Integer) = Store Integer(0)
                Call id(2), args( Qubit(0), Result(0), )
                Variable(1, Boolean) = Call id(3), args( Result(0), )
                Variable(2, Boolean) = Icmp Eq, Variable(1, Boolean), Bool(false)
                Branch Variable(2, Boolean), 2, 3
            Block 1:Block:
                Call id(4), args( Integer(0), EmptyTag, )
                Return
            Block 2:Block:
                Variable(0, Integer) = Store Integer(-1) !dbg package_id=2 span=[187-192] scope=1 scope_package_id=2 scope_span=[173-232] callable=Main
                Variable(0, Integer) = Store Integer(2) !dbg package_id=2 span=[211-216] scope=1 scope_package_id=2 scope_span=[173-232] callable=Main
                Jump(1) !dbg package_id=2 span=[173-232] scope=0 scope_package_id=2 scope_span=[64-303] callable=Main
            Block 3:Block:
                Variable(0, Integer) = Store Integer(1) !dbg package_id=2 span=[252-257] scope=2 scope_package_id=2 scope_span=[238-297] callable=Main
                Variable(0, Integer) = Store Integer(-2) !dbg package_id=2 span=[276-281] scope=2 scope_package_id=2 scope_span=[238-297] callable=Main
                Jump(1) !dbg package_id=2 span=[233-297] scope=0 scope_package_id=2 scope_span=[64-303] callable=Main"#]],
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
                Call id(1), args( Pointer, )
                Variable(0, Integer) = Store Integer(0)
                Call id(2), args( Qubit(0), Result(0), )
                Variable(1, Boolean) = Call id(3), args( Result(0), )
                Variable(2, Boolean) = Icmp Eq, Variable(1, Boolean), Bool(false)
                Branch Variable(2, Boolean), 2, 3
            Block 1:Block:
                Variable(0, Integer) = Store Integer(2)
                Call id(4), args( Integer(0), EmptyTag, )
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
                Call id(1), args( Pointer, )
                Variable(0, Integer) = Store Integer(0)
                Call id(2), args( Qubit(0), Result(0), )
                Variable(1, Boolean) = Call id(3), args( Result(0), )
                Variable(2, Boolean) = Icmp Eq, Variable(1, Boolean), Bool(false)
                Branch Variable(2, Boolean), 2, 6
            Block 1:Block:
                Call id(4), args( Integer(1), Tag(0, 3), )
                Return
            Block 2:Block:
                Variable(0, Integer) = Store Integer(-1)
                Variable(3, Boolean) = Call id(3), args( Result(0), )
                Variable(4, Boolean) = Icmp Eq, Variable(3, Boolean), Bool(false)
                Branch Variable(4, Boolean), 4, 5
            Block 3:Block:
                Jump(1) !dbg package_id=2 span=[172-381] scope=0 scope_package_id=2 scope_span=[63-628] callable=Main
            Block 4:Block:
                Variable(0, Integer) = Store Integer(1) !dbg package_id=2 span=[244-249] scope=2 scope_package_id=2 scope_span=[226-269] callable=Main
                Jump(3) !dbg package_id=2 span=[226-269] scope=1 scope_package_id=2 scope_span=[172-381] callable=Main
            Block 5:Block:
                Variable(0, Integer) = Store Integer(-3) !dbg package_id=2 span=[293-298] scope=3 scope_package_id=2 scope_span=[275-371] callable=Main
                Variable(0, Integer) = Store Integer(-1) !dbg package_id=2 span=[321-326] scope=3 scope_package_id=2 scope_span=[275-371] callable=Main
                Variable(0, Integer) = Store Integer(1) !dbg package_id=2 span=[349-354] scope=3 scope_package_id=2 scope_span=[275-371] callable=Main
                Jump(3) !dbg package_id=2 span=[270-371] scope=1 scope_package_id=2 scope_span=[172-381] callable=Main
            Block 6:Block:
                Variable(0, Integer) = Store Integer(1)
                Variable(5, Boolean) = Call id(3), args( Result(0), )
                Variable(6, Boolean) = Icmp Eq, Variable(5, Boolean), Bool(false)
                Branch Variable(6, Boolean), 8, 9
            Block 7:Block:
                Jump(1) !dbg package_id=2 span=[382-600] scope=0 scope_package_id=2 scope_span=[63-628] callable=Main
            Block 8:Block:
                Variable(0, Integer) = Store Integer(3) !dbg package_id=2 span=[459-464] scope=5 scope_package_id=2 scope_span=[441-512] callable=Main
                Variable(0, Integer) = Store Integer(1) !dbg package_id=2 span=[487-492] scope=5 scope_package_id=2 scope_span=[441-512] callable=Main
                Jump(7) !dbg package_id=2 span=[441-512] scope=4 scope_package_id=2 scope_span=[387-600] callable=Main
            Block 9:Block:
                Variable(0, Integer) = Store Integer(-1) !dbg package_id=2 span=[536-541] scope=6 scope_package_id=2 scope_span=[518-590] callable=Main
                Variable(0, Integer) = Store Integer(1) !dbg package_id=2 span=[564-569] scope=6 scope_package_id=2 scope_span=[518-590] callable=Main
                Jump(7) !dbg package_id=2 span=[513-590] scope=4 scope_package_id=2 scope_span=[387-600] callable=Main"#]],
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
                Call id(1), args( Pointer, )
                Variable(0, Integer) = Store Integer(0)
                Call id(2), args( Qubit(0), Result(0), )
                Variable(1, Boolean) = Call id(3), args( Result(0), )
                Variable(2, Boolean) = Icmp Eq, Variable(1, Boolean), Bool(false)
                Branch Variable(2, Boolean), 2, 1
            Block 1:Block:
                Variable(3, Integer) = Mul Variable(0, Integer), Integer(2)
                Variable(0, Integer) = Store Variable(3, Integer)
                Variable(0, Integer) = Store Integer(3)
                Variable(0, Integer) = Store Integer(4)
                Call id(4), args( Integer(0), EmptyTag, )
                Return
            Block 2:Block:
                Variable(0, Integer) = Store Integer(-1) !dbg package_id=2 span=[187-192] scope=1 scope_package_id=2 scope_span=[173-208] callable=Main
                Jump(1) !dbg package_id=2 span=[173-208] scope=0 scope_package_id=2 scope_span=[64-273] callable=Main"#]],
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
                Call id(1), args( Pointer, )
                Variable(0, Integer) = Store Integer(0)
                Call id(2), args( Qubit(0), Result(0), )
                Variable(1, Integer) = Store Integer(0)
                Variable(2, Boolean) = Call id(3), args( Result(0), )
                Variable(3, Boolean) = Icmp Eq, Variable(2, Boolean), Bool(false)
                Branch Variable(3, Boolean), 2, 3
            Block 1:Block:
                Variable(0, Integer) = Store Integer(0)
                Variable(1, Integer) = Store Integer(1)
                Variable(4, Boolean) = Call id(3), args( Result(0), )
                Variable(5, Boolean) = Icmp Eq, Variable(4, Boolean), Bool(false)
                Branch Variable(5, Boolean), 5, 6
            Block 2:Block:
                Variable(0, Integer) = Store Integer(-1) !dbg package_id=2 span=[218-223] scope=3 scope_package_id=2 scope_span=[200-271] discriminator=1 callable=Main
                Variable(0, Integer) = Store Integer(1) !dbg package_id=2 span=[246-251] scope=3 scope_package_id=2 scope_span=[200-271] discriminator=1 callable=Main
                Jump(1) !dbg package_id=2 span=[200-271] scope=2 scope_package_id=2 scope_span=[170-354] discriminator=1 callable=Main
            Block 3:Block:
                Variable(0, Integer) = Store Integer(1) !dbg package_id=2 span=[295-300] scope=4 scope_package_id=2 scope_span=[277-320] discriminator=1 callable=Main
                Jump(1) !dbg package_id=2 span=[272-320] scope=2 scope_package_id=2 scope_span=[170-354] discriminator=1 callable=Main
            Block 4:Block:
                Variable(0, Integer) = Store Integer(0)
                Variable(1, Integer) = Store Integer(2)
                Call id(4), args( Integer(0), Tag(0, 3), )
                Return
            Block 5:Block:
                Variable(0, Integer) = Store Integer(-1) !dbg package_id=2 span=[218-223] scope=3 scope_package_id=2 scope_span=[200-271] discriminator=2 callable=Main
                Variable(0, Integer) = Store Integer(1) !dbg package_id=2 span=[246-251] scope=3 scope_package_id=2 scope_span=[200-271] discriminator=2 callable=Main
                Jump(4) !dbg package_id=2 span=[200-271] scope=2 scope_package_id=2 scope_span=[170-354] discriminator=2 callable=Main
            Block 6:Block:
                Variable(0, Integer) = Store Integer(1) !dbg package_id=2 span=[295-300] scope=4 scope_package_id=2 scope_span=[277-320] discriminator=2 callable=Main
                Jump(4) !dbg package_id=2 span=[272-320] scope=2 scope_package_id=2 scope_span=[170-354] discriminator=2 callable=Main"#]],
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
                Call id(1), args( Pointer, )
                Variable(0, Integer) = Store Integer(0)
                Call id(2), args( Qubit(0), Result(0), )
                Variable(1, Boolean) = Call id(3), args( Result(0), )
                Variable(2, Boolean) = Icmp Eq, Variable(1, Boolean), Bool(false)
                Branch Variable(2, Boolean), 2, 1
            Block 1:Block:
                Variable(3, Integer) = Store Variable(0, Integer)
                Variable(4, Integer) = Add Variable(0, Integer), Integer(1)
                Variable(0, Integer) = Store Variable(4, Integer)
                Call id(4), args( Integer(2), EmptyTag, )
                Call id(5), args( Variable(3, Integer), Tag(0, 5), )
                Call id(5), args( Variable(0, Integer), Tag(1, 5), )
                Return
            Block 2:Block:
                Variable(0, Integer) = Store Integer(-1) !dbg package_id=2 span=[193-198] scope=1 scope_package_id=2 scope_span=[179-214] callable=Main
                Jump(1) !dbg package_id=2 span=[179-214] scope=0 scope_package_id=2 scope_span=[70-288] callable=Main"#]],
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
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), Result(0), )
                Variable(0, Boolean) = Call id(3), args( Result(0), )
                Variable(1, Boolean) = Icmp Eq, Variable(0, Boolean), Bool(false)
                Branch Variable(1, Boolean), 2, 3
            Block 1:Block:
                Variable(3, Double) = Store Variable(2, Double)
                Call id(4), args( Integer(0), EmptyTag, )
                Return
            Block 2:Block:
                Variable(2, Double) = Store Double(0.1) !dbg package_id=2 span=[184-191] scope=0 scope_package_id=2 scope_span=[64-211] callable=Main
                Jump(1) !dbg package_id=2 span=[184-191] scope=0 scope_package_id=2 scope_span=[64-211] callable=Main
            Block 3:Block:
                Variable(2, Double) = Store Double(1.1) !dbg package_id=2 span=[192-204] scope=0 scope_package_id=2 scope_span=[64-211] callable=Main
                Jump(1) !dbg package_id=2 span=[192-204] scope=0 scope_package_id=2 scope_span=[64-211] callable=Main"#]],
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
                name: OpA
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
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), Result(0), )
                Variable(0, Boolean) = Call id(3), args( Result(0), )
                Variable(1, Boolean) = Icmp Eq, Variable(0, Boolean), Bool(false)
                Branch Variable(1, Boolean), 2, 3
            Block 1:Block:
                Variable(3, Double) = Store Variable(2, Double)
                Call id(6), args( Integer(0), EmptyTag, )
                Return
            Block 2:Block:
                Call id(4), args( Qubit(1), )
                Variable(2, Double) = Store Double(0.1)
                Jump(1)
            Block 3:Block:
                Call id(5), args( Qubit(1), )
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
                name: OpA
                call_type: Regular
                input_type:
                    [0]: Double
                    [1]: Qubit
                output_type: <VOID>
                body: <NONE>"#]],
    );
    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), Result(0), )
                Variable(0, Boolean) = Call id(3), args( Result(0), )
                Variable(1, Boolean) = Icmp Eq, Variable(0, Boolean), Bool(false)
                Branch Variable(1, Boolean), 2, 3
            Block 1:Block:
                Variable(3, Double) = Store Variable(2, Double)
                Call id(4), args( Variable(3, Double), Qubit(1), )
                Call id(5), args( Integer(0), EmptyTag, )
                Return
            Block 2:Block:
                Variable(2, Double) = Store Double(0.1) !dbg package_id=2 span=[274-301] scope=0 scope_package_id=2 scope_span=[135-361] callable=Main
                Jump(1) !dbg package_id=2 span=[274-301] scope=0 scope_package_id=2 scope_span=[135-361] callable=Main
            Block 3:Block:
                Variable(2, Double) = Store Double(1.1) !dbg package_id=2 span=[302-334] scope=0 scope_package_id=2 scope_span=[135-361] callable=Main
                Jump(1) !dbg package_id=2 span=[302-334] scope=0 scope_package_id=2 scope_span=[135-361] callable=Main"#]],
    );
}
