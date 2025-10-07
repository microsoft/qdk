// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{assert_block_instructions, assert_callable, get_rir_program};
use expect_test::expect;
use indoc::indoc;
use qsc_rir::rir::{BlockId, CallableId};

#[test]
fn call_to_intrinsic_operation_using_double_literal() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            operation op(d : Double) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                op(1.0);
            }
        }
    "#});
    let op_callable_id = CallableId(1);
    assert_callable(
        &program,
        op_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__initialize
                call_type: Initialize
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
                Call id(2), args( Double(1), ) !dbg dbg_location=1
                Call id(3), args( Integer(0), EmptyTag, ) !dbg dbg_location=0
                Return !dbg dbg_location=0"#]],
    );
}

#[test]
fn calls_to_intrinsic_operation_using_inline_expressions() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            function PI() : Double { 3.14159 }
            operation op(d : Double) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                op(2.71828 * 0.0);
                op(PI() / PI());
                op((PI() + PI()) / (2.0 * PI()));
            }
        }
    "#});
    let op_callable_id = CallableId(1);
    assert_callable(
        &program,
        op_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__initialize
                call_type: Initialize
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
                Call id(2), args( Double(0), ) !dbg dbg_location=1
                Call id(2), args( Double(1), ) !dbg dbg_location=2
                Call id(2), args( Double(1), ) !dbg dbg_location=3
                Call id(3), args( Integer(0), EmptyTag, ) !dbg dbg_location=0
                Return !dbg dbg_location=0"#]],
    );
}

#[test]
fn calls_to_intrinsic_operation_using_variables() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            operation op(d : Double) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                let pi = 4.0;
                let pi_over_two = pi / 2.0;
                op(pi_over_two);
                mutable n_pi = 1.0 * pi;
                op(n_pi);
                set n_pi = 2.0 * pi;
                op(n_pi);
            }
        }
    "#});
    let op_callable_id = CallableId(1);
    assert_callable(
        &program,
        op_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__initialize
                call_type: Initialize
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
                Call id(2), args( Double(2), ) !dbg dbg_location=1
                Variable(0, Double) = Store Double(4) !dbg dbg_location=1
                Call id(2), args( Double(4), ) !dbg dbg_location=2
                Variable(0, Double) = Store Double(8) !dbg dbg_location=2
                Call id(2), args( Double(8), ) !dbg dbg_location=3
                Call id(3), args( Integer(0), EmptyTag, ) !dbg dbg_location=0
                Return !dbg dbg_location=0"#]],
    );
}
