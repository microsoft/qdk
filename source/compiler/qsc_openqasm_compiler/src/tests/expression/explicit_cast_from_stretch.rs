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
fn stretch_to_bool_fails() {
    let source = "
        stretch a;
        bool(a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type stretch to type const bool
               ,-[Test.qasm:3:9]
             2 |         stretch a;
             3 |         bool(a);
               :         ^^^^^^^
             4 |     
               `----

            Qasm.Compiler.NotSupported

              x stretch type values are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         stretch a;
               :         ^^^^^^^
             3 |         bool(a);
               `----

            Qasm.Compiler.NotSupported

              x timing literals are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         stretch a;
               :         ^^^^^^^^^^
             3 |         bool(a);
               `----
        "#]],
    );
}

//===================
// Casts to duration
//===================

#[test]
fn stretch_to_duration_fails() {
    let source = "
        stretch a;
        duration(a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Compiler.NotSupported

              x stretch type values are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         stretch a;
               :         ^^^^^^^
             3 |         duration(a);
               `----

            Qasm.Compiler.NotSupported

              x timing literals are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         stretch a;
               :         ^^^^^^^^^^
             3 |         duration(a);
               `----
        "#]],
    );
}

//===================
// Casts to stretch
//===================

#[test]
fn stretch_to_stretch() {
    let source = "
        stretch a;
        stretch(a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Compiler.NotSupported

              x stretch type values are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         stretch a;
               :         ^^^^^^^
             3 |         stretch(a);
               `----

            Qasm.Compiler.NotSupported

              x timing literals are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         stretch a;
               :         ^^^^^^^^^^
             3 |         stretch(a);
               `----
        "#]],
    );
}

//=========================
// Casts to int and int[n]
//=========================

#[test]
fn stretch_to_int_fails() {
    let source = "
        stretch a;
        int(a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type stretch to type const int
               ,-[Test.qasm:3:9]
             2 |         stretch a;
             3 |         int(a);
               :         ^^^^^^
             4 |     
               `----

            Qasm.Compiler.NotSupported

              x stretch type values are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         stretch a;
               :         ^^^^^^^
             3 |         int(a);
               `----

            Qasm.Compiler.NotSupported

              x timing literals are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         stretch a;
               :         ^^^^^^^^^^
             3 |         int(a);
               `----
        "#]],
    );
}

#[test]
fn stretch_to_sized_int_fails() {
    let source = "
        stretch a;
        int[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type stretch to type const int[32]
               ,-[Test.qasm:3:9]
             2 |         stretch a;
             3 |         int[32](a);
               :         ^^^^^^^^^^
             4 |     
               `----

            Qasm.Compiler.NotSupported

              x stretch type values are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         stretch a;
               :         ^^^^^^^
             3 |         int[32](a);
               `----

            Qasm.Compiler.NotSupported

              x timing literals are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         stretch a;
               :         ^^^^^^^^^^
             3 |         int[32](a);
               `----
        "#]],
    );
}

//===========================
// Casts to uint and uint[n]
//===========================

#[test]
fn stretch_to_uint_fails() {
    let source = "
        stretch a;
        uint(a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type stretch to type const uint
               ,-[Test.qasm:3:9]
             2 |         stretch a;
             3 |         uint(a);
               :         ^^^^^^^
             4 |     
               `----

            Qasm.Compiler.NotSupported

              x stretch type values are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         stretch a;
               :         ^^^^^^^
             3 |         uint(a);
               `----

            Qasm.Compiler.NotSupported

              x timing literals are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         stretch a;
               :         ^^^^^^^^^^
             3 |         uint(a);
               `----
        "#]],
    );
}

#[test]
fn stretch_to_sized_uint_fails() {
    let source = "
        stretch a;
        uint[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type stretch to type const uint[32]
               ,-[Test.qasm:3:9]
             2 |         stretch a;
             3 |         uint[32](a);
               :         ^^^^^^^^^^^
             4 |     
               `----

            Qasm.Compiler.NotSupported

              x stretch type values are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         stretch a;
               :         ^^^^^^^
             3 |         uint[32](a);
               `----

            Qasm.Compiler.NotSupported

              x timing literals are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         stretch a;
               :         ^^^^^^^^^^
             3 |         uint[32](a);
               `----
        "#]],
    );
}

//=============================
// Casts to float and float[n]
//=============================

#[test]
fn stretch_to_float_fails() {
    let source = "
        stretch a;
        float(a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type stretch to type const float
               ,-[Test.qasm:3:9]
             2 |         stretch a;
             3 |         float(a);
               :         ^^^^^^^^
             4 |     
               `----

            Qasm.Compiler.NotSupported

              x stretch type values are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         stretch a;
               :         ^^^^^^^
             3 |         float(a);
               `----

            Qasm.Compiler.NotSupported

              x timing literals are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         stretch a;
               :         ^^^^^^^^^^
             3 |         float(a);
               `----
        "#]],
    );
}

