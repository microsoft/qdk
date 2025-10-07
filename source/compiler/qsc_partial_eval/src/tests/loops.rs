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
                Variable(0, Integer) = Store Integer(1) !dbg dbg_location=1
                Call id(2), args( Qubit(0), ) !dbg dbg_location=2
                Variable(0, Integer) = Store Integer(2) !dbg dbg_location=2
                Call id(2), args( Qubit(0), ) !dbg dbg_location=3
                Variable(0, Integer) = Store Integer(3) !dbg dbg_location=3
                Call id(2), args( Qubit(0), ) !dbg dbg_location=4
                Variable(0, Integer) = Store Integer(4) !dbg dbg_location=4
                Call id(3), args( Integer(0), EmptyTag, ) !dbg dbg_location=0
                Return !dbg dbg_location=0"#]],
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
                Variable(0, Integer) = Store Integer(0) !dbg dbg_location=1
                Call id(2), args( Qubit(0), ) !dbg dbg_location=2
                Variable(0, Integer) = Store Integer(1) !dbg dbg_location=2
                Call id(2), args( Qubit(0), ) !dbg dbg_location=3
                Variable(0, Integer) = Store Integer(2) !dbg dbg_location=3
                Call id(2), args( Qubit(0), ) !dbg dbg_location=4
                Variable(0, Integer) = Store Integer(3) !dbg dbg_location=4
                Call id(3), args( Integer(0), EmptyTag, ) !dbg dbg_location=0
                Return !dbg dbg_location=0"#]],
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
                Variable(0, Integer) = Store Integer(0) !dbg dbg_location=1
                Variable(1, Boolean) = Store Bool(true) !dbg dbg_location=1
                Call id(2), args( Qubit(0), ) !dbg dbg_location=2
                Variable(0, Integer) = Store Integer(1) !dbg dbg_location=2
                Variable(1, Boolean) = Store Bool(true) !dbg dbg_location=2
                Call id(2), args( Qubit(0), ) !dbg dbg_location=3
                Variable(0, Integer) = Store Integer(2) !dbg dbg_location=3
                Variable(1, Boolean) = Store Bool(true) !dbg dbg_location=3
                Call id(2), args( Qubit(0), ) !dbg dbg_location=4
                Variable(0, Integer) = Store Integer(3) !dbg dbg_location=4
                Variable(1, Boolean) = Store Bool(false) !dbg dbg_location=4
                Call id(3), args( Integer(0), EmptyTag, ) !dbg dbg_location=0
                Return !dbg dbg_location=0"#]],
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
                Variable(0, Integer) = Store Integer(0) !dbg dbg_location=1
                Call id(2), args( Double(0), Qubit(0), ) !dbg dbg_location=2
                Variable(0, Integer) = Store Integer(1) !dbg dbg_location=2
                Call id(2), args( Double(1), Qubit(0), ) !dbg dbg_location=3
                Variable(0, Integer) = Store Integer(2) !dbg dbg_location=3
                Call id(2), args( Double(2), Qubit(0), ) !dbg dbg_location=4
                Variable(0, Integer) = Store Integer(3) !dbg dbg_location=4
                Call id(3), args( Integer(0), EmptyTag, ) !dbg dbg_location=0
                Return !dbg dbg_location=0"#]],
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
                Variable(0, Integer) = Store Integer(0) !dbg dbg_location=1
                Call id(2), args( Double(0), Qubit(0), ) !dbg dbg_location=2
                Variable(0, Integer) = Store Integer(1) !dbg dbg_location=2
                Call id(2), args( Double(1), Qubit(0), ) !dbg dbg_location=3
                Variable(0, Integer) = Store Integer(2) !dbg dbg_location=3
                Call id(2), args( Double(2), Qubit(0), ) !dbg dbg_location=4
                Variable(0, Integer) = Store Integer(3) !dbg dbg_location=4
                Call id(3), args( Integer(0), EmptyTag, ) !dbg dbg_location=0
                Return !dbg dbg_location=0"#]],
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
                Variable(0, Integer) = Store Integer(0) !dbg dbg_location=1
                Variable(1, Boolean) = Store Bool(true) !dbg dbg_location=1
                Call id(2), args( Double(0), Qubit(0), ) !dbg dbg_location=2
                Variable(0, Integer) = Store Integer(1) !dbg dbg_location=2
                Variable(1, Boolean) = Store Bool(true) !dbg dbg_location=2
                Call id(2), args( Double(1), Qubit(0), ) !dbg dbg_location=3
                Variable(0, Integer) = Store Integer(2) !dbg dbg_location=3
                Variable(1, Boolean) = Store Bool(true) !dbg dbg_location=3
                Call id(2), args( Double(2), Qubit(0), ) !dbg dbg_location=4
                Variable(0, Integer) = Store Integer(3) !dbg dbg_location=4
                Variable(1, Boolean) = Store Bool(false) !dbg dbg_location=4
                Call id(3), args( Integer(0), EmptyTag, ) !dbg dbg_location=0
                Return !dbg dbg_location=0"#]],
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
                Variable(0, Boolean) = Store Bool(false) !dbg dbg_location=1
                Variable(1, Integer) = Store Integer(1) !dbg dbg_location=1
                Call id(2), args( Qubit(0), Result(0), ) !dbg dbg_location=3
                Variable(2, Boolean) = Call id(3), args( Result(0), ) !dbg dbg_location=2
                Variable(3, Boolean) = Store Variable(2, Boolean) !dbg dbg_location=2
                Variable(0, Boolean) = Store Variable(3, Boolean) !dbg dbg_location=2
                Variable(1, Integer) = Store Integer(2) !dbg dbg_location=2
                Variable(4, Boolean) = LogicalNot Variable(0, Boolean) !dbg dbg_location=2
                Branch Variable(4, Boolean), 2, 1 !dbg dbg_location=4"#]],
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
                Variable(0, Integer) = Store Integer(1) !dbg dbg_location=1
                Variable(1, Integer) = Store Integer(1) !dbg dbg_location=1
                Call id(2), args( Qubit(0), Result(0), ) !dbg dbg_location=3
                Variable(2, Boolean) = Call id(3), args( Result(0), ) !dbg dbg_location=2
                Variable(3, Boolean) = Store Variable(2, Boolean) !dbg dbg_location=2
                Branch Variable(3, Boolean), 2, 1 !dbg dbg_location=2"#]],
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
                Variable(0, Double) = Store Double(1.1) !dbg dbg_location=1
                Variable(1, Integer) = Store Integer(1) !dbg dbg_location=1
                Call id(2), args( Qubit(0), Result(0), ) !dbg dbg_location=3
                Variable(2, Boolean) = Call id(3), args( Result(0), ) !dbg dbg_location=2
                Variable(3, Boolean) = Store Variable(2, Boolean) !dbg dbg_location=2
                Branch Variable(3, Boolean), 2, 1 !dbg dbg_location=2
            Block 1:Block:
                Variable(1, Integer) = Store Integer(2) !dbg dbg_location=2
                Variable(4, Boolean) = Fcmp Ogt, Variable(0, Double), Double(0.1) !dbg dbg_location=2
                Variable(5, Boolean) = Store Bool(false) !dbg dbg_location=2
                Branch Variable(4, Boolean), 4, 3 !dbg dbg_location=4
            Block 2:Block:
                Variable(0, Double) = Store Double(-1.1) !dbg dbg_location=2
                Jump(1) !dbg dbg_location=2
            Block 3:Block:
                Branch Variable(5, Boolean), 6, 5 !dbg dbg_location=4
            Block 4:Block:
                Call id(2), args( Qubit(0), Result(1), ) !dbg dbg_location=5
                Variable(6, Boolean) = Call id(3), args( Result(1), ) !dbg dbg_location=4
                Variable(7, Boolean) = Store Variable(6, Boolean) !dbg dbg_location=4
                Variable(5, Boolean) = Store Variable(7, Boolean) !dbg dbg_location=4
                Jump(3) !dbg dbg_location=4
            Block 5:Block:
                Variable(1, Integer) = Store Integer(3) !dbg dbg_location=4
                Variable(9, Boolean) = Fcmp Ogt, Variable(0, Double), Double(0.1) !dbg dbg_location=4
                Variable(10, Boolean) = Store Bool(false) !dbg dbg_location=4
                Branch Variable(9, Boolean), 8, 7 !dbg dbg_location=6
            Block 6:Block:
                Variable(8, Double) = Fmul Double(-1), Variable(0, Double) !dbg dbg_location=4
                Variable(0, Double) = Store Variable(8, Double) !dbg dbg_location=4
                Jump(5) !dbg dbg_location=4
            Block 7:Block:
                Branch Variable(10, Boolean), 10, 9 !dbg dbg_location=6
            Block 8:Block:
                Call id(2), args( Qubit(0), Result(2), ) !dbg dbg_location=7
                Variable(11, Boolean) = Call id(3), args( Result(2), ) !dbg dbg_location=6
                Variable(12, Boolean) = Store Variable(11, Boolean) !dbg dbg_location=6
                Variable(10, Boolean) = Store Variable(12, Boolean) !dbg dbg_location=6
                Jump(7) !dbg dbg_location=6
            Block 9:Block:
                Variable(1, Integer) = Store Integer(4) !dbg dbg_location=6
                Call id(4), args( Integer(0), EmptyTag, ) !dbg dbg_location=0
                Return !dbg dbg_location=0
            Block 10:Block:
                Variable(13, Double) = Fmul Double(-1), Variable(0, Double) !dbg dbg_location=6
                Variable(0, Double) = Store Variable(13, Double) !dbg dbg_location=6
                Jump(9) !dbg dbg_location=6"#]],
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
                    Variable(0, Integer) = Store Integer(1) !dbg dbg_location=2
                    Variable(0, Integer) = Store Integer(2) !dbg dbg_location=3
                    Variable(1, Integer) = Store Integer(0) !dbg
                    Call id(2), args( Qubit(0), Result(0), ) !dbg dbg_location=6
                    Variable(1, Integer) = Store Integer(1) !dbg dbg_location=5
                    Call id(2), args( Qubit(1), Result(1), ) !dbg dbg_location=8
                    Variable(1, Integer) = Store Integer(2) !dbg dbg_location=7
                    Variable(2, Integer) = Store Integer(0) !dbg dbg_location=4
                    Variable(3, Integer) = Store Integer(0) !dbg dbg_location=9
                    Variable(4, Boolean) = Call id(3), args( Result(0), ) !dbg dbg_location=9
                    Variable(5, Boolean) = Store Variable(4, Boolean) !dbg dbg_location=9
                    Branch Variable(5, Boolean), 2, 1 !dbg dbg_location=9
                Block 1: Block:
                    Variable(3, Integer) = Store Integer(1) !dbg dbg_location=9
                    Variable(6, Boolean) = Call id(3), args( Result(1), ) !dbg dbg_location=9
                    Variable(7, Boolean) = Store Variable(6, Boolean) !dbg dbg_location=9
                    Branch Variable(7, Boolean), 4, 3 !dbg dbg_location=9
                Block 2: Block:
                    Variable(2, Integer) = Store Integer(1) !dbg dbg_location=9
                    Jump(1) !dbg dbg_location=9
                Block 3: Block:
                    Variable(3, Integer) = Store Integer(2) !dbg dbg_location=9
                    Variable(9, Integer) = Store Variable(2, Integer) !dbg dbg_location=9
                    Variable(10, Integer) = Store Integer(0) !dbg
                    Variable(10, Integer) = Store Integer(1) !dbg dbg_location=11
                    Variable(10, Integer) = Store Integer(2) !dbg dbg_location=12
                    Call id(4), args( Variable(9, Integer), Tag(0, 3), ) !dbg dbg_location=0
                    Return !dbg dbg_location=0
                Block 4: Block:
                    Variable(8, Integer) = Add Variable(2, Integer), Integer(1) !dbg dbg_location=9
                    Variable(2, Integer) = Store Variable(8, Integer) !dbg dbg_location=9
                    Jump(3) !dbg dbg_location=9
            config: Config:
                capabilities: TargetCapabilityFlags(Adaptive | IntegerComputations | FloatingPointComputations | BackwardsBranching | HigherLevelConstructs | QubitReset)
            num_qubits: 2
            num_results: 2
            dbg_metadata_scopes:
                [0]: SubProgram name=entry location=(package_id=0 span=[0-0])
                [1]: SubProgram name=Main location=(package_id=2 span=[40-318])
                [2]: SubProgram name=AllocateQubitArray location=(package_id=0 span=[2577-2872])
                [3]: SubProgram name=MResetEachZ location=(package_id=1 span=[179488-179657])
                [4]: SubProgram name=MResetZ location=(package_id=1 span=[180988-181076])
                [5]: SubProgram name=ReleaseQubitArray location=(package_id=0 span=[2878-3011])
            dbg_locations:
                [0]:  scope=0location=(package_id=2 span=[0-0])
                [1]:  scope=1location=(package_id=2 span=[73-91]) inlined_at=0
                [2]:  scope=2location=(package_id=0 span=[2812-2843]) inlined_at=1
                [3]:  scope=2location=(package_id=0 span=[2812-2843]) inlined_at=1
                [4]:  scope=1location=(package_id=2 span=[114-129]) inlined_at=0
                [5]:  scope=3location=(package_id=1 span=[179621-179635]) inlined_at=4
                [6]:  scope=4location=(package_id=1 span=[181037-181074]) inlined_at=5
                [7]:  scope=3location=(package_id=1 span=[179621-179635]) inlined_at=4
                [8]:  scope=4location=(package_id=1 span=[181037-181074]) inlined_at=7
                [9]:  scope=1location=(package_id=2 span=[175-205]) inlined_at=0
                [10]:  scope=1location=(package_id=2 span=[73-91]) inlined_at=0
                [11]:  scope=5location=(package_id=0 span=[2963-2994]) inlined_at=10
                [12]:  scope=5location=(package_id=0 span=[2963-2994]) inlined_at=10
            tags:
                [0]: 0_i
    "#]].assert_eq(&program.to_string());
}
