// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{assert_blocks, get_rir_program, get_rir_program_with_capabilities};
use expect_test::expect;
use indoc::indoc;
use qsc_data_structures::target::Profile;

#[test]
fn baseline_qubit_ids_recycled_without_parallel() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            operation op(q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                // q1 and q2 are allocated and released inside the inner block
                { use q1 = Qubit(); op(q1); use q2 = Qubit(); op(q2); }
                // q1 and q2 are now released; next allocations reuse the same IDs
                use q3 = Qubit();
                op(q3);
                use q4 = Qubit();
                op(q4);
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
                Call id(2), args( Qubit(0), )
                Call id(2), args( Qubit(1), )
                Call id(2), args( Qubit(0), )
                Call id(2), args( Qubit(1), )
                Call id(3), args( Integer(0), Tag(0, 3), )
                Return"#]],
    );
    assert_eq!(program.num_qubits, 2);
    assert_eq!(program.num_results, 0);
}

#[test]
fn parallel_defers_qubit_release() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            operation op(q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                parallel {
                    // q1 and q2 are allocated and released inside the inner block
                    { use q1 = Qubit(); op(q1); use q2 = Qubit(); op(q2); }
                    // inside parallel their release is deferred, so q3 and q4 get fresh ids
                    use q3 = Qubit();
                    op(q3);
                    use q4 = Qubit();
                    op(q4);
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
                Call id(2), args( Qubit(0), )
                Call id(2), args( Qubit(1), )
                Call id(2), args( Qubit(2), )
                Call id(2), args( Qubit(3), )
                Call id(3), args( Integer(0), Tag(0, 3), )
                Return"#]],
    );
    assert_eq!(program.num_qubits, 4);
    assert_eq!(program.num_results, 0);
}

#[test]
fn parallel_releases_available_after_block_ends() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            operation op(q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                parallel {
                    use q = Qubit();
                    op(q);
                }
                parallel {
                    use q = Qubit();
                    op(q);
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
                Call id(2), args( Qubit(0), )
                Call id(2), args( Qubit(0), )
                Call id(3), args( Integer(0), Tag(0, 3), )
                Return"#]],
    );
    assert_eq!(program.num_qubits, 1);
    assert_eq!(program.num_results, 0);
}

#[test]
fn parallel_nested_defers_inner_releases_to_outer() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            operation op(q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                parallel {
                    use outer = Qubit();
                    op(outer);
                    parallel {
                        use inner1 = Qubit();
                        op(inner1);
                        use inner2 = Qubit();
                        op(inner2);
                    }
                    // inner qubits are now deferred in the outer layer, so a fresh id is allocated
                    use outer2 = Qubit();
                    op(outer2);
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
                Call id(2), args( Qubit(0), )
                Call id(2), args( Qubit(1), )
                Call id(2), args( Qubit(2), )
                Call id(2), args( Qubit(3), )
                Call id(3), args( Integer(0), Tag(0, 3), )
                Return"#]],
    );
    assert_eq!(program.num_qubits, 4);
    assert_eq!(program.num_results, 0);
}

#[test]
fn parallel_within_reuses_ids_after_limit() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            operation op(q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                parallel within 2 {
                    // Each nested block releases its qubit before the next allocation.
                    { use q1 = Qubit(); op(q1); }
                    { use q2 = Qubit(); op(q2); }
                    // 2 qubits have now been deferred; limit reached so q3 and q4 reuse ids
                    { use q3 = Qubit(); op(q3); }
                    { use q4 = Qubit(); op(q4); }
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
                Call id(2), args( Qubit(0), )
                Call id(2), args( Qubit(1), )
                Call id(2), args( Qubit(0), )
                Call id(2), args( Qubit(1), )
                Call id(3), args( Integer(0), Tag(0, 3), )
                Return"#]],
    );
    assert_eq!(program.num_qubits, 2);
    assert_eq!(program.num_results, 0);
}

