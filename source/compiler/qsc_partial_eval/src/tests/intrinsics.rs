// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{assert_block_instructions, assert_blocks, assert_callable, get_rir_program};
use expect_test::{Expect, expect};
use indoc::{formatdoc, indoc};
use qsc_rir::rir::{BlockId, CallableId};

fn check_call_to_single_qubit_instrinsic_adds_callable_and_generates_instruction(
    intrinsic_name: &str,
    expected_callable: &Expect,
    expected_block: &Expect,
) {
    let program = get_rir_program(
        formatdoc! {
            r#"
            namespace Test {{
                @EntryPoint()
                operation Main() : Unit {{
                    use q = Qubit();
                    QIR.Intrinsic.{intrinsic_name}(q);
                }}
            }}
            "#,
            intrinsic_name = intrinsic_name
        }
        .as_str(),
    );
    let op_callable_id = CallableId(1);
    assert_callable(&program, op_callable_id, expected_callable);
    assert_block_instructions(&program, BlockId(0), expected_block);
}

fn check_call_to_single_qubit_rotation_instrinsic_adds_callable_and_generates_instruction(
    intrinsic_name: &str,
    expected_callable: &Expect,
    expected_block: &Expect,
) {
    let program = get_rir_program(
        formatdoc! {
            r#"
            namespace Test {{
                @EntryPoint()
                operation Main() : Unit {{
                    use q = Qubit();
                    QIR.Intrinsic.{intrinsic_name}(0.0, q);
                }}
            }}
            "#,
            intrinsic_name = intrinsic_name
        }
        .as_str(),
    );
    let op_callable_id = CallableId(1);
    assert_callable(&program, op_callable_id, expected_callable);
    assert_block_instructions(&program, BlockId(0), expected_block);
}

fn check_call_to_two_qubits_rotation_instrinsic_adds_callable_and_generates_instruction(
    intrinsic_name: &str,
    expected_callable: &Expect,
    expected_block: &Expect,
) {
    let program = get_rir_program(
        formatdoc! {
            r#"
            namespace Test {{
                @EntryPoint()
                operation Main() : Unit {{
                    use (q0, q1) = (Qubit(), Qubit());
                    QIR.Intrinsic.{intrinsic_name}(0.0, q0, q1);
                }}
            }}
            "#,
            intrinsic_name = intrinsic_name
        }
        .as_str(),
    );
    let op_callable_id = CallableId(1);
    assert_callable(&program, op_callable_id, expected_callable);
    assert_block_instructions(&program, BlockId(0), expected_block);
}

fn check_call_to_two_qubits_instrinsic_adds_callable_and_generates_instruction(
    intrinsic_name: &str,
    expected_callable: &Expect,
    expected_block: &Expect,
) {
    let program = get_rir_program(
        formatdoc! {
            r#"
            namespace Test {{
                @EntryPoint()
                operation Main() : Unit {{
                    use (q0, q1) = (Qubit(), Qubit());
                    QIR.Intrinsic.{intrinsic_name}(q0, q1);
                }}
            }}
            "#,
            intrinsic_name = intrinsic_name
        }
        .as_str(),
    );
    let op_callable_id = CallableId(1);
    assert_callable(&program, op_callable_id, expected_callable);
    assert_block_instructions(&program, BlockId(0), expected_block);
}

fn check_call_to_three_qubits_instrinsic_adds_callable_and_generates_instruction(
    intrinsic_name: &str,
    expected_callable: &Expect,
    expected_block: &Expect,
) {
    let program = get_rir_program(
        formatdoc! {
            r#"
            namespace Test {{
                @EntryPoint()
                operation Main() : Unit {{
                    use (q0, q1, q2) = (Qubit(), Qubit(), Qubit());
                    QIR.Intrinsic.{intrinsic_name}(q0, q1, q2);
                }}
            }}
            "#,
            intrinsic_name = intrinsic_name
        }
        .as_str(),
    );
    let op_callable_id = CallableId(1);
    assert_callable(&program, op_callable_id, expected_callable);
    assert_block_instructions(&program, BlockId(0), expected_block);
}

