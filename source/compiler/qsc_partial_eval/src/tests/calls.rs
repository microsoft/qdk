// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(
    clippy::needless_raw_string_hashes,
    clippy::similar_names,
    clippy::too_many_lines
)]

use super::{
    assert_block_instructions, assert_blocks, assert_callable, assert_error,
    get_partial_evaluation_error_with_capabilities, get_rir_program,
    get_rir_program_with_capabilities,
};
use expect_test::expect;
use indoc::indoc;
use qsc::TargetCapabilityFlags;
use qsc_rir::rir::{BlockId, CallableId};

#[test]
fn call_to_single_qubit_unitary_with_two_calls_to_the_same_intrinsic() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            operation Op(q : Qubit) : Unit { body intrinsic; }
            operation OpSquared(q : Qubit) : Unit {
                Op(q);
                Op(q);
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                OpSquared(q);
            }
        }
    "#});
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
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Qubit(0), ) !dbg package_id=2 span=[125-127] scope=0 scope_package_id=2 scope_span=[115-152] callable=OpSquared dbg_location=3
                Call id(1), args( Qubit(0), ) !dbg package_id=2 span=[140-142] scope=0 scope_package_id=2 scope_span=[115-152] callable=OpSquared dbg_location=4
                Call id(2), args( Integer(0), Pointer, ) !dbg package_id=2 span=[185-189]
                Return !dbg package_id=2 span=[185-189]"#]],
    );
    expect![[r#"
        [
            DbgLocation {
                location: MetadataPackageSpan {
                    package: 2,
                    span: Span {
                        lo: 0,
                        hi: 0,
                    },
                },
                scope: 0,
                inlined_at: None,
            },
            DbgLocation {
                location: MetadataPackageSpan {
                    package: 2,
                    span: Span {
                        lo: 209,
                        hi: 225,
                    },
                },
                scope: 1,
                inlined_at: Some(
                    0,
                ),
            },
            DbgLocation {
                location: MetadataPackageSpan {
                    package: 2,
                    span: Span {
                        lo: 234,
                        hi: 246,
                    },
                },
                scope: 1,
                inlined_at: Some(
                    0,
                ),
            },
            DbgLocation {
                location: MetadataPackageSpan {
                    package: 2,
                    span: Span {
                        lo: 125,
                        hi: 130,
                    },
                },
                scope: 2,
                inlined_at: Some(
                    2,
                ),
            },
            DbgLocation {
                location: MetadataPackageSpan {
                    package: 2,
                    span: Span {
                        lo: 140,
                        hi: 145,
                    },
                },
                scope: 2,
                inlined_at: Some(
                    2,
                ),
            },
            DbgLocation {
                location: MetadataPackageSpan {
                    package: 2,
                    span: Span {
                        lo: 209,
                        hi: 225,
                    },
                },
                scope: 1,
                inlined_at: Some(
                    0,
                ),
            },
        ]
    "#]]
    .assert_debug_eq(&program.dbg_locations);

    expect![[r#"
        [
            SubProgram {
                name: "entry",
                location: MetadataPackageSpan {
                    package: 0,
                    span: Span {
                        lo: 0,
                        hi: 0,
                    },
                },
            },
            SubProgram {
                name: "Main",
                location: MetadataPackageSpan {
                    package: 2,
                    span: Span {
                        lo: 175,
                        hi: 253,
                    },
                },
            },
            SubProgram {
                name: "OpSquared",
                location: MetadataPackageSpan {
                    package: 2,
                    span: Span {
                        lo: 77,
                        hi: 152,
                    },
                },
            },
        ]
    "#]].assert_debug_eq(&program.dbg_metadata_scopes);
}

#[test]
fn call_to_single_qubit_unitary_with_calls_to_different_intrinsics() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            operation OpA(q : Qubit) : Unit { body intrinsic; }
            operation OpB(q : Qubit) : Unit { body intrinsic; }
            operation Combined(q : Qubit) : Unit {
                OpA(q);
                OpB(q);
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                Combined(q);
            }
        }
    "#});
    let op_a_callable_id = CallableId(1);
    let op_b_callable_id = CallableId(2);
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
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Qubit(0), ) !dbg package_id=2 span=[181-184] scope=0 scope_package_id=2 scope_span=[171-210] callable=Combined dbg_location=3
                Call id(2), args( Qubit(0), ) !dbg package_id=2 span=[197-200] scope=0 scope_package_id=2 scope_span=[171-210] callable=Combined dbg_location=4
                Call id(3), args( Integer(0), Pointer, ) !dbg package_id=2 span=[243-247]
                Return !dbg package_id=2 span=[243-247]"#]],
    );
}

