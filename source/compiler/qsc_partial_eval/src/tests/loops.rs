// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{assert_block_instructions, assert_blocks, assert_callable, get_rir_program};
use expect_test::expect;
use indoc::indoc;
use qsc_rir::rir::{BlockId, CallableId};

#[test]
fn unitary_call_within_a_for_loop() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            operation op(q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                for _ in 1..3 {
                    op(q);
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
                Call id(1), args( Pointer, )
                Variable(0, Integer) = Store Integer(1)
                Call id(2), args( Qubit(0), )
                Variable(0, Integer) = Store Integer(2)
                Call id(2), args( Qubit(0), )
                Variable(0, Integer) = Store Integer(3)
                Call id(2), args( Qubit(0), )
                Variable(0, Integer) = Store Integer(4)
                Call id(3), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn unitary_call_within_a_while_loop() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            operation op(q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                mutable idx = 0;
                while idx < 3 {
                    op(q);
                    set idx += 1;
                }
            }
        }
        "#,
    });

    let rotation_callable_id = CallableId(1);
    assert_callable(
        &program,
        rotation_callable_id,
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
                Call id(1), args( Pointer, )
                Variable(0, Integer) = Store Integer(0)
                Call id(2), args( Qubit(0), )
                Variable(0, Integer) = Store Integer(1)
                Call id(2), args( Qubit(0), )
                Variable(0, Integer) = Store Integer(2)
                Call id(2), args( Qubit(0), )
                Variable(0, Integer) = Store Integer(3)
                Call id(3), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn unitary_call_within_a_repeat_until_loop() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            operation op(q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                mutable idx = 0;
                repeat {
                    op(q);
                    set idx += 1;
                } until idx >= 3;
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
                Call id(1), args( Pointer, )
                Variable(0, Integer) = Store Integer(0)
                Variable(1, Boolean) = Store Bool(true)
                Call id(2), args( Qubit(0), )
                Variable(0, Integer) = Store Integer(1)
                Variable(1, Boolean) = Store Bool(true)
                Call id(2), args( Qubit(0), )
                Variable(0, Integer) = Store Integer(2)
                Variable(1, Boolean) = Store Bool(true)
                Call id(2), args( Qubit(0), )
                Variable(0, Integer) = Store Integer(3)
                Variable(1, Boolean) = Store Bool(false)
                Call id(3), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn rotation_call_within_a_for_loop() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            operation rotation(theta : Double, q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                for theta in [0.0, 1.0, 2.0] {
                    rotation(theta, q);
                }
            }
        }
        "#,
    });

    let rotation_callable_id = CallableId(1);
    assert_callable(
        &program,
        rotation_callable_id,
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
                Call id(1), args( Pointer, )
                Variable(0, Integer) = Store Integer(0)
                Call id(2), args( Double(0), Qubit(0), )
                Variable(0, Integer) = Store Integer(1)
                Call id(2), args( Double(1), Qubit(0), )
                Variable(0, Integer) = Store Integer(2)
                Call id(2), args( Double(2), Qubit(0), )
                Variable(0, Integer) = Store Integer(3)
                Call id(3), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn rotation_call_within_a_while_loop() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            operation rotation(theta : Double, q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                let angles = [0.0, 1.0, 2.0];
                mutable idx = 0;
                while idx < 3 {
                    rotation(angles[idx], q);
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
                Call id(1), args( Pointer, )
                Variable(0, Integer) = Store Integer(0)
                Call id(2), args( Double(0), Qubit(0), )
                Variable(0, Integer) = Store Integer(1)
                Call id(2), args( Double(1), Qubit(0), )
                Variable(0, Integer) = Store Integer(2)
                Call id(2), args( Double(2), Qubit(0), )
                Variable(0, Integer) = Store Integer(3)
                Call id(3), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn rotation_call_within_a_repeat_until_loop() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            operation rotation(theta : Double, q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                let angles = [0.0, 1.0, 2.0];
                mutable idx = 0;
                repeat {
                    rotation(angles[idx], q);
                    set idx += 1;
                } until idx >= 3;
            }
        }
        "#,
    });

    let rotation_callable_id = CallableId(1);
    assert_callable(
        &program,
        rotation_callable_id,
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
                Call id(1), args( Pointer, )
                Variable(0, Integer) = Store Integer(0)
                Variable(1, Boolean) = Store Bool(true)
                Call id(2), args( Double(0), Qubit(0), )
                Variable(0, Integer) = Store Integer(1)
                Variable(1, Boolean) = Store Bool(true)
                Call id(2), args( Double(1), Qubit(0), )
                Variable(0, Integer) = Store Integer(2)
                Variable(1, Boolean) = Store Bool(true)
                Call id(2), args( Double(2), Qubit(0), )
                Variable(0, Integer) = Store Integer(3)
                Variable(1, Boolean) = Store Bool(false)
                Call id(3), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn mutable_bool_updated_in_loop() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                mutable flag = false;
                for _ in 1..3 {
                    if not flag {
                        set flag = MResetZ(q) == One;
                    }
                }
            }
        }
        "#,
    });

    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Pointer, )
                Variable(0, Boolean) = Store Bool(false)
                Variable(1, Integer) = Store Integer(1)
                Call id(2), args( Qubit(0), Result(0), )
                Variable(2, Boolean) = Call id(3), args( Result(0), )
                Variable(3, Boolean) = Store Variable(2, Boolean)
                Variable(0, Boolean) = Store Variable(3, Boolean)
                Variable(1, Integer) = Store Integer(2)
                Variable(4, Boolean) = LogicalNot Variable(0, Boolean)
                Branch Variable(4, Boolean), 2, 1"#]],
    );
}

