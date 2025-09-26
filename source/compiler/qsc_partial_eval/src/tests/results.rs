// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{assert_block_instructions, assert_callable, get_rir_program};
use expect_test::expect;
use indoc::indoc;
use qsc_rir::rir::{BlockId, CallableId};

#[test]
fn result_ids_are_correct_for_measuring_and_resetting_one_qubit() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                QIR.Intrinsic.__quantum__qis__mresetz__body(q)
            }
        }
        "#,
    });
    let op_callable_id = CallableId(1);
    let result_record_id = CallableId(2);
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
    assert_callable(
        &program,
        result_record_id,
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
                Call id(3), args( Result(0), Tag(0, 3), )
                Return"#]],
    );
    assert_eq!(program.num_qubits, 1);
    assert_eq!(program.num_results, 1);
}

#[test]
fn result_ids_are_correct_for_measuring_one_qubit() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                QIR.Intrinsic.__quantum__qis__m__body(q)
            }
        }
        "#,
    });
    let op_callable_id = CallableId(1);
    let result_record_id = CallableId(2);
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
    assert_callable(
        &program,
        result_record_id,
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
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), Result(0), )
                Call id(3), args( Result(0), Tag(0, 3), )
                Return"#]],
    );
    assert_eq!(program.num_qubits, 1);
    assert_eq!(program.num_results, 1);
}

#[test]
fn result_ids_are_correct_for_measuring_one_qubit_multiple_times() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : (Result, Result, Result) {
                use q = Qubit();
                (QIR.Intrinsic.__quantum__qis__m__body(q),
                QIR.Intrinsic.__quantum__qis__m__body(q),
                QIR.Intrinsic.__quantum__qis__m__body(q))
            }
        }
        "#,
    });
    let op_callable_id = CallableId(1);
    let tuple_record_id = CallableId(2);
    let result_record_id = CallableId(3);
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
    assert_callable(
        &program,
        tuple_record_id,
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
    assert_callable(
        &program,
        result_record_id,
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
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), Result(0), )
                Call id(2), args( Qubit(0), Result(1), )
                Call id(2), args( Qubit(0), Result(2), )
                Call id(3), args( Integer(3), EmptyTag, )
                Call id(4), args( Result(0), Tag(0, 5), )
                Call id(4), args( Result(1), Tag(1, 5), )
                Call id(4), args( Result(2), Tag(2, 5), )
                Return"#]],
    );
}

#[test]
fn result_ids_are_correct_for_measuring_one_qubit_multiple_times_into_array() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result[] {
                use q = Qubit();
                [QIR.Intrinsic.__quantum__qis__m__body(q),
                QIR.Intrinsic.__quantum__qis__m__body(q),
                QIR.Intrinsic.__quantum__qis__m__body(q)]
            }
        }
        "#,
    });
    let op_callable_id = CallableId(1);
    let array_record_id = CallableId(2);
    let result_record_id = CallableId(3);
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
    assert_callable(
        &program,
        array_record_id,
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
    assert_callable(
        &program,
        result_record_id,
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
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), Result(0), )
                Call id(2), args( Qubit(0), Result(1), )
                Call id(2), args( Qubit(0), Result(2), )
                Call id(3), args( Integer(3), EmptyTag, )
                Call id(4), args( Result(0), Tag(0, 5), )
                Call id(4), args( Result(1), Tag(1, 5), )
                Call id(4), args( Result(2), Tag(2, 5), )
                Return"#]],
    );
    assert_eq!(program.num_qubits, 1);
    assert_eq!(program.num_results, 3);
}

#[test]
fn result_ids_are_correct_for_measuring_multiple_qubits() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : (Result, Result, Result) {
                use (q0, q1, q2) = (Qubit(), Qubit(), Qubit());
                (QIR.Intrinsic.__quantum__qis__m__body(q0),
                QIR.Intrinsic.__quantum__qis__m__body(q1),
                QIR.Intrinsic.__quantum__qis__m__body(q2))
            }
        }
        "#,
    });
    let op_callable_id = CallableId(1);
    let tuple_record_id = CallableId(2);
    let result_record_id = CallableId(3);
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
    assert_callable(
        &program,
        tuple_record_id,
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
    assert_callable(
        &program,
        result_record_id,
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
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), Result(0), )
                Call id(2), args( Qubit(1), Result(1), )
                Call id(2), args( Qubit(2), Result(2), )
                Call id(3), args( Integer(3), EmptyTag, )
                Call id(4), args( Result(0), Tag(0, 5), )
                Call id(4), args( Result(1), Tag(1, 5), )
                Call id(4), args( Result(2), Tag(2, 5), )
                Return"#]],
    );
    assert_eq!(program.num_qubits, 3);
    assert_eq!(program.num_results, 3);
}
