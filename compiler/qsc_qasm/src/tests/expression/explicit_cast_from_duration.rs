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
fn duration_to_bool_fails() {
    let source = "
        duration a;
        bool(a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.NotSupported

              x duration type values are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         duration a;
               :         ^^^^^^^^
             3 |         bool(a);
               `----

            Qasm.Lowerer.CannotCast

              x cannot cast expression of type Duration(false) to type Bool(false)
               ,-[Test.qasm:3:14]
             2 |         duration a;
             3 |         bool(a);
               :              ^
             4 |     
               `----

            Qasm.Compiler.NotSupported

              x timing literals are not supported
               ,-[Test.qasm:1:1]
             1 | 
               : ^
             2 |         duration a;
             3 |         bool(a);
               `----
        "#]],
    );
}

//===================
// Casts to duration
//===================

#[test]
fn duration_to_duration() {
    let source = "
        duration a;
        duration(a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.NotSupported

              x duration type values are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         duration a;
               :         ^^^^^^^^
             3 |         duration(a);
               `----

            Qasm.Compiler.NotSupported

              x timing literals are not supported
               ,-[Test.qasm:1:1]
             1 | 
               : ^
             2 |         duration a;
             3 |         duration(a);
               `----
        "#]],
    );
}

//=========================
// Casts to int and int[n]
//=========================

#[test]
fn duration_to_int_fails() {
    let source = "
        duration a;
        int(a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.NotSupported

              x duration type values are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         duration a;
               :         ^^^^^^^^
             3 |         int(a);
               `----

            Qasm.Lowerer.CannotCast

              x cannot cast expression of type Duration(false) to type Int(None, false)
               ,-[Test.qasm:3:13]
             2 |         duration a;
             3 |         int(a);
               :             ^
             4 |     
               `----

            Qasm.Compiler.NotSupported

              x timing literals are not supported
               ,-[Test.qasm:1:1]
             1 | 
               : ^
             2 |         duration a;
             3 |         int(a);
               `----
        "#]],
    );
}

#[test]
fn duration_to_sized_int_fails() {
    let source = "
        duration a;
        int[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.NotSupported

              x duration type values are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         duration a;
               :         ^^^^^^^^
             3 |         int[32](a);
               `----

            Qasm.Lowerer.CannotCast

              x cannot cast expression of type Duration(false) to type Int(Some(32),
              | false)
               ,-[Test.qasm:3:17]
             2 |         duration a;
             3 |         int[32](a);
               :                 ^
             4 |     
               `----

            Qasm.Compiler.NotSupported

              x timing literals are not supported
               ,-[Test.qasm:1:1]
             1 | 
               : ^
             2 |         duration a;
             3 |         int[32](a);
               `----
        "#]],
    );
}

//===========================
// Casts to uint and uint[n]
//===========================

#[test]
fn duration_to_uint_fails() {
    let source = "
        duration a;
        uint(a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.NotSupported

              x duration type values are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         duration a;
               :         ^^^^^^^^
             3 |         uint(a);
               `----

            Qasm.Lowerer.CannotCast

              x cannot cast expression of type Duration(false) to type UInt(None, false)
               ,-[Test.qasm:3:14]
             2 |         duration a;
             3 |         uint(a);
               :              ^
             4 |     
               `----

            Qasm.Compiler.NotSupported

              x timing literals are not supported
               ,-[Test.qasm:1:1]
             1 | 
               : ^
             2 |         duration a;
             3 |         uint(a);
               `----
        "#]],
    );
}

#[test]
fn duration_to_sized_uint_fails() {
    let source = "
        duration a;
        uint[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.NotSupported

              x duration type values are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         duration a;
               :         ^^^^^^^^
             3 |         uint[32](a);
               `----

            Qasm.Lowerer.CannotCast

              x cannot cast expression of type Duration(false) to type UInt(Some(32),
              | false)
               ,-[Test.qasm:3:18]
             2 |         duration a;
             3 |         uint[32](a);
               :                  ^
             4 |     
               `----

            Qasm.Compiler.NotSupported

              x timing literals are not supported
               ,-[Test.qasm:1:1]
             1 | 
               : ^
             2 |         duration a;
             3 |         uint[32](a);
               `----
        "#]],
    );
}

//=============================
// Casts to float and float[n]
//=============================

#[test]
fn duration_to_float_fails() {
    let source = "
        duration a;
        float(a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.NotSupported

              x duration type values are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         duration a;
               :         ^^^^^^^^
             3 |         float(a);
               `----

            Qasm.Lowerer.CannotCast

              x cannot cast expression of type Duration(false) to type Float(None, false)
               ,-[Test.qasm:3:15]
             2 |         duration a;
             3 |         float(a);
               :               ^
             4 |     
               `----

            Qasm.Compiler.NotSupported

              x timing literals are not supported
               ,-[Test.qasm:1:1]
             1 | 
               : ^
             2 |         duration a;
             3 |         float(a);
               `----
        "#]],
    );
}

