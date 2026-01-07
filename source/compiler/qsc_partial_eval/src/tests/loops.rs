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
                Variable(0, Integer) = Store Integer(1) !dbg dbg_location=0
                Call id(2), args( Qubit(0), ) !dbg dbg_location=1
                Variable(0, Integer) = Store Integer(2) !dbg dbg_location=1
                Call id(2), args( Qubit(0), ) !dbg dbg_location=2
                Variable(0, Integer) = Store Integer(3) !dbg dbg_location=2
                Call id(2), args( Qubit(0), ) !dbg dbg_location=3
                Variable(0, Integer) = Store Integer(4) !dbg dbg_location=3
                Call id(3), args( Integer(0), EmptyTag, ) !dbg
                Return !dbg"#]],
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
                Variable(0, Integer) = Store Integer(0) !dbg dbg_location=0
                Call id(2), args( Qubit(0), ) !dbg dbg_location=1
                Variable(0, Integer) = Store Integer(1) !dbg dbg_location=1
                Call id(2), args( Qubit(0), ) !dbg dbg_location=2
                Variable(0, Integer) = Store Integer(2) !dbg dbg_location=2
                Call id(2), args( Qubit(0), ) !dbg dbg_location=3
                Variable(0, Integer) = Store Integer(3) !dbg dbg_location=3
                Call id(3), args( Integer(0), EmptyTag, ) !dbg
                Return !dbg"#]],
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
                Variable(0, Integer) = Store Integer(0) !dbg dbg_location=0
                Variable(1, Boolean) = Store Bool(true) !dbg dbg_location=0
                Call id(2), args( Qubit(0), ) !dbg dbg_location=1
                Variable(0, Integer) = Store Integer(1) !dbg dbg_location=1
                Variable(1, Boolean) = Store Bool(true) !dbg dbg_location=1
                Call id(2), args( Qubit(0), ) !dbg dbg_location=2
                Variable(0, Integer) = Store Integer(2) !dbg dbg_location=2
                Variable(1, Boolean) = Store Bool(true) !dbg dbg_location=2
                Call id(2), args( Qubit(0), ) !dbg dbg_location=3
                Variable(0, Integer) = Store Integer(3) !dbg dbg_location=3
                Variable(1, Boolean) = Store Bool(false) !dbg dbg_location=3
                Call id(3), args( Integer(0), EmptyTag, ) !dbg
                Return !dbg"#]],
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
                Variable(0, Integer) = Store Integer(0) !dbg dbg_location=0
                Call id(2), args( Double(0), Qubit(0), ) !dbg dbg_location=1
                Variable(0, Integer) = Store Integer(1) !dbg dbg_location=1
                Call id(2), args( Double(1), Qubit(0), ) !dbg dbg_location=2
                Variable(0, Integer) = Store Integer(2) !dbg dbg_location=2
                Call id(2), args( Double(2), Qubit(0), ) !dbg dbg_location=3
                Variable(0, Integer) = Store Integer(3) !dbg dbg_location=3
                Call id(3), args( Integer(0), EmptyTag, ) !dbg
                Return !dbg"#]],
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
                Variable(0, Integer) = Store Integer(0) !dbg dbg_location=0
                Call id(2), args( Double(0), Qubit(0), ) !dbg dbg_location=1
                Variable(0, Integer) = Store Integer(1) !dbg dbg_location=1
                Call id(2), args( Double(1), Qubit(0), ) !dbg dbg_location=2
                Variable(0, Integer) = Store Integer(2) !dbg dbg_location=2
                Call id(2), args( Double(2), Qubit(0), ) !dbg dbg_location=3
                Variable(0, Integer) = Store Integer(3) !dbg dbg_location=3
                Call id(3), args( Integer(0), EmptyTag, ) !dbg
                Return !dbg"#]],
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
                Variable(0, Integer) = Store Integer(0) !dbg dbg_location=0
                Variable(1, Boolean) = Store Bool(true) !dbg dbg_location=0
                Call id(2), args( Double(0), Qubit(0), ) !dbg dbg_location=1
                Variable(0, Integer) = Store Integer(1) !dbg dbg_location=1
                Variable(1, Boolean) = Store Bool(true) !dbg dbg_location=1
                Call id(2), args( Double(1), Qubit(0), ) !dbg dbg_location=2
                Variable(0, Integer) = Store Integer(2) !dbg dbg_location=2
                Variable(1, Boolean) = Store Bool(true) !dbg dbg_location=2
                Call id(2), args( Double(2), Qubit(0), ) !dbg dbg_location=3
                Variable(0, Integer) = Store Integer(3) !dbg dbg_location=3
                Variable(1, Boolean) = Store Bool(false) !dbg dbg_location=3
                Call id(3), args( Integer(0), EmptyTag, ) !dbg
                Return !dbg"#]],
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
                Variable(0, Boolean) = Store Bool(false) !dbg dbg_location=0
                Variable(1, Integer) = Store Integer(1) !dbg dbg_location=0
                Call id(2), args( Qubit(0), Result(0), ) !dbg dbg_location=2
                Variable(2, Boolean) = Call id(3), args( Result(0), ) !dbg dbg_location=1
                Variable(3, Boolean) = Store Variable(2, Boolean) !dbg dbg_location=1
                Variable(0, Boolean) = Store Variable(3, Boolean) !dbg dbg_location=1
                Variable(1, Integer) = Store Integer(2) !dbg dbg_location=1
                Variable(4, Boolean) = LogicalNot Variable(0, Boolean) !dbg dbg_location=1
                Branch Variable(4, Boolean), 2, 1 !dbg dbg_location=3"#]],
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
                Variable(0, Integer) = Store Integer(1) !dbg dbg_location=0
                Variable(1, Integer) = Store Integer(1) !dbg dbg_location=0
                Call id(2), args( Qubit(0), Result(0), ) !dbg dbg_location=2
                Variable(2, Boolean) = Call id(3), args( Result(0), ) !dbg dbg_location=1
                Variable(3, Boolean) = Store Variable(2, Boolean) !dbg dbg_location=1
                Branch Variable(3, Boolean), 2, 1 !dbg dbg_location=1"#]],
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
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Pointer, )
                Variable(0, Double) = Store Double(1.1) !dbg dbg_location=0
                Variable(1, Integer) = Store Integer(1) !dbg dbg_location=0
                Call id(2), args( Qubit(0), Result(0), ) !dbg dbg_location=2
                Variable(2, Boolean) = Call id(3), args( Result(0), ) !dbg dbg_location=1
                Variable(3, Boolean) = Store Variable(2, Boolean) !dbg dbg_location=1
                Branch Variable(3, Boolean), 2, 1 !dbg dbg_location=1
            Block 1:Block:
                Variable(1, Integer) = Store Integer(2) !dbg dbg_location=1
                Variable(4, Boolean) = Fcmp Ogt, Variable(0, Double), Double(0.1) !dbg dbg_location=1
                Variable(5, Boolean) = Store Bool(false) !dbg dbg_location=1
                Branch Variable(4, Boolean), 4, 3 !dbg dbg_location=3
            Block 2:Block:
                Variable(0, Double) = Store Double(-1.1) !dbg dbg_location=1
                Jump(1) !dbg dbg_location=1
            Block 3:Block:
                Branch Variable(5, Boolean), 6, 5 !dbg dbg_location=3
            Block 4:Block:
                Call id(2), args( Qubit(0), Result(1), ) !dbg dbg_location=4
                Variable(6, Boolean) = Call id(3), args( Result(1), ) !dbg dbg_location=3
                Variable(7, Boolean) = Store Variable(6, Boolean) !dbg dbg_location=3
                Variable(5, Boolean) = Store Variable(7, Boolean) !dbg dbg_location=3
                Jump(3) !dbg dbg_location=3
            Block 5:Block:
                Variable(1, Integer) = Store Integer(3) !dbg dbg_location=3
                Variable(9, Boolean) = Fcmp Ogt, Variable(0, Double), Double(0.1) !dbg dbg_location=3
                Variable(10, Boolean) = Store Bool(false) !dbg dbg_location=3
                Branch Variable(9, Boolean), 8, 7 !dbg dbg_location=5
            Block 6:Block:
                Variable(8, Double) = Fmul Double(-1), Variable(0, Double) !dbg dbg_location=3
                Variable(0, Double) = Store Variable(8, Double) !dbg dbg_location=3
                Jump(5) !dbg dbg_location=3
            Block 7:Block:
                Branch Variable(10, Boolean), 10, 9 !dbg dbg_location=5
            Block 8:Block:
                Call id(2), args( Qubit(0), Result(2), ) !dbg dbg_location=6
                Variable(11, Boolean) = Call id(3), args( Result(2), ) !dbg dbg_location=5
                Variable(12, Boolean) = Store Variable(11, Boolean) !dbg dbg_location=5
                Variable(10, Boolean) = Store Variable(12, Boolean) !dbg dbg_location=5
                Jump(7) !dbg dbg_location=5
            Block 9:Block:
                Variable(1, Integer) = Store Integer(4) !dbg dbg_location=5
                Call id(4), args( Integer(0), EmptyTag, ) !dbg
                Return !dbg
            Block 10:Block:
                Variable(13, Double) = Fmul Double(-1), Variable(0, Double) !dbg dbg_location=5
                Variable(0, Double) = Store Variable(13, Double) !dbg dbg_location=5
                Jump(9) !dbg dbg_location=5"#]],
    );
}

