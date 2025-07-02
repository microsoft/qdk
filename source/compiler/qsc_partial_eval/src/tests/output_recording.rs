// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{assert_error, get_partial_evaluation_error, get_rir_program};
use expect_test::expect;
use indoc::indoc;

#[test]
fn output_recording_for_tuple_of_different_types() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : (Result, Bool) {
                use q = Qubit();
                let r = QIR.Intrinsic.__quantum__qis__mresetz__body(q);
                (r, r == Zero)
            }
        }
        "#,
    });

    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 0
                Callable 1: Callable:
                    name: __quantum__qis__mresetz__body
                    call_type: Measurement
                    input_type:
                        [0]: Qubit
                        [1]: Result
                    output_type: <VOID>
                    body: <NONE>
                Callable 2: Callable:
                    name: __quantum__qis__read_result__body
                    call_type: Readout
                    input_type:
                        [0]: Result
                    output_type: Boolean
                    body: <NONE>
                Callable 3: Callable:
                    name: __quantum__rt__tuple_record_output
                    call_type: OutputRecording
                    input_type:
                        [0]: Integer
                        [1]: Pointer
                    output_type: <VOID>
                    body: <NONE>
                Callable 4: Callable:
                    name: __quantum__rt__result_record_output
                    call_type: OutputRecording
                    input_type:
                        [0]: Result
                        [1]: Pointer
                    output_type: <VOID>
                    body: <NONE>
                Callable 5: Callable:
                    name: __quantum__rt__bool_record_output
                    call_type: OutputRecording
                    input_type:
                        [0]: Boolean
                        [1]: Pointer
                    output_type: <VOID>
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Call id(1), args( Qubit(0), Result(0), )
                    Variable(0, Boolean) = Call id(2), args( Result(0), )
                    Variable(1, Boolean) = Icmp Eq, Variable(0, Boolean), Bool(false)
                    Call id(3), args( Integer(2), Pointer, )
                    Call id(4), args( Result(0), Pointer, )
                    Call id(5), args( Variable(1, Boolean), Pointer, )
                    Return
            config: Config:
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations | BackwardsBranching | HigherLevelConstructs | QubitReset)
            num_qubits: 1
            num_results: 1"#]]
    .assert_eq(&program.to_string());
}

#[test]
fn output_recording_for_nested_tuples() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : (Result, (Bool, Result), (Bool,)) {
                use q = Qubit();
                let r = QIR.Intrinsic.__quantum__qis__mresetz__body(q);
                (r, (r == Zero, r), (r == One,))
            }
        }
        "#,
    });

    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 0
                Callable 1: Callable:
                    name: __quantum__qis__mresetz__body
                    call_type: Measurement
                    input_type:
                        [0]: Qubit
                        [1]: Result
                    output_type: <VOID>
                    body: <NONE>
                Callable 2: Callable:
                    name: __quantum__qis__read_result__body
                    call_type: Readout
                    input_type:
                        [0]: Result
                    output_type: Boolean
                    body: <NONE>
                Callable 3: Callable:
                    name: __quantum__rt__tuple_record_output
                    call_type: OutputRecording
                    input_type:
                        [0]: Integer
                        [1]: Pointer
                    output_type: <VOID>
                    body: <NONE>
                Callable 4: Callable:
                    name: __quantum__rt__result_record_output
                    call_type: OutputRecording
                    input_type:
                        [0]: Result
                        [1]: Pointer
                    output_type: <VOID>
                    body: <NONE>
                Callable 5: Callable:
                    name: __quantum__rt__bool_record_output
                    call_type: OutputRecording
                    input_type:
                        [0]: Boolean
                        [1]: Pointer
                    output_type: <VOID>
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Call id(1), args( Qubit(0), Result(0), )
                    Variable(0, Boolean) = Call id(2), args( Result(0), )
                    Variable(1, Boolean) = Icmp Eq, Variable(0, Boolean), Bool(false)
                    Variable(2, Boolean) = Call id(2), args( Result(0), )
                    Variable(3, Boolean) = Store Variable(2, Boolean)
                    Call id(3), args( Integer(3), Pointer, )
                    Call id(4), args( Result(0), Pointer, )
                    Call id(3), args( Integer(2), Pointer, )
                    Call id(5), args( Variable(1, Boolean), Pointer, )
                    Call id(4), args( Result(0), Pointer, )
                    Call id(3), args( Integer(1), Pointer, )
                    Call id(5), args( Variable(3, Boolean), Pointer, )
                    Return
            config: Config:
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations | BackwardsBranching | HigherLevelConstructs | QubitReset)
            num_qubits: 1
            num_results: 1"#]]
    .assert_eq(&program.to_string());
}