#[test]
fn call_to_two_qubit_unitary() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            operation Op(q0 : Qubit, q1 : Qubit) : Unit { body intrinsic; }
            operation ApplyOpCombinations(q0 : Qubit, q1 : Qubit) : Unit {
                Op(q0, q1);
                Op(q1, q0);
            }
            @EntryPoint()
            operation Main() : Unit {
                use (q0, q1) = (Qubit(), Qubit());
                ApplyOpCombinations(q0, q1);
            }
        }
    "#});
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
                [1]: Qubit
            output_type: <VOID>
            body: <NONE>"#]],
    );
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Qubit(0), Qubit(1), ) !dbg package_id=2 span=[161-163] scope=0 scope_package_id=2 scope_span=[151-198] callable=ApplyOpCombinations dbg_location=4
                Call id(1), args( Qubit(1), Qubit(0), ) !dbg package_id=2 span=[181-183] scope=0 scope_package_id=2 scope_span=[151-198] callable=ApplyOpCombinations dbg_location=5
                Call id(2), args( Integer(0), Pointer, ) !dbg package_id=2 span=[231-235]
                Return !dbg package_id=2 span=[231-235]"#]],
    );
}

#[test]
fn call_to_unitary_that_receives_double_and_qubit() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            operation DoubleFirst(d : Double, q : Qubit) : Unit { body intrinsic; }
            operation QubitFirst(q : Qubit, d : Double) : Unit { body intrinsic; }
            operation Op(d : Double, q : Qubit) : Unit {
                DoubleFirst(d, q);
                QubitFirst(q, d);
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                Op(1.0, q);
            }
        }
    "#});
    let double_first_callable_id = CallableId(1);
    let qubit_first_callable_id = CallableId(2);
    assert_callable(
        &program,
        double_first_callable_id,
        &expect![[r#"
        Callable:
            name: DoubleFirst
            call_type: Regular
            input_type:
                [0]: Double
                [1]: Qubit
            output_type: <VOID>
            body: <NONE>"#]],
    );
    assert_callable(
        &program,
        qubit_first_callable_id,
        &expect![[r#"
        Callable:
            name: QubitFirst
            call_type: Regular
            input_type:
                [0]: Qubit
                [1]: Double
            output_type: <VOID>
            body: <NONE>"#]],
    );
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Double(1), Qubit(0), ) !dbg package_id=2 span=[226-237] scope=0 scope_package_id=2 scope_span=[216-276] callable=Op dbg_location=3
                Call id(2), args( Qubit(0), Double(1), ) !dbg package_id=2 span=[253-263] scope=0 scope_package_id=2 scope_span=[216-276] callable=Op dbg_location=4
                Call id(3), args( Integer(0), Pointer, ) !dbg package_id=2 span=[309-313]
                Return !dbg package_id=2 span=[309-313]"#]],
    );
}

#[test]
fn calls_to_unitary_that_conditionally_calls_intrinsic_with_classical_bool() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            operation OpA(q : Qubit) : Unit { body intrinsic; }
            operation OpB(q : Qubit) : Unit { body intrinsic; }
            operation ConditionallyCallOp(b : Bool, q : Qubit) : Unit {
                if b {
                    OpA(q);
                } else {
                    OpB(q);
                }
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                ConditionallyCallOp(true, q);
                ConditionallyCallOp(false, q);
            }
        }
    "#});
    let op_a_callable_id = CallableId(1);
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
    let op_b_callable_id = CallableId(2);
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
    let output_recording_callable_id = CallableId(3);
    assert_callable(
        &program,
        output_recording_callable_id,
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
                Call id(1), args( Qubit(0), ) !dbg package_id=2 span=[221-224] scope=1 scope_package_id=2 scope_span=[207-238] callable=ConditionallyCallOp dbg_location=3
                Call id(2), args( Qubit(0), ) !dbg package_id=2 span=[258-261] scope=2 scope_package_id=2 scope_span=[244-275] callable=ConditionallyCallOp dbg_location=5
                Call id(3), args( Integer(0), Pointer, ) !dbg package_id=2 span=[314-318]
                Return !dbg package_id=2 span=[314-318]"#]],
    );
}

