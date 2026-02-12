// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![allow(
    clippy::needless_raw_string_hashes,
    clippy::similar_names,
    clippy::too_many_lines
)]

use crate::tests::{assert_blocks, get_rir_program_with_dbg_metadata};

use expect_test::expect;
use indoc::indoc;

#[test]
fn no_gates() {
    let program = get_rir_program_with_dbg_metadata(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                Message("hi");
            }
        }
    "#});

    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn one_gate() {
    let program = get_rir_program_with_dbg_metadata(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                H(q);
            }
        }
    "#});

    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), ) !dbg dbg_location=2
                Call id(3), args( Integer(0), EmptyTag, )
                Return

            dbg_metadata_scopes:
                0 = SubProgram name=Main location=(2-40)
                1 = SubProgram name=H location=(1-110222)
            dbg_locations:
                [0]: scope=0 location=(2-74)
                [1]: scope=0 location=(2-99)
                [2]: scope=1 location=(1-110294) inlined_at=1
                [3]: scope=0 location=(2-74)"#]],
    );
}

#[test]
fn one_measurement() {
    let program = get_rir_program_with_dbg_metadata(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result[] {
                use q = Qubit();
                H(q);
                let r1 = M(q);
                [r1]
            }
        }
    "#});

    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), ) !dbg dbg_location=2
                Call id(3), args( Qubit(0), Result(0), ) !dbg dbg_location=8
                Call id(4), args( Integer(1), EmptyTag, )
                Call id(5), args( Result(0), Tag(0, 5), )
                Return

            dbg_metadata_scopes:
                0 = SubProgram name=Main location=(2-40)
                1 = SubProgram name=H location=(1-110222)
                2 = SubProgram name=M location=(1-111931)
                3 = SubProgram name=Measure location=(1-112847)
                4 = SubProgram name=MapPauliAxis location=(1-55426)
                5 = SubProgram name=MapPauliAxis' location=(1-55426)
            dbg_locations:
                [0]: scope=0 location=(2-78)
                [1]: scope=0 location=(2-103)
                [2]: scope=1 location=(1-110294) inlined_at=1
                [3]: scope=0 location=(2-126)
                [4]: scope=2 location=(1-111973) inlined_at=3
                [5]: scope=3 location=(1-113034) inlined_at=4
                [6]: scope=3 location=(1-113087) inlined_at=4
                [7]: scope=4 location=(1-55512) inlined_at=6
                [8]: scope=3 location=(1-113160) inlined_at=4
                [9]: scope=3 location=(1-113087) inlined_at=4
                [10]: scope=5 location=(1-55512) inlined_at=9
                [11]: scope=0 location=(2-78)"#]],
    );
}

#[test]
fn calls_to_other_callables() {
    let program = get_rir_program_with_dbg_metadata(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                Foo(q);
                MResetZ(q);
            }

            operation Foo(q: Qubit) : Unit {
                H(q);
            }
        }
    "#});

    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), ) !dbg dbg_location=3
                Call id(3), args( Qubit(0), Result(0), ) !dbg dbg_location=5
                Call id(4), args( Integer(0), EmptyTag, )
                Return

            dbg_metadata_scopes:
                0 = SubProgram name=Main location=(2-40)
                1 = SubProgram name=Foo location=(2-138)
                2 = SubProgram name=H location=(1-110222)
                3 = SubProgram name=MResetZ location=(1-181274)
            dbg_locations:
                [0]: scope=0 location=(2-74)
                [1]: scope=0 location=(2-99)
                [2]: scope=1 location=(2-179) inlined_at=1
                [3]: scope=2 location=(1-110294) inlined_at=2
                [4]: scope=0 location=(2-115)
                [5]: scope=3 location=(1-181323) inlined_at=4
                [6]: scope=0 location=(2-74)"#]],
    );
}

