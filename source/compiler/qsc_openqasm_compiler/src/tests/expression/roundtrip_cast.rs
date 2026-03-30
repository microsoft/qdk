// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::tests::check_qasm_to_qsharp as check;
use expect_test::expect;

//===========================================
// Round-trips through Result eliminated
//===========================================

#[test]
fn bit_xor_no_redundant_cast() {
    let source = "
        bit a;
        bit b;
        bit c = a ^ b;
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = Zero;
            mutable b = Zero;
            mutable c = Std.OpenQASM.Convert.IntAsResult(Std.OpenQASM.Convert.ResultAsInt(a) ^^^ Std.OpenQASM.Convert.ResultAsInt(b));
        "#]],
    );
}

#[test]
fn bit_and_no_redundant_cast() {
    let source = "
        bit a;
        bit b;
        bit c = a & b;
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = Zero;
            mutable b = Zero;
            mutable c = Std.OpenQASM.Convert.IntAsResult(Std.OpenQASM.Convert.ResultAsInt(a) &&& Std.OpenQASM.Convert.ResultAsInt(b));
        "#]],
    );
}

#[test]
fn bit_or_no_redundant_cast() {
    let source = "
        bit a;
        bit b;
        bit c = a | b;
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = Zero;
            mutable b = Zero;
            mutable c = Std.OpenQASM.Convert.IntAsResult(Std.OpenQASM.Convert.ResultAsInt(a) ||| Std.OpenQASM.Convert.ResultAsInt(b));
        "#]],
    );
}

//===========================================
// Explicit round-trips also eliminated
//===========================================

#[test]
fn explicit_int_to_bit_to_int() {
    let source = "
        int a;
        int(bit(a));
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = 0;
            (a);
        "#]],
    );
}

#[test]
fn explicit_bit_to_bool_to_bit() {
    let source = "
        bit a;
        bit(bool(a));
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = Zero;
            (a);
        "#]],
    );
}

//===========================================
// Non-elimination (different Q# types)
//===========================================

#[test]
fn int_to_bit_to_bigint_not_eliminated() {
    let source = "
        int[32] a;
        int[128](bit(a));
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = 0;
            Std.OpenQASM.Convert.ResultAsBigInt(Std.OpenQASM.Convert.IntAsResult(a));
        "#]],
    );
}

//===========================================
// Paren preservation
//===========================================

#[test]
fn paren_wrapped_roundtrip() {
    let source = "
        int a;
        int((bit(a)));
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = 0;
            (a);
        "#]],
    );
}

#[test]
fn paren_roundtrip_in_larger_expr() {
    let source = "
        int a = 1;
        int b = 2;
        int c = 3;
        int d = int((bit(a + b))) * c;
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = 1;
            mutable b = 2;
            mutable c = 3;
            mutable d = (a + b) * c;
        "#]],
    );
}

#[test]
fn no_paren_roundtrip_in_larger_expr() {
    let source = "
        int a = 1;
        int b = 2;
        int c = 3;
        int d = int(bit(a + b)) * c;
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = 1;
            mutable b = 2;
            mutable c = 3;
            mutable d = (a + b) * c;
        "#]],
    );
}

#[test]
fn bit_array_xor_original_repro() {
    let source = r#"
        include "stdgates.inc";
        qubit[4] q;
        bit[1] r1;
        r1[0] = measure q[0];
        bit[1] r2;
        r2[0] = measure q[1];
        if ((r1^r2)!=0) cx q[2],q[3];
    "#;
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            borrow q = Qubit[4];
            mutable r1 = [Zero];
            set r1[0] = Std.Intrinsic.M(q[0]);
            mutable r2 = [Zero];
            set r2[0] = Std.Intrinsic.M(q[1]);
            if (Std.OpenQASM.Convert.ResultArrayAsIntBE(r1) ^^^ Std.OpenQASM.Convert.ResultArrayAsIntBE(r2)) != 0 {
                cx(q[2], q[3]);
            };
        "#]],
    );
}
