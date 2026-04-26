// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{logical_counts_with_lib, test_expression_with_lib};
use indoc::indoc;
use qsc::interpret::Value;

// Tests for memory qubits and Std.MemoryQubits namespace.

#[test]
fn qmem_store_load() {
    test_expression_with_lib(
        "Test.Main()",
        indoc! {r#"
            namespace Test {
                operation Main() : Result {
                    use q = Qubit();
                    let mem = Std.Memory.Allocate();
                    X(q);
                    Std.Memory.Store(q, mem);
                    Std.Memory.Load(mem, q);
                    Std.Memory.Free(mem);
                    return MResetZ(q);
                }
            }
        "#},
        &Value::RESULT_ONE,
    );
}

#[test]
fn qmem_store_load_counts_manual_memory_usage() {
    let counts = logical_counts_with_lib(
        "Test.Main()",
        indoc! {r#"
            namespace Test {
                operation Main() : Unit {
                    use q = Qubit();
                    let mem = Std.Memory.Allocate();
                    X(q);
                    Std.Memory.Store(q, mem);
                    Std.Memory.Load(mem, q);
                    Std.Memory.Free(mem);
                }
            }
        "#},
    );

    assert_eq!(counts.num_qubits, 2);
    assert_eq!(counts.num_compute_qubits, Some(1));
    assert_eq!(counts.read_from_memory_count, Some(1));
    assert_eq!(counts.write_to_memory_count, Some(1));
}

// Add a test for different syntaxes.
// Add a test for when QMem is released in non-zero state.
// Add a test for QMem arrays.
// Add a test for not reusing memory as compute in RE.