#[test]
fn classical_for_loop() {
    let program = get_rir_program_with_dbg_metadata(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                use q = Qubit();
                for i in 0..2 {
                    Foo(q);
                }
            }

            operation Foo(q: Qubit) : Unit {
                X(q);
                Y(q);
            }
        }
    "#});

    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Pointer, )
                Variable(0, Integer) = Store Integer(0)
                Call id(2), args( Qubit(0), ) !dbg dbg_location=4
                Call id(3), args( Qubit(0), ) !dbg dbg_location=6
                Variable(0, Integer) = Store Integer(1)
                Call id(2), args( Qubit(0), ) !dbg dbg_location=9
                Call id(3), args( Qubit(0), ) !dbg dbg_location=11
                Variable(0, Integer) = Store Integer(2)
                Call id(2), args( Qubit(0), ) !dbg dbg_location=14
                Call id(3), args( Qubit(0), ) !dbg dbg_location=16
                Variable(0, Integer) = Store Integer(3)
                Call id(4), args( Integer(0), EmptyTag, )
                Return

            dbg_metadata_scopes:
                0 = SubProgram name=Main location=(2-40)
                1 = LexicalBlockFile location=(2-99) discriminator=1
                2 = SubProgram name=Foo location=(2-156)
                3 = SubProgram name=X location=(1-133020)
                4 = SubProgram name=Y location=(1-134242)
                5 = LexicalBlockFile location=(2-99) discriminator=2
                6 = LexicalBlockFile location=(2-99) discriminator=3
            dbg_locations:
                [0]: scope=0 location=(2-74)
                [1]: scope=0 location=(2-99)
                [2]: scope=1 location=(2-127) inlined_at=1
                [3]: scope=2 location=(2-197) inlined_at=2
                [4]: scope=3 location=(1-133092) inlined_at=3
                [5]: scope=2 location=(2-211) inlined_at=2
                [6]: scope=4 location=(1-134314) inlined_at=5
                [7]: scope=5 location=(2-127) inlined_at=1
                [8]: scope=2 location=(2-197) inlined_at=7
                [9]: scope=3 location=(1-133092) inlined_at=8
                [10]: scope=2 location=(2-211) inlined_at=7
                [11]: scope=4 location=(1-134314) inlined_at=10
                [12]: scope=6 location=(2-127) inlined_at=1
                [13]: scope=2 location=(2-197) inlined_at=12
                [14]: scope=3 location=(1-133092) inlined_at=13
                [15]: scope=2 location=(2-211) inlined_at=12
                [16]: scope=4 location=(1-134314) inlined_at=15
                [17]: scope=0 location=(2-74)"#]],
    );
}