#[test]
fn parallel_within_nested_defers_through_outer_limit() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            operation op(q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                parallel within 6 { for _ in 0..2 {
                    { use q0 = Qubit(); op(q0); }
                    parallel within 2 {
                        { use q1 = Qubit(); op(q1); }
                        { use q2 = Qubit(); op(q2); }
                        { use q3 = Qubit(); op(q3); }
                        { use q4 = Qubit(); op(q4); }
                    }
                } }
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
                Variable(0, Integer) = Store Integer(0)
                Call id(2), args( Qubit(0), )
                Call id(2), args( Qubit(1), )
                Call id(2), args( Qubit(2), )
                Call id(2), args( Qubit(1), )
                Call id(2), args( Qubit(2), )
                Variable(0, Integer) = Store Integer(1)
                Call id(2), args( Qubit(3), )
                Call id(2), args( Qubit(4), )
                Call id(2), args( Qubit(5), )
                Call id(2), args( Qubit(4), )
                Call id(2), args( Qubit(5), )
                Variable(0, Integer) = Store Integer(2)
                Call id(2), args( Qubit(0), )
                Call id(2), args( Qubit(1), )
                Call id(2), args( Qubit(2), )
                Call id(2), args( Qubit(1), )
                Call id(2), args( Qubit(2), )
                Variable(0, Integer) = Store Integer(3)
                Call id(3), args( Integer(0), Tag(0, 3), )
                Return"#]],
    );
    assert_eq!(program.num_qubits, 6);
    assert_eq!(program.num_results, 0);
}

#[test]
fn parallel_nested_unlimited_outer_defers_all() {
    let program = get_rir_program(indoc! {
        r#"
        namespace Test {
            operation op(q : Qubit) : Unit { body intrinsic; }
            @EntryPoint()
            operation Main() : Unit {
                parallel { for _ in 0..2 {
                    { use q0 = Qubit(); op(q0); }
                    parallel within 2 {
                        { use q1 = Qubit(); op(q1); }
                        { use q2 = Qubit(); op(q2); }
                        { use q3 = Qubit(); op(q3); }
                        { use q4 = Qubit(); op(q4); }
                    }
                } }
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
                Variable(0, Integer) = Store Integer(0)
                Call id(2), args( Qubit(0), )
                Call id(2), args( Qubit(1), )
                Call id(2), args( Qubit(2), )
                Call id(2), args( Qubit(1), )
                Call id(2), args( Qubit(2), )
                Variable(0, Integer) = Store Integer(1)
                Call id(2), args( Qubit(3), )
                Call id(2), args( Qubit(4), )
                Call id(2), args( Qubit(5), )
                Call id(2), args( Qubit(4), )
                Call id(2), args( Qubit(5), )
                Variable(0, Integer) = Store Integer(2)
                Call id(2), args( Qubit(6), )
                Call id(2), args( Qubit(7), )
                Call id(2), args( Qubit(8), )
                Call id(2), args( Qubit(7), )
                Call id(2), args( Qubit(8), )
                Variable(0, Integer) = Store Integer(3)
                Call id(3), args( Integer(0), Tag(0, 3), )
                Return"#]],
    );
    assert_eq!(program.num_qubits, 9);
    assert_eq!(program.num_results, 0);
}

#[test]
fn parallel_forces_loop_unrolling_with_adaptive_rifla() {
    // Without parallel, the loop uses backward branching (multiple blocks).
    let program_no_parallel = get_rir_program_with_capabilities(
        indoc! {
            r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                for _ in 0..1 {
                    use q = Qubit();
                    H(q);
                }
            }
        }
        "#,
        },
        Profile::AdaptiveRIFLA.into(),
    );
    assert_blocks(
        &program_no_parallel,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Pointer, )
                Variable(0, Integer) = Store Integer(0)
                Jump(1)
            Block 1:Block:
                Variable(1, Boolean) = Icmp Sle, Variable(0, Integer), Integer(1)
                Variable(2, Boolean) = Store Bool(true)
                Branch Variable(1, Boolean), 3, 4
            Block 2:Block:
                Call id(3), args( Integer(0), Tag(0, 3), )
                Return
            Block 3:Block:
                Branch Variable(2, Boolean), 5, 2
            Block 4:Block:
                Variable(2, Boolean) = Store Bool(false)
                Jump(3)
            Block 5:Block:
                Call id(2), args( Qubit(0), )
                Variable(3, Integer) = Add Variable(0, Integer), Integer(1)
                Variable(0, Integer) = Store Variable(3, Integer)
                Jump(1)"#]],
    );

    // With parallel, the same loop is unrolled into a single block.
    let program_parallel = get_rir_program_with_capabilities(
        indoc! {
            r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Unit {
                parallel for _ in 0..1 {
                    use q = Qubit();
                    H(q);
                }
            }
        }
        "#,
        },
        Profile::AdaptiveRIFLA.into(),
    );
    assert_blocks(
        &program_parallel,
        &expect![[r#"
            Blocks:
            Block 0:Block:
                Call id(1), args( Pointer, )
                Variable(0, Integer) = Store Integer(0)
                Call id(2), args( Qubit(0), )
                Variable(0, Integer) = Store Integer(1)
                Call id(2), args( Qubit(1), )
                Variable(0, Integer) = Store Integer(2)
                Call id(3), args( Integer(0), Tag(0, 3), )
                Return"#]],
    );
}
