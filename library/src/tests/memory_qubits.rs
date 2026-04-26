// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{
    logical_counts_with_lib, test_expression_compile_fails, test_expression_fails,
    test_expression_with_lib,
};
use indoc::indoc;
use qsc::interpret::Value;

// Tests for memory qubits and Std.MemoryQubits namespace.

#[test]
fn check_store_load() {
    test_expression_with_lib(
        "Test.Main()",
        indoc! {r#"
            namespace Test {
                operation Main() : Result {
                    use (q, mem) = (Qubit(), MemoryQubit());
                    X(q);
                    Std.MemoryQubits.Store(q, mem);
                    Std.MemoryQubits.Load(mem, q);
                    return MResetZ(q);
                }
            }
        "#},
        &Value::RESULT_ONE,
    );
}

#[test]
fn check_array_store_load() {
    test_expression_with_lib(
        "Test.Main()",
        indoc! {r#"
            namespace Test {
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    use mems = MemoryQubit[2];
                    X(qs[0]);
                    Std.MemoryQubits.StoreArray(qs, mems);
                    Std.MemoryQubits.LoadArray(mems, qs);
                    return [MResetZ(qs[0]), MResetZ(qs[1])];
                }
            }
        "#},
        &Value::Array(vec![Value::RESULT_ONE, Value::RESULT_ZERO].into()),
    );
}

#[test]
fn check_cannot_apply_gate_to_memory_qubit() {
    let err = test_expression_compile_fails(indoc! {r#"
        {
            use q = MemoryQubit();
            X(q);
        }
    "#});

    assert!(err.contains("type error"));
}

#[test]
fn check_cannot_measure_memory_qubit() {
    let err = test_expression_compile_fails(indoc! {r#"
        {
            use q = MemoryQubit();
            M(q);
        }
    "#});

    assert!(err.contains("type error"));
}

// MemoryQubit cannot be released in non-zero state (same as Qubit).
#[test]
fn check_release_non_zero_fails() {
    let err = test_expression_fails(indoc! {r#"
        {
            use (q, m) = (Qubit(), MemoryQubit());
            X(q);
            Std.MemoryQubits.Store(q, m);
            Reset(q);
        }
    "#});

    assert!(err.contains("released while not in |0"));
}

// Check that after the computation, all qubits are released in 0 state.
#[test]
fn check_do_computation_with_qft() {
    test_expression_with_lib(
        "Test.Main()",
        indoc! {r#"
            namespace Test {
                operation Main() : Bool {
                    use (qs, mems) = (Qubit[4], MemoryQubit[4]);
                    Adjoint ApplyQFT(qs);
                    Std.MemoryQubits.StoreArray(qs, mems);
                    Std.MemoryQubits.DoComputation(mems, ApplyQFT);
                    return true;
                }
            }
        "#},
        &Value::Bool(true),
    );
}

// Resource estimation tests.

#[test]
fn check_resource_estimation_single_qubit() {
    let counts = logical_counts_with_lib(
        "Test.Main()",
        indoc! {r#"
            namespace Test {
                operation Main() : Unit {
                    use q = Qubit();
                    use mem = MemoryQubit();
                    X(q);
                    Std.MemoryQubits.Store(q, mem);
                    Std.MemoryQubits.Load(mem, q);
                }
            }
        "#},
    );

    assert_eq!(counts.num_qubits, 2);
    assert_eq!(counts.num_compute_qubits, Some(1));
    assert_eq!(counts.read_from_memory_count, Some(1));
    assert_eq!(counts.write_to_memory_count, Some(1));
}

// Resource estimation of computation with memory qubits.
#[test]
fn check_resource_estimation_do_computation() {
    let counts = logical_counts_with_lib(
        "Test.Main()",
        indoc! {r#"
            namespace Test {
                operation Main() : Unit {
                    use qs = MemoryQubit[10];
                    Std.MemoryQubits.DoComputation(qs[0..4], ApplyQFT);
                    Std.MemoryQubits.DoComputation(qs[5..9], ApplyQFT);
                }
            }
        "#},
    );

    // 10 memory qubits, 5 compute qubits.
    assert_eq!(counts.num_qubits, 15);
    assert_eq!(counts.num_compute_qubits, Some(5));
    assert_eq!(counts.read_from_memory_count, Some(10));
    assert_eq!(counts.write_to_memory_count, Some(10));
}

#[test]
fn check_resource_estimation_store_only() {
    let counts = logical_counts_with_lib(
        "Test.Main()",
        indoc! {r#"
            namespace Test {
                operation Main() : Unit {
                    use (qs, mem) = (Qubit[10], MemoryQubit[10]);
                    H(qs[5]);
                    Std.MemoryQubits.StoreArray(qs, mem);
                }
            }
        "#},
    );

    assert_eq!(counts.num_qubits, 20);
    assert_eq!(counts.num_compute_qubits, Some(10));
    assert_eq!(counts.read_from_memory_count, Some(0));
    assert_eq!(counts.write_to_memory_count, Some(10));
}

#[test]
fn check_resource_estimation_load_only() {
    let counts = logical_counts_with_lib(
        "Test.Main()",
        indoc! {r#"
            namespace Test {
                operation Main() : Unit {
                    use (qs, mem) = (Qubit[10], MemoryQubit[10]);
                    Std.MemoryQubits.LoadArray(mem, qs);
                }
            }
        "#},
    );

    assert_eq!(counts.num_qubits, 20);
    assert_eq!(counts.num_compute_qubits, Some(10));
    assert_eq!(counts.read_from_memory_count, Some(10));
    assert_eq!(counts.write_to_memory_count, Some(0));
}

// This test checks that MemoryQubits cannot be reused as Qubits.
// Resource estimator must draw them from separate pools.
#[test]
fn check_resource_estimation_separate_qubit_pools() {
    let counts = logical_counts_with_lib(
        "Test.Main()",
        indoc! {r#"
            namespace Test {
                operation Op1() : Unit {
                    use q = Qubit();
                    use mem = MemoryQubit();
                    H(q);
                    Std.MemoryQubits.Store(q, mem);
                    Std.MemoryQubits.Load(mem, q);
                }
                operation Op2() : Unit {
                    use qs = Qubit[2];
                    H(qs[0]); H(qs[1]);
                    M(qs[0]); M(qs[1]);
                }
                operation Main() : Unit {
                    Op1();
                    Op2();
                }
            }
        "#},
    );

    // Maximum allocated qubits at any point is 2, but we need 1 memory qubit and
    // 2 compute qubits, so total number of qubits needed is 3.
    assert_eq!(counts.num_qubits, 3);
    assert_eq!(counts.num_compute_qubits, Some(2));
}