#[test]
fn nested_classical_for_loop() {
    let program = get_rir_program_with_dbg_metadata(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                use qs = Qubit[3];
                for j in 0..2 {
                    for i in 0..2 {
                        Foo(qs[i]);
                    }
                }
            }

            operation Foo(q: Qubit) : Unit {
                X(q);
            }
        }
    "#});

    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Pointer, )
                Variable(0, Integer) = Store Integer(0)
                Variable(0, Integer) = Store Integer(1)
                Variable(0, Integer) = Store Integer(2)
                Variable(0, Integer) = Store Integer(3)
                Variable(1, Integer) = Store Integer(0)
                Variable(2, Integer) = Store Integer(0)
                Call id(2), args( Qubit(0), ) !dbg dbg_location=9
                Variable(2, Integer) = Store Integer(1)
                Call id(2), args( Qubit(1), ) !dbg dbg_location=12
                Variable(2, Integer) = Store Integer(2)
                Call id(2), args( Qubit(2), ) !dbg dbg_location=15
                Variable(2, Integer) = Store Integer(3)
                Variable(1, Integer) = Store Integer(1)
                Variable(3, Integer) = Store Integer(0)
                Call id(2), args( Qubit(0), ) !dbg dbg_location=19
                Variable(3, Integer) = Store Integer(1)
                Call id(2), args( Qubit(1), ) !dbg dbg_location=22
                Variable(3, Integer) = Store Integer(2)
                Call id(2), args( Qubit(2), ) !dbg dbg_location=25
                Variable(3, Integer) = Store Integer(3)
                Variable(1, Integer) = Store Integer(2)
                Variable(4, Integer) = Store Integer(0)
                Call id(2), args( Qubit(0), ) !dbg dbg_location=29
                Variable(4, Integer) = Store Integer(1)
                Call id(2), args( Qubit(1), ) !dbg dbg_location=32
                Variable(4, Integer) = Store Integer(2)
                Call id(2), args( Qubit(2), ) !dbg dbg_location=35
                Variable(4, Integer) = Store Integer(3)
                Variable(1, Integer) = Store Integer(3)
                Variable(5, Integer) = Store Integer(0)
                Variable(5, Integer) = Store Integer(1)
                Variable(5, Integer) = Store Integer(2)
                Variable(5, Integer) = Store Integer(3)
                Call id(3), args( Integer(0), EmptyTag, )
                Return

            dbg_metadata_scopes:
                0 = SubProgram name=Main location=(2-40)
                1 = SubProgram name=AllocateQubitArray location=(0-2577)
                2 = LexicalBlockFile location=(0-2766) discriminator=1
                3 = LexicalBlockFile location=(0-2766) discriminator=2
                4 = LexicalBlockFile location=(0-2766) discriminator=3
                5 = LexicalBlockFile location=(2-101) discriminator=1
                6 = LexicalBlockFile location=(2-129) discriminator=1
                7 = SubProgram name=Foo location=(2-208)
                8 = SubProgram name=X location=(1-133020)
                9 = LexicalBlockFile location=(2-129) discriminator=2
                10 = LexicalBlockFile location=(2-129) discriminator=3
                11 = LexicalBlockFile location=(2-101) discriminator=2
                12 = LexicalBlockFile location=(2-101) discriminator=3
                13 = SubProgram name=ReleaseQubitArray location=(0-2878)
                14 = LexicalBlockFile location=(0-2937) discriminator=1
                15 = LexicalBlockFile location=(0-2937) discriminator=2
                16 = LexicalBlockFile location=(0-2937) discriminator=3
            dbg_locations:
                [0]: scope=0 location=(2-74)
                [1]: scope=1 location=(0-2766) inlined_at=0
                [2]: scope=2 location=(0-2812) inlined_at=1
                [3]: scope=3 location=(0-2812) inlined_at=1
                [4]: scope=4 location=(0-2812) inlined_at=1
                [5]: scope=0 location=(2-101)
                [6]: scope=5 location=(2-129) inlined_at=5
                [7]: scope=6 location=(2-161) inlined_at=6
                [8]: scope=7 location=(2-249) inlined_at=7
                [9]: scope=8 location=(1-133092) inlined_at=8
                [10]: scope=9 location=(2-161) inlined_at=6
                [11]: scope=7 location=(2-249) inlined_at=10
                [12]: scope=8 location=(1-133092) inlined_at=11
                [13]: scope=10 location=(2-161) inlined_at=6
                [14]: scope=7 location=(2-249) inlined_at=13
                [15]: scope=8 location=(1-133092) inlined_at=14
                [16]: scope=11 location=(2-129) inlined_at=5
                [17]: scope=6 location=(2-161) inlined_at=16
                [18]: scope=7 location=(2-249) inlined_at=17
                [19]: scope=8 location=(1-133092) inlined_at=18
                [20]: scope=9 location=(2-161) inlined_at=16
                [21]: scope=7 location=(2-249) inlined_at=20
                [22]: scope=8 location=(1-133092) inlined_at=21
                [23]: scope=10 location=(2-161) inlined_at=16
                [24]: scope=7 location=(2-249) inlined_at=23
                [25]: scope=8 location=(1-133092) inlined_at=24
                [26]: scope=12 location=(2-129) inlined_at=5
                [27]: scope=6 location=(2-161) inlined_at=26
                [28]: scope=7 location=(2-249) inlined_at=27
                [29]: scope=8 location=(1-133092) inlined_at=28
                [30]: scope=9 location=(2-161) inlined_at=26
                [31]: scope=7 location=(2-249) inlined_at=30
                [32]: scope=8 location=(1-133092) inlined_at=31
                [33]: scope=10 location=(2-161) inlined_at=26
                [34]: scope=7 location=(2-249) inlined_at=33
                [35]: scope=8 location=(1-133092) inlined_at=34
                [36]: scope=0 location=(2-74)
                [37]: scope=13 location=(0-2937) inlined_at=36
                [38]: scope=14 location=(0-2963) inlined_at=37
                [39]: scope=15 location=(0-2963) inlined_at=37
                [40]: scope=16 location=(0-2963) inlined_at=37"#]],
    );
}

#[test]
fn lambda() {
    let program = get_rir_program_with_dbg_metadata(indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result[] { [] }
        }
    "#});

    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Integer(0), EmptyTag, )
                Return"#]],
    );
}

#[test]
fn result_comparison_to_literal() {
    let program = get_rir_program_with_dbg_metadata(indoc! {r#"
        namespace Test {
            operation Main() : Result[] {
                use q1 = Qubit();
                H(q1);
                let r1 = M(q1);
                if (r1 == One) {
                    X(q1);
                }
                Reset(q1);
                [r1]
            }
        }
    "#});

    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), ) !dbg dbg_location=2
                Call id(3), args( Qubit(0), Result(0), ) !dbg dbg_location=8
                Variable(0, Boolean) = Call id(4), args( Result(0), ) !dbg dbg_location=11
                Variable(1, Boolean) = Store Variable(0, Boolean)
                Branch Variable(1, Boolean), 2, 1 !dbg dbg_location=11
            Block 1:Block:
                Call id(6), args( Qubit(0), ) !dbg dbg_location=15
                Call id(7), args( Integer(1), EmptyTag, )
                Call id(8), args( Result(0), Tag(0, 5), )
                Return
            Block 2:Block:
                Call id(5), args( Qubit(0), ) !dbg dbg_location=13
                Jump(1)

            dbg_metadata_scopes:
                0 = SubProgram name=Main location=(2-22)
                1 = SubProgram name=H location=(1-110222)
                2 = SubProgram name=M location=(1-111931)
                3 = SubProgram name=Measure location=(1-112847)
                4 = SubProgram name=MapPauliAxis location=(1-55426)
                5 = SubProgram name=MapPauliAxis' location=(1-55426)
                6 = SubProgram name=X location=(1-133020)
                7 = SubProgram name=Reset location=(1-116320)
            dbg_locations:
                [0]: scope=0 location=(2-60)
                [1]: scope=0 location=(2-86)
                [2]: scope=1 location=(1-110294) inlined_at=1
                [3]: scope=0 location=(2-110)
                [4]: scope=2 location=(1-111973) inlined_at=3
                [5]: scope=3 location=(1-113034) inlined_at=4
                [6]: scope=3 location=(1-113087) inlined_at=4
                [7]: scope=4 location=(1-55512) inlined_at=6
                [8]: scope=3 location=(1-113160) inlined_at=4
                [9]: scope=3 location=(1-113087) inlined_at=4
                [10]: scope=5 location=(1-55512) inlined_at=9
                [11]: scope=0 location=(2-125)
                [12]: scope=0 location=(2-154)
                [13]: scope=6 location=(1-133092) inlined_at=12
                [14]: scope=0 location=(2-179)
                [15]: scope=7 location=(1-116364) inlined_at=14
                [16]: scope=0 location=(2-60)"#]],
    );
}