#[test]
fn stretch_to_sized_float_fails() {
    let source = "
        stretch a;
        float[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type stretch to type const float[32]
               ,-[Test.qasm:3:9]
             2 |         stretch a;
             3 |         float[32](a);
               :         ^^^^^^^^^^^^
             4 |     
               `----

            Qasm.Compiler.NotSupported

              x stretch type values are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         stretch a;
               :         ^^^^^^^
             3 |         float[32](a);
               `----

            Qasm.Compiler.NotSupported

              x timing literals are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         stretch a;
               :         ^^^^^^^^^^
             3 |         float[32](a);
               `----
        "#]],
    );
}

//=============================
// Casts to angle and angle[n]
//=============================

#[test]
fn stretch_to_angle_fails() {
    let source = "
        stretch a;
        angle(a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type stretch to type const angle
               ,-[Test.qasm:3:9]
             2 |         stretch a;
             3 |         angle(a);
               :         ^^^^^^^^
             4 |     
               `----

            Qasm.Compiler.NotSupported

              x stretch type values are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         stretch a;
               :         ^^^^^^^
             3 |         angle(a);
               `----

            Qasm.Compiler.NotSupported

              x timing literals are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         stretch a;
               :         ^^^^^^^^^^
             3 |         angle(a);
               `----
        "#]],
    );
}

#[test]
fn stretch_to_sized_angle_fails() {
    let source = "
        stretch a;
        angle[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type stretch to type const angle[32]
               ,-[Test.qasm:3:9]
             2 |         stretch a;
             3 |         angle[32](a);
               :         ^^^^^^^^^^^^
             4 |     
               `----

            Qasm.Compiler.NotSupported

              x stretch type values are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         stretch a;
               :         ^^^^^^^
             3 |         angle[32](a);
               `----

            Qasm.Compiler.NotSupported

              x timing literals are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         stretch a;
               :         ^^^^^^^^^^
             3 |         angle[32](a);
               `----
        "#]],
    );
}

//=================================
// Casts to complex and complex[n]
//=================================

#[test]
fn stretch_to_complex_fails() {
    let source = "
        stretch a;
        complex(a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type stretch to type const complex[float]
               ,-[Test.qasm:3:9]
             2 |         stretch a;
             3 |         complex(a);
               :         ^^^^^^^^^^
             4 |     
               `----

            Qasm.Compiler.NotSupported

              x stretch type values are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         stretch a;
               :         ^^^^^^^
             3 |         complex(a);
               `----

            Qasm.Compiler.NotSupported

              x timing literals are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         stretch a;
               :         ^^^^^^^^^^
             3 |         complex(a);
               `----
        "#]],
    );
}

#[test]
fn stretch_to_sized_complex_fails() {
    let source = "
        stretch a;
        complex[float[32]](a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type stretch to type const complex[float[32]]
               ,-[Test.qasm:3:9]
             2 |         stretch a;
             3 |         complex[float[32]](a);
               :         ^^^^^^^^^^^^^^^^^^^^^
             4 |     
               `----

            Qasm.Compiler.NotSupported

              x stretch type values are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         stretch a;
               :         ^^^^^^^
             3 |         complex[float[32]](a);
               `----

            Qasm.Compiler.NotSupported

              x timing literals are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         stretch a;
               :         ^^^^^^^^^^
             3 |         complex[float[32]](a);
               `----
        "#]],
    );
}

//=================================
// Casts to bit and bit[n]
//=================================

#[test]
fn stretch_to_bit_fails() {
    let source = "
        stretch a;
        bit(a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type stretch to type const bit
               ,-[Test.qasm:3:9]
             2 |         stretch a;
             3 |         bit(a);
               :         ^^^^^^
             4 |     
               `----

            Qasm.Compiler.NotSupported

              x stretch type values are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         stretch a;
               :         ^^^^^^^
             3 |         bit(a);
               `----

            Qasm.Compiler.NotSupported

              x timing literals are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         stretch a;
               :         ^^^^^^^^^^
             3 |         bit(a);
               `----
        "#]],
    );
}

#[test]
fn stretch_to_bitarray_fails() {
    let source = "
        stretch a;
        bit[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.CannotCast

              x cannot cast expression of type stretch to type const bit[32]
               ,-[Test.qasm:3:9]
             2 |         stretch a;
             3 |         bit[32](a);
               :         ^^^^^^^^^^
             4 |     
               `----

            Qasm.Compiler.NotSupported

              x stretch type values are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         stretch a;
               :         ^^^^^^^
             3 |         bit[32](a);
               `----

            Qasm.Compiler.NotSupported

              x timing literals are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         stretch a;
               :         ^^^^^^^^^^
             3 |         bit[32](a);
               `----
        "#]],
    );
}