#[test]
fn calls_to_unitary_that_conditionally_calls_intrinsic_with_dynamic_bool() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            operation OpA(q : Qubit) : Unit { body intrinsic; }
            operation OpB(q : Qubit) : Unit { body intrinsic; }
            operation ConditionallyCallOp(b : Bool, q : Qubit) : Unit {
                if b {
                    OpA(q);
                } else {
                    OpB(q);
                }
            }
            @EntryPoint()
            operation Main() : Unit {
                use (q0, q1) = (Qubit(), Qubit());
                let r = QIR.Intrinsic.__quantum__qis__m__body(q0);
                ConditionallyCallOp(r == One, q1);
            }
        }
    "#});
    let measure_callable_id = CallableId(1);
    assert_callable(
        &program,
        measure_callable_id,
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
    let output_recording_callable_id = CallableId(5);
    assert_callable(
        &program,
        output_recording_callable_id,
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[0-0] scope=3 scope_package_id=2 scope_span=[328-480] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[460-468] scope=3 scope_package_id=2 scope_span=[328-480] callable=Main
                Variable(1, Boolean) = Store Variable(0, Boolean) !dbg package_id=2 span=[460-468] scope=3 scope_package_id=2 scope_span=[328-480] callable=Main
                Branch Variable(1, Boolean), 2, 3 !dbg package_id=2 span=[205-206] scope=0 scope_package_id=2 scope_span=[192-281] callable=ConditionallyCallOp
            Block 1:Block:
                Call id(5), args( Integer(0), Pointer, ) !dbg package_id=2 span=[314-318]
                Return !dbg package_id=2 span=[314-318]
            Block 2:Block:
                Call id(3), args( Qubit(1), ) !dbg package_id=2 span=[221-224] scope=1 scope_package_id=2 scope_span=[207-238] callable=ConditionallyCallOp dbg_location=5
                Jump(1) !dbg package_id=2 span=[207-238] scope=0 scope_package_id=2 scope_span=[192-281] callable=ConditionallyCallOp
            Block 3:Block:
                Call id(4), args( Qubit(1), ) !dbg package_id=2 span=[258-261] scope=2 scope_package_id=2 scope_span=[244-275] callable=ConditionallyCallOp dbg_location=6
                Jump(1) !dbg package_id=2 span=[239-275] scope=0 scope_package_id=2 scope_span=[192-281] callable=ConditionallyCallOp"#]],
    );
}

#[test]
fn call_to_unitary_rotation_unitary_with_computation() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            operation Rotation(d : Double, q : Qubit) : Unit { body intrinsic; }
            operation RotationWithComputation(d : Double, q : Qubit) : Unit {
                Rotation(2.0 * d, q);
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                RotationWithComputation(2.0, q);
                RotationWithComputation(3.0, q);
            }
        }
    "#});
    let rotation_callable_id = CallableId(1);
    assert_callable(
        &program,
        rotation_callable_id,
        &expect![[r#"
        Callable:
            name: Rotation
            call_type: Regular
            input_type:
                [0]: Double
                [1]: Qubit
            output_type: <VOID>
            body: <NONE>"#]],
    );
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Double(4), Qubit(0), ) !dbg package_id=2 span=[169-177] scope=0 scope_package_id=2 scope_span=[159-196] callable=RotationWithComputation dbg_location=3
                Call id(1), args( Double(6), Qubit(0), ) !dbg package_id=2 span=[169-177] scope=0 scope_package_id=2 scope_span=[159-196] callable=RotationWithComputation dbg_location=5
                Call id(2), args( Integer(0), Pointer, ) !dbg package_id=2 span=[229-233]
                Return !dbg package_id=2 span=[229-233]"#]],
    );
}

#[test]
fn call_to_operation_that_returns_measurement_result() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            operation Op(q : Qubit) : Result {
                QIR.Intrinsic.__quantum__qis__m__body(q)
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                Op(q)
            }
        }
    "#});
    let measure_callable_id = CallableId(1);
    assert_callable(
        &program,
        measure_callable_id,
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
    let output_recording_callable_id = CallableId(2);
    assert_callable(
        &program,
        output_recording_callable_id,
        &expect![[r#"
        Callable:
            name: __quantum__rt__result_record_output
            call_type: OutputRecording
            input_type:
                [0]: Result
                [1]: Pointer
            output_type: <VOID>
            body: <NONE>"#]],
    );
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[195-200] scope=0 scope_package_id=2 scope_span=[55-111] callable=Op
                Call id(2), args( Result(0), Pointer, ) !dbg package_id=2 span=[144-148]
                Return !dbg package_id=2 span=[144-148]"#]],
    );
}

#[test]
fn call_to_operation_that_returns_dynamic_bool() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            operation Op(q : Qubit) : Bool {
                let r = QIR.Intrinsic.__quantum__qis__m__body(q);
                r == Zero
            }
            @EntryPoint()
            operation Main() : Bool {
                use q = Qubit();
                Op(q)
            }
        }
    "#});
    let measure_callable_id = CallableId(1);
    assert_callable(
        &program,
        measure_callable_id,
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
    let output_recording_callable_id = CallableId(3);
    assert_callable(
        &program,
        output_recording_callable_id,
        &expect![[r#"
            Callable:
                name: __quantum__rt__bool_record_output
                call_type: OutputRecording
                input_type:
                    [0]: Boolean
                    [1]: Pointer
                output_type: <VOID>
                body: <NONE>"#]],
    );
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[218-223] scope=0 scope_package_id=2 scope_span=[53-136] callable=Op
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[121-130] scope=0 scope_package_id=2 scope_span=[53-136] callable=Op
                Variable(1, Boolean) = Icmp Eq, Variable(0, Boolean), Bool(false) !dbg package_id=2 span=[121-130] scope=0 scope_package_id=2 scope_span=[53-136] callable=Op
                Variable(2, Boolean) = Store Variable(1, Boolean) !dbg package_id=2 span=[218-223] scope=1 scope_package_id=2 scope_span=[183-229] callable=Main
                Call id(3), args( Variable(2, Boolean), Pointer, ) !dbg package_id=2 span=[169-173]
                Return !dbg package_id=2 span=[169-173]"#]],
    );
}

#[test]
fn call_to_boolean_function_using_result_literal_as_argument_yields_constant() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            operation Op(q : Qubit) : Unit { body intrinsic; }
            function ResultAsBool(r : Result) : Bool {
                r == One
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                // Only one call to `Op` should be generated.
                if ResultAsBool(Zero) {
                    Op(q);
                }
                if ResultAsBool(One) {
                    Op(q);
                }
            }
        }
    "#});
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
    let output_recording_callable_id = CallableId(2);
    assert_callable(
        &program,
        output_recording_callable_id,
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
                Call id(1), args( Qubit(0), ) !dbg package_id=2 span=[374-376] scope=3 scope_package_id=2 scope_span=[360-390] callable=Main dbg_location=2
                Call id(2), args( Integer(0), Pointer, ) !dbg package_id=2 span=[175-179]
                Return !dbg package_id=2 span=[175-179]"#]],
    );
}

