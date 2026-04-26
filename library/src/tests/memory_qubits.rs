// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{logical_counts_with_lib, test_expression_with_lib};
use indoc::indoc;
use qsc::interpret::Value;

// Tests for memory qubits and Std.MemoryQubits namespace.

#[test]
fn memory_qubit_store_load() {
    test_expression_with_lib(
        "Test.Main()",
        indoc! {r#"
            namespace Test {
                operation Main() : Result {
                    use q = Qubit();
                    let mem = Std.MemoryQubits.Allocate();
                    X(q);
                    Std.MemoryQubits.Store(q, mem);
                    Std.MemoryQubits.Load(mem, q);
                    Std.MemoryQubits.Free(mem);
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
                    let mem = Std.MemoryQubits.Allocate();
                    X(q);
                    Std.MemoryQubits.Store(q, mem);
                    Std.MemoryQubits.Load(mem, q);
                    Std.MemoryQubits.Free(mem);
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
                    let mem = Std.MemoryQubits.Allocate();
                    H(q);
                    Std.MemoryQubits.Store(q, mem);
                    Std.MemoryQubits.Load(mem, q);
                    Std.MemoryQubits.Free(mem);
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