#[test]
fn if_else() {
    let program = get_rir_program_with_dbg_metadata(indoc! {r#"
        namespace Test {
            operation Main() : Result[] {
                use q0 = Qubit();
                use q1 = Qubit();
                H(q0);
                let r = M(q0);
                if r == One {
                    X(q1);
                } else {
                    Y(q1);
                }
                let r1 = M(q1);
                [r, r1]
            }
        }
    "#});

    assert_blocks(
        &program,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Pointer, )
                Call id(2), args( Qubit(0), ) !dbg dbg_location=3
                Call id(3), args( Qubit(0), Result(0), ) !dbg dbg_location=9
                Variable(0, Boolean) = Call id(4), args( Result(0), ) !dbg dbg_location=12
                Variable(1, Boolean) = Store Variable(0, Boolean)
                Branch Variable(1, Boolean), 2, 3 !dbg dbg_location=12
            Block 1:Block:
                Call id(3), args( Qubit(1), Result(1), ) !dbg dbg_location=22
                Call id(7), args( Integer(2), EmptyTag, )
                Call id(8), args( Result(0), Tag(0, 5), )
                Call id(8), args( Result(1), Tag(1, 5), )
                Return
            Block 2:Block:
                Call id(5), args( Qubit(1), ) !dbg dbg_location=14
                Jump(1)
            Block 3:Block:
                Call id(6), args( Qubit(1), ) !dbg dbg_location=16
                Jump(1)

            dbg_metadata_scopes:
                0 = SubProgram name=Main location=(2-22)
                1 = SubProgram name=H location=(1-110222)
                2 = SubProgram name=M location=(1-111931)
                3 = SubProgram name=Measure location=(1-112847)
                4 = SubProgram name=MapPauliAxis location=(1-55426)
                5 = SubProgram name=MapPauliAxis' location=(1-55426)
                6 = SubProgram name=X location=(1-133020)
                7 = SubProgram name=Y location=(1-134242)
            dbg_locations:
                [0]: scope=0 location=(2-60)
                [1]: scope=0 location=(2-86)
                [2]: scope=0 location=(2-112)
                [3]: scope=1 location=(1-110294) inlined_at=2
                [4]: scope=0 location=(2-135)
                [5]: scope=2 location=(1-111973) inlined_at=4
                [6]: scope=3 location=(1-113034) inlined_at=5
                [7]: scope=3 location=(1-113087) inlined_at=5
                [8]: scope=4 location=(1-55512) inlined_at=7
                [9]: scope=3 location=(1-113160) inlined_at=5
                [10]: scope=3 location=(1-113087) inlined_at=5
                [11]: scope=5 location=(1-55512) inlined_at=10
                [12]: scope=0 location=(2-150)
                [13]: scope=0 location=(2-176)
                [14]: scope=6 location=(1-133092) inlined_at=13
                [15]: scope=0 location=(2-212)
                [16]: scope=7 location=(1-134314) inlined_at=15
                [17]: scope=0 location=(2-246)
                [18]: scope=2 location=(1-111973) inlined_at=17
                [19]: scope=3 location=(1-113034) inlined_at=18
                [20]: scope=3 location=(1-113087) inlined_at=18
                [21]: scope=4 location=(1-55512) inlined_at=20
                [22]: scope=3 location=(1-113160) inlined_at=18
                [23]: scope=3 location=(1-113087) inlined_at=18
                [24]: scope=5 location=(1-55512) inlined_at=23
                [25]: scope=0 location=(2-86)
                [26]: scope=0 location=(2-60)"#]],
    );
}