#[test]
fn duration_to_sized_float_fails() {
    let source = "
        duration a;
        float[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.NotSupported

              x duration type values are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         duration a;
               :         ^^^^^^^^
             3 |         float[32](a);
               `----

            Qasm.Lowerer.CannotCast

              x cannot cast expression of type Duration(false) to type Float(Some(32),
              | false)
               ,-[Test.qasm:3:19]
             2 |         duration a;
             3 |         float[32](a);
               :                   ^
             4 |     
               `----

            Qasm.Compiler.NotSupported

              x timing literals are not supported
               ,-[Test.qasm:1:1]
             1 | 
               : ^
             2 |         duration a;
             3 |         float[32](a);
               `----
        "#]],
    );
}

//=============================
// Casts to angle and angle[n]
//=============================

#[test]
fn duration_to_angle_fails() {
    let source = "
        duration a;
        angle(a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.NotSupported

              x duration type values are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         duration a;
               :         ^^^^^^^^
             3 |         angle(a);
               `----

            Qasm.Lowerer.CannotCast

              x cannot cast expression of type Duration(false) to type Angle(None, false)
               ,-[Test.qasm:3:15]
             2 |         duration a;
             3 |         angle(a);
               :               ^
             4 |     
               `----

            Qasm.Compiler.NotSupported

              x timing literals are not supported
               ,-[Test.qasm:1:1]
             1 | 
               : ^
             2 |         duration a;
             3 |         angle(a);
               `----
        "#]],
    );
}

#[test]
fn duration_to_sized_angle_fails() {
    let source = "
        duration a;
        angle[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.NotSupported

              x duration type values are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         duration a;
               :         ^^^^^^^^
             3 |         angle[32](a);
               `----

            Qasm.Lowerer.CannotCast

              x cannot cast expression of type Duration(false) to type Angle(Some(32),
              | false)
               ,-[Test.qasm:3:19]
             2 |         duration a;
             3 |         angle[32](a);
               :                   ^
             4 |     
               `----

            Qasm.Compiler.NotSupported

              x timing literals are not supported
               ,-[Test.qasm:1:1]
             1 | 
               : ^
             2 |         duration a;
             3 |         angle[32](a);
               `----
        "#]],
    );
}

//=================================
// Casts to complex and complex[n]
//=================================

#[test]
fn duration_to_complex_fails() {
    let source = "
        duration a;
        complex(a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.NotSupported

              x duration type values are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         duration a;
               :         ^^^^^^^^
             3 |         complex(a);
               `----

            Qasm.Lowerer.CannotCast

              x cannot cast expression of type Duration(false) to type Complex(None,
              | false)
               ,-[Test.qasm:3:17]
             2 |         duration a;
             3 |         complex(a);
               :                 ^
             4 |     
               `----

            Qasm.Compiler.NotSupported

              x timing literals are not supported
               ,-[Test.qasm:1:1]
             1 | 
               : ^
             2 |         duration a;
             3 |         complex(a);
               `----
        "#]],
    );
}

#[test]
fn duration_to_sized_complex_fails() {
    let source = "
        duration a;
        complex[float[32]](a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.NotSupported

              x duration type values are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         duration a;
               :         ^^^^^^^^
             3 |         complex[float[32]](a);
               `----

            Qasm.Lowerer.CannotCast

              x cannot cast expression of type Duration(false) to type Complex(Some(32),
              | false)
               ,-[Test.qasm:3:28]
             2 |         duration a;
             3 |         complex[float[32]](a);
               :                            ^
             4 |     
               `----

            Qasm.Compiler.NotSupported

              x timing literals are not supported
               ,-[Test.qasm:1:1]
             1 | 
               : ^
             2 |         duration a;
             3 |         complex[float[32]](a);
               `----
        "#]],
    );
}

//=================================
// Casts to bit and bit[n]
//=================================

#[test]
fn duration_to_bit_fails() {
    let source = "
        duration a;
        bit(a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.NotSupported

              x duration type values are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         duration a;
               :         ^^^^^^^^
             3 |         bit(a);
               `----

            Qasm.Lowerer.CannotCast

              x cannot cast expression of type Duration(false) to type Bit(false)
               ,-[Test.qasm:3:13]
             2 |         duration a;
             3 |         bit(a);
               :             ^
             4 |     
               `----

            Qasm.Compiler.NotSupported

              x timing literals are not supported
               ,-[Test.qasm:1:1]
             1 | 
               : ^
             2 |         duration a;
             3 |         bit(a);
               `----
        "#]],
    );
}

#[test]
fn duration_to_bitarray_fails() {
    let source = "
        duration a;
        bit[32](a);
    ";
    check(
        source,
        &expect![[r#"
            Qasm.Lowerer.NotSupported

              x duration type values are not supported
               ,-[Test.qasm:2:9]
             1 | 
             2 |         duration a;
               :         ^^^^^^^^
             3 |         bit[32](a);
               `----

            Qasm.Lowerer.CannotCast

              x cannot cast expression of type Duration(false) to type BitArray(32, false)
               ,-[Test.qasm:3:17]
             2 |         duration a;
             3 |         bit[32](a);
               :                 ^
             4 |     
               `----

            Qasm.Compiler.NotSupported

              x timing literals are not supported
               ,-[Test.qasm:1:1]
             1 | 
               : ^
             2 |         duration a;
             3 |         bit[32](a);
               `----
        "#]],
    );
}