#[test]
fn output_recording_for_tuple_of_arrays() {
    // This program would not actually pass RCA checks as it shows up as using a dynamically sized array.
    // However, the output recording should still be correct if/when we support this kind of return in the future.
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : (Result, Bool[]) {
                use q = Qubit();
                let r = QIR.Intrinsic.__quantum__qis__mresetz__body(q);
                (r, [r == Zero, r == One])
            }
        }
        "#,
    });

    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 0
                Callable 1: Callable:
                    name: __quantum__qis__mresetz__body
                    call_type: Measurement
                    input_type:
                        [0]: Qubit
                        [1]: Result
                    output_type: <VOID>
                    body: <NONE>
                Callable 2: Callable:
                    name: __quantum__qis__read_result__body
                    call_type: Readout
                    input_type:
                        [0]: Result
                    output_type: Boolean
                    body: <NONE>
                Callable 3: Callable:
                    name: __quantum__rt__tuple_record_output
                    call_type: OutputRecording
                    input_type:
                        [0]: Integer
                        [1]: Pointer
                    output_type: <VOID>
                    body: <NONE>
                Callable 4: Callable:
                    name: __quantum__rt__result_record_output
                    call_type: OutputRecording
                    input_type:
                        [0]: Result
                        [1]: Pointer
                    output_type: <VOID>
                    body: <NONE>
                Callable 5: Callable:
                    name: __quantum__rt__array_record_output
                    call_type: OutputRecording
                    input_type:
                        [0]: Integer
                        [1]: Pointer
                    output_type: <VOID>
                    body: <NONE>
                Callable 6: Callable:
                    name: __quantum__rt__bool_record_output
                    call_type: OutputRecording
                    input_type:
                        [0]: Boolean
                        [1]: Pointer
                    output_type: <VOID>
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Call id(1), args( Qubit(0), Result(0), )
                    Variable(0, Boolean) = Call id(2), args( Result(0), )
                    Variable(1, Boolean) = Icmp Eq, Variable(0, Boolean), Bool(false)
                    Variable(2, Boolean) = Call id(2), args( Result(0), )
                    Variable(3, Boolean) = Store Variable(2, Boolean)
                    Call id(3), args( Integer(2), Pointer, )
                    Call id(4), args( Result(0), Pointer, )
                    Call id(5), args( Integer(2), Pointer, )
                    Call id(6), args( Variable(1, Boolean), Pointer, )
                    Call id(6), args( Variable(3, Boolean), Pointer, )
                    Return
            config: Config:
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations | BackwardsBranching | HigherLevelConstructs | QubitReset)
            num_qubits: 1
            num_results: 1"#]]
    .assert_eq(&program.to_string());
}

#[test]
fn output_recording_for_array_of_tuples() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : (Result, Bool)[] {
                use q = Qubit();
                let r = QIR.Intrinsic.__quantum__qis__mresetz__body(q);
                [(r, r == Zero), (r, r == One)]
            }
        }
        "#,
    });

    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 0
                Callable 1: Callable:
                    name: __quantum__qis__mresetz__body
                    call_type: Measurement
                    input_type:
                        [0]: Qubit
                        [1]: Result
                    output_type: <VOID>
                    body: <NONE>
                Callable 2: Callable:
                    name: __quantum__qis__read_result__body
                    call_type: Readout
                    input_type:
                        [0]: Result
                    output_type: Boolean
                    body: <NONE>
                Callable 3: Callable:
                    name: __quantum__rt__array_record_output
                    call_type: OutputRecording
                    input_type:
                        [0]: Integer
                        [1]: Pointer
                    output_type: <VOID>
                    body: <NONE>
                Callable 4: Callable:
                    name: __quantum__rt__tuple_record_output
                    call_type: OutputRecording
                    input_type:
                        [0]: Integer
                        [1]: Pointer
                    output_type: <VOID>
                    body: <NONE>
                Callable 5: Callable:
                    name: __quantum__rt__result_record_output
                    call_type: OutputRecording
                    input_type:
                        [0]: Result
                        [1]: Pointer
                    output_type: <VOID>
                    body: <NONE>
                Callable 6: Callable:
                    name: __quantum__rt__bool_record_output
                    call_type: OutputRecording
                    input_type:
                        [0]: Boolean
                        [1]: Pointer
                    output_type: <VOID>
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Call id(1), args( Qubit(0), Result(0), )
                    Variable(0, Boolean) = Call id(2), args( Result(0), )
                    Variable(1, Boolean) = Icmp Eq, Variable(0, Boolean), Bool(false)
                    Variable(2, Boolean) = Call id(2), args( Result(0), )
                    Variable(3, Boolean) = Store Variable(2, Boolean)
                    Call id(3), args( Integer(2), Pointer, )
                    Call id(4), args( Integer(2), Pointer, )
                    Call id(5), args( Result(0), Pointer, )
                    Call id(6), args( Variable(1, Boolean), Pointer, )
                    Call id(4), args( Integer(2), Pointer, )
                    Call id(5), args( Result(0), Pointer, )
                    Call id(6), args( Variable(3, Boolean), Pointer, )
                    Return
            config: Config:
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations | BackwardsBranching | HigherLevelConstructs | QubitReset)
            num_qubits: 1
            num_results: 1"#]]
    .assert_eq(&program.to_string());
}

