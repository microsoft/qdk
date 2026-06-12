// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;

#[test]
fn type_preservation_array_backed_qubit_return() {
    // Array-backed return slot for a Qubit-returning loop; terminal/block type
    // parity is enforced by the centralized PostReturnUnify invariant run inside
    // `compile_return_unified`.
    compile_return_unified(indoc! {r#"
        namespace Test {
            operation Pick(q : Qubit) : Qubit {
                mutable i = 0;
                while i < 1 {
                    return q;
                }
                q
            }

            operation Main() : Unit {
                use q = Qubit();
                let returned = Pick(q);
                Reset(returned);
            }
        }
    "#});
}

#[test]
fn type_preservation_double_return() {
    // Double return type; terminal/block type parity is enforced by the
    // centralized PostReturnUnify invariant run inside `compile_return_unified`.
    compile_return_unified(indoc! {r#"
        namespace Test {
            function Main() : Double {
                if true {
                    return 1.0;
                }
                2.0
            }
        }
    "#});
}
