// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! These unit tests check six properties for each target type.
//! Let's call the type we are casting from `T` and the type we are casting to `Q`.
//! We want to test that for each type `Q` we correctly:
//!   1. cast from T to Q.
//!   2. cast from T to Q[n].
//!   3. cast from T[n] to Q.
//!   4. cast from T[n] to Q[n].
//!   5. cast from T[n] to Q[m] when n > m; a truncating cast.
//!   6. cast from T[n] to Q[m] when n < m; an expanding cast.

use crate::tests::check_qasm_to_qsharp as check;
use expect_test::expect;

//===============
// Casts to bool
//===============

#[test]
fn bool_to_bool() {
    let source = "
        bool a;
        bool(a);
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = false;
            a;
        "#]],
    );
}

//===================
// Casts to duration
//===================

#[test]
fn bool_to_duration_fails() {
    let source = "
        bool a;
        duration(a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type bool to type duration
               ,-[Test.qasm:3:9]
             2 |         bool a;
             3 |         duration(a);
               :         ^^^^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

//=========================
// Casts to int and int[n]
//=========================

#[test]
fn bool_to_int() {
    let source = "
        bool a;
        int(a);
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = false;
            Std.OpenQASM.Convert.BoolAsInt(a);
        "#]],
    );
}

#[test]
fn bool_to_sized_int() {
    let source = "
        bool a;
        int[32](a);
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = false;
            Std.OpenQASM.Convert.BoolAsInt(a);
        "#]],
    );
}

//===========================
// Casts to uint and uint[n]
//===========================

#[test]
fn bool_to_uint() {
    let source = "
        bool a;
        uint(a);
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = false;
            Std.OpenQASM.Convert.BoolAsInt(a);
        "#]],
    );
}

#[test]
fn bool_to_sized_uint() {
    let source = "
        bool a;
        uint[32](a);
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = false;
            Std.OpenQASM.Convert.BoolAsInt(a);
        "#]],
    );
}

//=============================
// Casts to float and float[n]
//=============================

#[test]
fn bool_to_float() {
    let source = "
        bool a;
        float(a);
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = false;
            Std.OpenQASM.Convert.BoolAsDouble(a);
        "#]],
    );
}

#[test]
fn bool_to_sized_float() {
    let source = "
        bool a;
        float[32](a);
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = false;
            Std.OpenQASM.Convert.BoolAsDouble(a);
        "#]],
    );
}

//=============================
// Casts to angle and angle[n]
//=============================

#[test]
fn bool_to_angle_fails() {
    let source = "
        bool a;
        angle(a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type bool to type angle
               ,-[Test.qasm:3:9]
             2 |         bool a;
             3 |         angle(a);
               :         ^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn bool_to_sized_angle_fails() {
    let source = "
        bool a;
        angle[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type bool to type angle[32]
               ,-[Test.qasm:3:9]
             2 |         bool a;
             3 |         angle[32](a);
               :         ^^^^^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

//=================================
// Casts to complex and complex[n]
//=================================

#[test]
fn bool_to_complex_fails() {
    let source = "
        bool a;
        complex(a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type bool to type complex[float]
               ,-[Test.qasm:3:9]
             2 |         bool a;
             3 |         complex(a);
               :         ^^^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

#[test]
fn bool_to_sized_complex_fails() {
    let source = "
        bool a;
        complex[float[32]](a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type bool to type complex[float[32]]
               ,-[Test.qasm:3:9]
             2 |         bool a;
             3 |         complex[float[32]](a);
               :         ^^^^^^^^^^^^^^^^^^^^^
             4 |     
               `----
        "#]],
    );
}

//=================================
// Casts to bit and bit[n]
//=================================

#[test]
fn bool_to_bit() {
    let source = "
        bool a;
        bit(a);
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = false;
            Std.OpenQASM.Convert.BoolAsResult(a);
        "#]],
    );
}

#[test]
fn bool_to_bitarray() {
    let source = "
        bool a;
        bit[32](a);
    ";
    check(
        source,
        &expect![[r#"
            import Std.OpenQASM.Intrinsic.*;
            mutable a = false;
            Std.OpenQASM.Convert.BoolAsResultArrayBE(a, 32);
        "#]],
    );
}