#[test]
fn output_recording_for_literal_bool() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Bool {
                true
            }
        }
        "#,
    });

    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 0
                Callable 1: Callable:
                    name: __quantum__rt__bool_record_output
                    call_type: OutputRecording
                    input_type:
                        [0]: Boolean
                        [1]: Pointer
                    output_type: <VOID>
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Call id(1), args( Bool(true), Pointer, )
                    Return
            config: Config:
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations | BackwardsBranching | HigherLevelConstructs | QubitReset)
            num_qubits: 0
            num_results: 0"#]]
    .assert_eq(&program.to_string());
}

#[test]
fn output_recording_for_literal_double() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Double {
                42.1
            }
        }
        "#,
    });

    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 0
                Callable 1: Callable:
                    name: __quantum__rt__double_record_output
                    call_type: OutputRecording
                    input_type:
                        [0]: Double
                        [1]: Pointer
                    output_type: <VOID>
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Call id(1), args( Double(42.1), Pointer, )
                    Return
            config: Config:
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations | BackwardsBranching | HigherLevelConstructs | QubitReset)
            num_qubits: 0
            num_results: 0"#]]
    .assert_eq(&program.to_string());
}

#[test]
fn output_recording_for_literal_int() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                42
            }
        }
        "#,
    });

    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 0
                Callable 1: Callable:
                    name: __quantum__rt__int_record_output
                    call_type: OutputRecording
                    input_type:
                        [0]: Integer
                        [1]: Pointer
                    output_type: <VOID>
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Call id(1), args( Integer(42), Pointer, )
                    Return
            config: Config:
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations | BackwardsBranching | HigherLevelConstructs | QubitReset)
            num_qubits: 0
            num_results: 0"#]]
    .assert_eq(&program.to_string());
}

#[test]
fn output_recording_for_mix_of_literal_and_variable() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : (Result, Bool) {
                use q = Qubit();
                let r = QIR.Intrinsic.__quantum__qis__mresetz__body(q);
                (r, true)
            }
        }
        "#,
    });

    expect![[r#"
        Program:
            entry: 0
            callables:
                Callable 0: Callable:
                    name: main
                    call_type: Regular
                    input_type: <VOID>
                    output_type: Integer
                    body: 0
                Callable 1: Callable:
                    name: __quantum__qis__mresetz__body
                    call_type: Measurement
                    input_type:
                        [0]: Qubit
                        [1]: Result
                    output_type: <VOID>
                    body: <NONE>
                Callable 2: Callable:
                    name: __quantum__rt__tuple_record_output
                    call_type: OutputRecording
                    input_type:
                        [0]: Integer
                        [1]: Pointer
                    output_type: <VOID>
                    body: <NONE>
                Callable 3: Callable:
                    name: __quantum__rt__result_record_output
                    call_type: OutputRecording
                    input_type:
                        [0]: Result
                        [1]: Pointer
                    output_type: <VOID>
                    body: <NONE>
                Callable 4: Callable:
                    name: __quantum__rt__bool_record_output
                    call_type: OutputRecording
                    input_type:
                        [0]: Boolean
                        [1]: Pointer
                    output_type: <VOID>
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Call id(1), args( Qubit(0), Result(0), )
                    Call id(2), args( Integer(2), Pointer, )
                    Call id(3), args( Result(0), Pointer, )
                    Call id(4), args( Bool(true), Pointer, )
                    Return
            config: Config:
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations | BackwardsBranching | HigherLevelConstructs | QubitReset)
            num_qubits: 1
            num_results: 1"#]]
    .assert_eq(&program.to_string());
}

#[test]
fn output_recording_fails_with_result_literal_one() {
    let error = get_partial_evaluation_error(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result {
                One
            }
        }
        "#,
    });

    assert_error(
        &error,
        &expect![
            "OutputResultLiteral(PackageSpan { package: PackageId(2), span: Span { lo: 50, hi: 54 } })"
        ],
    );
}

#[test]
fn output_recording_fails_with_result_literal_zero() {
    let error = get_partial_evaluation_error(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result {
                Zero
            }
        }
        "#,
    });

    assert_error(
        &error,
        &expect![
            "OutputResultLiteral(PackageSpan { package: PackageId(2), span: Span { lo: 50, hi: 54 } })"
        ],
    );
}

#[test]
fn output_recording_fails_with_result_literal_in_array() {
    let error = get_partial_evaluation_error(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result[] {
                use q = Qubit();
                [QIR.Intrinsic.__quantum__qis__mresetz__body(q), Zero]
            }
        }
        "#,
    });

    assert_error(
        &error,
        &expect![
            "OutputResultLiteral(PackageSpan { package: PackageId(2), span: Span { lo: 50, hi: 54 } })"
        ],
    );
}

#[test]
fn output_recording_fails_with_result_literal_in_tuple() {
    let error = get_partial_evaluation_error(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : (Result, Result) {
                use q = Qubit();
                (QIR.Intrinsic.__quantum__qis__mresetz__body(q), Zero)
            }
        }
        "#,
    });

    assert_error(
        &error,
        &expect![
            "OutputResultLiteral(PackageSpan { package: PackageId(2), span: Span { lo: 50, hi: 54 } })"
        ],
    );
}