#[test]
fn call_to_boolean_function_using_dynamic_result_as_argument_generates_branches() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            open QIR.Intrinsic;
            operation Op(q : Qubit) : Unit { body intrinsic; }
            function ResultAsBool(r : Result) : Bool {
                r == One
            }
            @EntryPoint()
            operation Main() : Unit {
                use (q0, q1) = (Qubit(), Qubit());
                let r = __quantum__qis__m__body(q0);
                // Only one call to `Op` should be generated.
                if ResultAsBool(r) {
                    Op(q1);
                }
            }
        }
    "#});
    let measure_callable_id = CallableId(1);
    assert_callable(
        &program,
        measure_callable_id,
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
    let op_callable_id = CallableId(3);
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
    let output_recording_callable_id = CallableId(4);
    assert_callable(
        &program,
        output_recording_callable_id,
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
                Call id(1), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[0-0] scope=1 scope_package_id=2 scope_span=[213-421] callable=Main
                Variable(0, Boolean) = Call id(2), args( Result(0), ) !dbg package_id=2 span=[152-160] scope=0 scope_package_id=2 scope_span=[142-166] callable=ResultAsBool
                Variable(1, Boolean) = Store Variable(0, Boolean) !dbg package_id=2 span=[152-160] scope=0 scope_package_id=2 scope_span=[142-166] callable=ResultAsBool
                Branch Variable(1, Boolean), 2, 1 !dbg package_id=2 span=[368-383] scope=1 scope_package_id=2 scope_span=[213-421] callable=Main
            Block 1:Block:
                Call id(4), args( Integer(0), Pointer, ) !dbg package_id=2 span=[199-203]
                Return !dbg package_id=2 span=[199-203]
            Block 2:Block:
                Call id(3), args( Qubit(1), ) !dbg package_id=2 span=[398-400] scope=2 scope_package_id=2 scope_span=[384-415] callable=Main dbg_location=5
                Jump(1) !dbg package_id=2 span=[384-415] scope=1 scope_package_id=2 scope_span=[213-421] callable=Main"#]],
    );
}

