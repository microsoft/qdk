// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(clippy::too_many_lines)]

use super::{
    assert_block_instructions, assert_callable, assert_error, get_partial_evaluation_error,
    get_rir_program,
};
use crate::tests::get_rir_program_with_capabilities;
use expect_test::expect;
use indoc::indoc;
use qsc_data_structures::target::Profile;
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
            name: __quantum__rt__initialize
            call_type: Regular
            input_type:
                [0]: Pointer
            output_type: <VOID>
            body: <NONE>"#]]
    .assert_eq(&program.get_callable(CallableId(1)).to_string());
    expect![[r#"
        Callable:
            name: op
            call_type: Regular
            input_type:
                [0]: Qubit
            output_type: <VOID>
            body: <NONE>"#]]
    .assert_eq(&program.get_callable(CallableId(2)).to_string());
    expect![[r#"
        Block:
            Call id(1), args( Pointer, )
            Call id(2), args( Qubit(0), )
            Call id(3), args( Integer(0), Tag(0, 3), )
            Return Integer(0)"#]]
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
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let tuple_callable_id = CallableId(2);
    assert_callable(
        &program,
        tuple_callable_id,
        &expect![[r#"
            Callable:
                name: op
                call_type: Regular
                input_type:
                    [0]: Qubit
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
                Call id(2), args( Qubit(1), )
                Call id(2), args( Qubit(2), )
                Call id(3), args( Integer(0), Tag(0, 3), )
                Return Integer(0)"#]],
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
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let tuple_callable_id = CallableId(2);
    assert_callable(
        &program,
        tuple_callable_id,
        &expect![[r#"
            Callable:
                name: op
                call_type: Regular
                input_type:
                    [0]: Qubit
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
                Call id(2), args( Qubit(0), )
                Call id(3), args( Integer(0), Tag(0, 3), )
                Return Integer(0)"#]],
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
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let tuple_callable_id = CallableId(2);
    assert_callable(
        &program,
        tuple_callable_id,
        &expect![[r#"
            Callable:
                name: op
                call_type: Regular
                input_type:
                    [0]: Qubit
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
                Call id(2), args( Qubit(1), )
                Call id(2), args( Qubit(2), )
                Call id(2), args( Qubit(2), )
                Call id(2), args( Qubit(3), )
                Call id(3), args( Integer(0), Tag(0, 3), )
                Return Integer(0)"#]],
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
                name: __quantum__rt__initialize
                call_type: Regular
                input_type:
                    [0]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let tuple_record_callable_id = CallableId(2);
    assert_callable(
        &program,
        tuple_record_callable_id,
        &expect![[r#"
            Callable:
                name: Op
                call_type: Regular
                input_type:
                    [0]: Qubit
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
                Variable(0, Integer) = Store Integer(1)
                Variable(0, Integer) = Store Integer(2)
                Variable(0, Integer) = Store Integer(3)
                Call id(2), args( Qubit(0), )
                Call id(2), args( Qubit(1), )
                Call id(2), args( Qubit(2), )
                Call id(3), args( Integer(0), Tag(0, 3), )
                Return Integer(0)"#]],
    );
    assert_eq!(program.num_qubits, 3);
    assert_eq!(program.num_results, 0);
}

#[test]
fn qubit_array_length_is_preserved() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                use qs = Qubit[4];
                Length(qs)
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
                Variable(0, Integer) = Store Integer(0)
                Variable(0, Integer) = Store Integer(1)
                Variable(0, Integer) = Store Integer(2)
                Variable(0, Integer) = Store Integer(3)
                Variable(0, Integer) = Store Integer(4)
                Call id(2), args( Integer(4), Tag(0, 3), )
                Return Integer(0)"#]],
    );
    assert_eq!(program.num_qubits, 4);
    assert_eq!(program.num_results, 0);
}

#[test]
fn qubit_array_chunks_can_be_indexed() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            import Std.Arrays.*;

            operation Op(q : Qubit) : Unit { body intrinsic; }

            @EntryPoint()
            operation Main() : Unit {
                use qs = Qubit[4];
                let chunks = Chunks(2, qs);
                Op(chunks[0][0]);
                Op(chunks[1][1]);
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
    let tuple_callable_id = CallableId(2);
    assert_callable(
        &program,
        tuple_callable_id,
        &expect![[r#"
            Callable:
                name: Op
                call_type: Regular
                input_type:
                    [0]: Qubit
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
                Variable(0, Integer) = Store Integer(1)
                Variable(0, Integer) = Store Integer(2)
                Variable(0, Integer) = Store Integer(3)
                Variable(0, Integer) = Store Integer(4)
                Call id(2), args( Qubit(0), )
                Call id(2), args( Qubit(3), )
                Call id(3), args( Integer(0), Tag(0, 3), )
                Return Integer(0)"#]],
    );
    assert_eq!(program.num_qubits, 4);
    assert_eq!(program.num_results, 0);
}

#[test]
fn qubit_array_chunk_count_is_preserved() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            import Std.Arrays.*;

            @EntryPoint()
            operation Main() : Int {
                use qs = Qubit[4];
                let chunks = Chunks(2, qs);
                Length(chunks)
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
                Variable(0, Integer) = Store Integer(0)
                Variable(0, Integer) = Store Integer(1)
                Variable(0, Integer) = Store Integer(2)
                Variable(0, Integer) = Store Integer(3)
                Variable(0, Integer) = Store Integer(4)
                Call id(2), args( Integer(2), Tag(0, 3), )
                Return Integer(0)"#]],
    );
    assert_eq!(program.num_qubits, 4);
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

#[test]
fn qubit_relabel_uses_expected_ids_in_adaptive_global_arrays() {
    let program = get_rir_program_with_capabilities(
        indoc! {
            r#"
        operation Main() : Unit {
            use qs = Qubit[2];
            use aux = Qubit[2];
            for q in aux {
                H(q);
            }
            Relabel(qs+aux, aux+qs);
            for q in qs {
                H(q);
            }
        }
        "#,
        },
        Profile::Adaptive.into(),
    );

    // Since the qubits are relabeled, both loops should iterate over the same qubit IDs and only
    // one array literal of ids should be included in the program.
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
                    name: __quantum__rt__initialize
                    call_type: Regular
                    input_type:
                        [0]: Pointer
                    output_type: <VOID>
                    body: <NONE>
                Callable 2: Callable:
                    name: H
                    call_type: Regular
                    input_type:
                        [0]: Qubit
                    input_vars:
                        [0]: 6
                    output_type: <VOID>
                    body: 4
                Callable 3: Callable:
                    name: __quantum__qis__h__body
                    call_type: Regular
                    input_type:
                        [0]: Qubit
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
            blocks:
                Block 0: Block:
                    Call id(1), args( Pointer, )
                    Variable(0, Integer) = Store Integer(0)
                    Variable(0, Integer) = Store Integer(1)
                    Variable(0, Integer) = Store Integer(2)
                    Variable(1, Integer) = Store Integer(0)
                    Variable(1, Integer) = Store Integer(1)
                    Variable(1, Integer) = Store Integer(2)
                    Variable(2, Integer) = Store Integer(0)
                    Jump(1)
                Block 1: Block:
                    Variable(3, Boolean) = Icmp Slt, Variable(2, Integer), Integer(2)
                    Branch Variable(3, Boolean), 3, 2
                Block 2: Block:
                    Variable(8, Integer) = Store Integer(0)
                    Jump(5)
                Block 3: Block:
                    Variable(4, Qubit) = Index Array(0), Variable(2, Integer)
                    Variable(5, Qubit) = Store Variable(4, Qubit)
                    Call id(2), args( Variable(5, Qubit), )
                    Variable(7, Integer) = Add Variable(2, Integer), Integer(1)
                    Variable(2, Integer) = Store Variable(7, Integer)
                    Jump(1)
                Block 4: Block:
                    Call id(3), args( Variable(6, Qubit), )
                    Return
                Block 5: Block:
                    Variable(9, Boolean) = Icmp Slt, Variable(8, Integer), Integer(2)
                    Branch Variable(9, Boolean), 7, 6
                Block 6: Block:
                    Call id(4), args( Integer(0), Tag(0, 3), )
                    Return Integer(0)
                Block 7: Block:
                    Variable(10, Qubit) = Index Array(0), Variable(8, Integer)
                    Variable(11, Qubit) = Store Variable(10, Qubit)
                    Call id(2), args( Variable(11, Qubit), )
                    Variable(12, Integer) = Add Variable(8, Integer), Integer(1)
                    Variable(8, Integer) = Store Variable(12, Integer)
                    Jump(5)
            config: Config:
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations | BackwardsBranching | StaticSizedArrays | CallSupport)
            num_qubits: 4
            num_results: 0
            tags:
                [0]: 0_t
            array_literals:
                [0]: [Qubit(2), Qubit(3)]
    "#]].assert_eq(&program.to_string());
}
