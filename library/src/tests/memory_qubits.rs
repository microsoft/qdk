// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{logical_counts_with_lib, test_expression_fails, test_expression_with_lib};
use indoc::indoc;
use qsc::interpret::Value;

// Tests for memory qubits and Std.MemoryQubits namespace.

#[test]
fn memory_qubit_store_load_array_syntax() {
    test_expression_with_lib(
        "Test.Main()",
        indoc! {r#"
            namespace Test {
                operation Main() : Result[] {
                    use qs = Qubit[2];
                    use mems = MemoryQubit[2];
                    X(qs[0]);
                    Std.MemoryQubits.Store(qs[0], mems[0]);
                    Std.MemoryQubits.Load(mems[0], qs[1]);
                    return [MResetZ(qs[0]), MResetZ(qs[1])];
                }
            }
        "#},
        &Value::Array(vec![Value::RESULT_ZERO, Value::RESULT_ONE].into()),
    );
}

#[test]
fn memory_qubit_store_load_tuple_syntax() {
    test_expression_with_lib(
        "Test.Main()",
        indoc! {r#"
            namespace Test {
                operation Main() : Result[] {
                    use qs = Qubit[3];
                    use (m1, m2, m3) = (MemoryQubit(), MemoryQubit(), MemoryQubit());
                    X(qs[0]);
                    X(qs[2]);
                    Std.MemoryQubits.Store(qs[0], m1);
                    Std.MemoryQubits.Store(qs[1], m2);
                    Std.MemoryQubits.Store(qs[2], m3);
                    Std.MemoryQubits.Load(m1, qs[0]);
                    Std.MemoryQubits.Load(m2, qs[1]);
                    Std.MemoryQubits.Load(m3, qs[2]);
                    return [MResetZ(qs[0]), MResetZ(qs[1]), MResetZ(qs[2])];
                }
            }
        "#},
        &Value::Array(vec![Value::RESULT_ONE, Value::RESULT_ZERO, Value::RESULT_ONE].into()),
    );
}

#[test]
fn memory_qubit_release_non_zero_fails() {
    let err = test_expression_fails(indoc! {r#"
        {
            use q = Qubit();
            use m = MemoryQubit();
            X(q);
            Std.MemoryQubits.Store(q, m);
            ()
        }
    "#});

    assert!(
        err.contains("released while not in |0"),
        "expected non-zero release error, got: {err}"
    );
}

#[test]
fn memory_qubit_store_load() {
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

// Resource estimation tests.

#[test]
fn re_store_load_counts_manual_memory_usage() {
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

// This tests checks that MemoryQubits cannot be reused as Qubits.
// Resource estimator must draw them from separate pools.
#[test]
fn re_separate_qubit_pools() {
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

// Add a test for different syntaxes.
// Add a test for when MemoryQubit is released in non-zero state.
// Add a test for MemoryQubit arrays.
// Add a test for not reusing memory as compute in RE.
// Add RE test with uneven load/stores.