#[test]
fn mutable_int_updated_in_loop() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                mutable count = 1;
                for _ in 1..3 {
                    if count > 0 and MResetZ(q) == One {
                        set count = -count;
                    }
                }
            }
        }
        "#,
    });

    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Pointer, )
                Variable(0, Integer) = Store Integer(1)
                Variable(1, Integer) = Store Integer(1)
                Call id(2), args( Qubit(0), Result(0), )
                Variable(2, Boolean) = Call id(3), args( Result(0), )
                Variable(3, Boolean) = Store Variable(2, Boolean)
                Branch Variable(3, Boolean), 2, 1"#]],
    );
}

#[test]
fn mutable_double_updated_in_loop() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                mutable count = 1.1;
                for _ in 1..3 {
                    if count > 0.1 and MResetZ(q) == One {
                        set count = -count;
                    }
                }
            }
        }
        "#,
    });

    assert_blocks(
        &program,
        //BlockId(0),
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Pointer, )
                Variable(0, Double) = Store Double(1.1)
                Variable(1, Integer) = Store Integer(1)
                Call id(2), args( Qubit(0), Result(0), )
                Variable(2, Boolean) = Call id(3), args( Result(0), )
                Variable(3, Boolean) = Store Variable(2, Boolean)
                Branch Variable(3, Boolean), 2, 1
            Block 1:Block:
                Variable(1, Integer) = Store Integer(2)
                Variable(4, Boolean) = Fcmp Ogt, Variable(0, Double), Double(0.1)
                Variable(5, Boolean) = Store Bool(false)
                Branch Variable(4, Boolean), 4, 3
            Block 2:Block:
                Variable(0, Double) = Store Double(-1.1)
                Jump(1)
            Block 3:Block:
                Branch Variable(5, Boolean), 6, 5
            Block 4:Block:
                Call id(2), args( Qubit(0), Result(1), )
                Variable(6, Boolean) = Call id(3), args( Result(1), )
                Variable(7, Boolean) = Store Variable(6, Boolean)
                Variable(5, Boolean) = Store Variable(7, Boolean)
                Jump(3)
            Block 5:Block:
                Variable(1, Integer) = Store Integer(3)
                Variable(9, Boolean) = Fcmp Ogt, Variable(0, Double), Double(0.1)
                Variable(10, Boolean) = Store Bool(false)
                Branch Variable(9, Boolean), 8, 7
            Block 6:Block:
                Variable(8, Double) = Fmul Double(-1), Variable(0, Double)
                Variable(0, Double) = Store Variable(8, Double)
                Jump(5)
            Block 7:Block:
                Branch Variable(10, Boolean), 10, 9
            Block 8:Block:
                Call id(2), args( Qubit(0), Result(2), )
                Variable(11, Boolean) = Call id(3), args( Result(2), )
                Variable(12, Boolean) = Store Variable(11, Boolean)
                Variable(10, Boolean) = Store Variable(12, Boolean)
                Jump(7)
            Block 9:Block:
                Variable(1, Integer) = Store Integer(4)
                Call id(4), args( Integer(0), EmptyTag, )
                Return
            Block 10:Block:
                Variable(13, Double) = Fmul Double(-1), Variable(0, Double)
                Variable(0, Double) = Store Variable(13, Double)
                Jump(9)"#]],
    );
}
