// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;

#[test]
fn type_preservation_array_backed_qubit_return() {
    // Array-backed return slot for a Qubit-returning loop. Beyond the
    // centralized PostReturnUnify invariant run inside `compile_return_unified`,
    // the snapshot explicitly pins the preserved `Qubit` output type and the
    // `Qubit` block type of the unified body, so a regression that widened or
    // dropped the return type would surface here directly.
    check_structure(
        indoc! {r#"
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
    "#},
        &["Pick"],
        &expect![[r#"
            callable Pick: input_ty=Qubit, output_ty=Qubit
                body: block_ty=Qubit
                    [0] Local(Mutable, _.has_returned: Bool): Lit(Bool(false))
                    [1] Local(Mutable, _.ret_val: (Qubit)[]): Array(len=0)
                    [2] Local(Mutable, i: Int): Lit(Int(0))
                    [3] Expr While[ty=Unit]
                    [4] Expr If(cond=Var[ty=Bool], then=Index, else=Block[ty=Qubit])"#]],
    );
}

#[test]
fn type_preservation_double_return() {
    // Double return type. Beyond the centralized PostReturnUnify invariant run
    // inside `compile_return_unified`, the snapshot explicitly pins the preserved
    // `Double` output type and `Double` block type of the unified body.
    check_structure(
        indoc! {r#"
        namespace Test {
            function Main() : Double {
                if true {
                    return 1.0;
                }
                2.0
            }
        }
    "#},
        &["Main"],
        &expect![[r#"
            callable Main: input_ty=Unit, output_ty=Double
                body: block_ty=Double
                    [0] Expr If(cond=Lit(Bool(true)), then=Block[ty=Double], else=Block[ty=Double])"#]],
    );
}