#[test]
fn call_to_intrinsic_h_adds_callable_and_generates_instruction() {
    check_call_to_single_qubit_instrinsic_adds_callable_and_generates_instruction(
        "__quantum__qis__h__body",
        &expect![[r#"
            Callable:
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
        &expect![[r#"
            Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), )
                Call id(3), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn call_to_intrinsic_s_adds_callable_and_generates_instruction() {
    check_call_to_single_qubit_instrinsic_adds_callable_and_generates_instruction(
        "__quantum__qis__s__body",
        &expect![[r#"
            Callable:
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
        &expect![[r#"
            Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), )
                Call id(3), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn call_to_intrinsic_adjoint_s_adds_callable_and_generates_instruction() {
    check_call_to_single_qubit_instrinsic_adds_callable_and_generates_instruction(
        "__quantum__qis__s__adj",
        &expect![[r#"
            Callable:
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
        &expect![[r#"
            Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), )
                Call id(3), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn call_to_intrinsic_sx_adds_callable_and_generates_instruction() {
    check_call_to_single_qubit_instrinsic_adds_callable_and_generates_instruction(
        "__quantum__qis__sx__body",
        &expect![[r#"
            Callable:
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
        &expect![[r#"
            Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), )
                Call id(3), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn call_to_intrinsic_t_adds_callable_and_generates_instruction() {
    check_call_to_single_qubit_instrinsic_adds_callable_and_generates_instruction(
        "__quantum__qis__t__body",
        &expect![[r#"
            Callable:
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
        &expect![[r#"
            Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), )
                Call id(3), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn call_to_intrinsic_adjoint_t_adds_callable_and_generates_instruction() {
    check_call_to_single_qubit_instrinsic_adds_callable_and_generates_instruction(
        "__quantum__qis__t__adj",
        &expect![[r#"
            Callable:
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
        &expect![[r#"
            Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), )
                Call id(3), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn call_to_intrinsic_x_adds_callable_and_generates_instruction() {
    check_call_to_single_qubit_instrinsic_adds_callable_and_generates_instruction(
        "__quantum__qis__x__body",
        &expect![[r#"
            Callable:
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
        &expect![[r#"
            Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), )
                Call id(3), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn call_to_intrinsic_y_adds_callable_and_generates_instruction() {
    check_call_to_single_qubit_instrinsic_adds_callable_and_generates_instruction(
        "__quantum__qis__y__body",
        &expect![[r#"
            Callable:
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
        &expect![[r#"
            Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), )
                Call id(3), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn call_to_intrinsic_z_adds_callable_and_generates_instruction() {
    check_call_to_single_qubit_instrinsic_adds_callable_and_generates_instruction(
        "__quantum__qis__z__body",
        &expect![[r#"
            Callable:
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
        &expect![[r#"
            Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), )
                Call id(3), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn call_to_intrinsic_swap_adds_callable_and_generates_instruction() {
    check_call_to_two_qubits_instrinsic_adds_callable_and_generates_instruction(
        "__quantum__qis__swap__body",
        &expect![[r#"
            Callable:
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
        &expect![[r#"
            Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), Qubit(1), )
                Call id(3), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn call_to_intrinsic_cx_adds_callable_and_generates_instruction() {
    check_call_to_two_qubits_instrinsic_adds_callable_and_generates_instruction(
        "__quantum__qis__cx__body",
        &expect![[r#"
            Callable:
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
        &expect![[r#"
            Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), Qubit(1), )
                Call id(3), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn call_to_intrinsic_cy_adds_callable_and_generates_instruction() {
    check_call_to_two_qubits_instrinsic_adds_callable_and_generates_instruction(
        "__quantum__qis__cy__body",
        &expect![[r#"
            Callable:
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
        &expect![[r#"
            Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), Qubit(1), )
                Call id(3), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn call_to_intrinsic_cz_adds_callable_and_generates_instruction() {
    check_call_to_two_qubits_instrinsic_adds_callable_and_generates_instruction(
        "__quantum__qis__cz__body",
        &expect![[r#"
            Callable:
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
        &expect![[r#"
            Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), Qubit(1), )
                Call id(3), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn call_to_intrinsic_ccx_adds_callable_and_generates_instruction() {
    check_call_to_three_qubits_instrinsic_adds_callable_and_generates_instruction(
        "__quantum__qis__ccx__body",
        &expect![[r#"
            Callable:
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
        &expect![[r#"
            Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), Qubit(1), Qubit(2), )
                Call id(3), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn call_to_intrinsic_rx_adds_callable_and_generates_instruction() {
    check_call_to_single_qubit_rotation_instrinsic_adds_callable_and_generates_instruction(
        "__quantum__qis__rx__body",
        &expect![[r#"
            Callable:
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
        &expect![[r#"
            Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Double(0), Qubit(0), )
                Call id(3), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn call_to_intrinsic_rxx_adds_callable_and_generates_instruction() {
    check_call_to_two_qubits_rotation_instrinsic_adds_callable_and_generates_instruction(
        "__quantum__qis__rxx__body",
        &expect![[r#"
            Callable:
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
        &expect![[r#"
            Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Double(0), Qubit(0), Qubit(1), )
                Call id(3), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn call_to_intrinsic_ry_adds_callable_and_generates_instruction() {
    check_call_to_single_qubit_rotation_instrinsic_adds_callable_and_generates_instruction(
        "__quantum__qis__ry__body",
        &expect![[r#"
            Callable:
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
        &expect![[r#"
            Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Double(0), Qubit(0), )
                Call id(3), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn call_to_intrinsic_ryy_adds_callable_and_generates_instruction() {
    check_call_to_two_qubits_rotation_instrinsic_adds_callable_and_generates_instruction(
        "__quantum__qis__ryy__body",
        &expect![[r#"
            Callable:
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
        &expect![[r#"
            Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Double(0), Qubit(0), Qubit(1), )
                Call id(3), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn call_to_intrinsic_rz_adds_callable_and_generates_instruction() {
    check_call_to_single_qubit_rotation_instrinsic_adds_callable_and_generates_instruction(
        "__quantum__qis__rz__body",
        &expect![[r#"
            Callable:
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
        &expect![[r#"
            Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Double(0), Qubit(0), )
                Call id(3), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn call_to_intrinsic_rzz_adds_callable_and_generates_instruction() {
    check_call_to_two_qubits_rotation_instrinsic_adds_callable_and_generates_instruction(
        "__quantum__qis__rzz__body",
        &expect![[r#"
            Callable:
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
        &expect![[r#"
            Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Double(0), Qubit(0), Qubit(1), )
                Call id(3), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn check_partial_eval_for_call_to_reset() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                QIR.Intrinsic.__quantum__qis__reset__body(q);
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
                Call id(2), args( Qubit(0), )
                Call id(3), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn call_to_intrinsic_m_adds_callable_and_generates_instruction() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                QIR.Intrinsic.__quantum__qis__m__body(q);
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
                Call id(2), args( Qubit(0), Result(0), )
                Call id(3), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn call_to_intrinsic_mresetz_adds_callable_and_generates_instruction() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                QIR.Intrinsic.__quantum__qis__mresetz__body(q);
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
                Call id(2), args( Qubit(0), Result(0), )
                Call id(3), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn calls_to_intrinsic_begin_estimate_caching_with_classical_values_always_yield_true() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            import Std.ResourceEstimation.*;
            operation Op(q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                if BeginEstimateCaching("test0", 0) {
                    Op(q);
                }
                if BeginEstimateCaching("test1", 1) {
                    Op(q);
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
                Call id(2), args( Qubit(0), )
                Call id(2), args( Qubit(0), )
                Call id(3), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn call_to_intrinsic_begin_estimate_caching_with_dynamic_values_yields_true() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            import Std.ResourceEstimation.*;
            open QIR.Intrinsic;
            operation Op(q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                let i = __quantum__qis__m__body(q) == Zero ? 0 | 1;
                if BeginEstimateCaching("test0", i) {
                    Op(q);
                }
            }
        }
        "#,
    });
    let measure_callable_id = CallableId(1);
    assert_callable(
        &program,
        measure_callable_id,
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
                name: __quantum__qis__m__body
                call_type: Measurement
                input_type:
                    [0]: Qubit
                    [1]: Result
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let op_callable_id = CallableId(3);
    assert_callable(
        &program,
        op_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__read_result
                call_type: Readout
                input_type:
                    [0]: Result
                output_type: Boolean
                body: <NONE>"#]],
    );
    let output_recording_callable_id = CallableId(4);
    assert_callable(
        &program,
        output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: Op
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
                Call id(4), args( Qubit(0), )
                Call id(5), args( Integer(0), EmptyTag, )
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
fn call_to_intrinsic_end_estimate_caching_does_not_generate_instructions() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            import Std.ResourceEstimation.*;
            @EntryPoint()
            operation Main() : Unit {
                EndEstimateCaching();
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
                Call id(2), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn call_to_account_for_estimates_does_not_generate_instructions() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            import Std.ResourceEstimation.*;
            @EntryPoint()
            operation Main() : Unit {
                // Calls to internal operation `AccountForEstimatesInternal`, which is intrinsic.
                AccountForEstimates([], 0, []);
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
                Call id(2), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn call_to_begin_repeat_estimates_does_not_generate_instructions() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            import Std.ResourceEstimation.*;
            @EntryPoint()
            operation Main() : Unit {
                // Calls to internal operation `BeginRepeatEstimatesInternal`, which is intrinsic.
                BeginRepeatEstimates(0);
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
                Call id(2), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn call_to_end_repeat_estimates_does_not_generate_instructions() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            import Std.ResourceEstimation.*;
            @EntryPoint()
            operation Main() : Unit {
                // Calls to internal operation `EndRepeatEstimatesInternal`, which is intrinsic.
                EndRepeatEstimates();
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
                Call id(2), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn call_to_dump_machine_does_not_generate_instructions() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            import Std.Diagnostics.*;
            @EntryPoint()
            operation Main() : Unit {
                DumpMachine();
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
                Call id(2), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn call_to_dump_register_does_not_generate_instructions() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            import Std.Diagnostics.*;
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                DumpRegister([q]);
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
                Call id(2), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn use_of_noise_does_not_generate_instructions() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            import Std.Diagnostics.*;
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                ConfigurePauliNoise(0.2, 0.2, 0.2);
                ApplyIdleNoise(q);
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
                Call id(2), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
#[should_panic(
    expected = "partial evaluation failed: UnsupportedSimulationIntrinsic(\"CheckZero\","
)]
fn call_to_check_zero_panics() {
    _ = get_rir_program(indoc! {
        r#"
        namespace Test {
            import Std.Diagnostics.*;
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                let _ = CheckZero(q);
            }
        }
        "#,
    });
}

#[test]
#[should_panic(expected = "`DrawRandomInt` is not a supported by partial evaluation")]
fn call_to_draw_random_int_panics() {
    _ = get_rir_program(indoc! {
        r#"
        namespace Test {
            import Std.Random.*;
            @EntryPoint()
            operation Main() : Unit {
                let _ = DrawRandomInt(0, 1);
            }
        }
        "#,
    });
}

#[test]
#[should_panic(expected = "`DrawRandomDouble` is not a supported by partial evaluation")]
fn call_to_draw_random_double_panics() {
    _ = get_rir_program(indoc! {
        r#"
        namespace Test {
            import Std.Random.*;
            @EntryPoint()
            operation Main() : Unit {
                let _ = DrawRandomDouble(0.0, 1.0);
            }
        }
        "#,
    });
}

#[test]
#[should_panic(expected = "`DrawRandomBool` is not a supported by partial evaluation")]
fn call_to_draw_random_bool_panics() {
    _ = get_rir_program(indoc! {
        r#"
        namespace Test {
            import Std.Random.*;
            @EntryPoint()
            operation Main() : Unit {
                let _ = DrawRandomBool(0.0);
            }
        }
        "#,
    });
}

#[test]
fn call_to_length_in_inner_function_succeeds() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                mutable results = [One];
                set results w/= 0 <- MResetZ(q);
                Inner(results)
            }

            function Inner(results : Result[]) : Int {
                Length(results)
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
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), Result(0), )
                Call id(3), args( Integer(1), Tag(0, 3), )
                Return"#]],
    );
}

#[test]
fn call_to_pauli_i_rotation_for_global_phase_is_noop() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                R(PauliI, 1.0, q);
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
                Call id(2), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn call_to_operation_with_codegen_intrinsic_override_should_skip_impl() {
    let program = get_rir_program(indoc! {"
        namespace Test {
            operation Op1() : Unit {
                body intrinsic;
            }
            @SimulatableIntrinsic()
            operation Op2() : Unit {
                Op1();
            }
            operation Op3() : Unit {
                Op1();
            }
            @EntryPoint()
            operation Main() : Unit {
                Op1();
                Op2();
                Op3();
            }
        }
    "});

    let op1_callable_id = CallableId(1);
    assert_callable(
        &program,
        op1_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let op2_callable_id = CallableId(2);
    assert_callable(
        &program,
        op2_callable_id,
        &expect![[r#"
            Callable:
                name: Op1
                call_type: Regular
                input_type: <VOID>
                output_type: <VOID>
                body: <NONE>"#]],
    );
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Pointer, )
                Call id(2), args( )
                Call id(3), args( )
                Call id(2), args( )
                Call id(4), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn call_to_intrinsic_operation_that_returns_bool_value_should_produce_variable_usage() {
    let program = get_rir_program(indoc! {"
        namespace Test {
            operation Op1() : Bool {
                body intrinsic;
            }
            @EntryPoint()
            operation Main() : Bool {
                Op1()
            }
        }
    "});

    let op1_callable_id = CallableId(1);
    assert_callable(
        &program,
        op1_callable_id,
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
                Variable(0, Boolean) = Call id(2), args( )
                Call id(3), args( Variable(0, Boolean), Tag(0, 3), )
                Return"#]],
    );
}

#[test]
fn call_to_intrinsic_operation_that_returns_int_value_should_produce_variable_usage() {
    let program = get_rir_program(indoc! {"
        namespace Test {
            operation Op1() : Int {
                body intrinsic;
            }
            @EntryPoint()
            operation Main() : Int {
                Op1()
            }
        }
    "});

    let op1_callable_id = CallableId(1);
    assert_callable(
        &program,
        op1_callable_id,
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
                Variable(0, Integer) = Call id(2), args( )
                Call id(3), args( Variable(0, Integer), Tag(0, 3), )
                Return"#]],
    );
}

#[test]
fn call_to_intrinsic_operation_that_returns_double_value_should_produce_variable_usage() {
    let program = get_rir_program(indoc! {"
        namespace Test {
            operation Op1() : Double {
                body intrinsic;
            }
            @EntryPoint()
            operation Main() : Double {
                Op1()
            }
        }
    "});

    let op1_callable_id = CallableId(1);
    assert_callable(
        &program,
        op1_callable_id,
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
                Variable(0, Double) = Call id(2), args( )
                Call id(3), args( Variable(0, Double), Tag(0, 3), )
                Return"#]],
    );
}

#[test]
#[should_panic(
    expected = "partial evaluation failed: UnexpectedDynamicIntrinsicReturnType(\"Result\", PackageSpan { package: PackageId(2), span: Span { lo: 137, hi: 140 } })"
)]
fn call_to_intrinsic_operation_that_returns_result_value_should_fail() {
    let _ = get_rir_program(indoc! {"
        namespace Test {
            operation Op1() : Result {
                body intrinsic;
            }
            @EntryPoint()
            operation Main() : Result {
                Op1()
            }
        }
    "});
}

#[test]
#[should_panic(
    expected = "partial evaluation failed: UnexpectedDynamicIntrinsicReturnType(\"Qubit\", PackageSpan { package: PackageId(2), span: Span { lo: 142, hi: 145 } })"
)]
fn call_to_intrinsic_operation_that_returns_qubit_value_should_fail() {
    let _ = get_rir_program(indoc! {"
        namespace Test {
            operation Op1() : Qubit {
                body intrinsic;
            }
            @EntryPoint()
            operation Main() : Unit {
                let q = Op1();
            }
        }
    "});
}
