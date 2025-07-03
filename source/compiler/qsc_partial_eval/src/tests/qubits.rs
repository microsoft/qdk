// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{
    assert_block_instructions, assert_callable, assert_error, get_partial_evaluation_error,
    get_rir_program,
};
use expect_test::expect;
use indoc::indoc;
use qsc_rir::rir::{BlockId, CallableId};

#[test]
fn qubit_ids_are_correct_for_allocate_use_release_one_qubit() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            operation op(q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                let q = QIR.Runtime.__quantum__rt__qubit_allocate();
                op(q);
                QIR.Runtime.__quantum__rt__qubit_release(q);
            }
        }
        "#,
    });
    expect![[r#"
        Callable:
            name: op
            call_type: Regular
            input_type:
                [0]: Qubit
            output_type: <VOID>
            body: <NONE>"#]]
    .assert_eq(&program.get_callable(CallableId(1)).to_string());
    expect![[r#"
        Callable:
            name: __quantum__rt__tuple_record_output
            call_type: OutputRecording
            input_type:
                [0]: Integer
                [1]: Pointer
            output_type: <VOID>
            body: <NONE>"#]]
    .assert_eq(&program.get_callable(CallableId(2)).to_string());
    expect![[r#"
        Block:
            Call id(1), args( Qubit(0), )
            Call id(2), args( Integer(0), EmptyTag, )
            Return"#]]
    .assert_eq(&program.get_block(BlockId(0)).to_string());
}

#[test]
fn qubit_ids_are_correct_for_allocate_use_release_multiple_qubits() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            operation op(q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                let q0 = QIR.Runtime.__quantum__rt__qubit_allocate();
                let q1 = QIR.Runtime.__quantum__rt__qubit_allocate();
                let q2 = QIR.Runtime.__quantum__rt__qubit_allocate();
                op(q0);
                op(q1);
                op(q2);
                QIR.Runtime.__quantum__rt__qubit_release(q2);
                QIR.Runtime.__quantum__rt__qubit_release(q1);
                QIR.Runtime.__quantum__rt__qubit_release(q0);
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
    let tuple_callable_id = CallableId(2);
    assert_callable(
        &program,
        tuple_callable_id,
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
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Qubit(0), )
                Call id(1), args( Qubit(1), )
                Call id(1), args( Qubit(2), )
                Call id(2), args( Integer(0), EmptyTag, )
                Return"#]],
    );
    assert_eq!(program.num_qubits, 3);
    assert_eq!(program.num_results, 0);
}

#[test]
fn qubit_ids_are_correct_for_allocate_use_release_one_qubit_multiple_times() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            operation op(q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                let q0 = QIR.Runtime.__quantum__rt__qubit_allocate();
                op(q0);
                QIR.Runtime.__quantum__rt__qubit_release(q0);
                let q1 = QIR.Runtime.__quantum__rt__qubit_allocate();
                op(q1);
                QIR.Runtime.__quantum__rt__qubit_release(q1);
                let q2 = QIR.Runtime.__quantum__rt__qubit_allocate();
                op(q2);
                QIR.Runtime.__quantum__rt__qubit_release(q2);
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
    let tuple_callable_id = CallableId(2);
    assert_callable(
        &program,
        tuple_callable_id,
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
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Qubit(0), )
                Call id(1), args( Qubit(0), )
                Call id(1), args( Qubit(0), )
                Call id(2), args( Integer(0), EmptyTag, )
                Return"#]],
    );
    assert_eq!(program.num_qubits, 1);
    assert_eq!(program.num_results, 0);
}

#[test]
fn qubit_ids_are_correct_for_allocate_use_release_multiple_qubits_interleaved() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            operation op(q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                let q0 = QIR.Runtime.__quantum__rt__qubit_allocate();
                op(q0);
                let q1 = QIR.Runtime.__quantum__rt__qubit_allocate();
                op(q1);
                let q2 = QIR.Runtime.__quantum__rt__qubit_allocate();
                op(q2);
                QIR.Runtime.__quantum__rt__qubit_release(q2);
                let q3 = QIR.Runtime.__quantum__rt__qubit_allocate();
                let q4 = QIR.Runtime.__quantum__rt__qubit_allocate();
                op(q3);
                op(q4);
                QIR.Runtime.__quantum__rt__qubit_release(q4);
                QIR.Runtime.__quantum__rt__qubit_release(q3);
                QIR.Runtime.__quantum__rt__qubit_release(q1);
                QIR.Runtime.__quantum__rt__qubit_release(q0);
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
    let tuple_callable_id = CallableId(2);
    assert_callable(
        &program,
        tuple_callable_id,
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
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Qubit(0), )
                Call id(1), args( Qubit(1), )
                Call id(1), args( Qubit(2), )
                Call id(1), args( Qubit(2), )
                Call id(1), args( Qubit(3), )
                Call id(2), args( Integer(0), EmptyTag, )
                Return"#]],
    );
    assert_eq!(program.num_qubits, 4);
    assert_eq!(program.num_results, 0);
}

#[test]
fn qubit_array_allocation_and_access() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            operation Op(q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                use qs = Qubit[3];
                Op(qs[0]);
                Op(qs[1]);
                Op(qs[2]);
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
                name: Op
                call_type: Regular
                input_type:
                    [0]: Qubit
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let tuple_record_callable_id = CallableId(2);
    assert_callable(
        &program,
        tuple_record_callable_id,
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
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Variable(0, Integer) = Store Integer(0)
                Variable(0, Integer) = Store Integer(1)
                Variable(0, Integer) = Store Integer(2)
                Variable(0, Integer) = Store Integer(3)
                Call id(1), args( Qubit(0), )
                Call id(1), args( Qubit(1), )
                Call id(1), args( Qubit(2), )
                Variable(1, Integer) = Store Integer(0)
                Variable(1, Integer) = Store Integer(1)
                Variable(1, Integer) = Store Integer(2)
                Variable(1, Integer) = Store Integer(3)
                Call id(2), args( Integer(0), EmptyTag, )
                Return"#]],
    );
    assert_eq!(program.num_qubits, 3);
    assert_eq!(program.num_results, 0);
}

#[test]
fn qubit_escaping_scope_triggers_runtime_error() {
    let error = get_partial_evaluation_error(indoc! {
        r#"
        namespace Test {
            operation Op(q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                let q = {
                    use q = Qubit();
                    q
                };
                Op(q);
            }
        }
        "#,
    });
    assert_error(
        &error,
        &expect![[
            r#"EvaluationFailed("qubit used after release", PackageSpan { package: PackageId(2), span: Span { lo: 204, hi: 205 } })"#
        ]],
    );
}

#[test]
fn qubit_double_release_triggers_runtime_error() {
    let error = get_partial_evaluation_error(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                let q = QIR.Runtime.__quantum__rt__qubit_allocate();
                QIR.Runtime.__quantum__rt__qubit_release(q);
                QIR.Runtime.__quantum__rt__qubit_release(q);
            }
        }
        "#,
    });
    assert_error(
        &error,
        &expect![[
            r#"EvaluationFailed("qubit double release", PackageSpan { package: PackageId(2), span: Span { lo: 229, hi: 230 } })"#
        ]],
    );
}

#[test]
fn qubit_relabel_in_dynamic_block_triggers_capability_error() {
    let error = get_partial_evaluation_error(indoc! {
        r#"
        operation Main() : Result {
            use qs = Qubit[2];
            if M(qs[0]) == One {
                Relabel(qs, Std.Arrays.Reversed(qs));
            }
            MResetZ(qs[1])
        }
        "#,
    });

    assert_error(
        &error,
        &expect!["CapabilityError(UseOfDynamicQubit(Span { lo: 67160, hi: 67173 }))"],
    );
}