#[test]
fn call_to_unitary_operation_with_one_qubit_argument_using_one_control_qubit() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            operation IntrinsicA(q : Qubit) : Unit { body intrinsic; }
            operation IntrinsicB(control: Qubit, target : Qubit) : Unit { body intrinsic; }
            operation Op(q : Qubit) : Unit is Ctl {
                body ... {
                    IntrinsicA(q);
                }
                controlled (ctls, ...) {
                    IntrinsicB(ctls[0], q);
                }
            }
            @EntryPoint()
            operation Main() : Unit {
                use (ctl, target) = (Qubit(), Qubit());
                Op(target);
                Controlled Op([ctl], target);
            }
        }
    "#});
    let intrinsic_a_callable_id = CallableId(1);
    assert_callable(
        &program,
        intrinsic_a_callable_id,
        &expect![[r#"
        Callable:
            name: IntrinsicA
            call_type: Regular
            input_type:
                [0]: Qubit
            output_type: <VOID>
            body: <NONE>"#]],
    );
    let intrinsic_b_callable_id = CallableId(2);
    assert_callable(
        &program,
        intrinsic_b_callable_id,
        &expect![[r#"
        Callable:
            name: IntrinsicB
            call_type: Regular
            input_type:
                [0]: Qubit
                [1]: Qubit
            output_type: <VOID>
            body: <NONE>"#]],
    );
    let output_recording_callable_id = CallableId(3);
    assert_callable(
        &program,
        output_recording_callable_id,
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
                Call id(1), args( Qubit(1), ) !dbg package_id=2 span=[240-250] scope=0 scope_package_id=2 scope_span=[226-264] callable=Op dbg_location=4
                Call id(2), args( Qubit(0), Qubit(1), ) !dbg package_id=2 span=[310-320] scope=1 scope_package_id=2 scope_span=[296-343] callable=Op dbg_location=6
                Call id(3), args( Integer(0), Pointer, ) !dbg package_id=2 span=[382-386]
                Return !dbg package_id=2 span=[382-386]"#]],
    );
}

#[test]
fn call_to_unitary_operation_with_one_qubit_argument_using_mutiple_control_qubits() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            operation IntrinsicA(q : Qubit) : Unit { body intrinsic; }
            operation IntrinsicB(control0: Qubit, control1: Qubit, target : Qubit) : Unit { body intrinsic; }
            operation Op(q : Qubit) : Unit is Ctl {
                body ... {
                    IntrinsicA(q);
                }
                controlled (ctls, ...) {
                    IntrinsicB(ctls[0], ctls[1], q);
                }
            }
            @EntryPoint()
            operation Main() : Unit {
                use (ctl0, ctl1, target) = (Qubit(), Qubit(), Qubit());
                Op(target);
                Controlled Op([ctl0, ctl1], target);
            }
        }
    "#});
    let intrinsic_a_callable_id = CallableId(1);
    assert_callable(
        &program,
        intrinsic_a_callable_id,
        &expect![[r#"
        Callable:
            name: IntrinsicA
            call_type: Regular
            input_type:
                [0]: Qubit
            output_type: <VOID>
            body: <NONE>"#]],
    );
    let intrinsic_b_callable_id = CallableId(2);
    assert_callable(
        &program,
        intrinsic_b_callable_id,
        &expect![[r#"
            Callable:
                name: IntrinsicB
                call_type: Regular
                input_type:
                    [0]: Qubit
                    [1]: Qubit
                    [2]: Qubit
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let output_recording_callable_id = CallableId(3);
    assert_callable(
        &program,
        output_recording_callable_id,
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
                Call id(1), args( Qubit(2), ) !dbg package_id=2 span=[258-268] scope=0 scope_package_id=2 scope_span=[244-282] callable=Op dbg_location=5
                Call id(2), args( Qubit(0), Qubit(1), Qubit(2), ) !dbg package_id=2 span=[328-338] scope=1 scope_package_id=2 scope_span=[314-370] callable=Op dbg_location=7
                Call id(3), args( Integer(0), Pointer, ) !dbg package_id=2 span=[409-413]
                Return !dbg package_id=2 span=[409-413]"#]],
    );
}

#[test]
fn call_to_unitary_operation_with_two_qubit_arguments_using_one_control_qubit() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            operation IntrinsicA(q0 : Qubit, q1 : Qubit) : Unit { body intrinsic; }
            operation IntrinsicB(control: Qubit, target0 : Qubit, target1 : Qubit) : Unit { body intrinsic; }
            operation Op(q0 : Qubit, q1: Qubit) : Unit is Ctl {
                body ... {
                    IntrinsicA(q0, q1);
                }
                controlled (ctls, ...) {
                    IntrinsicB(ctls[0], q0, q1);
                }
            }
            @EntryPoint()
            operation Main() : Unit {
                use (ctl, target0, target1) = (Qubit(), Qubit(), Qubit());
                Op(target0, target1);
                Controlled Op([ctl], (target0, target1));
            }
        }
    "#});
    let intrinsic_a_callable_id = CallableId(1);
    assert_callable(
        &program,
        intrinsic_a_callable_id,
        &expect![[r#"
            Callable:
                name: IntrinsicA
                call_type: Regular
                input_type:
                    [0]: Qubit
                    [1]: Qubit
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let intrinsic_b_callable_id = CallableId(2);
    assert_callable(
        &program,
        intrinsic_b_callable_id,
        &expect![[r#"
            Callable:
                name: IntrinsicB
                call_type: Regular
                input_type:
                    [0]: Qubit
                    [1]: Qubit
                    [2]: Qubit
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let output_recording_callable_id = CallableId(3);
    assert_callable(
        &program,
        output_recording_callable_id,
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
                Call id(1), args( Qubit(1), Qubit(2), ) !dbg package_id=2 span=[283-293] scope=0 scope_package_id=2 scope_span=[269-312] callable=Op dbg_location=5
                Call id(2), args( Qubit(0), Qubit(1), Qubit(2), ) !dbg package_id=2 span=[358-368] scope=1 scope_package_id=2 scope_span=[344-396] callable=Op dbg_location=7
                Call id(3), args( Integer(0), Pointer, ) !dbg package_id=2 span=[435-439]
                Return !dbg package_id=2 span=[435-439]"#]],
    );
}

#[test]
fn call_to_unitary_operation_using_multiple_controlled_functors() {
    let program = get_rir_program(indoc! {r#"
    namespace Test {
        operation IntrinsicA1(q : Qubit) : Unit { body intrinsic; }
        operation IntrinsicA2(q : Qubit) : Unit { body intrinsic; }
        operation IntrinsicB(control: Qubit, target : Qubit) : Unit { body intrinsic; }
        operation IntrinsicC(control0: Qubit, control1: Qubit, target : Qubit) : Unit { body intrinsic; }
        operation Op(q : Qubit) : Unit is Ctl {
            body ... {
                IntrinsicA1(q);
            }
            controlled (ctls, ...) {
                let len = Length(ctls);
                if len == 1 {
                    IntrinsicB(ctls[0], q);
                } elif len == 2 {
                    IntrinsicC(ctls[0], ctls[1], q);
                } else {
                    IntrinsicA2(ctls[2]);
                }
            }
        }
        @EntryPoint()
        operation Main() : Unit {
            use (target, ctl1, ctl2, ctl3,) = (Qubit(), Qubit(), Qubit(), Qubit());
            Op(target);
            Controlled Op([ctl1], target);
            Controlled Controlled Op([ctl1], ([ctl2], target));
            Controlled Controlled Controlled Op([ctl1], ([ctl2], ([ctl3], target)));
        }
    }
    "#});
    let intrinsic_a1_callable_id = CallableId(1);
    assert_callable(
        &program,
        intrinsic_a1_callable_id,
        &expect![[r#"
            Callable:
                name: IntrinsicA1
                call_type: Regular
                input_type:
                    [0]: Qubit
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let intrinsic_b_callable_id = CallableId(2);
    assert_callable(
        &program,
        intrinsic_b_callable_id,
        &expect![[r#"
            Callable:
                name: IntrinsicB
                call_type: Regular
                input_type:
                    [0]: Qubit
                    [1]: Qubit
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let intrinsic_c_callable_id = CallableId(3);
    assert_callable(
        &program,
        intrinsic_c_callable_id,
        &expect![[r#"
            Callable:
                name: IntrinsicC
                call_type: Regular
                input_type:
                    [0]: Qubit
                    [1]: Qubit
                    [2]: Qubit
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let intrinsic_a2_callable_id = CallableId(4);
    assert_callable(
        &program,
        intrinsic_a2_callable_id,
        &expect![[r#"
            Callable:
                name: IntrinsicA2
                call_type: Regular
                input_type:
                    [0]: Qubit
                output_type: <VOID>
                body: <NONE>"#]],
    );
    let output_recording_callable_id = CallableId(5);
    assert_callable(
        &program,
        output_recording_callable_id,
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
                Call id(1), args( Qubit(0), ) !dbg package_id=2 span=[407-418] scope=0 scope_package_id=2 scope_span=[393-432] callable=Op dbg_location=6
                Call id(2), args( Qubit(1), Qubit(0), ) !dbg package_id=2 span=[544-554] scope=2 scope_package_id=2 scope_span=[526-581] callable=Op dbg_location=8
                Call id(3), args( Qubit(1), Qubit(2), Qubit(0), ) !dbg package_id=2 span=[614-624] scope=3 scope_package_id=2 scope_span=[596-660] callable=Op dbg_location=10
                Call id(4), args( Qubit(3), ) !dbg package_id=2 span=[684-695] scope=4 scope_package_id=2 scope_span=[666-719] callable=Op dbg_location=12
                Call id(5), args( Integer(0), Pointer, ) !dbg package_id=2 span=[768-772]
                Return !dbg package_id=2 span=[768-772]"#]],
    );
}

#[test]
fn call_to_closue_with_no_bound_locals() {
    let program = get_rir_program(indoc! {"
        namespace Test {
            operation Op() : (Qubit => Unit) {
                X(_)
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                (Op())(q);
            }
        }
    "});
    assert_callable(
        &program,
        CallableId(1),
        &expect![[r#"
        Callable:
            name: __quantum__qis__x__body
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
                Call id(1), args( Qubit(0), ) !dbg package_id=1 span=[133086-133109] scope=3 scope_package_id=2 scope_span=[65-69] callable=<lambda> dbg_location=4
                Call id(2), args( Integer(0), Pointer, ) !dbg package_id=2 span=[108-112]
                Return !dbg package_id=2 span=[108-112]"#]],
    );
}

#[test]
fn call_to_closue_with_one_bound_local() {
    let program = get_rir_program(indoc! {"
        namespace Test {
            operation Op() : (Qubit => Unit) {
                Rx(1.0, _)
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                (Op())(q);
            }
        }
    "});
    assert_callable(
        &program,
        CallableId(1),
        &expect![[r#"
            Callable:
                name: __quantum__qis__rx__body
                call_type: Regular
                input_type:
                    [0]: Double
                    [1]: Qubit
                output_type: <VOID>
                body: <NONE>"#]],
    );
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Double(1), Qubit(0), ) !dbg package_id=1 span=[118832-118856] scope=3 scope_package_id=2 scope_span=[65-75] callable=<lambda> dbg_location=4
                Call id(2), args( Integer(0), Pointer, ) !dbg package_id=2 span=[114-118]
                Return !dbg package_id=2 span=[114-118]"#]],
    );
}

#[test]
fn call_to_closue_with_two_bound_locals() {
    let program = get_rir_program(indoc! {"
        namespace Test {
            operation Op() : (Qubit => Unit) {
                R(PauliX, 1.0, _)
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                (Op())(q);
            }
        }
    "});
    assert_callable(
        &program,
        CallableId(1),
        &expect![[r#"
            Callable:
                name: __quantum__qis__rx__body
                call_type: Regular
                input_type:
                    [0]: Double
                    [1]: Qubit
                output_type: <VOID>
                body: <NONE>"#]],
    );
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Double(1), Qubit(0), ) !dbg package_id=1 span=[118832-118856] scope=3 scope_package_id=2 scope_span=[65-82] callable=<lambda> dbg_location=5
                Call id(2), args( Integer(0), Pointer, ) !dbg package_id=2 span=[121-125]
                Return !dbg package_id=2 span=[121-125]"#]],
    );
}

#[test]
fn call_to_closue_with_one_bound_local_two_unbound() {
    let program = get_rir_program(indoc! {"
        namespace Test {
            operation Op() : ((Double, Qubit) => Unit) {
                R(PauliX, _, _)
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                (Op())(1.0, q);
            }
        }
    "});
    assert_callable(
        &program,
        CallableId(1),
        &expect![[r#"
            Callable:
                name: __quantum__qis__rx__body
                call_type: Regular
                input_type:
                    [0]: Double
                    [1]: Qubit
                output_type: <VOID>
                body: <NONE>"#]],
    );
    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Double(1), Qubit(0), ) !dbg package_id=1 span=[118832-118856] scope=3 scope_package_id=2 scope_span=[75-90] callable=<lambda> dbg_location=5
                Call id(2), args( Integer(0), Pointer, ) !dbg package_id=2 span=[129-133]
                Return !dbg package_id=2 span=[129-133]"#]],
    );
}

#[test]
fn call_to_unresolved_callee_with_classical_arg_allowed() {
    let program = get_rir_program_with_capabilities(
        indoc! {"
        namespace Test {
            import Std.Convert.*;
            operation Op(i : Int, q : Qubit) : Unit {
                Rx(IntAsDouble(i), q);
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                let f = [Op][0];
                f(1, q);
            }
        }"},
        TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::IntegerComputations,
    );

    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Double(1), Qubit(0), ) !dbg package_id=1 span=[118832-118856] scope=0 scope_package_id=2 scope_span=[88-126] callable=Op dbg_location=4
                Call id(2), args( Integer(0), Pointer, ) !dbg package_id=2 span=[159-163]
                Return !dbg package_id=2 span=[159-163]"#]],
    );
}

#[test]
fn call_to_unresolved_callee_with_dynamic_arg_fails() {
    let error = get_partial_evaluation_error_with_capabilities(
        indoc! {"
        namespace Test {
            import Std.Convert.*;
            operation Op(i : Int, q : Qubit) : Unit {
                Rx(IntAsDouble(i), q);
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                let i = if MResetZ(q) == One { 1 } else { 0 };
                let f = [Op][0];
                f(i, q);
            }
        }"},
        TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::IntegerComputations,
    );

    assert_error(
        &error,
        &expect!["CapabilityError(UseOfDynamicDouble(Span { lo: 288, hi: 295 }))"],
    );
}

#[test]
fn call_to_unresolved_callee_producing_dynamic_value_fails() {
    let error = get_partial_evaluation_error_with_capabilities(
        indoc! {"
        namespace Test {
            import Std.Convert.*;
            operation Op(i : Int, q : Qubit) : Int {
                X(q);
                i
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                let i = if MResetZ(q) == One { 1 } else { 0 };
                let f = [Op][0];
                let _ = f(i, q);
            }
        }"},
        TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::IntegerComputations,
    );

    assert_error(
        &error,
        &expect![
            "UnexpectedDynamicValue(PackageSpan { package: PackageId(2), span: Span { lo: 288, hi: 295 } })"
        ],
    );
}

#[test]
fn call_to_unresolved_callee_via_closure_with_dynamic_arg_fails() {
    let error = get_partial_evaluation_error_with_capabilities(
        indoc! {"
        namespace Test {
            import Std.Convert.*;
            operation Op() : (Int, Qubit) => Unit {
                (i, q) => Rx(IntAsDouble(i), q)
            }
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                let i = if MResetZ(q) == One { 1 } else { 0 };
                let f = Op();
                f(i, q);
            }
        }"},
        TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::IntegerComputations,
    );

    assert_error(
        &error,
        &expect!["CapabilityError(UseOfDynamicDouble(Span { lo: 292, hi: 299 }))"],
    );
}

#[test]
fn call_to_unresolved_callee_with_static_arg_and_entry_return_value_succeeds() {
    let program = get_rir_program_with_capabilities(
        indoc! {"
        namespace Test {
            import Std.Convert.*;
            operation Op(i : Int, q : Qubit) : Unit {
                Rx(IntAsDouble(i), q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                let f = [Op][0];
                f(1, q);
                MResetZ(q)
            }
        }"},
        TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::IntegerComputations,
    );

    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Double(1), Qubit(0), ) !dbg package_id=1 span=[118832-118856] scope=0 scope_package_id=2 scope_span=[88-126] callable=Op dbg_location=4
                Call id(2), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[252-262] scope=1 scope_package_id=2 scope_span=[175-268] callable=Main
                Call id(3), args( Result(0), Pointer, ) !dbg package_id=2 span=[159-163]
                Return !dbg package_id=2 span=[159-163]"#]],
    );
}

#[test]
fn call_to_recursive_callable_succeeds() {
    let program = get_rir_program_with_capabilities(
        indoc! {"
        namespace Test {
            operation Main() : Result {
                use q = Qubit();
                Recursive(3, q);
                MResetZ(q)
            }
            operation Recursive(n : Int, q : Qubit) : Unit {
                if n > 0 {
                    H(q);
                    Recursive(n - 1, q)
                }
            }
        }"},
        TargetCapabilityFlags::empty(),
    );

    assert_block_instructions(
        &program,
        BlockId(0),
        &expect![[r#"
            Block:
                Call id(1), args( Qubit(0), ) !dbg package_id=1 span=[110288-110311] scope=2 scope_package_id=2 scope_span=[195-256] callable=Recursive dbg_location=4
                Call id(1), args( Qubit(0), ) !dbg package_id=1 span=[110288-110311] scope=2 scope_package_id=2 scope_span=[195-256] callable=Recursive dbg_location=7
                Call id(1), args( Qubit(0), ) !dbg package_id=1 span=[110288-110311] scope=2 scope_package_id=2 scope_span=[195-256] callable=Recursive dbg_location=10
                Call id(2), args( Qubit(0), Result(0), ) !dbg package_id=2 span=[108-118] scope=0 scope_package_id=2 scope_span=[48-124] callable=Main
                Call id(3), args( Result(0), Pointer, ) !dbg package_id=2 span=[32-36]
                Return !dbg package_id=2 span=[32-36]"#]],
    );
}

#[test]
fn call_to_recursive_callable_with_unsupported_capabilities_fails() {
    let error = get_partial_evaluation_error_with_capabilities(
        indoc! {"
        namespace Test {
            operation Main() : Result[] {
                use qs = Qubit[2];
                Recursive(3, 0, qs);
                MResetEachZ(qs)
            }
            operation Recursive(n : Int, idx : Int, qs : Qubit[]) : Unit {
                if n > 0 {
                    H(qs[idx]);
                    Recursive(n - 1, if MResetZ(qs[idx]) == One { 1 } else { 0 }, qs)
                }
            }
        }"},
        TargetCapabilityFlags::Adaptive | TargetCapabilityFlags::IntegerComputations,
    );

    assert_error(
        &error,
        &expect!["CapabilityError(UseOfDynamicQubit(Span { lo: 260, hi: 325 }))"],
    );
}

#[test]
fn call_to_test_callable_triggers_error() {
    let error = get_partial_evaluation_error_with_capabilities(
        indoc! {"
        @Test()
        operation Op() : Unit {
            use q = Qubit();
            Std.Diagnostics.CheckZero(q);
        }
        operation Main() : Unit {
            Op();
        }
        "},
        TargetCapabilityFlags::Adaptive,
    );

    assert_error(
        &error,
        &expect![
            "UnsupportedTestCallable(PackageSpan { package: PackageId(2), span: Span { lo: 120, hi: 122 } })"
        ],
    );
}