#[allow(clippy::too_many_lines)]
#[test]
fn result_array_index_range_in_for_loop() {
    let program = get_rir_program(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Int {
                use qs = Qubit[2];
                let results = MResetEachZ(qs);
                mutable count = 0;
                for i in Std.Arrays.IndexRange(results) {
                    if results[i] == One {
                        set count += 1;
                    }
                }
                count
            }
        }
    "#});
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
                    call_type: Initialize
                    input_type:
                        [0]: Pointer
                    output_type: <VOID>
                    body: <NONE>
                Callable 2: Callable:
                    name: __quantum__qis__mresetz__body
                    call_type: Measurement
                    input_type:
                        [0]: Qubit
                        [1]: Result
                    output_type: <VOID>
                    body: <NONE>
                Callable 3: Callable:
                    name: __quantum__rt__read_result
                    call_type: Readout
                    input_type:
                        [0]: Result
                    output_type: Boolean
                    body: <NONE>
                Callable 4: Callable:
                    name: __quantum__rt__int_record_output
                    call_type: OutputRecording
                    input_type:
                        [0]: Integer
                        [1]: Pointer
                    output_type: <VOID>
                    body: <NONE>
            blocks:
                Block 0: Block:
                    Call id(1), args( Pointer, )
                    Variable(0, Integer) = Store Integer(0) !dbg
                    Variable(0, Integer) = Store Integer(1) !dbg dbg_location=1
                    Variable(0, Integer) = Store Integer(2) !dbg dbg_location=2
                    Variable(1, Integer) = Store Integer(0) !dbg
                    Call id(2), args( Qubit(0), Result(0), ) !dbg dbg_location=5
                    Variable(1, Integer) = Store Integer(1) !dbg dbg_location=4
                    Call id(2), args( Qubit(1), Result(1), ) !dbg dbg_location=7
                    Variable(1, Integer) = Store Integer(2) !dbg dbg_location=6
                    Variable(2, Integer) = Store Integer(0) !dbg dbg_location=3
                    Variable(3, Integer) = Store Integer(0) !dbg dbg_location=8
                    Variable(4, Boolean) = Call id(3), args( Result(0), ) !dbg dbg_location=8
                    Variable(5, Boolean) = Store Variable(4, Boolean) !dbg dbg_location=8
                    Branch Variable(5, Boolean), 2, 1 !dbg dbg_location=8
                Block 1: Block:
                    Variable(3, Integer) = Store Integer(1) !dbg dbg_location=8
                    Variable(6, Boolean) = Call id(3), args( Result(1), ) !dbg dbg_location=8
                    Variable(7, Boolean) = Store Variable(6, Boolean) !dbg dbg_location=8
                    Branch Variable(7, Boolean), 4, 3 !dbg dbg_location=8
                Block 2: Block:
                    Variable(2, Integer) = Store Integer(1) !dbg dbg_location=8
                    Jump(1) !dbg dbg_location=8
                Block 3: Block:
                    Variable(3, Integer) = Store Integer(2) !dbg dbg_location=8
                    Variable(9, Integer) = Store Variable(2, Integer) !dbg dbg_location=8
                    Variable(10, Integer) = Store Integer(0) !dbg
                    Variable(10, Integer) = Store Integer(1) !dbg dbg_location=10
                    Variable(10, Integer) = Store Integer(2) !dbg dbg_location=11
                    Call id(4), args( Variable(9, Integer), Tag(0, 3), ) !dbg
                    Return !dbg
                Block 4: Block:
                    Variable(8, Integer) = Add Variable(2, Integer), Integer(1) !dbg dbg_location=8
                    Variable(2, Integer) = Store Variable(8, Integer) !dbg dbg_location=8
                    Jump(3) !dbg dbg_location=8
            config: Config:
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations | BackwardsBranching | HigherLevelConstructs)
            num_qubits: 2
            num_results: 2
            dbg_metadata_scopes:
                0 = SubProgram name=entry location=(package_id=0 span=[0-0]) item_id=((0, 0))
                1 = SubProgram name=Main location=(package_id=2 span=[40-318]) item_id=((2, 1))
                2 = SubProgram name=AllocateQubitArray location=(package_id=0 span=[2577-2872]) item_id=((0, 10))
                3 = SubProgram name=MResetEachZ location=(package_id=1 span=[179494-179663]) item_id=((1, 503))
                4 = SubProgram name=MResetZ location=(package_id=1 span=[180994-181082]) item_id=((1, 506))
                5 = SubProgram name=ReleaseQubitArray location=(package_id=0 span=[2878-3011]) item_id=((0, 11))
            dbg_locations:
                [0]:  scope=1location=(package_id=2 span=[73-91])
                [1]:  scope=2location=(package_id=0 span=[2812-2843]) inlined_at=0
                [2]:  scope=2location=(package_id=0 span=[2812-2843]) inlined_at=0
                [3]:  scope=1location=(package_id=2 span=[114-129])
                [4]:  scope=3location=(package_id=1 span=[179627-179641]) inlined_at=3
                [5]:  scope=4location=(package_id=1 span=[181043-181080]) inlined_at=4
                [6]:  scope=3location=(package_id=1 span=[179627-179641]) inlined_at=3
                [7]:  scope=4location=(package_id=1 span=[181043-181080]) inlined_at=6
                [8]:  scope=1location=(package_id=2 span=[175-205])
                [9]:  scope=1location=(package_id=2 span=[73-91])
                [10]:  scope=5location=(package_id=0 span=[2963-2994]) inlined_at=9
                [11]:  scope=5location=(package_id=0 span=[2963-2994]) inlined_at=9
            tags:
                [0]: 0_i
    "#]].assert_eq(&program.to_string());
}
